//! Every way a picker goes away either answers `cancelled` or, when the
//! daemon has already given up on it, answers nothing at all.

use super::*;
use crate::palette::Filter;

#[gpui::test]
fn escape_and_every_other_way_out_of_the_page_answer_cancelled(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);

    deliver(&view, 0, open("pick-1", true), cx);
    redraw(cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(sent(&mut first), Sent::cancelled("pick-1"));
    assert_closed(&view, cx);

    // Dismissing the menu, as a click outside its panel does.
    deliver(&view, 0, open("pick-2", true), cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    assert_eq!(sent(&mut first), Sent::cancelled("pick-2"));
    assert_closed(&view, cx);

    // Another page taking the menu.
    deliver(&view, 0, open("pick-3", true), cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_palette(Filter::All, window, cx)));
    assert_eq!(sent(&mut first), Sent::cancelled("pick-3"));
    view.read_with(cx, |view, _| {
        assert!(view.fork.picker.is_none());
        assert_eq!(view.menu.page, Some(Page::Palette));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));

    // The menu reset behind the picker's back, as replacing the selected
    // connection does: the next poll notices and answers.
    deliver(&view, 0, open("pick-4", true), cx);
    view.update(cx, |view, _| view.menu.reset());
    assert_eq!(sent(&mut first), Sent::nothing());
    poll(&view, cx);
    assert_eq!(sent(&mut first), Sent::cancelled("pick-4"));
    assert_closed(&view, cx);

    // None of it was said to the other connection.
    assert_eq!(sent(&mut second), Sent::nothing());
}

#[gpui::test]
fn a_close_for_the_open_picker_dismisses_it_without_an_answer(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    // Another picker's close, or this id on a connection that did not ask,
    // is not this picker's.
    arrive(&view, 0, [close("pick-9")], cx);
    arrive(&view, 1, [close("pick-1")], cx);
    assert!(view.read_with(cx, |view, _| shown(view).is_some()));
    arrive(&view, 0, [close("pick-1")], cx);
    assert_closed(&view, cx);
    assert_eq!(sent(&mut second), Sent::nothing());
    assert_eq!(sent(&mut first), Sent::nothing());

    // A picker opened and closed before the window looked is never shown.
    arrive(
        &view,
        0,
        [opening(open("pick-2", true)), close("pick-2")],
        cx,
    );
    assert_closed(&view, cx);
    assert_eq!(sent(&mut first), Sent::nothing());
}

#[gpui::test]
fn a_lost_connection_or_a_new_boot_dismisses_without_an_answer(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);

    // A new daemon boot on the connection that asked.
    deliver(&view, 0, open("pick-1", true), cx);
    arrive(&view, 0, [snapshot("boot-v2")], cx);
    assert_closed(&view, cx);
    view.update(cx, |view, _| {
        feed(&view.endpoints[0].connection, snapshot(BOOT))
    });

    // The other connection restarting or dropping is not this picker's.
    deliver(&view, 0, open("pick-2", true), cx);
    arrive(&view, 1, [snapshot("boot-v2"), disconnected()], cx);
    assert!(view.read_with(cx, |view, _| shown(view).is_some()));

    // The connection that asked reports it is gone.
    arrive(&view, 0, [disconnected()], cx);
    assert_closed(&view, cx);
    assert_eq!(sent(&mut second), Sent::nothing());
    assert_eq!(sent(&mut first), Sent::nothing());
}

#[gpui::test]
fn a_removed_or_replaced_connection_takes_its_picker_with_it(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    // The connection leaves the window while its handle still works, so an
    // answer sent by mistake would reach the peer.
    deliver(&view, 0, open("pick-1", true), cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let other = ConnectionBridge::new(ConnectTarget::Socket("/unused.sock".into()));
            let own = std::mem::replace(&mut view.endpoints[0].connection, other);
            view.poll_picker(window, cx);
            assert!(view.fork.picker.is_none());
            view.endpoints[0].connection = own;
        })
    });
    assert_closed(&view, cx);
    assert_eq!(sent(&mut first), Sent::nothing());

    // Detaching replaces the bridge's mailboxes before any event says so.
    deliver(&view, 0, open("pick-2", true), cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints[0].connection.detach(false);
            view.poll_picker(window, cx);
        })
    });
    assert_closed(&view, cx);
    assert!(!picker_error(&view, cx));
}

#[gpui::test]
fn a_picker_arriving_over_another_page_is_cancelled_and_leaves_it_alone(
    cx: &mut gpui::TestAppContext,
) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_palette(Filter::All, window, cx)));
    deliver(&view, 0, open("pick-1", true), cx);
    assert_eq!(sent(&mut first), Sent::cancelled("pick-1"));
    view.read_with(cx, |view, _| {
        assert!(view.fork.picker.is_none());
        assert_eq!(view.menu.page, Some(Page::Palette));
        assert!(view.menu.palette.is_some());
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));

    // Another host's picker does not replace the one being answered.
    deliver(&view, 0, open("pick-2", true), cx);
    deliver(&view, 1, open("pick-3", true), cx);
    assert_eq!(sent(&mut second), Sent::cancelled("pick-3"));
    assert_eq!(sent(&mut first), Sent::nothing());
    view.read_with(cx, |view, _| {
        assert_eq!(pick_id(shown(view).unwrap()), "pick-2")
    });
    // The cancelled picker's late close is not the shown picker's.
    arrive(&view, 1, [close("pick-3")], cx);
    view.read_with(cx, |view, _| {
        assert_eq!(pick_id(shown(view).unwrap()), "pick-2")
    });
    press(&view, "enter", cx);
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-2", json!({"type": "picked", "id": "home"}))
    );
}
