//! The commands the gjermundgaraba/herdr fork adds, which the window runs
//! itself (`HerdrWindow::run_fork_command`). Their `[keys]` names are the
//! Herdr fork's, and each is unbound by default, as there.

use herdr_client::{ForkMethod, Method, protocol::ClientShellSnapshot};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum ForkCommand {
    /// Asks the daemon to hold the focused agent's completion as unread.
    MarkUnread,
    /// The agent picker over every enabled host.
    AgentPicker,
    /// The workspace list over every enabled host.
    WorkspaceList,
}

/// `agent.mark_unread` for the focused pane: the daemon holds that agent's
/// acknowledged completion, so its badge comes back, for every client, once
/// this one leaves the pane. `None` without a focused pane.
///
/// Whether the pane has an agent, and whether that agent is idle, is the
/// daemon's to decide: it answers `agent_not_found` or `agent_not_idle`.
pub fn mark_unread(snapshot: &ClientShellSnapshot) -> Option<(Method, Value)> {
    let pane_id = snapshot.focused_pane_id.as_deref()?;
    Some((
        Method::Fork(ForkMethod::AgentMarkUnread),
        json!({"pane_id": pane_id}),
    ))
}
