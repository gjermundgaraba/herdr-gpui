//! A pending traversal, with no window: only its target is taken, once its
//! focus request is out, and the three other ways it ends. The cases of the
//! Herdr fork's `src/client/shell/history.rs`.

use super::visits::{LOCAL, REMOTE, observe_local, online, panes};
use super::*;

/// Local `p1`, Remote `r5`, then Local `p1` and `p2`, the cursor on the last.
fn across_hosts() -> History {
    let mut history = History::default();
    observe_local(&mut history, "p1");
    history.observe(REMOTE, "rboot", Some("r5"));
    observe_local(&mut history, "p1");
    observe_local(&mut history, "p2");
    history
}

fn at(history: &History) -> (&str, &str) {
    let visit = &history.visits[history.cursor];
    (visit.endpoint_id.as_str(), visit.pane_id.as_str())
}

/// One step back whose focus request goes out at once, as it does on a host
/// that is presented and ready.
pub(super) fn back_and_send(history: &mut History) -> Step {
    let target = history.back(online).unwrap();
    history.traversal_sent(&target);
    target
}

#[test]
fn what_the_arrival_host_last_focused_is_not_a_visit() {
    let mut history = across_hosts();
    assert_eq!(back_and_send(&mut history).visit.pane_id, "p1");
    observe_local(&mut history, "p1");
    assert_eq!(back_and_send(&mut history).visit.pane_id, "r5");
    history.observe(REMOTE, "rboot", Some("r5"));

    // Back to Local `p1`. Local is presented with the snapshot it last
    // sent, and something focused `p9` there while the window was away:
    // a pane that is neither the target nor between it and the origin.
    let target = history.back(online).unwrap();
    assert_eq!(target.visit.pane_id, "p1");
    observe_local(&mut history, "p9");
    observe_local(&mut history, "p9");
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);
    assert_eq!(history.cursor, 0);
    assert!(history.traversal_step().is_some());

    // Local's surface comes, the focus request goes out, and it lands.
    history.traversal_sent(&target);
    observe_local(&mut history, "p1");
    assert!(history.traversal_step().is_none());
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "r5");
}

#[test]
fn a_pending_traversal_names_its_target() {
    let mut history = across_hosts();
    assert_eq!(history.traversal_step(), None);
    let target = history.back(online).unwrap();
    assert_eq!(history.traversal_step().as_ref(), Some(&target));
    // A second step before the first showed: the target is the newer one.
    let target = history.back(online).unwrap();
    assert_eq!(target.visit.endpoint_id, REMOTE);
    assert_eq!(history.traversal_step().as_ref(), Some(&target));
    history.traversal_sent(&target);
    history.observe(REMOTE, "rboot", Some("r5"));
    assert_eq!(history.traversal_step(), None);
}

#[test]
fn an_abandoned_traversal_returns_to_its_origin_unless_its_host_accepted_it() {
    // Never sent, and sent but not answered: neither arrived anywhere.
    for sent in [false, true] {
        let mut history = across_hosts();
        history.back(online).unwrap();
        let target = history.back(online).unwrap();
        assert_eq!(at(&history), (REMOTE, "r5"));
        if sent {
            history.traversal_sent(&target);
        }
        // The user went to `r7` before `r5` showed: not a visit while the
        // step is pending, however often it is observed.
        history.observe(REMOTE, "rboot", Some("r7"));
        history.observe(REMOTE, "rboot", Some("r7"));
        assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);

        history.traversal_abandoned();
        assert_eq!(history.traversal_step(), None);
        // Back where the steps began, with nothing lost.
        assert_eq!(at(&history), (LOCAL, "p2"));
        assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);

        // Now `r7` is a visit, after the pane the steps left.
        history.observe(REMOTE, "rboot", Some("r7"));
        assert_eq!(panes(&history), ["p1", "r5", "p1", "p2", "r7"]);
        assert_eq!(history.back(online).unwrap().visit.pane_id, "p2");
    }
}

#[test]
fn an_abandoned_traversal_its_host_accepted_keeps_its_cursor() {
    let mut history = across_hosts();
    history.back(online).unwrap();
    let target = history.back(online).unwrap();
    history.traversal_sent(&target);
    history.traversal_accepted(&target);
    // The host focused `r5` and then `r7`, and only `r7` was observed.
    history.observe(REMOTE, "rboot", Some("r7"));
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);
    assert!(history.traversal_step().is_some());

    history.traversal_abandoned();
    assert_eq!(at(&history), (REMOTE, "r5"));
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);

    // `r7` is a visit after `r5`, and like any visit from the middle it
    // drops what lay ahead.
    history.observe(REMOTE, "rboot", Some("r7"));
    assert_eq!(panes(&history), ["p1", "r5", "r7"]);
    assert_eq!(at(&history), (REMOTE, "r7"));
    assert_eq!(history.back(online).unwrap().visit.pane_id, "r5");
}

#[test]
fn only_the_pending_steps_own_request_counts() {
    let mut history = across_hosts();
    // Two quick steps back: what became of the first one's request says
    // nothing about the second, which has not been sent.
    let first = back_and_send(&mut history);
    let second = history.back(online).unwrap();
    history.traversal_accepted(&first);
    history.traversal_abandoned();
    assert_eq!(at(&history), (LOCAL, "p2"));

    // Nor is a request accepted before it was sent.
    history.back(online).unwrap();
    history.back(online).unwrap();
    history.traversal_accepted(&second);
    history.traversal_abandoned();
    assert_eq!(at(&history), (LOCAL, "p2"));
}

#[test]
fn abandoning_an_accepted_traversal_whose_target_shows_is_arriving() {
    let mut history = across_hosts();
    let target = back_and_send(&mut history);
    history.traversal_accepted(&target);
    history.traversal_abandoned();
    observe_local(&mut history, "p1");
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);
    assert_eq!(history.cursor, 2);
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "p2");
}

#[test]
fn a_failed_traversal_returns_and_what_it_left_unrecorded_is_then_a_visit() {
    let mut history = across_hosts();
    history.back(online).unwrap();
    let target = history.back(online).unwrap();
    history.observe(REMOTE, "rboot", Some("r7"));
    history.traversal_failed(&target, false);
    // Back where the steps began, with nothing lost.
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2"]);
    assert_eq!(at(&history), (LOCAL, "p2"));

    history.observe(REMOTE, "rboot", Some("r7"));
    assert_eq!(panes(&history), ["p1", "r5", "p1", "p2", "r7"]);
}

#[test]
fn a_target_that_closes_ends_its_traversal() {
    let mut history = History::default();
    for pane_id in ["p1", "p2", "p3"] {
        observe_local(&mut history, pane_id);
    }
    assert_eq!(history.back(online).unwrap().visit.pane_id, "p2");
    // The pane closed before it could show; the host focused `p4` instead.
    history.observe_panes(LOCAL, "boot", ["p1", "p3", "p4"]);
    assert_eq!(history.traversal_step(), None);
    assert_eq!(panes(&history), ["p1", "p3"]);

    observe_local(&mut history, "p4");
    assert_eq!(panes(&history), ["p1", "p4"]);
    assert_eq!(history.cursor, 1);
}
