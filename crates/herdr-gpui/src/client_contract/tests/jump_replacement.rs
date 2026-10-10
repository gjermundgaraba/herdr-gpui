//! The newest jump wins: a jump that arrives while another is still open
//! replaces it, and the replaced one is never answered. Its daemon told the
//! caller so itself, and answers a late answer `jump_not_found`, which the
//! window keeps to itself. What the replaced jump asked for may still be on
//! its way, so the jump that replaced it always navigates. The replacement
//! cases follow the Herdr fork's `src/client/shell/tests/client_jump.rs`.

use super::jumps::{answers, arrive, asked, client, focus, focused, jump, turn};
use super::wiring::{BOOT, answer, received};
use super::*;
use crate::{client_contract::jump::NAVIGATION_SETTLE_DEADLINE, navigation::NavigationTarget};
use serde_json::json;
use std::time::{Duration, Instant};

fn only(id: &str, outcome: ClientJumpOutcome) -> Vec<(String, ClientJumpOutcome)> {
    vec![(id.to_owned(), outcome)]
}

#[gpui::test]
fn a_newer_jump_replaces_the_open_one_which_is_never_answered(cx: &mut gpui::TestAppContext) {
    let (mut local, mut remote, view, cx) = client(cx);
    let start = Instant::now();

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(focused(&sent), ["pane_2"]);
    assert!(answers(&sent).is_empty());

    // The tool asks for a pane on Build before the first one shows.
    arrive(&view, 0, jump("jump-2", &host_id(1), BOOT, "pane_7"), cx);
    turn(&view, start + Duration::from_millis(40), cx);
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 1));

    // The first jump's pane shows after all, and nobody is told.
    focus(&view, 0, "pane_2", cx);
    turn(&view, start + Duration::from_millis(80), cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    view.update(cx, |view, _| presented(view));
    focus(&view, 1, "pane_7", cx);
    turn(&view, start + Duration::from_secs(1), cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        only("jump-2", ClientJumpOutcome::Focused)
    );

    // Nor does the replaced jump time out later.
    turn(&view, start + NAVIGATION_SETTLE_DEADLINE * 2, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    assert!(answers(&asked(&mut remote, &view, 1, cx)).is_empty());
    view.read_with(cx, |view, _| assert_eq!(view.local_error, None));
}

#[gpui::test]
fn a_jump_that_replaces_another_navigates_even_to_the_pane_presented(
    cx: &mut gpui::TestAppContext,
) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    let waiting = |view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let pending = view.pending_navigation.as_ref();
            pending.map(|pending| pending.target.clone())
        })
    };
    // The window is being resized: it presents `pane_1`, and a navigation
    // waits for the surface of the new size.
    view.update(cx, |view, _| view.options.surface_size.cols += 1);

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    assert_eq!(
        waiting(&view, cx),
        Some(NavigationTarget::Pane("pane_2".into()))
    );

    // The tool asks for the pane still presented before that navigation went
    // out. Answering `focused` at once would leave the first one waiting, and
    // the window would end on `pane_2`.
    arrive(&view, 0, jump("jump-2", "local", BOOT, "pane_1"), cx);
    turn(&view, start + Duration::from_millis(40), cx);
    assert_eq!(
        waiting(&view, cx),
        Some(NavigationTarget::Pane("pane_1".into())),
        "the newest navigation took the replaced one's place"
    );
    assert!(asked(&mut local, &view, 0, cx).is_empty());

    // It is answered like any other jump: once its pane shows.
    turn(&view, start + Duration::from_millis(80), cx);
    let sent = asked(&mut local, &view, 0, cx);
    assert_eq!(answers(&sent), only("jump-2", ClientJumpOutcome::Focused));
    assert!(focused(&sent).is_empty(), "`pane_2` was never asked for");
}

#[gpui::test]
fn a_jump_that_replaces_another_waits_for_its_own_pane_to_show(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();

    // The first jump's focus request is on its way to Local.
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    assert_eq!(focused(&asked(&mut local, &view, 0, cx)), ["pane_2"]);

    // The second names the pane Local still shows focused. Its navigation
    // waits behind the first one's fence, and nothing is answered meanwhile.
    arrive(&view, 0, jump("jump-2", "local", BOOT, "pane_1"), cx);
    turn(&view, start + Duration::from_millis(40), cx);
    assert!(asked(&mut local, &view, 0, cx).is_empty());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.target.as_deref()),
            Some(NavigationTarget::Pane("pane_1"))
        );
    });

    // The first jump's pane shows, and is not the newest jump's.
    view.update(cx, |view, _| presented(view));
    focus(&view, 0, "pane_2", cx);
    turn(&view, start + Duration::from_millis(80), cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    focus(&view, 0, "pane_1", cx);
    turn(&view, start + Duration::from_secs(1), cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        only("jump-2", ClientJumpOutcome::Focused)
    );
}

#[gpui::test]
fn a_replaced_jumps_refused_focus_request_ends_no_jump(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_2"), cx);
    turn(&view, start, cx);
    let sent = received(&mut local);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1["method"], "pane.focus");

    arrive(&view, 0, jump("jump-2", &host_id(1), BOOT, "pane_7"), cx);
    turn(&view, start, cx);

    // Local refuses the first jump's focus request only now.
    answer(
        &mut local,
        &view,
        0,
        &sent[0].1,
        json!({"error": {"code": "pane_not_found", "message": "pane pane_2 not found"}}),
        cx,
    );
    turn(&view, start, cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1, "the newer jump goes on");
        assert_eq!(view.local_error, None, "a tool asked, not the user");
    });

    view.update(cx, |view, _| presented(view));
    focus(&view, 1, "pane_7", cx);
    turn(&view, start + Duration::from_secs(1), cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        only("jump-2", ClientJumpOutcome::Focused)
    );
}

#[gpui::test]
fn a_daemon_that_no_longer_knows_the_answered_jump_is_not_shown(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let now = Instant::now();

    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_1"), cx);
    turn(&view, now, cx);
    let sent = received(&mut local);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1["method"], "client.jump.resolve");

    // The daemon replaced the jump while the answer was on its way.
    answer(
        &mut local,
        &view,
        0,
        &sent[0].1,
        json!({"error": {"code": "jump_not_found", "message": "no such jump"}}),
        cx,
    );
    turn(&view, now, cx);

    assert!(asked(&mut local, &view, 0, cx).is_empty(), "not sent again");
    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        assert_eq!(inbox.error, None);
        assert_eq!(view.local_error, None);
    });
}
