//! A jump whose navigation waits for its host. Why the focus request is
//! followed waits with that navigation and nowhere else: a navigation the
//! window drops meanwhile leaves it alone, and it goes when the waiting
//! navigation is given up, so a later navigation to a pane of the same id,
//! which another host may have, is the user's own.

use super::jumps::{answer_of, answers, arrive, asked, client, jump, turn};
use super::wiring::{BOOT, answer, received};
use super::*;
use crate::{navigation::NavigationTarget, window::MockPeer};
use serde_json::{Value, json};
use std::time::Instant;

/// The `pane.focus` among `sent`, which must be for `pane`.
fn focus_request(sent: Vec<(String, Value)>, pane: &str) -> Value {
    let request = sent
        .into_iter()
        .map(|(_, request)| request)
        .find(|request| request["method"] == "pane.focus")
        .expect("a focus request");
    assert_eq!(request["params"]["pane_id"], pane);
    request
}

/// The presented host's surface comes, of the size the window asked for,
/// and its connection's reader holds the same.
fn surface_comes(view: &mut HerdrWindow) {
    presented(view);
    let mut surface = (**view.live.surface.as_ref().unwrap()).clone();
    surface.frame.width = view.options.surface_size.cols;
    surface.frame.height = view.options.surface_size.rows;
    view.live.surface = Some(Arc::new(surface));
    let endpoint = &mut view.endpoints[view.selected_endpoint];
    endpoint.live = view.live.clone();
    *endpoint.connection.inbox.lock().unwrap() = view.live.clone();
}

/// The error Local's live state holds for the status bar.
fn shown_error(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        inbox.error.clone()
    })
}

#[gpui::test]
fn a_navigation_the_window_drops_leaves_the_waiting_jump_its_answer(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    // The window is being resized: the jump's navigation waits for the
    // surface of the new size.
    view.update(cx, |view, _| view.options.surface_size.cols += 1);
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);

    // The user clicks `pane_1` meanwhile. The window is not ready and drops
    // the click: the jump's navigation is still the one that waits.
    view.update(cx, |view, cx| {
        assert!(!view.navigate(NavigationTarget::Pane("pane_1"), cx));
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.target.as_deref()),
            Some(NavigationTarget::Pane("pane_2"))
        );
    });

    // The surface comes and the focus request goes out, which Local refuses.
    view.update(cx, |view, cx| {
        surface_comes(view);
        view.poll_endpoints(cx);
    });
    let request = focus_request(received(&mut local), "pane_2");
    answer(
        &mut local,
        &view,
        0,
        &request,
        json!({"error": {"code": "pane_not_found", "message": "pane pane_2 not found"}}),
        cx,
    );
    turn(&view, start, cx);

    // The refusal is still the jump's answer, and not the user's to read.
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::NotFound)
    );
    assert_eq!(shown_error(&view, cx), None, "a tool asked, not the user");
    view.read_with(cx, |view, _| assert_eq!(view.local_error, None));
}

/// A jump to Build's `pane_1`, which Local has too, whose navigation waits
/// for Build's surface. Both peers are returned: Build stays online only
/// while its peer lives.
fn jump_waits_for_build(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    let (local, remote, view, cx) = client(cx);
    arrive(&view, 0, jump("jump-1", &host_id(1), BOOT, "pane_1"), cx);
    turn(&view, Instant::now(), cx);
    view.update(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert!(view.pending_navigation.is_some());
        // Build's reader holds what the window does, for the polls to come.
        let build = &view.endpoints[1];
        *build.connection.inbox.lock().unwrap() = build.live.clone();
    });
    (local, remote, view, cx)
}

/// Local refuses the focus request for its `pane_1`. The refusal is the
/// user's to read, and says nothing about Build's pane: the jump stays open.
fn local_refuses_the_user(
    local: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) {
    let request = focus_request(received(local), "pane_1");
    answer(
        local,
        view,
        0,
        &request,
        json!({"error": {"code": "pane_focus_failed", "message": "the pane cannot take focus"}}),
        cx,
    );
    turn(view, Instant::now(), cx);
    let error = shown_error(view, cx).expect("the user's navigation failed");
    assert!(error.contains("the pane cannot take focus"), "{error}");
    assert_eq!(answers(&asked(local, view, 0, cx)), []);
}

#[gpui::test]
fn a_navigation_after_the_jumps_host_was_left_is_the_users_own(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = jump_waits_for_build(cx);

    // The user goes back to Local, which gives that navigation up, and then
    // clicks Local's `pane_1`.
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint(crate::endpoint::LOCAL, cx));
        assert!(view.pending_navigation.is_none());
        surface_comes(view);
        assert!(view.navigate(NavigationTarget::Pane("pane_1"), cx));
    });
    local_refuses_the_user(&mut local, &view, cx);
}

#[gpui::test]
fn a_pane_of_the_same_id_picked_on_another_host_is_the_users_own(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = jump_waits_for_build(cx);

    // The user picks Local's `pane_1` in the sidebar, which leaves Build in
    // the same move. That navigation waits for Local's surface in turn.
    view.update(cx, |view, cx| {
        let local = crate::endpoint::LOCAL;
        assert!(view.navigate_endpoint(local, NavigationTarget::Pane("pane_1"), cx));
        assert_eq!(view.selected_endpoint, 0);
        surface_comes(view);
        view.poll_endpoints(cx);
    });
    local_refuses_the_user(&mut local, &view, cx);
}
