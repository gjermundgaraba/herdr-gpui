//! A history that holds the same pane twice, as going back and forth between
//! two panes leaves it: a pane equal to a step's target is the one the step
//! leaves until the step's own focus request is out, and what becomes of an
//! earlier step's request is not a later step's to an equal pane.

use super::traversal::back_and_send;
use super::visits::{LOCAL, REMOTE, observe_local, online, panes};
use super::*;
use crate::navigation::NavigationTarget;

/// Local `a`, `b`, `a`, the cursor on the last.
fn there_and_back() -> History {
    let mut history = History::default();
    for pane_id in ["a", "b", "a"] {
        observe_local(&mut history, pane_id);
    }
    history
}

#[test]
fn the_pane_a_chained_step_leaves_does_not_confirm_it() {
    let mut history = there_and_back();
    // Back to `b`, and back again to the first `a` before `b` shows: the
    // second step waits behind the focus the first one asked for.
    let first = back_and_send(&mut history);
    assert_eq!(first.visit.pane_id, "b");
    let second = history.back(online).unwrap();
    assert_eq!(history.cursor, 0);

    // Newer snapshots still show the `a` the steps left.
    observe_local(&mut history, "a");
    observe_local(&mut history, "a");
    assert_eq!(history.traversal_step().as_ref(), Some(&second));
    // The first step's answer says nothing about the second.
    history.traversal_accepted(&first);
    observe_local(&mut history, "a");
    assert_eq!(history.traversal_step().as_ref(), Some(&second));

    // `b` shows, which lets the second request out; then `a` shows.
    observe_local(&mut history, "b");
    history.traversal_sent(&second);
    observe_local(&mut history, "b");
    assert_eq!(history.traversal_step().as_ref(), Some(&second));
    observe_local(&mut history, "a");

    assert_eq!(history.traversal_step(), None);
    assert_eq!(panes(&history), ["a", "b", "a"]);
    assert_eq!(history.cursor, 0);
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "b");
}

#[test]
fn a_chained_step_forward_is_not_confirmed_by_the_pane_it_leaves_either() {
    let mut history = there_and_back();
    for _ in 0..2 {
        back_and_send(&mut history);
    }
    observe_local(&mut history, "a");
    assert_eq!(history.cursor, 0);

    let first = history.forward(online).unwrap();
    history.traversal_sent(&first);
    let second = history.forward(online).unwrap();
    observe_local(&mut history, "a");
    assert_eq!(history.traversal_step().as_ref(), Some(&second));
    observe_local(&mut history, "b");
    history.traversal_sent(&second);
    observe_local(&mut history, "a");

    assert_eq!(history.traversal_step(), None);
    assert_eq!(panes(&history), ["a", "b", "a"]);
    assert_eq!(history.cursor, 2);
    assert_eq!(history.back(online).unwrap().visit.pane_id, "b");
}

#[test]
fn a_step_past_an_offline_host_to_an_equal_pane_waits_for_its_request() {
    // Local `a`, Remote `x`, Local `a`. Remote is away, so Back leads to the
    // first `a`, which equals the pane on screen.
    let mut history = History::default();
    observe_local(&mut history, "a");
    history.observe(REMOTE, "rboot", Some("x"));
    observe_local(&mut history, "a");
    let target = history.back(|id| id == LOCAL).unwrap();
    assert_eq!(history.cursor, 0);

    observe_local(&mut history, "a");
    assert_eq!(history.traversal_step().as_ref(), Some(&target));
    history.traversal_sent(&target);
    observe_local(&mut history, "a");

    assert_eq!(history.traversal_step(), None);
    assert_eq!(history.cursor, 0);
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "x");
}

/// Local `p1`, `p2`, `p1`, `p2` and three quick steps back from the last:
/// the first, whose request is out, and the third lead to equal visits.
fn three_steps_back() -> (History, [Step; 3]) {
    let mut history = History::default();
    for pane_id in ["p1", "p2", "p1", "p2"] {
        observe_local(&mut history, pane_id);
    }
    let first = back_and_send(&mut history);
    let second = history.back(online).unwrap();
    let third = history.back(online).unwrap();
    assert_eq!(first.visit, third.visit);
    assert_ne!(first, third);
    (history, [first, second, third])
}

#[test]
fn an_earlier_steps_answer_or_failure_is_not_that_of_a_later_step_to_an_equal_pane() {
    let (mut history, [first, second, third]) = three_steps_back();
    // The first step is accepted and its `p1` shows, and the second one
    // fails: neither is the third step's.
    history.traversal_accepted(&first);
    observe_local(&mut history, "p1");
    history.traversal_failed(&second, false);
    assert_eq!(history.cursor, 0);
    assert_eq!(history.traversal_step().as_ref(), Some(&third));

    history.traversal_sent(&third);
    observe_local(&mut history, "p1");
    assert_eq!(panes(&history), ["p1", "p2", "p1", "p2"]);
    assert_eq!(history.cursor, 0);
    assert_eq!(history.traversal_step(), None);
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "p2");
}

#[test]
fn an_earlier_steps_refusal_or_acceptance_leaves_a_later_step_as_it_was() {
    // Refused: the third step is not taken back.
    let (mut history, [first, _, third]) = three_steps_back();
    history.traversal_failed(&first, false);
    assert_eq!(history.cursor, 0);
    assert_eq!(history.traversal_step().as_ref(), Some(&third));

    // Accepted, after the third step's own request went out: the third is
    // not an accepted step for it, so abandoned it returns to its origin.
    let (mut history, [first, _, third]) = three_steps_back();
    history.traversal_sent(&third);
    history.traversal_accepted(&first);
    history.traversal_abandoned();
    assert_eq!(history.cursor, 3);
}

#[gpui::test]
fn the_refusal_of_an_earlier_step_does_not_take_back_a_later_one_to_an_equal_pane(
    cx: &mut gpui::TestAppContext,
) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    for pane in ["p2", "p1", "p2"] {
        visit(&view, pane, cx);
    }
    let recorded = vec![local("p1"), local("p2"), local("p1"), local("p2")];
    assert_eq!(visits(&view, cx), (recorded.clone(), 3));
    requests(&mut peer);
    let pending = |view: &HerdrWindow| view.history.history.traversal_step().is_some();

    // Three quick steps back. Only the first one's request is out, to the
    // second `p1`, and the host refuses it.
    for _ in 0..3 {
        step(&view, Command::Back, cx);
    }
    assert_eq!(visits(&view, cx), (recorded.clone(), 0));
    let asked = exchange(&mut peer, Some("pane_focus_failed"));
    let refused: Vec<_> = asked
        .iter()
        .filter(|asked| asked.method == "pane.focus")
        .map(|asked| asked.params["pane_id"].clone())
        .collect();
    assert_eq!(refused, ["p1"]);
    answer(&view, 0, asked, cx);

    // The third step, to the first `p1`, is still on its way.
    view.read_with(cx, |view, _| assert!(pending(view)));
    assert_eq!(visits(&view, cx), (recorded.clone(), 0));

    // Input is ready again with `p2` still shown: the third step's request
    // goes out, and its pane shows.
    arrive(&view, "p2", cx);
    assert_eq!(focused(&mut peer), ["p1"]);
    visit(&view, "p1", cx);
    view.read_with(cx, |view, _| assert!(!pending(view)));
    assert_eq!(visits(&view, cx), (recorded, 0));
}

#[gpui::test]
fn a_second_step_is_not_confirmed_by_the_pane_the_first_one_leaves(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    visit(&view, "p2", cx);
    visit(&view, "p1", cx);
    let recorded = vec![local("p1"), local("p2"), local("p1")];
    assert_eq!(visits(&view, cx), (recorded.clone(), 2));
    requests(&mut peer);
    let target = |view: &HerdrWindow| view.history.history.traversal_step();

    // Back to `p2`, whose request goes out, and Back again before `p2`
    // shows: the second step waits behind the focus the first one asked for.
    step(&view, Command::Back, cx);
    step(&view, Command::Back, cx);
    assert_eq!(focused(&mut peer), ["p2"]);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.target.as_deref()),
            Some(NavigationTarget::Pane("p1"))
        );
    });
    assert_eq!(visits(&view, cx), (recorded.clone(), 0));

    // Something else on the host changes: newer snapshots, `p1` still
    // focused. It is the `p1` the steps left, not the one they lead to.
    for _ in 0..2 {
        view.update(cx, |view, _| {
            let next = (**view.live.snapshot.as_ref().unwrap()).clone();
            view.live.snapshot = Some(Arc::new(next));
            view.sync_history();
            assert!(target(view).is_some());
        });
        assert_eq!(visits(&view, cx), (recorded.clone(), 0));
    }

    // `p2` shows: the second step's request goes out, and `p2`, which is
    // not its target, is no visit.
    arrive(&view, "p2", cx);
    assert_eq!(focused(&mut peer), ["p1"]);
    view.read_with(cx, |view, _| assert!(target(view).is_some()));
    assert_eq!(visits(&view, cx), (recorded.clone(), 0));

    // `p1` shows: the step arrived at the first visit, with both ahead.
    visit(&view, "p1", cx);
    view.read_with(cx, |view, _| assert!(target(view).is_none()));
    assert_eq!(visits(&view, cx), (recorded, 0));
    step(&view, Command::Forward, cx);
    assert_eq!(focused(&mut peer), ["p2"]);
}
