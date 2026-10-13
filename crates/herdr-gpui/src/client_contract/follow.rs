//! Mark Agent Unread, and the requests the window follows to their answers.
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
use herdr_client::{ForkMethod, Method, SendError, protocol::ClientShellSnapshot};
use serde_json::{Value, json};

/// `agent.mark_unread` for the focused pane: the daemon holds that agent's
/// acknowledged completion, so its badge comes back, for every client, once
/// this one leaves the pane. `None` without a focused pane.
///
/// Whether the pane has an agent, and whether that agent is idle, is the
/// daemon's to decide: it answers `agent_not_found` or `agent_not_idle`.
pub(super) fn mark_unread(snapshot: &ClientShellSnapshot) -> Option<(Method, Value)> {
    let pane_id = snapshot.focused_pane_id.as_deref()?;
    Some((
        Method::Fork(ForkMethod::AgentMarkUnread),
        json!({"pane_id": pane_id}),
    ))
}

impl HerdrWindow {
    /// Asks the daemon to hold the focused agent's acknowledged completion
    /// (`agent.mark_unread`), on the connection of the host that owns the
    /// pane: the selected one, which is the client presenting it. The daemon
    /// keeps the hold, so no record of it lives here. The badge returns with a
    /// later snapshot's `agent_status`, and a refusal such as `agent_not_idle`
    /// comes back as the request's error, shown like any other daemon error.
    ///
    /// Counterpart of `mark_focused_pane_unread` in the Herdr fork's
    /// `src/client/shell/endpoints.rs`.
    pub(crate) fn mark_focused_agent_unread(&mut self) {
        let Some(snapshot) = self.live.snapshot.clone() else {
            self.local_error =
                Some("Mark Agent Unread: the host has not shown its panes yet.".into());
            return;
        };
        let Some((method, params)) = mark_unread(&snapshot) else {
            self.local_error = Some("Mark Agent Unread: no pane is focused.".into());
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let Some(handle) = connection.handle.as_ref() else {
            self.local_error = Some(format!("Mark Agent Unread: {}", SendError::Disconnected));
            return;
        };
        // Followed to the daemon's answer, which is all that confirms it. The
        // mailbox stays locked across the queueing, so the answer cannot reach
        // it before it knows to keep it; the reader holds this lock only to
        // file one event. A poisoned mailbox belongs to a connection that is
        // over: the request still goes, unfollowed. Only a failure is
        // written: a queued request leaves any other error shown.
        let mut fork = connection.fork.lock().ok();
        match (handle.request(&snapshot.boot_id, method, params), &mut fork) {
            (Ok(id), Some(fork)) => fork.clients.follow(id),
            (Ok(_), None) => {}
            (Err(error), _) => self.local_error = Some(format!("Mark Agent Unread: {error}")),
        }
    }

    /// Whether the daemon accepted a followed request on any connection the
    /// window holds since the last call. Every mailbox is asked, not only
    /// up to the first acceptance, so none is said twice. A busy mailbox is
    /// not waited on: it keeps its acceptance for the next tick. A connection
    /// the window let go of took its mailbox, and its answers, with it.
    pub(super) fn take_accepted_requests(&self) -> bool {
        self.connections().fold(false, |accepted, connection| {
            let taken = connection
                .fork
                .try_lock()
                .is_ok_and(|mut fork| fork.clients.take_accepted());
            accepted | taken
        })
    }
}
