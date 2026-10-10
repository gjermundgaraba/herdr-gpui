//! Requests the window follows to their answers.
//!
//! Most requests are sent and forgotten: a refusal reaches the status bar
//! through the live state and nothing else depends on it. A jump has to tell
//! its tool how the focus request ended, so that request is followed: its id
//! is registered in the connection's contract mailbox under the lock it was
//! queued under, and the window collects the answer on a later tick.
//!
//! A navigation may be dispatched long after it was asked for, once the host
//! it leads to is presented. Its purpose therefore waits beside the pending
//! navigation and is claimed by the dispatch that sends the focus request.

use super::{Answer, Inbox, Refusal};
use crate::{HerdrWindow, navigation::NavigationTarget, navigation::OwnedNavigationTarget};
use gpui::Context;
use herdr_client::{ClientHandle, SendError};
use std::sync::{Arc, Mutex, Weak};

/// Why the window follows a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The navigation of the jump with this key.
    Jump(u64),
}

impl Purpose {
    /// Whether the answer is kept from the live state. A tool asked for a
    /// jump, not the user, so its refusal is the tool's to hear.
    fn quiet(self) -> bool {
        match self {
            Self::Jump(_) => true,
        }
    }
}

struct Request {
    /// The mailbox of the connection it went out on, which names it.
    connection: Weak<Mutex<Inbox>>,
    id: String,
    purpose: Purpose,
    /// The focus the request asked for, which input is fenced behind.
    focus: OwnedNavigationTarget,
}

#[derive(Default)]
pub(crate) struct Follows {
    /// The purpose of a navigation the window accepted and has not dispatched.
    tagged: Option<(OwnedNavigationTarget, Purpose)>,
    requests: Vec<Request>,
    /// Answers known without a mailbox: navigations that sent nothing.
    ready: Vec<(Purpose, Answer)>,
}

impl Follows {
    /// The purpose of the navigation to `target` being dispatched now. Any
    /// dispatch claims the tag, so one whose navigation was replaced does not
    /// outlive it.
    pub(crate) fn claim(&mut self, target: &NavigationTarget<&str>) -> Option<Purpose> {
        let (tagged, purpose) = self.tagged.take()?;
        (tagged.as_deref() == *target).then_some(purpose)
    }

    /// Records that the navigation for `purpose` sent nothing.
    pub(crate) fn unsent(&mut self, purpose: Purpose) {
        self.ready.push((purpose, Err(Refusal::Unsent)));
    }
}

impl HerdrWindow {
    /// The window's cross-host navigation, with the focus request it leads
    /// to followed for `purpose`. Returns whether the window took it.
    pub(crate) fn navigate_endpoint_for(
        &mut self,
        endpoint: &str,
        target: NavigationTarget<&str>,
        purpose: Purpose,
        cx: &mut Context<Self>,
    ) -> bool {
        self.client_contract.follows.tagged = Some(((&target).into(), purpose));
        let navigating = self.navigate_endpoint(endpoint, target, cx);
        if !navigating {
            self.client_contract.follows.tagged = None;
        }
        navigating
    }

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
        let mut request = None;
        let queued =
            self.request_focus_change("Navigate", Some((&target).into()), |handle, boot| {
                let id = enqueue(handle, boot)?;
                mailbox.follow(id.clone(), purpose.quiet());
                request = Some(id.clone());
                Ok(id)
            });
        drop(mailbox);
        match request {
            Some(id) => self.client_contract.follows.requests.push(Request {
                connection: Arc::downgrade(&clients),
                id,
                purpose,
                focus: (&target).into(),
            }),
            None => self.client_contract.follows.unsent(purpose),
        }
        queued
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
            let Some(connection) = self.connections().find(|connection| {
                std::ptr::eq(
                    Arc::as_ptr(&connection.clients),
                    request.connection.as_ptr(),
                )
            }) else {
                continue;
            };
            let Ok(mut mailbox) = connection.clients.try_lock() else {
                waiting.push(request);
                continue;
            };
            match mailbox.take_answer(&request.id) {
                Some(answer) => {
                    if answer.is_err() {
                        refused.push((request.connection, request.focus));
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
        let Some(index) = self.endpoints.iter().position(|endpoint| {
            std::ptr::eq(
                Arc::as_ptr(&endpoint.connection.clients),
                connection.as_ptr(),
            )
        }) else {
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
