//! The parked connections, for the fork's mailboxes: a daemon may address a
//! picker to any of a window's clients, and Mark Agent Unread's answer comes
//! back on the connection it went out on.

use crate::{HerdrWindow, connection::ConnectionBridge};

impl HerdrWindow {
    /// The parked connections.
    pub(crate) fn parked_connections(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.browser
            .terminals
            .parked
            .iter()
            .map(|parked| &parked.connection)
    }
}
