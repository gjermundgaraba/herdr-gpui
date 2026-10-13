//! Requests the window follows to their answers.
//!
//! Most requests are sent and forgotten: a refusal reaches the status bar
//! through the live state and nothing else depends on it. An accepted
//! `agent.mark_unread` is worth a word, because nothing on screen changes
//! until the user leaves the pane, so that request is followed: the id is
//! registered in the connection's contract mailbox under the lock it was
//! queued under, and the window collects the answer on a later tick.
//!
//! The Herdr fork's terminal client sends `agent.mark_unread` in
//! `src/client/shell/endpoints.rs`.

use super::Inbox;
use crate::HerdrWindow;
use herdr_client::{ClientHandle, SendError};
use std::sync::{Arc, Mutex, Weak};

struct Request {
    /// The mailbox of the connection it went out on, which names it.
    connection: Weak<Mutex<Inbox>>,
    id: String,
}

#[derive(Default)]
pub(crate) struct Follows {
    requests: Vec<Request>,
}

impl HerdrWindow {
    /// Queues a request on the selected host and follows its answer.
    /// Nothing is followed when it could not be queued; the caller reports
    /// that.
    pub(crate) fn follow_request(
        &mut self,
        enqueue: impl FnOnce(&ClientHandle) -> Result<String, SendError>,
    ) -> Result<(), SendError> {
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let handle = connection.handle.as_ref().ok_or(SendError::Disconnected)?;
        let clients = connection.clients.clone();
        // Held across the queueing, so the answer cannot reach the mailbox
        // before it knows to keep it. The reader holds this lock only to
        // file one event.
        let mut mailbox = clients.lock().map_err(|_| SendError::Disconnected)?;
        let id = enqueue(handle)?;
        mailbox.follow(id.clone());
        drop(mailbox);
        self.client_contract.follows.requests.push(Request {
            connection: Arc::downgrade(&clients),
            id,
        });
        Ok(())
    }

    /// Whether the daemon accepted a followed request since the last call.
    /// A request whose connection the window no longer holds, or whose
    /// mailbox was reset with its connection, is forgotten without an
    /// answer. No mailbox is waited on: a busy one is asked next tick.
    pub(super) fn take_accepted_requests(&mut self) -> bool {
        let mut accepted = false;
        let mut waiting = Vec::new();
        for request in std::mem::take(&mut self.client_contract.follows.requests) {
            let Some(connection) = self.connection_of(&request.connection) else {
                continue;
            };
            let Ok(mut mailbox) = connection.clients.try_lock() else {
                waiting.push(request);
                continue;
            };
            match mailbox.take_answer(&request.id) {
                Some(answer) => accepted |= answer,
                None if mailbox.follows(&request.id) => waiting.push(request),
                None => {}
            }
        }
        self.client_contract.follows.requests = waiting;
        accepted
    }
}
