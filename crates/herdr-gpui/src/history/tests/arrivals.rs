//! A step whose pane has not shown yet: what the host shows meanwhile is not
//! a visit, least of all the pane a host the step arrives at last had
//! focused, and a step that is no longer under way does not hold the history
//! up.

use super::*;
use crate::{endpoint::LOCAL, navigation::NavigationTarget};

/// Local `p1`, Build `r1`, then Local `p1` and `p2`. While the window is on
/// Local, something else focuses `r2` on Build.
fn build_focused_elsewhere(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.sync_history());
    present(view, 1, cx);
    present(view, 0, cx);
    visit(view, "p2", cx);
    view.update(cx, |view, _| {
        let build = &mut view.endpoints[1];
        build.live.snapshot = Some(Arc::new(snapshot(&["r1", "r2"], "r2")));
        *build.connection.inbox.lock().unwrap() = build.live.clone();
        view.sync_history();
    });
    assert_eq!(visits(view, cx), (recorded(), 3));
}

fn recorded() -> Vec<(String, String)> {
    vec![local("p1"), build("r1"), local("p1"), local("p2")]
}

/// Two steps back from Local `p2`: to Local `p1`, which shows, and on to
/// Build `r1`, whose navigation waits for Build's surface.
fn step_back_to_build(peer: &mut MockPeer, view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    requests(peer);
    step(view, Command::Back, cx);
    assert_eq!(focused(peer), ["p1"]);
    visit(view, "p1", cx);
    step(view, Command::Back, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.target.as_deref()),
            Some(NavigationTarget::Pane("r1"))
        );
    });
}

fn pending(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        view.history.history.traversal_step().is_some()
    })
}

/// Local lets go of its surface and Build's comes, Build as it last was:
/// the navigation of the step there, which waited for that, goes out.
fn builds_surface_comes(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, cx| view.poll_endpoints(cx));
    let released = requests(peer);
    view.update(cx, |view, cx| {
        let local = view.endpoints[0].connection.mailboxes();
        for asked in released {
            local.deliver(asked.answer);
        }
        // Local has let go, so Build is asked for its surface.
        view.poll_endpoints(cx);
        assert!(view.endpoints[1].surface_requested());
        let held = (**view.endpoints[1].live.snapshot.as_ref().unwrap()).clone();
        surface_comes(view, held);
        view.poll_endpoints(cx);
        view.sync_history();
    });
}

#[gpui::test]
fn the_pane_a_host_last_focused_is_not_a_visit_on_arrival(cx: &mut gpui::TestAppContext) {
    let (mut peer, mut remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);

    // Build is presented as it last was, `r2` focused, tick after tick,
    // until the step's focus request gets there.
    for _ in 0..3 {
        view.update(cx, |view, _| view.sync_history());
        assert_eq!(visits(&view, cx), (recorded(), 1));
    }
    assert!(pending(&view, cx));
    assert!(focused(&mut remote).is_empty());
    // Build's surface comes and the request goes out; `r2` is still what
    // Build shows.
    builds_surface_comes(&mut peer, &view, cx);
    assert_eq!(focused(&mut remote), ["r1"]);
    assert!(pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 1));

    // The pane shows: the step is confirmed and nothing ahead was lost.
    visit(&view, "r1", cx);
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 1));
    step(&view, Command::Forward, cx);
    view.update(cx, |view, _| view.sync_history());
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 0));
    assert_eq!(visits(&view, cx), (recorded(), 2));
}

#[gpui::test]
fn going_elsewhere_on_the_arrival_host_ends_the_step(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);
    view.update(cx, |view, _| view.sync_history());
    assert_eq!(visits(&view, cx), (recorded(), 1));

    // The user picks `r2` before `r1` was asked for: the navigation takes
    // the step's place.
    view.update(cx, |view, cx| {
        assert!(view.navigate_endpoint(BUILD, NavigationTarget::Pane("r2"), cx));
        view.sync_history();
    });

    // The step never arrived, so the cursor is back on Local `p1`, where
    // it began, and what the window presents is a visit from there on.
    assert!(!pending(&view, cx));
    let mut after = recorded();
    after.truncate(3);
    after.push(build("r2"));
    assert_eq!(visits(&view, cx), (after, 3));
}

#[gpui::test]
fn selecting_another_host_ends_the_step(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);
    view.update(cx, |view, _| view.sync_history());

    // The user selects Local before Build's surface came.
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint(LOCAL, cx));
        view.sync_history();
    });

    // The step never happened: the cursor is back on Local `p1`, which is
    // what the window presents again, and Forward is where it was.
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 2));
    // And Local's next pane is recorded like any other.
    view.update(cx, |view, _| {
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.focused_pane_id = Some("p3".into());
        view.live.snapshot = Some(Arc::new(next));
        view.sync_history();
    });
    let mut after = recorded();
    after.truncate(3);
    after.push(local("p3"));
    assert_eq!(visits(&view, cx), (after, 3));
}

#[gpui::test]
fn leaving_the_host_before_it_answers_takes_the_step_back(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    visit(&view, "p2", cx);
    visit(&view, "p3", cx);
    let three = vec![local("p1"), local("p2"), local("p3")];
    requests(&mut peer);

    // Back to `p2`: the request is out, and Local has not answered it when
    // the user selects Build.
    step(&view, Command::Back, cx);
    assert_eq!(visits(&view, cx), (three.clone(), 1));
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint(BUILD, cx));
        view.sync_history();
    });

    // Nothing says `p2` ever had the focus, so the cursor is back on `p3`,
    // and Build's pane is a visit after it.
    assert!(!pending(&view, cx));
    let mut after = three;
    after.push(build("r1"));
    assert_eq!(visits(&view, cx), (after, 3));
}

#[gpui::test]
fn a_host_whose_surface_never_comes_takes_the_step_back(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);
    view.update(cx, |view, _| view.sync_history());
    assert_eq!(visits(&view, cx), (recorded(), 1));

    // Build is asked for its surface, which never comes. Its time runs out,
    // and the window goes back to Local, where the step began.
    view.update(cx, |view, cx| {
        view.poll_endpoints(cx);
        assert_eq!(view.selected_endpoint, 1);
        view.sync_history();
        view.activation_deadline = Some(std::time::Instant::now());
        view.poll_endpoints(cx);
        assert_eq!(view.selected_endpoint, 0);
        view.sync_history();
    });

    // The step never happened: nothing is lost, and Forward is where it was.
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 2));
}

#[gpui::test]
fn a_step_that_could_not_be_sent_is_taken_back(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    visit(&view, "p2", cx);
    visit(&view, "p3", cx);
    let three = vec![local("p1"), local("p2"), local("p3")];
    step(&view, Command::Back, cx);
    visit(&view, "p2", cx);
    assert_eq!(visits(&view, cx), (three.clone(), 1));

    // The connection's command queue is full: the focus request of the next
    // step is not taken, and nothing of it is on its way.
    requests(&mut peer);
    let _held = peer.stall_commands(BOOT);
    step(&view, Command::Back, cx);
    assert_eq!(visits(&view, cx), (three.clone(), 0));

    // One window tick: the history first, then the answers.
    view.update(cx, |view, cx| {
        view.sync_history();
        view.tick_client_contract(std::time::Instant::now(), cx);
        view.sync_history();
    });
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (three, 1));
}

#[gpui::test]
fn a_focus_elsewhere_waits_for_the_step_whose_request_is_out(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    visit(&view, "p2", cx);
    visit(&view, "p3", cx);
    let three = vec![local("p1"), local("p2"), local("p3")];
    requests(&mut peer);

    step(&view, Command::Back, cx);
    // Something else focuses `p1`, which is not between `p3` and `p2`,
    // before the host has answered the step.
    visit(&view, "p1", cx);
    assert_eq!(visits(&view, cx), (three.clone(), 1));
    assert!(pending(&view, cx));

    // The host accepts the request, and the window holds nothing more of
    // the step, whose pane never showed. It had the focus all the same, so
    // the cursor stays on it, and `p1` is a visit from then on.
    let asked = requests(&mut peer);
    assert!(asked.iter().any(|asked| asked.method == "pane.focus"));
    answer(&view, 0, asked, cx);
    // Accepted is not shown: the step is still the snapshot's to confirm.
    assert!(pending(&view, cx));
    view.update(cx, |view, _| view.sync_history());
    assert!(!pending(&view, cx));
    assert_eq!(
        visits(&view, cx),
        (vec![local("p1"), local("p2"), local("p1")], 2)
    );
}

#[gpui::test]
fn a_navigation_the_window_drops_leaves_the_waiting_step_followed(cx: &mut gpui::TestAppContext) {
    let (mut peer, mut remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);

    // Last Pane, a same-host agent or workspace step, or a pane click asks
    // for `r2` before Build's surface came. The window is not ready and
    // drops it: the step's own navigation is still the one that waits.
    view.update(cx, |view, cx| {
        assert!(!view.navigate(NavigationTarget::Pane("r2"), cx));
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.target.as_deref()),
            Some(NavigationTarget::Pane("r1"))
        );
        view.sync_history();
    });
    assert!(pending(&view, cx));

    // So its focus request is still the step's: the pane showing confirms
    // it, and nothing ahead was lost.
    builds_surface_comes(&mut peer, &view, cx);
    assert_eq!(focused(&mut remote), ["r1"]);
    assert!(pending(&view, cx));
    visit(&view, "r1", cx);
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 1));
}

#[gpui::test]
fn asking_for_the_steps_own_pane_again_leaves_the_step_followed(cx: &mut gpui::TestAppContext) {
    let (mut peer, mut remote, view, cx) = window(cx);
    build_focused_elsewhere(&view, cx);
    step_back_to_build(&mut peer, &view, cx);

    // The user picks `r1` in the sidebar before Build's surface came. That
    // is the navigation that waits, asked for again, and still the step's.
    view.update(cx, |view, cx| {
        assert!(view.navigate_endpoint(BUILD, NavigationTarget::Pane("r1"), cx));
        view.sync_history();
    });
    assert!(pending(&view, cx));

    builds_surface_comes(&mut peer, &view, cx);
    assert_eq!(focused(&mut remote), ["r1"]);
    visit(&view, "r1", cx);
    assert!(!pending(&view, cx));
    assert_eq!(visits(&view, cx), (recorded(), 1));
}
