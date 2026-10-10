//! The client contract's traffic: this window's `client.report` to the local
//! daemon, and the jumps daemons forward to it.
//!
//! The Herdr fork's daemon computes what its clients must agree on, and each
//! client tells its local daemon what its Agents list shows so tools can read
//! it (`docs/next/website/src/content/docs/client-contract.md` in that fork).
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

pub(crate) use follow::Purpose;
pub(crate) use inbox::{Answer, Inbox, Refusal};

use crate::{HerdrWindow, connection::ConnectionBridge};
use herdr_client::{
    Method,
    protocol::clients::{ClientReport, ClientReportError},
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
    connection: Weak<Mutex<Inbox>>,
    boot: String,
    fingerprint: u64,
    report: ClientReport,
}

/// A window's side of the client contract.
#[derive(Default)]
pub(crate) struct ClientContract {
    sent: Option<SentReport>,
    pub(crate) follows: follow::Follows,
    jumps: jump::Jumps,
}

impl HerdrWindow {
    /// One window tick of the contract: what the hosts answered, the jumps
    /// they forwarded, then the report, which so reflects where a jump led.
    pub(crate) fn tick_client_contract(&mut self, now: Instant, cx: &mut gpui::Context<Self>) {
        for (purpose, answer) in self.take_followed_answers() {
            match (purpose, answer) {
                (Purpose::Jump(key), Err(refusal)) => {
                    self.client_contract.jumps.refused(key, refusal);
                }
                // An accepted focus request proves nothing yet: the jump is
                // answered by the snapshot that shows its pane focused.
                (Purpose::Jump(_), Ok(())) => {}
            }
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
            Some(self.active),
            self.panel_agents(),
        )
    }

    /// Changes whenever [`Self::client_report`] may return something else.
    /// Scalars only: no row is built and nothing is sorted, so the window can
    /// afford it on every tick.
    pub(crate) fn client_report_fingerprint(&self) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.active.hash(&mut hash);
        self.selected_endpoint.hash(&mut hash);
        std::mem::discriminant(&self.agent_sort).hash(&mut hash);
        self.device_filter.hash(&mut hash);
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            endpoint.id.hash(&mut hash);
            endpoint.label.hash(&mut hash);
            std::mem::discriminant(&endpoint.availability()).hash(&mut hash);
            let live = self.host_live(index);
            live.status.is_connected().hash(&mut hash);
            // Every accepted snapshot is a new allocation; the boot and
            // revision keep a reused address from reading as no change.
            live.snapshot
                .as_ref()
                .map(|snapshot| {
                    (
                        Arc::as_ptr(snapshot) as usize,
                        &snapshot.boot_id,
                        snapshot.revision,
                    )
                })
                .hash(&mut hash);
        }
        hash.finish()
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
        self.client_contract
            .sent
            .as_ref()
            .and_then(|sent| {
                self.connections().find(|connection| {
                    std::ptr::eq(Arc::as_ptr(&connection.clients), sent.connection.as_ptr())
                })
            })
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
        let clients = connection.clients.clone();
        // Never wait on the event reader: a busy mailbox is asked next tick.
        let Ok(mut mailbox) = clients.try_lock() else {
            return;
        };
        // The request names the daemon's boot, which its first snapshot brings.
        let Some(boot) = mailbox.report_boot().map(str::to_owned) else {
            return;
        };
        let fingerprint = self.client_report_fingerprint();
        let held = self.client_contract.sent.as_ref().filter(|sent| {
            std::ptr::eq(sent.connection.as_ptr(), Arc::as_ptr(&clients)) && sent.boot == boot
        });
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
            let queued = report::fit(report.clone())
                .ok_or(crate::Error::ClientReportTooLarge)
                .and_then(|report| Ok(serde_json::to_value(report)?))
                .and_then(|params| Ok(handle.request(&boot, Method::ClientReport, params)?));
            match queued {
                Ok(request) => mailbox.report_sent(request),
                // A full queue drains; the report is still owed.
                Err(crate::Error::Client(herdr_client::Error::Full)) => return,
                // Anything else is not retried: a failed connection is
                // replaced by a new one, which is reported to afresh.
                Err(error) => {
                    tracing::warn!(category = "client_report", %error, "report not queued");
                }
            }
        }
        self.client_contract.sent = Some(SentReport {
            connection: Arc::downgrade(&clients),
            boot,
            fingerprint,
            report,
        });
    }
}
