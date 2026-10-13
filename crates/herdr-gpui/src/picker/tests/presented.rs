//! A picker is shown only for the host the window presents. One that arrives
//! on another host's connection is answered `cancelled` as it arrives, as in
//! the Herdr fork's terminal client: the case is its
//! `a_picker_from_an_endpoint_not_presented_is_cancelled_as_it_arrives` in
//! `src/client/shell/tests/picks.rs`.

use super::*;

#[gpui::test]
fn a_picker_from_a_host_not_presented_is_cancelled_as_it_arrives(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);

    deliver(&view, 1, open("pick-9", false), cx);
    assert_eq!(sent(&mut second), Sent::cancelled("pick-9"));
    assert_eq!(sent(&mut first), Sent::nothing());
    assert_closed(&view, cx);

    // The presented host's picker stays open and unanswered meanwhile.
    deliver(&view, 0, open("pick-1", false), cx);
    deliver(&view, 1, open("pick-10", false), cx);
    assert_eq!(sent(&mut second), Sent::cancelled("pick-10"));
    assert_eq!(sent(&mut first), Sent::nothing());
    view.read_with(cx, |view, _| {
        assert_eq!(pick_id(shown(view).unwrap()), "pick-1")
    });
    press(&view, "escape", cx);
    assert_eq!(sent(&mut first), Sent::cancelled("pick-1"));
    assert_closed(&view, cx);
}

#[gpui::test]
fn the_host_the_window_presents_decides_not_the_connections_place(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    // The window presents its second host, as after a switch to it.
    view.update(cx, |view, _| view.selected_endpoint = 1);

    // The host it left still holds its connection, and sends a picker late.
    deliver(&view, 0, open("pick-1", false), cx);
    assert_eq!(sent(&mut first), Sent::cancelled("pick-1"));
    assert_closed(&view, cx);

    deliver(&view, 1, open("pick-2", false), cx);
    view.read_with(cx, |view, _| {
        assert_eq!(pick_id(shown(view).unwrap()), "pick-2")
    });
    press(&view, "enter", cx);
    assert_eq!(
        sent(&mut second),
        Sent::resolve("pick-2", json!({"type": "picked", "id": "home"}))
    );
    assert_eq!(sent(&mut first), Sent::nothing());
}
