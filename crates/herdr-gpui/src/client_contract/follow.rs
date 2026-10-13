//! Requests the window follows to their answers.
//!
//! Most requests are sent and forgotten: a refusal reaches the status bar
//! through the live state and nothing else depends on it. An accepted
//! `agent.mark_unread` is worth a word, because nothing on screen changes
//! until the user leaves the pane, so that request is followed: the id is
//! registered in the connection's contract mailbox under the lock it was
//! queued under, the mailbox notes when the daemon accepts it, and the window
//! collects that on a later tick.
//!
//! The Herdr fork's terminal client sends `agent.mark_unread` in
//! `src/client/shell/endpoints.rs`.

use crate::HerdrWindow;
use herdr_client::{ClientHandle, SendError};

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
        // Held across the queueing, so the answer cannot reach the mailbox
        // before it knows to keep it. The reader holds this lock only to
        // file one event.
        let mut mailbox = connection
            .clients
            .lock()
            .map_err(|_| SendError::Disconnected)?;
        let id = enqueue(handle)?;
        mailbox.follow(id);
        Ok(())
    }

    /// Whether the daemon accepted a followed request on any connection the
    /// window holds since the last call. Every mailbox is asked, not only
    /// up to the first acceptance, so none is said twice. A busy mailbox is
    /// not waited on: it keeps its acceptance for the next tick. A connection
    /// the window let go of took its mailbox, and its answers, with it.
    pub(super) fn take_accepted_requests(&self) -> bool {
        self.connections().fold(false, |accepted, connection| {
            let taken = connection
                .clients
                .try_lock()
                .is_ok_and(|mut mailbox| mailbox.take_accepted());
            accepted | taken
        })
    }
}
