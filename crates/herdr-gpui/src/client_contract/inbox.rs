//! One connection's client-contract mailbox, filled by its event reader and
//! read by the window.
//!
//! It keeps what the contract needs of a connection without the live state
//! having to know: the boot a lane request must name, what the daemon takes,
//! the jump it forwarded last, the answers to the requests it sent for the
//! contract, and whether the daemon accepted a request the window follows.
//! The answer to a report or to a jump's answer never reaches the live state,
//! so a rejected one is logged and does not appear in the window as a failed
//! command. The answer to a followed request reaches it too, so a refusal
//! shows like any other.

use herdr_client::{ClientEvent, Method, protocol::clients::EndpointClientJump};
use serde_json::Value;

/// Characters of a daemon's error code that are logged.
const CODE_CHARS: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The boot of this connection's snapshots; a lane request names one.
    boot: Option<String>,
    /// Whether the daemon behind this connection advertised `client.report`.
    reports: bool,
    /// Whether it advertised `client.jump.resolve`.
    resolves: bool,
    /// The report the daemon has not answered yet. One at a time, so reports
    /// coalesce to the latest instead of queueing behind one another.
    report: Option<String>,
    /// The jump the daemon forwarded last, until the window takes it. A
    /// newer one replaces it: the daemon has given up on the older itself.
    jump: Option<EndpointClientJump>,
    /// The requests of the jump answers the daemon has not acknowledged.
    /// Each stays until its reply or its rejection is filed here, or the
    /// connection goes: the oldest is the next to be answered, so none gives
    /// way to a newer one. A daemon holds one jump open per client, but it
    /// gives up on that one as it forwards the next, while the answers still
    /// wait behind the connection's one request in flight, so there can be
    /// many. The connection bounds them: its command queue takes 64
    /// requests, its worker holds one more back and one in flight, its event
    /// queue holds 8 replies not filed yet, and it ends when the one in
    /// flight is not answered in time.
    resolutions: Vec<String>,
    /// The requests the window follows, each until its answer or its
    /// rejection is filed here, or the connection goes. The connection bounds
    /// them as it does `resolutions`.
    followed: Vec<String>,
    /// Whether the daemon accepted one of `followed` since the window last
    /// asked.
    accepted: bool,
}

/// The error an endpoint response carries, when it carries one.
fn error_of(response: &Value) -> Option<&Value> {
    response.get("error").filter(|error| !error.is_null())
}

/// The code of the error an endpoint response carries, bounded for a log.
fn rejection(response: &Value) -> Option<String> {
    let code = error_of(response)?
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("invalid_response");
    Some(crate::notifications::safe_text(code, CODE_CHARS))
}

impl Inbox {
    /// Keeps the events this mailbox waits for and passes every other one on.
    pub(crate) fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        let event = match event {
            ClientEvent::ClientJump(jump) => {
                self.jump = Some(jump);
                return None;
            }
            event => event,
        };
        match &event {
            ClientEvent::Connected(welcome) => {
                self.reports = Method::ClientReport.advertised_in(&welcome.methods);
                self.resolves = Method::ClientJumpResolve.advertised_in(&welcome.methods);
            }
            // A new boot forgets no request. The worker refuses what was
            // queued for the old boot one by one, after this snapshot, and
            // each refusal has to find its request here.
            ClientEvent::Snapshot(snapshot) => {
                if self.boot.as_deref() != Some(snapshot.boot_id.as_str()) {
                    self.boot = Some(snapshot.boot_id.clone());
                }
            }
            // The daemon gave up on this connection's jumps with it.
            ClientEvent::Disconnected { .. } => *self = Self::default(),
            ClientEvent::Response {
                request_id,
                response,
            } => return self.answered(request_id, response).then_some(event),
            ClientEvent::CommandRejected {
                request_id: Some(request_id),
                reason,
            } => return self.rejected(request_id, reason).then_some(event),
            _ => {}
        }
        Some(event)
    }

    /// Records the daemon's answer to `request`. Returns whether the live
    /// state should see it too.
    fn answered(&mut self, request: &str, response: &Value) -> bool {
        if self.report.as_deref() == Some(request) {
            self.report = None;
            // Nothing is retried: the next report replaces a rejected one.
            if let Some(code) = rejection(response) {
                tracing::warn!(
                    category = "client_report",
                    code,
                    "daemon rejected the report"
                );
            }
            return false;
        }
        if let Some(index) = self.resolutions.iter().position(|id| id == request) {
            self.resolutions.remove(index);
            match rejection(response).as_deref() {
                None => {}
                // Expected: a newer jump replaced this one, or the daemon
                // stopped waiting. The tool was told so; nobody else is.
                Some("jump_not_found") => {
                    tracing::debug!(category = "client_jump", "answer came too late");
                }
                Some(code) => {
                    tracing::warn!(category = "client_jump", code, "daemon rejected the answer");
                }
            }
            return false;
        }
        self.record(request, error_of(response).is_none());
        true
    }

    /// Records that the connection worker would not send `request`. Returns
    /// whether the live state should see it too.
    fn rejected(&mut self, request: &str, reason: &herdr_client::Error) -> bool {
        if self.report.as_deref() == Some(request) {
            self.report = None;
            tracing::warn!(category = "client_report", %reason, "report not sent");
            return false;
        }
        if let Some(index) = self.resolutions.iter().position(|id| id == request) {
            self.resolutions.remove(index);
            tracing::warn!(category = "client_jump", %reason, "answer not sent");
            return false;
        }
        self.record(request, false);
        true
    }

    /// Ends the following of `request`, when the window follows it, and
    /// keeps for the window whether the daemon accepted it.
    fn record(&mut self, request: &str, accepted: bool) {
        if let Some(index) = self.followed.iter().position(|id| id == request) {
            self.followed.remove(index);
            self.accepted |= accepted;
        }
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

    /// The jump the daemon forwarded since the last call, the newest when
    /// it forwarded several.
    pub(crate) fn take_jump(&mut self) -> Option<EndpointClientJump> {
        self.jump.take()
    }

    /// Whether a jump the daemon forwarded here is not taken yet, or its
    /// answer not acknowledged yet.
    pub(crate) fn owes_jump_answer(&self) -> bool {
        self.jump.is_some() || !self.resolutions.is_empty()
    }

    /// The boot a jump's answer on this connection names, when its daemon
    /// takes answers.
    pub(crate) fn resolve_boot(&self) -> Option<&str> {
        self.boot.as_deref().filter(|_| self.resolves)
    }

    /// Records the jump answer just queued, under the lock it was queued
    /// under, so the daemon's acknowledgement stays out of the live state.
    pub(crate) fn resolution_sent(&mut self, request: String) {
        self.resolutions.push(request);
    }

    /// Follows `request` to its answer. Called under the same lock as the
    /// queueing, so even an immediate rejection finds it.
    pub(crate) fn follow(&mut self, request: String) {
        self.followed.push(request);
    }

    /// Whether the daemon accepted a followed request since the last call.
    pub(crate) fn take_accepted(&mut self) -> bool {
        std::mem::take(&mut self.accepted)
    }
}
