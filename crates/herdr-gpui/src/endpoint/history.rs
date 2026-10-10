//! Which way a Back or Forward step goes.
//!
//! Upstream keeps a trail of focused panes per endpoint here. This fork
//! steps through one history of pane visits across hosts instead
//! (`crate::history`), as the Herdr fork's terminal client does, so the title
//! bar's buttons, the `back` and `forward` commands, and the daemon's
//! `history_back` and `history_forward` keys all walk the same visits.

/// Which way along the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Back,
    Forward,
}
