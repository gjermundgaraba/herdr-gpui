//! Where the window feeds its history and steps through it.
//!
//! The Herdr fork's terminal client feeds its `src/client/shell/history.rs`
//! from `src/client/shell/endpoints.rs` and steps through it in
//! `src/client/shell/endpoint_navigation.rs`, observing each snapshot as it
//! accepts it. Here snapshots are accepted on the connections' reader
//! threads, so the window observes what it holds once per tick: every host's
//! snapshot that is new since the last tick prunes that host's closed panes
//! and old boot, and the presented host's records the focused pane. A tick
//! that changed nothing compares a few pointers and leaves.
//!
//! A step is pending until its pane shows. A step to another host presents
//! that host first, with the snapshot it last sent, whose focused pane is
//! whatever that host had focused while the window was away; the history
//! leaves it unrecorded, and the window offers it again every tick. The step
//! is over when its pane shows after its focus request went out; when it
//! failed, which takes the cursor back: its focus request was refused or
//! never sent, or the host's surface never came; or when the window finds its
//! navigation no longer under way (`step_under_way`). That last ending takes
//! the cursor back too, as the terminal client does for a step another
//! selection replaced, unless the host had accepted the step's focus
//! request: the pane then had the focus there, and the cursor stays on it,
//! as it does in the terminal client once a step's request was answered.
//! After either of the last two, what the window presents is a visit like
//! any other.

use super::{History, Step};
use crate::{
    HerdrWindow,
    client_contract::Purpose,
    endpoint::{self, Availability, Endpoint},
    navigation::NavigationTarget,
    window::Flash,
};
use gpui::Context;
use herdr_client::protocol::ClientShellSnapshot;
use std::sync::{Arc, Weak};

/// One accepted snapshot, by identity. Every accepted snapshot is a new
/// allocation, and the `Weak` held here keeps its allocation, so a newer
/// snapshot never reuses its address.
#[derive(Debug)]
struct Seen {
    endpoint_id: String,
    snapshot: Weak<ClientShellSnapshot>,
}

impl Seen {
    fn of(endpoint_id: &str, snapshot: &Arc<ClientShellSnapshot>) -> Self {
        Self {
            endpoint_id: endpoint_id.to_owned(),
            snapshot: Arc::downgrade(snapshot),
        }
    }

    fn is(&self, endpoint_id: &str, snapshot: &Arc<ClientShellSnapshot>) -> bool {
        self.endpoint_id == endpoint_id
            && std::ptr::eq(self.snapshot.as_ptr(), Arc::as_ptr(snapshot))
    }
}

/// Whether the window holds `endpoint`: it is switched on and not detached.
fn attached(endpoint: &Endpoint) -> bool {
    !matches!(
        endpoint.availability(),
        Availability::Disabled | Availability::Detached
    )
}

/// Whether the host `id` is one a step can lead to.
fn online(endpoints: &[Endpoint], id: &str) -> bool {
    endpoints
        .iter()
        .any(|endpoint| endpoint.id == id && endpoint.online())
}

/// A window's history and what it has observed for it.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    pub(super) history: History,
    /// The snapshot last observed for each host.
    hosts: Vec<Seen>,
    /// The presented host's snapshot last observed for its focused pane.
    presented: Option<Seen>,
}

#[cfg(test)]
impl Tracker {
    /// A history that visited `panes` of one host in order and then went
    /// back `back` steps, each of them landed.
    pub(crate) fn visited(endpoint_id: &str, boot_id: &str, panes: &[&str], back: usize) -> Self {
        let mut history = History::default();
        for pane in panes {
            history.observe(endpoint_id, boot_id, Some(pane));
        }
        for _ in 0..back {
            if let Some(step) = history.back(|_| true) {
                history.traversal_sent(&step);
                history.observe(endpoint_id, boot_id, Some(&step.visit.pane_id));
            }
        }
        Self {
            history,
            ..Self::default()
        }
    }
}

impl HerdrWindow {
    /// Whether the navigation of `step` is still on its way:
    /// the window presents the visit's host and still has the navigation to
    /// send, follows the focus request it sent, or holds input behind the
    /// focus that request asked for.
    ///
    /// Each of the three ends by itself. The surface the deferred navigation
    /// and the fence wait for has a deadline, after which the window fails
    /// the step ([`Self::history_host_failed`]) and reconnects or goes back
    /// to the local host; the request is answered or its connection is
    /// replaced. And each is given up when the user goes
    /// somewhere else: selecting another host presents that one, and a
    /// navigation on the same host takes the deferred one's place. Once the
    /// focus request is out, the window's own navigation waits behind its
    /// fence, so the step lands first and the navigation is a visit after it.
    fn step_under_way(&self, step: &Step) -> bool {
        let visit = &step.visit;
        let pane = NavigationTarget::Pane(visit.pane_id.as_str());
        self.endpoints[self.selected_endpoint].id == visit.endpoint_id
            && (self
                .pending_navigation
                .as_ref()
                .is_some_and(|pending| pending.target.as_deref() == pane)
                || self.client_contract.follows.follows_step(step)
                || self
                    .live
                    .activation
                    .as_ref()
                    .and_then(|activation| activation.focus.as_ref())
                    .is_some_and(|focus| focus.as_deref() == pane))
    }

    /// Observes the snapshots accepted since the last tick, and forgets the
    /// hosts the window no longer holds.
    pub(crate) fn sync_history(&mut self) {
        let abandoned = self
            .history
            .history
            .traversal_step()
            .is_some_and(|step| !self.step_under_way(&step));
        let Self {
            history: tracker,
            endpoints,
            live,
            selected_endpoint,
            ..
        } = self;
        tracker.history.retain_endpoints(|id| {
            endpoints
                .iter()
                .any(|endpoint| endpoint.id == id && attached(endpoint))
        });
        tracker.hosts.retain(|seen| {
            endpoints
                .iter()
                .any(|endpoint| endpoint.id == seen.endpoint_id)
        });
        for (index, endpoint) in endpoints.iter().enumerate() {
            let snapshot = if index == *selected_endpoint {
                live.snapshot.as_ref()
            } else {
                endpoint.live.snapshot.as_ref()
            };
            let Some(snapshot) = snapshot else {
                continue;
            };
            let seen = tracker
                .hosts
                .iter_mut()
                .find(|seen| seen.endpoint_id == endpoint.id);
            if seen
                .as_ref()
                .is_some_and(|seen| seen.is(&endpoint.id, snapshot))
            {
                continue;
            }
            // A restart invalidates the visits recorded under the old boot,
            // and closed panes theirs, even while the host is not presented.
            tracker
                .history
                .observe_boot(&endpoint.id, &snapshot.boot_id);
            tracker.history.observe_panes(
                &endpoint.id,
                &snapshot.boot_id,
                snapshot
                    .panes
                    .iter()
                    .map(|pane| pane.pane_id.as_str())
                    .chain(snapshot.focused_pane_id.as_deref()),
            );
            let now = Seen::of(&endpoint.id, snapshot);
            match seen {
                Some(seen) => *seen = now,
                None => tracker.hosts.push(now),
            }
        }
        if abandoned {
            tracker.history.traversal_abandoned();
        }
        let endpoint = &endpoints[*selected_endpoint];
        let Some(snapshot) = live.snapshot.as_ref() else {
            return;
        };
        // What an abandoned step left unrecorded is a visit now, new or not.
        if !abandoned
            && tracker
                .presented
                .as_ref()
                .is_some_and(|seen| seen.is(&endpoint.id, snapshot))
        {
            return;
        }
        // A host the window is detached from shows nothing to visit.
        if attached(endpoint) {
            tracker.history.observe(
                &endpoint.id,
                &snapshot.boot_id,
                snapshot.focused_pane_id.as_deref(),
            );
            // Left unrecorded behind a step: offered again next tick, so it
            // is a visit as soon as the step is over without its pane.
            if tracker.history.traversal_step().is_some() {
                return;
            }
        }
        tracker.presented = Some(Seen::of(&endpoint.id, snapshot));
    }

    /// Back or Forward through the pane visit history, skipping hosts that
    /// are not online, with the window's own cross-host navigation. The title
    /// bar's buttons, the `back` and `forward` commands, and the daemon's
    /// `history_back` and `history_forward` keys all step here.
    pub(crate) fn travel(&mut self, direction: endpoint::Step, cx: &mut Context<Self>) {
        // What the window holds now is observed first, so the step starts
        // from the pane on screen.
        self.sync_history();
        let endpoints = &self.endpoints;
        let online = |id: &str| online(endpoints, id);
        let history = &mut self.history.history;
        let step = match direction {
            endpoint::Step::Back => history.back(online),
            endpoint::Step::Forward => history.forward(online),
        };
        let Some(step) = step else {
            self.show_flash(
                Flash::warning(match direction {
                    endpoint::Step::Back => "No earlier pane in history",
                    endpoint::Step::Forward => "No later pane in history",
                }),
                cx,
            );
            return;
        };
        let navigating = self.navigate_endpoint_for(
            &step.visit.endpoint_id,
            NavigationTarget::Pane(&step.visit.pane_id),
            Some(Purpose::History(step.clone())),
            cx,
        );
        if !navigating {
            self.history_step_failed(&step, false);
        }
    }

    /// Whether [`Self::travel`] has a visit to step to, as of the snapshots
    /// last observed. Read while rendering, so it observes nothing itself.
    pub(crate) fn can_travel(&self, direction: endpoint::Step) -> bool {
        let online = |id: &str| online(&self.endpoints, id);
        match direction {
            endpoint::Step::Back => self.history.history.can_back(online),
            endpoint::Step::Forward => self.history.history.can_forward(online),
        }
    }

    /// The window gave up on the host it presents, whose surface never
    /// came: a step that was on its way to a pane there failed with it.
    pub(crate) fn history_host_failed(&mut self) {
        let host = &self.endpoints[self.selected_endpoint].id;
        let step = self.history.history.traversal_step();
        if let Some(step) = step.filter(|step| step.visit.endpoint_id == *host) {
            self.history_step_failed(&step, false);
        }
    }

    /// The focus request of a step was refused or never sent: the cursor
    /// returns to where the step began, and a pane that no longer exists is
    /// forgotten.
    pub(crate) fn history_step_failed(&mut self, step: &Step, pane_closed: bool) {
        self.history.history.traversal_failed(step, pane_closed);
    }

    /// The focus request of `step` was queued for its host.
    pub(crate) fn history_step_sent(&mut self, step: &Step) {
        self.history.history.traversal_sent(step);
    }

    /// The host accepted the focus request of `step`.
    pub(crate) fn history_step_accepted(&mut self, step: &Step) {
        self.history.history.traversal_accepted(step);
    }
}
