//! `endpoint.client-jump.v1`: a daemon forwards a tool's `client.jump`, and
//! this window shows the pane, switching host when it has to. The answer goes
//! back as `client.jump.resolve` on the connection the jump arrived on.
//!
//! The navigation is the window's own: the same cross-host navigation a click
//! on an agent row starts, and nothing follows its request or its error. This
//! module decides where a jump leads and how it ended, from the retained
//! snapshots and explicit times. It mirrors `src/client/shell/client_jump.rs`
//! in the Herdr fork, and answers jumps by the same rules; its cases are
//! ported in `tests/jump_target.rs` and `tests/jumps.rs`.
//!
//! A jump is answered by what the window holds, checked when it arrives and
//! again on every tick while it waits: the first of [`jump_target`]'s checks
//! that fails is the answer, sent at once. Otherwise the window navigates,
//! answers `focused` once it presents the jump's host with the pane focused,
//! and `timeout` when that has not happened [`NAVIGATION_SETTLE_DEADLINE`]
//! after the jump arrived. So a host's refusal of the focus request shows in
//! the status bar like a click's, and the jump times out.
//!
//! The newest jump wins. A window performs one jump at a time, and a jump
//! that arrives while another is still open replaces it. The replaced jump
//! is never answered: its daemon told the caller so itself when it forwarded
//! the newer one, and takes no answer for it. A daemon forwards a jump only
//! to a client that reported, and a window reports on one connection (see
//! [`super`]), so its jumps arrive on that connection alone and its mailbox,
//! which keeps the last jump it was sent, is all the ordering there is.
//!
//! What the replaced jump asked for may still be on its way, such as a
//! navigation that waits for the surface. So a jump that replaces another
//! always navigates, even to the pane the window presents at that moment:
//! its navigation is then the last one dispatched, and it is answered like
//! any other, once its pane shows or the deadline passes. Only a jump that
//! replaces nothing is answered `focused` without navigating. The terminal
//! client keeps the same rule.
//!
//! Two things differ from the terminal client. A jump that arrives while a
//! menu or dialog is open waits for it to close instead of taking the window
//! from under the user, and times out like any jump that never showed its
//! pane. And a host switch the user started selects the new host at the
//! click, so a jump to the host the window is leaving no longer finds its
//! pane presented, even the pane it was showing: it navigates back, and is
//! answered `focused` once that host's surface is active again. The terminal
//! client changes host only when the switch completes, so there a first jump,
//! one that replaces none, to the pane it still shows does not navigate: the
//! switch finishes, and the jump answers `timeout`. A jump there to another
//! pane of the host being left navigates, as here, and can answer `focused`.
//!
//! A host that goes away while a jump waits is no longer online, which
//! answers it `unavailable` on the next tick.
//!
//! The navigation can itself retire the connection the jump arrived on: an
//! editor group's parked connection belongs to the workspace it was parked
//! in, and the window lets it go when it leaves that workspace. Such a
//! connection is kept, out of every group, while it still owes a jump its
//! answer.

use super::report::Host;
use crate::fork::Inbox;
use crate::{HerdrWindow, connection::ConnectionBridge, navigation::NavigationTarget};
use gpui::Context;
use herdr_client::{
    ForkMethod, Method,
    protocol::clients::{
        ClientEndpointStatus, ClientJumpOutcome, ClientJumpResolveParams, EndpointClientJump,
    },
};
use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

/// How long a navigation may take to show its target as focused before the
/// window reports that it did not settle. The daemon stops waiting for a jump
/// after [`DAEMON_JUMP_TIMEOUT`], so a client answers well before that.
pub(crate) const NAVIGATION_SETTLE_DEADLINE: Duration = Duration::from_secs(12);
/// How long the daemon waits for a jump's answer. A connection kept only to
/// answer is let go after this: nobody listens for the answer any more.
const DAEMON_JUMP_TIMEOUT: Duration = Duration::from_secs(20);

/// The host a jump leads to, as its index in `hosts`, or why it cannot be
/// performed. The pane id is only meaningful under the boot it was read from,
/// so a boot the retained snapshot no longer has is checked before anything
/// else about the host.
pub(super) fn jump_target(
    hosts: &[Host<'_>],
    jump: &EndpointClientJump,
) -> Result<usize, ClientJumpOutcome> {
    let (index, host) = hosts
        .iter()
        .enumerate()
        .find(|(_, host)| host.id == jump.endpoint_id)
        .ok_or(ClientJumpOutcome::NotFound)?;
    let Some(snapshot) = host.snapshot else {
        return Err(ClientJumpOutcome::Unavailable);
    };
    if snapshot.boot_id != jump.boot_id {
        return Err(ClientJumpOutcome::StaleBoot);
    }
    if host.status != ClientEndpointStatus::Online {
        return Err(ClientJumpOutcome::Unavailable);
    }
    if !snapshot
        .panes
        .iter()
        .any(|pane| pane.pane_id == jump.pane_id)
    {
        return Err(ClientJumpOutcome::NotFound);
    }
    Ok(index)
}

struct Pending {
    /// The mailbox of the connection the jump arrived on, which names that
    /// connection: its answer goes back there and nowhere else.
    origin: Weak<Mutex<Inbox>>,
    jump: EndpointClientJump,
    deadline: Instant,
    /// Whether the window has been asked to navigate. Asked once: a
    /// navigation the user has since replaced is not taken back.
    navigating: bool,
    /// Whether it took the place of a jump still open, whose navigation may
    /// still land. Such a jump is not `focused` before it has navigated.
    replaced_another: bool,
}

/// A jump whose answer is ready to send.
struct Resolved {
    origin: Weak<Mutex<Inbox>>,
    jump_id: String,
    outcome: ClientJumpOutcome,
}

/// A connection the window let go of while it still owed a jump its answer.
struct Retiring {
    connection: ConnectionBridge,
    /// When it is let go whatever it still owes: once the daemon has stopped
    /// waiting for the last jump it forwarded here.
    until: Instant,
}

/// The jump this window is performing, and answers not yet sent.
#[derive(Default)]
pub(super) struct Jumps {
    /// One at a time: a newer jump replaces it.
    pending: Option<Pending>,
    resolved: Vec<Resolved>,
    retiring: Vec<Retiring>,
}

impl Jumps {
    /// The connections kept only to answer their jumps.
    pub(super) fn retiring(&self) -> impl Iterator<Item = &ConnectionBridge> {
        self.retiring.iter().map(|retiring| &retiring.connection)
    }

    /// Whether `connection` still owes a jump its answer: one it forwarded is
    /// open or answered but not acknowledged. A mailbox its reader holds
    /// right now is taken to owe one, and asked again next tick.
    fn owed_on(&self, connection: &ConnectionBridge) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| connection.owns(&pending.origin))
            || self
                .resolved
                .iter()
                .any(|resolved| connection.owns(&resolved.origin))
            || connection
                .fork
                .try_lock()
                .map_or(true, |fork| fork.clients.owes_jump_answer())
    }

    /// Takes `jump` in place of the one still open, which is dropped without
    /// an answer.
    fn receive(&mut self, origin: Weak<Mutex<Inbox>>, jump: EndpointClientJump, now: Instant) {
        // A retired connection still carries the report until another does,
        // so its daemon may forward it a jump long after it retired. The
        // daemon waits for that answer from now on.
        for retiring in &mut self.retiring {
            if retiring.connection.owns(&origin) {
                retiring.until = now + DAEMON_JUMP_TIMEOUT;
            }
        }
        let replaced_another = self.pending.is_some();
        if let Some(replaced) = &self.pending {
            tracing::debug!(
                category = "client_jump",
                jump_id = %replaced.jump.jump_id,
                "replaced by a newer jump"
            );
        }
        self.pending = Some(Pending {
            origin,
            jump,
            deadline: now + NAVIGATION_SETTLE_DEADLINE,
            navigating: false,
            replaced_another,
        });
    }

    fn resolve(&mut self, pending: Pending, outcome: ClientJumpOutcome) {
        self.resolved.push(Resolved {
            origin: pending.origin,
            jump_id: pending.jump.jump_id,
            outcome,
        });
    }
}

impl HerdrWindow {
    /// Whether the window presents host `index` with `pane` focused: the host
    /// is selected, its surface is active and drawn from the snapshot that
    /// shows the pane focused. A host just selected still shows the snapshot
    /// it last sent, whose focus says nothing about what is on screen.
    fn presents_pane(&self, index: usize, pane: &str) -> bool {
        index == self.selected_endpoint
            && self.surface_activation_ready()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.focused_pane_id.as_deref() == Some(pane))
    }

    /// Lets go of `connection`, which the window no longer uses. One that
    /// still owes a jump its answer is kept, out of every group, until it owes nothing or
    /// the daemon has stopped waiting; dropping it would disconnect the
    /// client the daemon is waiting on.
    pub(crate) fn retire_connection(&mut self, connection: ConnectionBridge, now: Instant) {
        let jumps = &mut self.fork.client_contract.jumps;
        if jumps.owed_on(&connection) {
            jumps.retiring.push(Retiring {
                connection,
                until: now + DAEMON_JUMP_TIMEOUT,
            });
        }
    }

    /// Lets go of every retired connection that owes nothing any more.
    fn release_retired(&mut self, now: Instant) {
        let jumps = &mut self.fork.client_contract.jumps;
        let retiring = std::mem::take(&mut jumps.retiring);
        jumps.retiring = retiring
            .into_iter()
            .filter(|retiring| now < retiring.until && jumps.owed_on(&retiring.connection))
            .collect();
    }

    /// Takes the jumps every connection's daemon forwarded, settles the open
    /// one against what the window now presents, and sends each answer that
    /// is ready. Called once per window tick.
    pub(super) fn settle_client_jumps(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.receive_client_jumps(now);
        if let Some(mut pending) = self.fork.client_contract.jumps.pending.take() {
            let target = jump_target(&self.contract_hosts(), &pending.jump);
            let outcome = match target {
                Err(outcome) => Some(outcome),
                // What a replaced jump asked for may still land, so the jump
                // that replaced it navigates before its pane showing counts.
                Ok(host)
                    if (pending.navigating || !pending.replaced_another)
                        && self.presents_pane(host, &pending.jump.pane_id) =>
                {
                    Some(ClientJumpOutcome::Focused)
                }
                Ok(_) if now >= pending.deadline => Some(ClientJumpOutcome::Timeout),
                // An open menu keeps the window; the jump waits for it.
                Ok(_) if pending.navigating || self.menu.page.is_some() => None,
                // Navigated once: what the window does with it, or what the
                // host answers, shows only in what the window then presents.
                Ok(_) => {
                    pending.navigating = true;
                    self.navigate_endpoint(
                        &pending.jump.endpoint_id,
                        NavigationTarget::Pane(&pending.jump.pane_id),
                        cx,
                    );
                    None
                }
            };
            let jumps = &mut self.fork.client_contract.jumps;
            match outcome {
                Some(outcome) => jumps.resolve(pending, outcome),
                None => jumps.pending = Some(pending),
            }
        }
        self.answer_client_jumps();
        self.release_retired(now);
    }

    /// Each mailbox is asked without waiting on its reader; one that is busy
    /// keeps its jump for the next tick. Only the connection that carries
    /// the window's report is forwarded jumps, and every connection is asked
    /// because the window may keep that one anywhere: as an endpoint's own,
    /// parked in an editor group, or retiring.
    fn receive_client_jumps(&mut self, now: Instant) {
        let arrived: Vec<_> = self
            .connections()
            .filter_map(|connection| {
                let jump = connection.fork.try_lock().ok()?.clients.take_jump()?;
                Some((Arc::downgrade(&connection.fork), jump))
            })
            .collect();
        for (origin, jump) in arrived {
            self.fork.client_contract.jumps.receive(origin, jump, now);
        }
    }

    /// Sends every ready answer on the connection its jump arrived on. An
    /// answer whose connection the window no longer holds is dropped: the
    /// daemon behind the new one never asked.
    fn answer_client_jumps(&mut self) {
        let mut owed = Vec::new();
        for resolved in std::mem::take(&mut self.fork.client_contract.jumps.resolved) {
            let Some(connection) = self.connection_of(&resolved.origin) else {
                continue;
            };
            let Some(handle) = &connection.handle else {
                continue;
            };
            let Ok(mut fork) = connection.fork.try_lock() else {
                owed.push(resolved);
                continue;
            };
            let mailbox = &mut fork.clients;
            let Some(boot) = mailbox.resolve_boot().map(str::to_owned) else {
                continue;
            };
            let params = ClientJumpResolveParams {
                jump_id: resolved.jump_id.clone(),
                outcome: resolved.outcome.clone(),
            };
            let queued = serde_json::to_value(params)
                .map_err(crate::Error::from)
                .and_then(|params| {
                    let method = Method::Fork(ForkMethod::ClientJumpResolve);
                    Ok(handle.request(&boot, method, params)?)
                });
            match queued {
                Ok(request) => mailbox.resolution_sent(request),
                // A full queue drains; the answer is still owed.
                Err(crate::Error::Client(herdr_client::Error::Full)) => {
                    drop(fork);
                    owed.push(resolved);
                }
                Err(error) => {
                    tracing::warn!(category = "client_jump", %error, "answer not queued");
                }
            }
        }
        self.fork.client_contract.jumps.resolved = owed;
    }
}
