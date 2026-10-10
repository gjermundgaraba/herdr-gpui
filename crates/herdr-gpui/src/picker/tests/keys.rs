//! The picker owns the keyboard while it is open: text goes to its filter,
//! the list keys move and choose, and nothing reaches the pane beneath.

use super::*;

/// Drives the window's own key and text handlers, the route typing takes to
/// the pane when nothing isolates it.
pub(super) fn type_at_the_terminal(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            use gpui::EntityInputHandler;
            // Drawing sized the terminal; give it the frame it asked for, so
            // a menu is the only thing that can stand between it and input.
            let size = view.options.surface_size;
            let frame = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).frame;
            (frame.width, frame.height) = (size.cols, size.rows);
            assert!(view.input_ready());
            view.key_down(&key("a"), window, cx);
            view.key_down(&key("enter"), window, cx);
            view.replace_text_in_range(None, "typed", window, cx);
            view.send(ClientPaneInputEvent::Paste("pasted".into()), cx);
        })
    });
}

#[gpui::test]
fn typed_text_and_list_keys_stay_in_the_picker_and_never_reach_the_pane(
    cx: &mut gpui::TestAppContext,
) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    // Before the picker the fixture really does type into its pane, so the
    // empty input lists below mean something.
    type_at_the_terminal(&view, cx);
    let before = sent(&mut first);
    assert!(before.input.len() >= 3, "{before:?}");

    deliver(&view, 0, open("pick-1", true), cx);
    redraw(cx);
    // Through the window's real focus and key dispatch: the filter field has
    // the keyboard, and the keys it leaves alone are the list's.
    cx.simulate_input("o");
    cx.simulate_keystrokes("down down down down up ctrl-p ctrl-n ctrl-n left right a backspace");
    view.read_with(cx, |view, cx| {
        let picker = shown(view).unwrap();
        assert_eq!(picker.search.read(cx).text(), "o");
        assert_eq!(rows(picker), ["Work", "Home", "+ o"]);
        // Editing the query away and back selected the first row again.
        assert_eq!(picker.list.selected(), Some(0));
        // Nothing was composed into the terminal's own input either.
        assert!(view.marked.is_empty());
    });
    // Even handed straight to the terminal's handlers, nothing is typed.
    type_at_the_terminal(&view, cx);
    cx.simulate_keystrokes("ctrl-n ctrl-n ctrl-n ctrl-p");
    view.read_with(cx, |view, _| {
        assert_eq!(shown(view).unwrap().list.selected(), Some(1));
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "picked", "id": "home"}))
    );
    assert_eq!(sent(&mut second), Sent::nothing());
    assert_closed(&view, cx);

    // Closed, the keyboard is the pane's again, and nothing typed while the
    // picker was open was held back to arrive now.
    assert!(cx.update(|window, cx| view.read(cx).focus.is_focused(window)));
    type_at_the_terminal(&view, cx);
    let after = sent(&mut first);
    assert_eq!(after.input, before.input);
    assert!(after.requests.is_empty());
}

#[gpui::test]
fn the_selection_stops_at_both_ends_and_other_keys_are_left_to_the_filter(
    cx: &mut gpui::TestAppContext,
) {
    let (_first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", false), cx);
    let selected = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| shown(view).unwrap().list.selected().unwrap())
    };
    assert_eq!(selected(cx), 1);
    for (keystroke, expected) in [
        ("down", 1),
        ("up", 0),
        ("up", 0),
        ("ctrl-n", 1),
        ("ctrl-n", 1),
        ("ctrl-p", 0),
        ("ctrl-p", 0),
        // Only bare Control makes these list keys.
        ("n", 0),
        ("ctrl-shift-n", 0),
        ("cmd-n", 0),
        ("alt-n", 0),
        ("tab", 0),
        ("pagedown", 0),
    ] {
        press(&view, keystroke, cx);
        assert_eq!(selected(cx), expected, "{keystroke}");
        assert!(view.read_with(cx, |view, _| shown(view).is_some()));
    }
}

#[gpui::test]
fn a_composition_keeps_its_keys_until_it_is_committed(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    redraw(cx);
    let search = view.read_with(cx, |view, _| shown(view).unwrap().search.clone());
    cx.update(|window, cx| {
        search.update(cx, |input, cx| {
            use gpui::EntityInputHandler;
            input.replace_and_mark_text_in_range(None, "設定", Some(2..2), window, cx)
        })
    });
    view.read_with(cx, |view, cx| {
        let picker = shown(view).unwrap();
        assert!(picker.search.read(cx).is_composing());
        // The marked text already filters, as it does in the palette.
        assert_eq!(rows(picker), ["+ 設定"]);
        assert!(view.marked.is_empty());
    });
    // Enter commits the composition and Escape cancels it: neither is the
    // picker's while the input method has them.
    for keystroke in ["enter", "escape", "down", "ctrl-n", "up"] {
        press(&view, keystroke, cx);
        assert!(
            view.read_with(cx, |view, _| shown(view).is_some()),
            "{keystroke}"
        );
    }
    assert_eq!(sent(&mut first), Sent::nothing());
    cx.update(|window, cx| {
        search.update(cx, |input, cx| {
            use gpui::EntityInputHandler;
            input.replace_text_in_range(None, "設定", window, cx)
        })
    });
    assert!(!search.read_with(cx, |input, _| input.is_composing()));
    press(&view, "enter", cx);
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "created", "text": "設定"}))
    );
}

#[gpui::test]
fn the_picker_fits_a_narrow_window_and_scrolls_a_long_list(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window_with_two_connections(cx);
    let mut request = open("pick-1", true);
    request.title = "A very long picker title ".repeat(20);
    request.items = (0..200)
        .map(|index| UiPickItem {
            id: format!("item-{index}"),
            label: format!("item {index} with a very long label ").repeat(10),
            detail: Some("a long detail ".repeat(10)),
            badge: Some(format!("{index}")),
        })
        .collect();
    request.selected = Some("item-150".into());
    for (width, height) in [(360., 300.), (900., 700.)] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
        deliver(&view, 0, request.clone(), cx);
        redraw(cx);
        // The preselected row is scrolled into view; the first is not drawn.
        redraw(cx);
        assert!(cx.debug_bounds("picker-row-0").is_none());
        for selector in [
            "menu-panel",
            "picker-title",
            "picker-row-150",
            "picker-status",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.left() >= gpui::px(0.) && bounds.right() <= gpui::px(width),
                "{selector}: {bounds:?}"
            );
            assert!(
                bounds.top() >= gpui::px(0.) && bounds.bottom() <= gpui::px(height),
                "{selector}: {bounds:?}"
            );
        }
        press(&view, "escape", cx);
        assert_closed(&view, cx);
    }
}
