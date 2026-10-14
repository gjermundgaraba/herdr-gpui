//! The client-lane methods of the gjermundgaraba/herdr fork's contract: the
//! answer to a picker, and the unread hold. Stock Herdr advertises none of
//! them, so each is refused before it is sent there, as any method a daemon
//! does not advertise is.

/// A fork method. `as_str` is its wire spelling, as the fork's fixtures
/// spell it, and the only place it is written out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForkMethod {
    AgentMarkUnread,
    UiPickResolve,
}

impl ForkMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AgentMarkUnread => "agent.mark_unread",
            Self::UiPickResolve => "ui.pick.resolve",
        }
    }
}
