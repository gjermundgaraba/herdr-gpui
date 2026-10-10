//! Back and Forward in a window: the one focus request a step sends, on
//! which host, and what a refusal does to the history. The cases follow the
//! history steps of the Herdr fork's
//! `src/client/shell/tests/endpoint_requests.rs`.

use super::*;
use crate::window::Flash;

/// Local visits `p1`, `p2`, `p3` in turn.
fn three_visits(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.sync_history());
    visit(view, "p2", cx);
    visit(view, "p3", cx);
    assert_eq!(
        visits(view, cx),
        (vec![local("p1"), local("p2"), local("p3")], 2)
    );
}

#[gpui::test]
fn back_and_forward_focus_the_neighbouring_visits(cx: &mut gpui::TestAppContext) {
    let (mut peer, mut remote, view, cx) = window(cx);
    three_visits(&view, cx);
    requests(&mut peer);

    step(&view, Command::Back, cx);
    // The request a click on that pane sends, on the host that has it.
    assert_eq!(focused(&mut peer), ["p2"]);
    // The cursor moved at once; the list is untouched.
    assert_eq!(visits(&view, cx).1, 1);

    // The snapshot that shows the pane focused confirms the step instead of
    // becoming a new visit, so Forward still has somewhere to go.
    visit(&view, "p2", cx);
    assert_eq!(
        visits(&view, cx),
        (vec![local("p1"), local("p2"), local("p3")], 1)
    );
    step(&view, Command::Forward, cx);
    assert_eq!(focused(&mut peer), ["p3"]);
    visit(&view, "p3", cx);
    step(&view, Command::Back, cx);
    visit(&view, "p2", cx);
    step(&view, Command::Back, cx);
    visit(&view, "p1", cx);
    assert_eq!(focused(&mut peer), ["p2", "p1"]);
    assert_eq!(visits(&view, cx).1, 0);

    // A fresh visit from the middle drops what lay ahead.
    visit(&view, "p3", cx);
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p3")], 1));
    assert!(requests(&mut remote).is_empty());
}

#[gpui::test]
fn a_step_past_either_end_says_so_and_sends_nothing(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    requests(&mut peer);

    for (command, text) in [
        (Command::Back, "No earlier pane in history"),
        (Command::Forward, "No later pane in history"),
    ] {
        step(&view, command, cx);
        view.read_with(cx, |view, _| {
            let flash = view.flash.as_ref().map(|(flash, _)| flash.clone());
            assert_eq!(flash, Some(Flash::warning(text)));
        });
    }
    assert!(requests(&mut peer).is_empty());
    assert_eq!(visits(&view, cx), (vec![local("p1")], 0));
}

#[gpui::test]
fn a_step_to_another_host_presents_it_with_the_pane_as_target(cx: &mut gpui::TestAppContext) {
    let (mut peer, mut remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    present(&view, 1, cx);
    assert_eq!(visits(&view, cx), (vec![local("p1"), build("r1")], 1));
    requests(&mut peer);

    step(&view, Command::Back, cx);

    // The window's own cross-host navigation: Local is presented again and
    // asked for the pane. Build is asked for nothing.
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 0));
    assert_eq!(focused(&mut peer), ["p1"]);
    assert!(focused(&mut remote).is_empty());
    assert_eq!(visits(&view, cx).1, 0);
}

#[gpui::test]
fn a_host_that_is_not_online_is_stepped_over(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    // Local `p1`, Build `r1`, then Local `p1` and `p2`.
    view.update(cx, |view, _| view.sync_history());
    present(&view, 1, cx);
    present(&view, 0, cx);
    visit(&view, "p2", cx);
    view.update(cx, |view, _| {
        let live = &mut view.endpoints[1].live;
        live.status = ConnectionStatus::Disconnected;
        live.error = Some("connection lost".into());
    });
    requests(&mut peer);

    step(&view, Command::Back, cx);
    assert_eq!(focused(&mut peer), ["p1"]);
    visit(&view, "p1", cx);
    // Build's visit is next, and Build is away: the step goes past it.
    step(&view, Command::Back, cx);
    assert_eq!(focused(&mut peer), ["p1"]);
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 0));
    // The visit itself is kept for when Build is back.
    assert_eq!(visits(&view, cx).0.len(), 4);
    assert_eq!(visits(&view, cx).1, 0);
}

#[gpui::test]
fn a_refused_step_rewinds_and_forgets_a_pane_that_is_gone(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    three_visits(&view, cx);
    requests(&mut peer);
    let refuse = |peer: &mut MockPeer,
                  view: &Entity<HerdrWindow>,
                  code: &str,
                  cx: &mut VisualTestContext| {
        let asked = exchange(peer, Some(code));
        assert_eq!(asked[0].method, "pane.focus");
        assert_eq!(asked[0].params["pane_id"], "p2");
        view.update(cx, |view, cx| {
            // Every answer, as the connection's reader hands them over.
            for asked in asked {
                view.endpoints[0]
                    .connection
                    .mailboxes()
                    .deliver(asked.answer);
            }
            view.tick_client_contract(std::time::Instant::now(), cx);
        });
    };

    // A refusal that says nothing about the pane only takes the step back.
    step(&view, Command::Back, cx);
    assert_eq!(visits(&view, cx).1, 1);
    refuse(&mut peer, &view, "pane_focus_failed", cx);
    assert_eq!(
        visits(&view, cx),
        (vec![local("p1"), local("p2"), local("p3")], 2)
    );
    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        // The user asked, so the status bar says why nothing happened.
        assert!(
            inbox
                .error
                .as_deref()
                .is_some_and(|error| error.contains("the host said no"))
        );
        // Input is not left waiting for a focus the host refused.
        assert_eq!(inbox.activation.as_ref().unwrap().focus, None);
    });

    // A pane that no longer exists is forgotten as well.
    visit(&view, "p3", cx);
    requests(&mut peer);
    step(&view, Command::Back, cx);
    refuse(&mut peer, &view, "pane_not_found", cx);
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p3")], 1));

    // The next step back goes to what is left.
    visit(&view, "p3", cx);
    requests(&mut peer);
    step(&view, Command::Back, cx);
    assert_eq!(focused(&mut peer), ["p1"]);
}

#[gpui::test]
fn a_step_under_a_menu_does_nothing(cx: &mut gpui::TestAppContext) {
    let (mut peer, _remote, view, cx) = window(cx);
    three_visits(&view, cx);
    requests(&mut peer);
    view.update(cx, |view, _| {
        view.menu.page = Some(crate::menu::Page::About)
    });

    step(&view, Command::Back, cx);

    assert!(requests(&mut peer).is_empty());
    assert_eq!(visits(&view, cx).1, 2, "the cursor did not move");
}
