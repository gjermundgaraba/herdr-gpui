//! The client contract's traffic: this window's `client.report` to the local
//! daemon, and the jumps daemons forward to it.
//!
//! The Herdr fork's daemon computes what its clients must agree on, and each
//! client tells its local daemon what its Agents list shows so tools can read
//! it (`docs/next/website/src/content/docs/client-contract.md` in that fork):
//! every enabled host's agents, whatever the sidebar's device filter shows.
//! [`report`] decides what to say, [`jump`] where a jump leads and how it
//! ended, and [`Inbox`] holds what a connection sent and answered; this
//! module only says it, at most once per window tick and never from a render.
//! It mirrors `src/client/client_contract.rs` in the Herdr fork.
//!
//! # Which connection reports
//!
//! A report goes to exactly one daemon, the local one, and a window can hold
//! several connections to it: its own, and one per editor group that shows
//! another Herdr tab. Each is a client of its own to the daemon, so the
//! window reports on one of them: its own connection to the local host, the
//! first endpoint's. It then stays with the connection that carries its
//! report for as long as the window holds it, even while an editor group has
//! that connection parked. Using a parked group swaps connections, and moving
//! the report with every swap would leave the daemon two reports for one
//! window, the older one stale until its connection ended. A new connection,
//! which is also how a restarted daemon is met, is reported to afresh.
//!
//! # Not on its own lane
//!
//! The daemon takes a report or a jump's answer without the command lease.
//! This client still queues them with its other requests, because its
//! connection worker holds one request at a time and correlates every
//! answer; a report can therefore wait behind a slow command, and is then
//! the latest one, not a backlog.

mod follow;
mod inbox;
pub(crate) mod jump;
pub(crate) mod report;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub(crate) use inbox::Inbox;

use crate::{HerdrWindow, connection::ConnectionBridge};
use herdr_client::{
    ForkMethod, Method,
    protocol::{
        ClientShellSnapshot,
        clients::{ClientReport, ClientReportError},
    },
};
use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

/// The report the local daemon holds for this window.
struct SentReport {
    /// The mailbox of the connection it went out on. A bridge replaces its
    /// mailbox when it reconnects, so this names one connection for its life.
    connection: Weak<Mutex<crate::fork::Inbox>>,
    boot: String,
    fingerprint: Fingerprint,
    report: ClientReport,
}

/// What a report is built from, cheap to take on every tick: the window's
/// scalars hashed, and each host's snapshot by identity.
#[derive(Debug)]
pub(crate) struct Fingerprint {
    scalars: u64,
    /// Every accepted snapshot is a new allocation, and a held `Weak` keeps
    /// its allocation, so a newer snapshot never reuses the address of one
    /// held here.
    snapshots: Vec<Option<Weak<ClientShellSnapshot>>>,
}

/// Equal when nothing a report is built from changed: the same scalars and
/// the very same snapshots.
impl PartialEq for Fingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.scalars == other.scalars
            && self.snapshots.len() == other.snapshots.len()
            && self
                .snapshots
                .iter()
                .zip(&other.snapshots)
                .all(|pair| match pair {
                    (Some(left), Some(right)) => left.ptr_eq(right),
                    (left, right) => left.is_none() && right.is_none(),
                })
    }
}

/// A window's side of the client contract.
#[derive(Default)]
pub(crate) struct ClientContract {
    sent: Option<SentReport>,
    jumps: jump::Jumps,
}

impl HerdrWindow {
    /// One window tick of the contract: what the hosts answered to the
    /// requests the window follows, the jumps they forwarded, then the
    /// report, which so reflects where a jump led.
    pub(crate) fn tick_client_contract(&mut self, now: Instant, cx: &mut gpui::Context<Self>) {
        // Nothing on screen changes until the user leaves the pane, so the
        // daemon's acceptance of `agent.mark_unread` is said. A refusal is
        // the status bar's, like any other.
        if self.take_accepted_requests() {
            self.show_flash(crate::window::Flash::success("Marked unread"), cx);
        }
        self.settle_client_jumps(now, cx);
        self.sync_client_contract();
    }

    /// Every host as the contract sees it: the id and status its report
    /// lists, and the snapshot the window retains for it.
    fn contract_hosts(&self) -> Vec<report::Host<'_>> {
        self.endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| report::Host {
                id: &endpoint.id,
                label: &endpoint.label,
                status: endpoint.availability().into(),
                snapshot: self.host_live(index).snapshot.as_deref(),
            })
            .collect()
    }

    /// The report for what the window shows now.
    pub(crate) fn client_report(&self) -> Result<ClientReport, ClientReportError> {
        report::build(
            &self.contract_hosts(),
            self.selected_endpoint,
            self.active,
            self.reported_agents(),
        )
    }

    /// Changes whenever [`Self::client_report`] may return something else.
    /// No row is built and nothing is sorted, so the window can afford it on
    /// every tick.
    pub(crate) fn client_report_fingerprint(&self) -> Fingerprint {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.active.hash(&mut hash);
        self.selected_endpoint.hash(&mut hash);
        std::mem::discriminant(&self.agent_sort).hash(&mut hash);
        let mut snapshots = Vec::with_capacity(self.endpoints.len());
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            endpoint.id.hash(&mut hash);
            endpoint.label.hash(&mut hash);
            endpoint.availability().hash(&mut hash);
            let snapshot = self.host_live(index).snapshot.as_ref();
            snapshots.push(snapshot.map(Arc::downgrade));
        }
        Fingerprint {
            scalars: hash.finish(),
            snapshots,
        }
    }

    /// Every connection this window holds: each endpoint's own, each editor
    /// group's parked one, and those it let go of that still owe a jump its
    /// answer.
    pub(crate) fn connections(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.endpoints
            .iter()
            .map(|endpoint| &endpoint.connection)
            .chain(self.parked_connections())
            .chain(self.fork.client_contract.jumps.retiring())
    }

    /// The connection one of whose mailboxes is `mailbox`, while the window
    /// holds it. A bridge replaces its mailboxes when it reconnects, so a
    /// mailbox names one connection for its whole life, wherever the window
    /// keeps it meanwhile.
    pub(crate) fn connection_of(
        &self,
        mailbox: &Weak<Mutex<crate::fork::Inbox>>,
    ) -> Option<&ConnectionBridge> {
        self.connections()
            .find(|connection| connection.owns(mailbox))
    }

    /// The connection that carries this window's report: the one that already
    /// does, while the window holds it and it is alive, else the window's own
    /// connection to the local host.
    fn report_connection(&self) -> &ConnectionBridge {
        let alive = |connection: &&ConnectionBridge| {
            connection
                .handle
                .as_ref()
                .is_some_and(|handle| !handle.is_disconnected())
        };
        self.fork
            .client_contract
            .sent
            .as_ref()
            .and_then(|sent| self.connection_of(&sent.connection))
            .filter(alive)
            .unwrap_or(&self.endpoints[0].connection)
    }

    /// Sends the report to the local daemon when it differs from the one
    /// that daemon holds for this window. Without a local connection, or
    /// with a local daemon that does not take reports, nothing is sent.
    pub(crate) fn sync_client_contract(&mut self) {
        let connection = self.report_connection();
        let Some(handle) = connection.handle.clone() else {
            return;
        };
        let fork = connection.fork.clone();
        // Never wait on the event reader: a busy mailbox is asked next tick.
        let Ok(mut fork_inbox) = fork.try_lock() else {
            return;
        };
        let mailbox = &mut fork_inbox.clients;
        // The request names the daemon's boot, which its first snapshot brings.
        let Some(boot) = mailbox.report_boot().map(str::to_owned) else {
            return;
        };
        let fingerprint = self.client_report_fingerprint();
        let held = self
            .fork
            .client_contract
            .sent
            .as_ref()
            .filter(|sent| connection.owns(&sent.connection) && sent.boot == boot);
        if held.is_some_and(|sent| sent.fingerprint == fingerprint) {
            return;
        }
        // The daemon answers one report before it is sent the next, so every
        // change meanwhile becomes one report.
        if mailbox.report_pending() {
            return;
        }
        let report = match self.client_report() {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(category = "client_report", %error, "report not built");
                return;
            }
        };
        if held.is_none_or(|sent| sent.report != report) {
            // A report too long for one daemon command is trimmed, and one
            // that cannot be is not sent, nor retried until it changes.
            let method = Method::Fork(ForkMethod::ClientReport);
            let queued = report::one_command(report.clone())
                .map(|params| handle.request(&boot, method, params));
            match queued {
                Ok(Ok(request)) => mailbox.report_sent(request),
                // A full queue drains; the report is still owed.
                Ok(Err(herdr_client::Error::Full)) => return,
                // Anything else is not retried: a failed connection is
                // replaced by a new one, which is reported to afresh.
                Ok(Err(error)) => {
                    tracing::warn!(category = "client_report", %error, "report not queued");
                }
                Err(error) => {
                    tracing::warn!(category = "client_report", %error, "report not sent");
                }
            }
        }
        self.fork.client_contract.sent = Some(SentReport {
            connection: Arc::downgrade(&fork),
            boot,
            fingerprint,
            report,
        });
    }
}
