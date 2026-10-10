//! One connection's client-contract mailbox, filled by its event reader and
//! read by the window.
//!
//! It keeps what the contract needs of a connection without the live state
//! having to know: the boot a lane request must name, what the daemon takes,
//! the jump it forwarded last, and the answers to the requests the window follows
//! or sent for the contract. The answer to a report or to a jump's answer
//! never reaches the live state, so a rejected one is logged and does not
//! appear in the window as a failed command.

use herdr_client::{ClientEvent, Method, protocol::clients::EndpointClientJump};
use serde_json::Value;

/// Characters of a daemon's error code that are logged.
const CODE_CHARS: usize = 64;
/// Characters of a daemon's error message that are kept.
const MESSAGE_CHARS: usize = 256;

/// Why a followed request did not do what it asked. The daemon's error codes
/// are read here, at the boundary; the window matches on the cause.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Refusal {
    #[error("the pane no longer exists")]
    PaneNotFound,
    #[error("the daemon has restarted")]
    StaleBoot,
    /// The request never reached the daemon: the window could not queue it,
    /// or the connection worker would not send it.
    #[error("the request was not sent")]
    Unsent,
    /// Any other refusal, with the daemon's own words for it.
    #[error("{message}")]
    Other { message: String },
}

/// How a followed request ended.
pub(crate) type Answer = Result<(), Refusal>;

#[derive(Debug)]
struct Followed {
    request: String,
    /// Whether the answer is kept from the live state: a tool asked, not the
    /// user, so a refusal must not appear as a failed command.
    quiet: bool,
    answer: Option<Answer>,
}

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
    /// The requests the window follows, each until the window collects its
    /// answer or the connection goes. The connection bounds the unanswered
    /// ones as it does `resolutions`, and the window holds one entry of its
    /// own for each, which it settles against this list on every tick.
    followed: Vec<Followed>,
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

/// What a daemon's answer to a followed request says.
fn answer_of(response: &Value) -> Answer {
    let Some(error) = error_of(response) else {
        return Ok(());
    };
    Err(match error.get("code").and_then(Value::as_str) {
        Some("pane_not_found") => Refusal::PaneNotFound,
        Some("stale_boot") => Refusal::StaleBoot,
        _ => Refusal::Other {
            message: crate::notifications::safe_text(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("malformed response"),
                MESSAGE_CHARS,
            ),
        },
    })
}

/// Why the connection worker would not send a followed request. Whatever
/// the reason, the daemon never saw it, so only a boot that changed since
/// it was queued is more than a request that was not sent.
fn unsent(reason: &herdr_client::Error) -> Refusal {
    match reason {
        herdr_client::Error::CommandBoot => Refusal::StaleBoot,
        _ => Refusal::Unsent,
    }
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
        self.record(request, || answer_of(response))
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
        self.record(request, || {
            tracing::debug!(category = "followed_request", %reason, "request not sent");
            Err(unsent(reason))
        })
    }

    /// Keeps `answer` for the window when it follows `request`. Returns
    /// whether the live state should see the event too.
    fn record(&mut self, request: &str, answer: impl FnOnce() -> Answer) -> bool {
        let Some(followed) = self
            .followed
            .iter_mut()
            .find(|followed| followed.request == request)
        else {
            return true;
        };
        followed.answer = Some(answer());
        !followed.quiet
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
    /// queueing, so even an immediate rejection finds it. A `quiet` request's
    /// answer is kept from the live state.
    pub(crate) fn follow(&mut self, request: String, quiet: bool) {
        self.followed.push(Followed {
            request,
            quiet,
            answer: None,
        });
    }

    /// Whether `request` is still followed: not yet collected, and not lost
    /// with its connection.
    pub(crate) fn follows(&self, request: &str) -> bool {
        self.followed
            .iter()
            .any(|followed| followed.request == request)
    }

    /// The answer to `request` once it has one, which ends the following.
    pub(crate) fn take_answer(&mut self, request: &str) -> Option<Answer> {
        let index = self
            .followed
            .iter()
            .position(|followed| followed.request == request && followed.answer.is_some())?;
        self.followed.remove(index).answer
    }
}
