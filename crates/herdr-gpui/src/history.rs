//! Bounded, coalesced Back/Forward history of pane visits across hosts, per
//! window and in memory.
//!
//! A visit is recorded once per accepted snapshot of the presented host whose
//! focused pane differs from the current entry, so rapid intermediate focus
//! changes that never produced a snapshot are not visits. Traversal moves the
//! cursor optimistically. Until the step's target shows, what the presented
//! host shows is not a visit: it may be the pane the step left, or the pane a
//! host the step has just arrived at last had focused, and recording it would
//! drop the forward list. So a pending traversal takes one observation, its
//! target, and only once its own focus request is out: a history can hold
//! the same pane twice, and the pane shown before that is the one the step
//! leaves even when it equals the target. A traversal ends in one of three
//! other ways. Its navigation failed, which returns the cursor to where it
//! started. It was abandoned, because the navigation is no longer under way:
//! the cursor returns as well, unless the host had accepted the focus
//! request, in which case the pane had the focus there and the cursor stays
//! on it. Or its target was pruned. Visits to panes a host no longer reports
//! are pruned whenever that host's snapshot is accepted.
//!
//! What becomes of a step's focus request belongs to that step alone, told
//! apart by [`Step`] and not by where the step leads: with equal visits in
//! the history, an earlier step's answer would otherwise pass for that of a
//! later step to an equal pane.
//!
//! [`History`] mirrors `src/client/shell/history.rs` in the Herdr fork, whose
//! `history_back` and `history_forward` keys step through it; its cases are
//! ported in `tests/visits.rs` and `tests/traversal.rs`. [`sync`] is where
//! this window feeds it and decides that a step is no longer under way.

mod sync;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub(crate) use sync::Tracker;

use std::collections::{HashSet, VecDeque};

const CAPACITY: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Visit {
    /// The host, by endpoint id.
    pub(crate) endpoint_id: String,
    pub(crate) boot_id: String,
    pub(crate) pane_id: String,
}

/// A Back or Forward step as the window follows it: where it leads, and
/// which step it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) visit: Visit,
    id: u64,
}

/// How far the navigation of a traversal has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Navigation {
    /// Not sent yet: it waits for its host's surface, or behind the focus an
    /// earlier step asked for.
    Waiting,
    /// The focus request is out.
    Sent,
    /// The host accepted the request, so the pane had the focus there.
    Accepted,
}

/// An in-flight traversal whose target is the cursor.
#[derive(Debug)]
struct Traversal {
    /// The [`Step`] that began it. A chained step replaces the traversal, so
    /// what became of an earlier step's request no longer finds it.
    step: u64,
    /// Index of the visit that was current before the traversal began, where
    /// one that did not arrive returns.
    origin: usize,
    navigation: Navigation,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    visits: VecDeque<Visit>,
    /// Index of the current visit; meaningful only while `visits` is non-empty.
    cursor: usize,
    /// Traversal whose target has not shown yet. Nothing else is recorded
    /// meanwhile, so the forward list survives until it has.
    pending: Option<Traversal>,
    /// Steps begun so far; the next step's id.
    steps: u64,
}

impl History {
    /// Record the focused pane of the presented host after an accepted snapshot.
    /// While a traversal is pending only its target is taken, which confirms
    /// it, and only once the traversal's focus request is out: until then a
    /// pane equal to the target is the one the step leaves. Anything else is
    /// left for the caller to offer again once the traversal has ended.
    pub(crate) fn observe(&mut self, endpoint_id: &str, boot_id: &str, pane_id: Option<&str>) {
        let Some(pane_id) = pane_id else {
            return;
        };
        let current = self.current().is_some_and(|visit| {
            visit.endpoint_id == endpoint_id && visit.boot_id == boot_id && visit.pane_id == pane_id
        });
        if let Some(traversal) = &self.pending {
            if current && traversal.navigation != Navigation::Waiting {
                self.pending = None;
            }
            return;
        }
        if current {
            return;
        }
        if !self.visits.is_empty() {
            self.visits.truncate(self.cursor + 1);
        }
        self.visits.push_back(Visit {
            endpoint_id: endpoint_id.to_owned(),
            boot_id: boot_id.to_owned(),
            pane_id: pane_id.to_owned(),
        });
        while self.visits.len() > CAPACITY {
            self.visits.pop_front();
        }
        self.cursor = self.visits.len() - 1;
    }

    /// The step of the pending traversal.
    pub(crate) fn traversal_step(&self) -> Option<Step> {
        let traversal = self.pending.as_ref()?;
        Some(Step {
            visit: self.current()?.clone(),
            id: traversal.step,
        })
    }

    /// The focus request of `step` went out. If the step is still the
    /// pending one, its target can show from here on.
    pub(crate) fn traversal_sent(&mut self, step: &Step) {
        self.advance(step, Navigation::Waiting, Navigation::Sent);
    }

    /// The host accepted the focus request of `step`. If the step is still
    /// the pending one, its pane had the focus there, whether or not it is
    /// ever observed.
    pub(crate) fn traversal_accepted(&mut self, step: &Step) {
        self.advance(step, Navigation::Sent, Navigation::Accepted);
    }

    fn advance(&mut self, step: &Step, from: Navigation, to: Navigation) {
        if let Some(traversal) = &mut self.pending
            && traversal.step == step.id
            && traversal.navigation == from
        {
            traversal.navigation = to;
        }
    }

    /// The navigation of the pending traversal is no longer under way and
    /// nothing said it failed: the user navigated somewhere else before the
    /// target showed, or the target showed and was left again unobserved.
    /// The cursor stays on a target its host accepted the focus request for;
    /// any other traversal never arrived, and the cursor returns to where it
    /// began. Observations are visits again.
    pub(crate) fn traversal_abandoned(&mut self) {
        let Some(traversal) = self.pending.take() else {
            return;
        };
        if traversal.navigation != Navigation::Accepted {
            self.cursor = traversal.origin;
        }
    }

    /// A server reboot invalidates every visit recorded under the old boot.
    pub(crate) fn observe_boot(&mut self, endpoint_id: &str, boot_id: &str) {
        self.rebuild(|visit| visit.endpoint_id != endpoint_id || visit.boot_id == boot_id);
    }

    /// Forget visits to panes that a host's accepted snapshot no longer contains.
    pub(crate) fn observe_panes<'a>(
        &mut self,
        endpoint_id: &str,
        boot_id: &str,
        pane_ids: impl IntoIterator<Item = &'a str>,
    ) {
        let tracked = |visit: &Visit| visit.endpoint_id == endpoint_id && visit.boot_id == boot_id;
        if !self.visits.iter().any(tracked) {
            return;
        }
        let live: HashSet<&str> = pane_ids.into_iter().collect();
        self.rebuild(|visit| !tracked(visit) || live.contains(visit.pane_id.as_str()));
    }

    /// The navigation of `step` failed or was refused: if the step is still
    /// the pending one, return to the visit it started from. Forget its
    /// target when the pane no longer exists.
    pub(crate) fn traversal_failed(&mut self, step: &Step, pane_closed: bool) {
        if let Some(traversal) = self.pending.take_if(|traversal| traversal.step == step.id) {
            self.cursor = traversal.origin;
        }
        if pane_closed {
            self.rebuild(|candidate| candidate != &step.visit);
        }
    }

    /// Forget every visit to a host `held` no longer answers for: one that
    /// was removed, switched off, or detached.
    pub(crate) fn retain_endpoints(&mut self, held: impl Fn(&str) -> bool) {
        self.rebuild(|visit| held(&visit.endpoint_id));
    }

    /// Step to the nearest earlier visit on an online host.
    pub(crate) fn back(&mut self, online: impl Fn(&str) -> bool) -> Option<Step> {
        let index = self.earlier(online)?;
        self.traverse(index)
    }

    /// Step to the nearest later visit on an online host.
    pub(crate) fn forward(&mut self, online: impl Fn(&str) -> bool) -> Option<Step> {
        let index = self.later(online)?;
        self.traverse(index)
    }

    /// Whether [`Self::back`] has a visit to step to.
    pub(crate) fn can_back(&self, online: impl Fn(&str) -> bool) -> bool {
        self.earlier(online).is_some()
    }

    /// Whether [`Self::forward`] has a visit to step to.
    pub(crate) fn can_forward(&self, online: impl Fn(&str) -> bool) -> bool {
        self.later(online).is_some()
    }

    fn earlier(&self, online: impl Fn(&str) -> bool) -> Option<usize> {
        (0..self.cursor.min(self.visits.len()))
            .rev()
            .find(|index| online(&self.visits[*index].endpoint_id))
    }

    fn later(&self, online: impl Fn(&str) -> bool) -> Option<usize> {
        (self.cursor + 1..self.visits.len()).find(|index| online(&self.visits[*index].endpoint_id))
    }

    fn traverse(&mut self, index: usize) -> Option<Step> {
        let visit = self.visits.get(index)?.clone();
        // Chained steps keep the first origin: the last visit that was seen.
        let origin = self
            .pending
            .as_ref()
            .map_or(self.cursor, |traversal| traversal.origin);
        let id = self.steps;
        self.steps = self.steps.wrapping_add(1);
        self.cursor = index;
        self.pending = Some(Traversal {
            step: id,
            origin,
            navigation: Navigation::Waiting,
        });
        Some(Step { visit, id })
    }

    fn current(&self) -> Option<&Visit> {
        self.visits.get(self.cursor)
    }

    /// Keep the visits matching `keep`, merge neighbours that removals made
    /// equal, and remap the cursor and any in-flight traversal onto survivors.
    /// A dropped cursor moves to the nearest earlier survivor.
    fn rebuild(&mut self, keep: impl Fn(&Visit) -> bool) {
        if self.visits.iter().all(&keep) {
            return;
        }
        let mut survivors = VecDeque::with_capacity(self.visits.len());
        // Old index -> surviving index: itself, the equal neighbour it merged
        // into, or the nearest earlier survivor when dropped.
        let mut mapped = Vec::with_capacity(self.visits.len());
        let mut dropped = Vec::with_capacity(self.visits.len());
        for visit in self.visits.drain(..) {
            let kept = keep(&visit);
            if kept && survivors.back() != Some(&visit) {
                survivors.push_back(visit);
            }
            mapped.push(survivors.len().checked_sub(1));
            dropped.push(!kept);
        }
        self.visits = survivors;
        let remap = |index: usize| mapped.get(index).copied().flatten().unwrap_or(0);
        let was_dropped = |index: usize| dropped.get(index).copied().unwrap_or(true);
        let cursor = self.cursor;
        self.pending = self
            .pending
            .take()
            .filter(|traversal| !was_dropped(cursor) && !was_dropped(traversal.origin))
            .map(|traversal| Traversal {
                origin: remap(traversal.origin),
                ..traversal
            });
        self.cursor = remap(cursor);
    }
}
