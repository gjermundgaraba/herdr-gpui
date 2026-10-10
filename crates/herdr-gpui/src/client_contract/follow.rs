//! Requests the window follows to their answers.
//!
//! Most requests are sent and forgotten: a refusal reaches the status bar
//! through the live state and nothing else depends on it. A jump has to tell
//! its tool how the focus request ended, and a Back or Forward step has to
//! take its cursor back when the pane would not take focus, and an accepted
//! `agent.mark_unread` is worth a word because nothing on screen changes
//! until the user leaves the pane. So those requests are followed: the id is
//! registered in the connection's contract mailbox under the lock it was
//! queued under, and the window collects the answer on a later tick.
//!
//! A navigation may be dispatched long after it was asked for, once the host
//! it leads to is presented. Its purpose waits inside the pending navigation
//! ([`PendingNavigation`]), so it goes wherever that navigation goes: to the
//! dispatch that sends its focus request, or away with it.
//!
//! The Herdr fork's terminal client follows the same requests where it
//! finishes an endpoint request, in `src/client/shell/actions.rs`, and sends
//! `agent.mark_unread` in `src/client/shell/endpoints.rs`.

use super::{Answer, Inbox, Refusal};
use crate::{HerdrWindow, navigation::NavigationTarget, navigation::OwnedNavigationTarget};
use herdr_client::{ClientHandle, SendError};
use std::sync::{Arc, Mutex, Weak};

/// Why the window follows a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The navigation of the jump with this key.
    Jump(u64),
    /// This Back or Forward step.
    History(crate::history::Step),
    /// `agent.mark_unread`, which the daemon accepts or refuses.
    MarkUnread,
}

impl Purpose {
    /// Whether the answer is kept from the live state. A tool asked for a
    /// jump, not the user, so its refusal is the tool's to hear; the user's
    /// own step reports its failure like any navigation.
    fn quiet(&self) -> bool {
        match self {
            Self::Jump(_) => true,
            Self::History(_) | Self::MarkUnread => false,
        }
    }
}

/// A navigation the window accepted and sends once its host is ready for
/// it. Whatever replaces or discards it takes its purpose too, so only its
/// own dispatch follows a request for that purpose. A navigation to a pane
/// of the same id cannot, unless it is this one asked for again on the host
/// still presented ([`HerdrWindow::navigate_endpoint_for`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingNavigation {
    pub(crate) target: OwnedNavigationTarget,
    /// Why its focus request is followed; none for a navigation nobody
    /// follows.
    pub(crate) purpose: Option<Purpose>,
}

impl From<OwnedNavigationTarget> for PendingNavigation {
    fn from(target: OwnedNavigationTarget) -> Self {
        Self {
            target,
            purpose: None,
        }
    }
}

struct Request {
    /// The mailbox of the connection it went out on, which names it.
    connection: Weak<Mutex<Inbox>>,
    id: String,
    purpose: Purpose,
    /// The focus the request asked for, which input is fenced behind; none
    /// for a request that moves no focus.
    focus: Option<OwnedNavigationTarget>,
}

#[derive(Default)]
pub(crate) struct Follows {
    requests: Vec<Request>,
    /// Answers known without a mailbox: navigations that sent nothing.
    ready: Vec<(Purpose, Answer)>,
}

impl Follows {
    /// Whether the focus request of the Back or Forward step `step` still
    /// waits for its answer, or has one the window has not collected: until
    /// then the step is not over, and a refusal still takes it back.
    pub(crate) fn follows_step(&self, step: &crate::history::Step) -> bool {
        self.requests
            .iter()
            .map(|request| &request.purpose)
            .chain(self.ready.iter().map(|(purpose, _)| purpose))
            .any(|purpose| matches!(purpose, Purpose::History(followed) if followed == step))
    }

    /// Records that the navigation for `purpose` sent nothing.
    fn unsent(&mut self, purpose: Purpose) {
        self.ready.push((purpose, Err(Refusal::Unsent)));
    }
}

impl HerdrWindow {
    /// Queues a focus request on the selected host and follows its answer.
    pub(crate) fn follow_focus_change(
        &mut self,
        purpose: Purpose,
        target: NavigationTarget<&str>,
        enqueue: impl FnOnce(&ClientHandle, &str) -> Result<String, SendError>,
    ) -> bool {
        let clients = self.endpoints[self.selected_endpoint]
            .connection
            .clients
            .clone();
        // Held across the queueing, so the answer cannot reach the mailbox
        // before it knows to keep it. The reader holds this lock only to
        // file one event, as it does the inbox the fence below waits for.
        let Ok(mut mailbox) = clients.lock() else {
            self.client_contract.follows.unsent(purpose);
            return false;
        };
        let quiet = purpose.quiet();
        let mut request = None;
        let queued =
            self.request_focus_change("Navigate", Some((&target).into()), |handle, boot| {
                let id = enqueue(handle, boot)?;
                mailbox.follow(id.clone(), quiet);
                request = Some(id.clone());
                Ok(id)
            });
        drop(mailbox);
        match request {
            Some(id) => {
                // Only what the host shows from here on can be the step's
                // pane and not one that merely equals it.
                if let Purpose::History(step) = &purpose {
                    self.history_step_sent(step);
                }
                self.client_contract.follows.requests.push(Request {
                    connection: Arc::downgrade(&clients),
                    id,
                    purpose,
                    focus: Some((&target).into()),
                });
            }
            None => self.client_contract.follows.unsent(purpose),
        }
        queued
    }

    /// Queues a request that moves no focus on the selected host and follows
    /// its answer. Nothing is followed when it could not be queued; the
    /// caller reports that.
    pub(crate) fn follow_request(
        &mut self,
        purpose: Purpose,
        enqueue: impl FnOnce(&ClientHandle) -> Result<String, SendError>,
    ) -> Result<(), SendError> {
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let handle = connection.handle.as_ref().ok_or(SendError::Disconnected)?;
        let clients = connection.clients.clone();
        // Held across the queueing, as for a focus request.
        let mut mailbox = clients.lock().map_err(|_| SendError::Disconnected)?;
        let id = enqueue(handle)?;
        mailbox.follow(id.clone(), purpose.quiet());
        drop(mailbox);
        self.client_contract.follows.requests.push(Request {
            connection: Arc::downgrade(&clients),
            id,
            purpose,
            focus: None,
        });
        Ok(())
    }

    /// The answers that arrived since the last call. A request whose
    /// connection the window no longer holds, or whose mailbox was reset with
    /// its connection, is forgotten without one. No mailbox is waited on: a
    /// busy one is asked next tick.
    pub(super) fn take_followed_answers(&mut self) -> Vec<(Purpose, Answer)> {
        let follows = &mut self.client_contract.follows;
        let mut answers = std::mem::take(&mut follows.ready);
        let mut refused = Vec::new();
        let mut waiting = Vec::new();
        for request in std::mem::take(&mut follows.requests) {
            let Some(connection) = self.connection_of(&request.connection) else {
                continue;
            };
            let Ok(mut mailbox) = connection.clients.try_lock() else {
                waiting.push(request);
                continue;
            };
            match mailbox.take_answer(&request.id) {
                Some(answer) => {
                    if answer.is_err()
                        && let Some(focus) = request.focus
                    {
                        refused.push((request.connection, focus));
                    }
                    answers.push((request.purpose, answer));
                }
                None if mailbox.follows(&request.id) => waiting.push(request),
                None => {}
            }
        }
        self.client_contract.follows.requests = waiting;
        for (connection, focus) in refused {
            self.release_focus_fence(&connection, &focus);
        }
        answers
    }

    /// Stops input waiting for a focus its host refused. The fence would
    /// otherwise hold until its deadline and then replace the connection, as
    /// it does for a navigation nobody answered.
    fn release_focus_fence(
        &mut self,
        connection: &Weak<Mutex<Inbox>>,
        focus: &OwnedNavigationTarget,
    ) {
        let Some(index) = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.connection.owns(connection))
        else {
            return;
        };
        let release = |state: &mut crate::LiveState| {
            if let Some(activation) = &mut state.activation
                && activation.focus.as_ref() == Some(focus)
            {
                activation.focus = None;
                state.dirty = true;
            }
        };
        if let Ok(mut state) = self.endpoints[index].connection.inbox.lock() {
            release(&mut state);
        }
        if index == self.selected_endpoint {
            release(&mut self.live);
        }
    }
}
