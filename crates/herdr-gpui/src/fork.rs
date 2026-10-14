//! The fork's one slot in each upstream type that carries its traffic and
//! state, so upstream files hold a field and a call instead of the fork's
//! features:
//!
//! - [`Inbox`]: a connection's mailbox for what only the fork reads, the
//!   plugin picker's controls and answers and Mark Agent Unread's answers
//! - [`ForkState`]: what a window keeps for them
//! - [`HerdrWindow::tick_fork`]: their part of a window tick
//! - [`HerdrWindow::run_fork_command`]: the commands the fork adds
//!
//! The daemon's named controls reach the mailbox as `ClientEvent::Control`,
//! still unread: each feature's mailbox decodes the kinds it owns.

use crate::{
    HerdrWindow, connection::ConnectionBridge, controls::ForkCommand, mark_unread, picker,
};
use gpui::{Context, Window};
use herdr_client::ClientEvent;
use std::sync::{Arc, Mutex, Weak};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

/// One connection's fork mailbox, filled by its event reader after the
/// upstream mailboxes and read by the window. Replaced with the connection,
/// so it also names the connection that answers what arrived in it.
///
/// The requests sent from here that wait for the daemon's reply are kept by
/// id, each until its reply or its rejection is filed, or the connection
/// goes. They can be many, as each waits behind the connection's one request
/// in flight, but the connection bounds them: its command queue takes 64
/// requests, its worker holds one more back and one in flight, its event
/// queue holds 8 replies not filed yet, and it ends when the one in flight is
/// not answered in time.
#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The `ui.pick` picker the daemon asked this connection to show.
    pub(crate) picks: picker::plugin::Inbox,
    /// The `agent.mark_unread` requests the window follows to their answer.
    pub(crate) unread: mark_unread::Inbox,
}

impl Inbox {
    /// Keeps the events the fork waits for and passes every other one on.
    pub(crate) fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        let event = self.picks.apply(event)?;
        self.unread.observe(&event);
        Some(event)
    }
}

/// What a window keeps for the fork.
#[derive(Default)]
pub(crate) struct ForkState {
    /// The picker on the menu's picker page, and whoever waits on its answer.
    pub(crate) picker: Option<picker::Picker>,
}

impl ConnectionBridge {
    /// Whether `mailbox` is this connection's fork mailbox, and so this the
    /// connection that answers what arrived in it.
    pub(crate) fn owns(&self, mailbox: &Weak<Mutex<Inbox>>) -> bool {
        std::ptr::eq(Arc::as_ptr(&self.fork), mailbox.as_ptr())
    }
}

impl HerdrWindow {
    /// The fork's part of a window tick, after every connection was polled
    /// and settled into its slot.
    pub(crate) fn tick_fork(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.poll_picker(window, cx);
        self.tick_mark_unread(cx);
    }

    /// Every connection this window holds: each endpoint's own, and each
    /// editor group's parked one. Each is a client of its own to its daemon,
    /// so anything a daemon addresses to one client may arrive on any of them.
    pub(crate) fn connections(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.endpoints
            .iter()
            .map(|endpoint| &endpoint.connection)
            .chain(self.parked_connections())
    }

    /// The connection whose fork mailbox is `mailbox`, while the window holds
    /// it. A bridge replaces its mailboxes when it reconnects, so a mailbox
    /// names one connection for its whole life, wherever the window keeps it
    /// meanwhile.
    pub(crate) fn connection_of(&self, mailbox: &Weak<Mutex<Inbox>>) -> Option<&ConnectionBridge> {
        self.connections()
            .find(|connection| connection.owns(mailbox))
    }

    /// Runs one of the commands the fork adds.
    pub(crate) fn run_fork_command(
        &mut self,
        command: ForkCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            ForkCommand::MarkUnread => {
                self.mark_focused_agent_unread();
                cx.notify();
            }
            ForkCommand::AgentPicker => {
                self.open_navigator(picker::navigator::Layout::Agents, window, cx);
            }
            ForkCommand::WorkspaceList => {
                self.open_navigator(picker::navigator::Layout::Workspaces, window, cx);
            }
        }
    }
}

/// The named control `kind` carrying `payload`, as a connection forwards it.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) fn control(kind: &str, payload: &impl serde::Serialize) -> ClientEvent {
    ClientEvent::Control {
        kind: kind.into(),
        data: serde_json::to_string(payload).unwrap(),
    }
}
