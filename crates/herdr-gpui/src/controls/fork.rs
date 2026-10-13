//! The commands the gjermundgaraba/herdr fork adds, which the window runs
//! itself (`HerdrWindow::run_fork_command`). Their `[keys]` names are the
//! Herdr fork's, and each is unbound by default, as there.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum ForkCommand {
    /// Asks the daemon to hold the focused agent's completion as unread.
    MarkUnread,
    /// The agent picker over every enabled host.
    AgentPicker,
    /// The workspace list over every enabled host.
    WorkspaceList,
}
