//! A plugin's `ui.pick`, shown in the window's picker.
//!
//! The daemon sends `endpoint.pick.open.v1` to the one client that should
//! show the picker and holds the plugin's request open until that client
//! answers with `ui.pick.resolve`. So the answer goes back on the connection
//! the request arrived on, whichever of the window's connections that is, and
//! every way the picker can go away answers `cancelled` unless the daemon has
//! already given up on it: an `endpoint.pick.close.v1`, a lost connection, or
//! a new daemon boot.

use super::{
    Source,
    list::{Choice, Item, List},
};
use crate::{HerdrWindow, connection::ConnectionBridge};
use gpui::{Context, Window};
use herdr_client::{
    ClientEvent, Method,
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
            event => Some(event),
        }
    }

    fn forget(&mut self) {
        self.open = None;
        if let Some(shown) = self.shown.take() {
            self.closed = Some(shown);
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
    /// Every connection this window holds: each endpoint's own, and each
    /// editor group's parked one.
    pub(crate) fn connections(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.endpoints
            .iter()
            .map(|endpoint| &endpoint.connection)
            .chain(self.parked_connections())
    }

    /// Shows the pickers the daemon asked for and dismisses the ones it ended.
    /// Each connection's mailbox is asked without waiting on its reader; one
    /// that is busy keeps what it has for the next poll.
    pub(crate) fn poll_plugin_picks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let updates: Vec<_> = self
            .connections()
            .filter_map(|connection| {
                let update = connection.picks.try_lock().ok()?.take();
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
        self.connections().find(|connection| {
            std::ptr::eq(Arc::as_ptr(&connection.picks), pick.connection.as_ptr())
        })
    }

    fn plugin_pick(&self) -> Option<&PluginPick> {
        self.picker.as_ref().map(|picker| match &picker.source {
            Source::Plugin(pick) => pick,
        })
    }

    /// Opens the window's picker for `request`. A window already showing a
    /// menu, a dialog, or another picker keeps it: an unasked-for list must
    /// not take over what the user is typing into, so the request is answered
    /// `cancelled` at once instead of being left to hang.
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
        if self.menu.page.is_some() {
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

    /// Queues `ui.pick.resolve` on the connection that asked.
    fn queue_pick_answer(&self, pick: &PluginPick, outcome: UiPickOutcome) -> crate::Result<()> {
        let handle = self
            .pick_connection(pick)
            .and_then(|connection| connection.handle.as_ref())
            .ok_or(crate::Error::NotConnected)?;
        let params = serde_json::to_value(UiPickResolveParams {
            pick_id: pick.pick_id.clone(),
            outcome,
        })?;
        handle.request(&pick.boot, Method::UiPickResolve, params)?;
        Ok(())
    }

    /// Answers a plugin's picker. A connection that is gone has nothing to
    /// answer: the daemon cancelled its pickers when the client left. Any
    /// other answer that cannot be queued, such as created text too long for
    /// a frame, is reported and replaced by `cancelled`, so the plugin is not
    /// left waiting on a picker that has closed.
    pub(super) fn answer_plugin_pick(&mut self, pick: PluginPick, outcome: UiPickOutcome) {
        let cancelled = outcome == UiPickOutcome::Cancelled;
        match self.queue_pick_answer(&pick, outcome) {
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
            Err(error) => {
                if !cancelled
                    && let Err(error) = self.queue_pick_answer(&pick, UiPickOutcome::Cancelled)
                {
                    tracing::warn!(category = "pick_resolve", %error, "picker not cancelled");
                }
                self.local_error = Some(format!("Picker: {error}"));
            }
        }
    }
}
