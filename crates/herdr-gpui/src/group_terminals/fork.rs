//! Parked connections and the client contract.
//!
//! A daemon may have forwarded a tool's jump to a parked connection, and the
//! navigation to that jump's pane is often what leaves the workspace. So the
//! window retires the parked connections it no longer needs instead of
//! dropping them: one that still owes a jump its answer is kept by the client
//! contract until it has answered (`HerdrWindow::retire_connection`).

use super::Parked;
use crate::{HerdrWindow, connection::ConnectionBridge};
use std::time::Instant;

impl HerdrWindow {
    /// The parked connections, each a client of its own: anything the daemon
    /// addresses to one client may arrive on any of them.
    pub(crate) fn parked_connections(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.browser
            .terminals
            .parked
            .iter()
            .map(|parked| &parked.connection)
    }

    /// Keeps the parked connections `keep` accepts, and retires the rest.
    pub(super) fn retain_parked(&mut self, now: Instant, mut keep: impl FnMut(&Parked) -> bool) {
        let (kept, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.browser.terminals.parked)
            .into_iter()
            .partition(|parked| keep(parked));
        self.browser.terminals.parked = kept;
        for parked in left {
            self.retire_connection(parked.connection, now);
        }
    }
}
