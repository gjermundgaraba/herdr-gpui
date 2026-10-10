//! A plugin's `ui.pick`, shown in the window's picker.
//!
//! The daemon sends `endpoint.pick.open.v1` to the one client that should
//! show the picker and holds the plugin's request open until that client
//! answers with `ui.pick.resolve`. So the answer goes back on the connection
//! the request arrived on, whichever of the window's connections that is, and
//! every way the picker can go away answers `cancelled` unless the daemon has
//! already given up on it: an `endpoint.pick.close.v1`, a lost connection, or
//! a new daemon boot. An answer the connection's full command queue would not
//! take waits in that connection's mailbox and is sent on a later poll.
//!
//! A plugin can close its picker while the answer is on its way. The daemon
//! then replies `pick_not_found`, which is that crossing and nothing else:
//! the mailbox keeps it from the status bar, as the terminal client does. Any
//! other error for an answer is shown like a failed command.
//!
//! A picker is shown only for the host the window presents. One that arrives
//! on another host's connection, such as the host the window has just
//! switched away from, is answered `cancelled` as it arrives and never shown.
//!
//! Mirrors `src/client/shell/picks.rs` in the Herdr fork, which answers by
//! the same rules but one. A picker that arrives while this window shows a
//! menu, a dialog, or a picker of its own is answered `cancelled` and never
//! shown; the terminal client closes the overlay it has open and shows the
//! picker.

use super::{
    Source,
    list::{Choice, Item, List},
};
use crate::{HerdrWindow, connection::ConnectionBridge};
use gpui::{Context, Window};
use herdr_client::{
    ClientEvent, ClientHandle, Method,
    protocol::pick::{EndpointPickOpen, UiPickOutcome, UiPickResolveParams},
};
use std::sync::{Arc, Mutex, Weak};

/// A picker the daemon asked for, with the boot it was asked under: the
/// answer is only meaningful to that boot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(super) boot: String,
    pub(super) open: EndpointPickOpen,
}

/// What a connection's picker mailbox has for the window since it last asked.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Update {
    /// The picker this connection last handed over is gone on the daemon's
    /// side: dismiss it without answering. Applies before `open`.
    pub(crate) closed: Option<String>,
    pub(crate) open: Option<Request>,
}

/// One connection's picker mailbox, filled by its event reader and drained by
/// the window. The daemon allows a client one picker at a time, so this holds
/// at most one picker not handed over yet and one closure of the picker that
/// was: bounded whatever the peer sends, and never a queue.
#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The boot of this connection's snapshots; a request needs one.
    boot: Option<String>,
    /// Asked for, not handed to the window yet.
    open: Option<Request>,
    /// The picker last handed to the window, until the daemon ends it. The
    /// window answering it itself is not reported back here; a later picker
    /// simply replaces it.
    shown: Option<String>,
    /// `shown` ended on the daemon's side; the window has not been told yet.
    closed: Option<String>,
    /// An answer the command queue was too full to take. The daemon lets a
    /// client hold one picker, so there is at most one; it goes with a new
    /// boot or the connection, whose daemon forgot the picker.
    owed: Option<Answer>,
    /// The requests of the answers the daemon has not replied to. Each stays
    /// until its reply or its rejection is filed here, or the connection or
    /// the daemon's boot goes. The daemon lets a client hold one picker, but
    /// a plugin can close one and open the next while the answers still wait
    /// behind the connection's one request in flight, so there can be many.
    /// The connection bounds them: its command queue takes 64 requests, its
    /// worker holds one more back and one in flight, its event queue holds 8
    /// replies not filed yet, and it ends when the one in flight is not
    /// answered in time.
    sent: Vec<String>,
}

/// `ui.pick.resolve` for one picker, ready to queue.
#[derive(Debug)]
struct Answer {
    boot: String,
    params: UiPickResolveParams,
}

impl Answer {
    /// Queues the answer and returns its request.
    fn queue(&self, handle: Option<&ClientHandle>) -> crate::Result<String> {
        let handle = handle.ok_or(crate::Error::NotConnected)?;
        let params = serde_json::to_value(&self.params)?;
        Ok(handle.request(&self.boot, Method::UiPickResolve, params)?)
    }
}

impl Inbox {
    /// Keeps the picker controls and passes every other event on.
    pub(crate) fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        match event {
            ClientEvent::PickOpen(open) => {
                match &self.boot {
                    Some(boot) => {
                        self.open = Some(Request {
                            boot: boot.clone(),
                            open,
                        });
                    }
                    // Nothing can be answered before a snapshot names the boot.
                    None => tracing::debug!(category = "pick_open", "picker before snapshot"),
                }
                None
            }
            ClientEvent::PickClose(close) => {
                if self
                    .open
                    .as_ref()
                    .is_some_and(|request| request.open.pick_id == close.pick_id)
                {
                    // Opened and closed between two polls: never shown.
                    self.open = None;
                } else if self.shown.as_deref() == Some(close.pick_id.as_str()) {
                    self.closed = self.shown.take();
                }
                None
            }
            ClientEvent::Snapshot(snapshot) => {
                if self.boot.as_deref() != Some(snapshot.boot_id.as_str()) {
                    // A restarted daemon has forgotten every picker.
                    self.forget();
                    self.boot = Some(snapshot.boot_id.clone());
                }
                Some(ClientEvent::Snapshot(snapshot))
            }
            ClientEvent::Disconnected { .. } => {
                // The daemon cancels a departed client's pickers itself.
                self.forget();
                self.boot = None;
                Some(event)
            }
            ClientEvent::Response {
                request_id,
                response,
            } => {
                // Expected: the plugin closed the picker while this answer
                // was on its way. Any other reply is the live state's.
                if self.replied(&request_id)
                    && response["error"]["code"].as_str() == Some("pick_not_found")
                {
                    tracing::debug!(category = "pick_resolve", "the picker had closed");
                    return None;
                }
                Some(ClientEvent::Response {
                    request_id,
                    response,
                })
            }
            ClientEvent::CommandRejected {
                request_id: Some(request_id),
                reason,
            } => {
                self.replied(&request_id);
                Some(ClientEvent::CommandRejected {
                    request_id: Some(request_id),
                    reason,
                })
            }
            event => Some(event),
        }
    }

    fn forget(&mut self) {
        self.open = None;
        self.owed = None;
        self.sent.clear();
        if let Some(shown) = self.shown.take() {
            self.closed = Some(shown);
        }
    }

    /// Whether `request` is an answer sent from here, which this reply ends.
    fn replied(&mut self, request: &str) -> bool {
        let sent = self.sent.iter().position(|id| id == request);
        sent.map(|index| self.sent.remove(index)).is_some()
    }

    /// Queues `answer` on `handle` and keeps its request for the daemon's
    /// reply. The caller holds this mailbox's lock across the queueing, so
    /// the reply cannot be filed before its request is known. A full queue
    /// keeps the answer for a later poll.
    fn send(&mut self, answer: Answer, handle: Option<&ClientHandle>) -> crate::Result<()> {
        match answer.queue(handle) {
            Ok(request) => self.sent.push(request),
            Err(crate::Error::Client(herdr_client::Error::Full)) => {
                if self.owed.replace(answer).is_some() {
                    tracing::warn!(category = "pick_resolve", "an older owed answer dropped");
                }
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Queues the answer still owed, if any, on `handle`. A queue still full
    /// keeps it for the next poll; any other failure means nobody waits.
    fn send_owed(&mut self, handle: Option<&ClientHandle>) {
        let Some(answer) = self.owed.take() else {
            return;
        };
        if let Err(error) = self.send(answer, handle) {
            tracing::debug!(category = "pick_resolve", %error, "owed answer dropped");
        }
    }

    /// Hands over what changed, once.
    pub(crate) fn take(&mut self) -> Update {
        let update = Update {
            closed: self.closed.take(),
            open: self.open.take(),
        };
        if let Some(request) = &update.open {
            self.shown = Some(request.open.pick_id.clone());
        }
        update
    }
}

/// The plugin picker a window is showing: enough to answer it.
pub(crate) struct PluginPick {
    /// The mailbox of the connection that asked. A bridge replaces its
    /// mailbox when it reconnects, so this names one connection for its
    /// whole life, wherever the window keeps that connection meanwhile.
    connection: Weak<Mutex<Inbox>>,
    boot: String,
    pick_id: String,
}

impl PluginPick {
    #[cfg(test)]
    pub(super) fn pick_id(&self) -> &str {
        &self.pick_id
    }

    fn is(&self, connection: &Weak<Mutex<Inbox>>, pick_id: &str) -> bool {
        self.connection.ptr_eq(connection) && self.pick_id == pick_id
    }
}

/// A picker's answer in the daemon's terms; no choice is a cancellation.
pub(super) fn outcome(choice: Option<Choice>) -> UiPickOutcome {
    match choice {
        Some(Choice::Item(id)) => UiPickOutcome::Picked { id },
        Some(Choice::Create(text)) => UiPickOutcome::Created { text },
        None => UiPickOutcome::Cancelled,
    }
}

impl HerdrWindow {
    /// Sends the answers a full queue held back, shows the pickers the daemon
    /// asked for, and dismisses the ones it ended. Each connection's mailbox
    /// is asked without waiting on its reader; one that is busy keeps what it
    /// has for the next poll.
    pub(crate) fn poll_plugin_picks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let updates: Vec<_> = self
            .connections()
            .filter_map(|connection| {
                let mut picks = connection.picks.try_lock().ok()?;
                picks.send_owed(connection.handle.as_ref());
                let update = picks.take();
                (update != Update::default()).then(|| (Arc::downgrade(&connection.picks), update))
            })
            .collect();
        for (connection, update) in updates {
            if let Some(closed) = update.closed
                && self
                    .plugin_pick()
                    .is_some_and(|pick| pick.is(&connection, &closed))
            {
                self.close_picker(window, cx);
            }
            if let Some(request) = update.open {
                self.show_plugin_pick(connection, request, window, cx);
            }
        }
        // A connection that was dropped or replaced took its picker with it.
        if self
            .plugin_pick()
            .is_some_and(|pick| self.pick_connection(pick).is_none())
        {
            self.close_picker(window, cx);
        }
    }

    /// The connection `pick` arrived on, while the window still holds it.
    fn pick_connection(&self, pick: &PluginPick) -> Option<&ConnectionBridge> {
        self.connection_of(&pick.connection)
    }

    /// Whether `connection` is one to the host the window presents: its own
    /// connection to that host, or one an editor group has parked, which
    /// shows another tab of the same host.
    fn presents(&self, connection: &Weak<Mutex<Inbox>>) -> bool {
        std::iter::once(&self.endpoints[self.selected_endpoint].connection)
            .chain(self.parked_connections())
            .any(|presented| presented.owns(connection))
    }

    fn plugin_pick(&self) -> Option<&PluginPick> {
        self.picker
            .as_ref()
            .and_then(|picker| match &picker.source {
                Source::Plugin(pick) => Some(pick),
                Source::Navigator(_) => None,
            })
    }

    /// Opens the window's picker for `request`. A window already showing a
    /// menu, a dialog, or another picker keeps it: an unasked-for list must
    /// not take over what the user is typing into, so the request is answered
    /// `cancelled` at once instead of being left to hang. So is one from a
    /// host the window does not present: its list is about another machine
    /// than the one on screen.
    fn show_plugin_pick(
        &mut self,
        connection: Weak<Mutex<Inbox>>,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Request { boot, open } = request;
        let pick = PluginPick {
            connection,
            boot,
            pick_id: open.pick_id,
        };
        if self.menu.page.is_some() || !self.presents(&pick.connection) {
            self.answer_plugin_pick(pick, UiPickOutcome::Cancelled);
            return;
        }
        let items = open
            .items
            .into_iter()
            .map(|item| {
                Item::new(
                    item.id,
                    &item.label,
                    item.detail.as_deref(),
                    item.badge.as_deref(),
                )
            })
            .collect();
        let list = List::new(
            items,
            open.create.as_ref().map(|create| create.label.as_str()),
            open.selected.as_deref(),
        );
        if let Err(Source::Plugin(pick)) =
            self.open_picker(&open.title, list, Source::Plugin(pick), window, cx)
        {
            self.answer_plugin_pick(pick, UiPickOutcome::Cancelled);
        }
    }

    /// Answers a plugin's picker on the connection that asked. A connection
    /// that is gone has nothing to answer: the daemon cancelled its pickers
    /// when the client left. A full command queue keeps the answer in the
    /// connection's mailbox until a later poll can queue it. Any other answer
    /// that cannot be queued, such as created text too long for one daemon
    /// command, is reported and replaced by `cancelled`, so the plugin is not
    /// left waiting on a picker that has closed.
    pub(super) fn answer_plugin_pick(&mut self, pick: PluginPick, outcome: UiPickOutcome) {
        let cancelled = outcome == UiPickOutcome::Cancelled;
        let answer = Answer {
            boot: pick.boot.clone(),
            params: UiPickResolveParams {
                pick_id: pick.pick_id.clone(),
                outcome,
            },
        };
        // The reader holds the mailbox's lock only to file one event. One it
        // poisoned belongs to a connection that is over.
        let sent = self
            .pick_connection(&pick)
            .and_then(|connection| Some((connection.picks.lock().ok()?, &connection.handle)))
            .ok_or(crate::Error::NotConnected)
            .and_then(|(mut picks, handle)| picks.send(answer, handle.as_ref()));
        match sent {
            Ok(()) => {}
            Err(
                crate::Error::NotConnected
                | crate::Error::Client(herdr_client::Error::Disconnected),
            ) => {
                tracing::debug!(
                    category = "pick_resolve",
                    "connection gone; nothing to answer"
                );
            }
            Err(error) if cancelled => {
                tracing::warn!(category = "pick_resolve", %error, "picker not cancelled");
            }
            Err(error) => {
                self.local_error = Some(format!("Picker: {error}"));
                self.answer_plugin_pick(pick, UiPickOutcome::Cancelled);
            }
        }
    }
}
