//! One connection's client-contract mailbox, filled by its event reader and
//! read by the window.
//!
//! It keeps what the contract needs of a connection without the live state
//! having to know: the boot a lane request must name, whether the daemon
//! takes reports, and the answer to the one report still in flight. That
//! answer never reaches the live state, so a rejected report is logged and
//! does not appear in the window as a failed command.

use herdr_client::{ClientEvent, Method};
use serde_json::Value;

/// Characters of a daemon's error code that are logged.
const CODE_CHARS: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The boot of this connection's snapshots; a lane request names one.
    boot: Option<String>,
    /// Whether the daemon behind this connection advertised `client.report`.
    reports: bool,
    /// The report the daemon has not answered yet. One at a time, so reports
    /// coalesce to the latest instead of queueing behind one another.
    report: Option<String>,
}

/// The code of the error an endpoint response carries, when it carries one.
fn rejection(response: &Value) -> Option<String> {
    let error = response.get("error").filter(|error| !error.is_null())?;
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("invalid_response");
    Some(crate::notifications::safe_text(code, CODE_CHARS))
}

impl Inbox {
    /// Keeps the answers this mailbox waits for and passes every other event on.
    pub(crate) fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        match &event {
            ClientEvent::Connected(welcome) => {
                self.reports = Method::ClientReport.advertised_in(&welcome.methods);
            }
            ClientEvent::Snapshot(snapshot) => {
                if self.boot.as_deref() != Some(snapshot.boot_id.as_str()) {
                    self.boot = Some(snapshot.boot_id.clone());
                }
            }
            ClientEvent::Disconnected { .. } => *self = Self::default(),
            ClientEvent::Response {
                request_id,
                response,
            } if self.report.as_deref() == Some(request_id.as_str()) => {
                self.report = None;
                // Nothing is retried: the next report replaces a rejected one.
                if let Some(code) = rejection(response) {
                    tracing::warn!(
                        category = "client_report",
                        code,
                        "daemon rejected the report"
                    );
                }
                return None;
            }
            ClientEvent::CommandRejected {
                request_id: Some(request_id),
                reason,
            } if self.report.as_deref() == Some(request_id.as_str()) => {
                self.report = None;
                tracing::warn!(category = "client_report", %reason, "report not sent");
                return None;
            }
            _ => {}
        }
        Some(event)
    }

    /// The boot a report on this connection names, once its daemon has said
    /// it takes reports and has sent a snapshot.
    pub(crate) fn report_boot(&self) -> Option<&str> {
        self.boot.as_deref().filter(|_| self.reports)
    }

    /// Whether the daemon still owes an answer to this connection's report.
    pub(crate) fn report_pending(&self) -> bool {
        self.report.is_some()
    }

    /// Records the report just queued. Called under the same lock as the
    /// queueing, so even an immediate rejection finds it.
    pub(crate) fn report_sent(&mut self, request: String) {
        self.report = Some(request);
    }
}
