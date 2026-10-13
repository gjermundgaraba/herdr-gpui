//! The fork's one slot in each upstream type that carries its traffic and
//! state, so upstream files hold a field and a call instead of the fork's
//! features:
//!
//! - [`Inbox`]: a connection's mailbox for what only the fork reads, the
//!   plugin picker's controls and answers and the client contract's
//! - [`ForkState`]: what a window keeps for them
//! - [`HerdrWindow::tick_fork`]: their part of a window tick
//! - [`HerdrWindow::run_fork_command`]: the commands the fork adds
//!
//! The daemon's named controls reach the mailbox as `ClientEvent::Control`,
//! still unread: each feature's mailbox decodes the kinds it owns.

use crate::{
    HerdrWindow, client_contract, connection::ConnectionBridge, controls::ForkCommand, picker,
};
use gpui::{Context, Window};
use herdr_client::ClientEvent;
use std::{
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

/// One connection's fork mailbox, filled by its event reader after the
/// upstream mailboxes and read by the window. Replaced with the connection,
/// so it also names the connection that answers what arrived in it.
#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The `ui.pick` picker the daemon asked this connection to show.
    pub(crate) picks: picker::plugin::Inbox,
    /// What the client contract needs of this connection: whether its daemon
    /// takes reports, and the answers to the requests the contract sent.
    pub(crate) clients: client_contract::Inbox,
}

impl Inbox {
    /// Keeps the events the fork waits for and passes every other one on.
    pub(crate) fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        self.picks
            .apply(event)
            .and_then(|event| self.clients.apply(event))
    }
}

/// What a window keeps for the fork.
#[derive(Default)]
pub(crate) struct ForkState {
    /// The picker on the menu's picker page, and whoever waits on its answer.
    pub(crate) picker: Option<picker::Picker>,
    /// What this window last told its local daemon about itself.
    pub(crate) client_contract: client_contract::ClientContract,
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
        // A picker is answered on the connection that asked for it.
        self.poll_picker(window, cx);
        // The report goes out on one connection, and says what every host
        // shows after this tick's updates.
        self.tick_client_contract(Instant::now(), cx);
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

/// A connection's mailboxes as its event reader passes an event through
/// them, for tests that deliver events without a peer. The upstream
/// mailboxes before the fork's keep none of the events these tests deliver.
#[cfg(test)]
pub(crate) struct Mailboxes {
    inbox: Arc<Mutex<crate::state::LiveState>>,
    fork: Arc<Mutex<Inbox>>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
impl Mailboxes {
    pub(crate) fn deliver(&self, event: ClientEvent) {
        let event = self.fork.lock().unwrap().apply(event);
        let mut state = self.inbox.lock().unwrap();
        match event {
            Some(event) => state.apply(event),
            None => state.dirty = true,
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

#[cfg(test)]
impl ConnectionBridge {
    /// The mailboxes of the connection this bridge holds now.
    pub(crate) fn mailboxes(&self) -> Mailboxes {
        Mailboxes {
            inbox: self.inbox.clone(),
            fork: self.fork.clone(),
        }
    }
}
