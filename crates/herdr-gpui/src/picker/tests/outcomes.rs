//! A plugin's picker answers `ui.pick.resolve` with what was chosen, on the
//! connection that asked and no other.

use super::*;

#[gpui::test]
fn enter_picks_the_preselected_item_on_the_connection_that_asked(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", false), cx);
    view.read_with(cx, |view, _| {
        let picker = shown(view).unwrap();
        assert_eq!(picker.title.as_ref(), "Space group");
        assert_eq!(rows(picker), ["Work", "Home"]);
        assert_eq!(picker.list.selected(), Some(1));
        assert_eq!(pick_id(picker), "pick-1");
    });
    redraw(cx);
    press(&view, "enter", cx);
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "picked", "id": "home"}))
    );
    assert_eq!(sent(&mut second), Sent::nothing());
    assert_closed(&view, cx);
}

#[gpui::test]
fn an_unmatched_query_creates_only_when_the_picker_allows_it(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    redraw(cx);
    cx.simulate_input(" Side ");
    view.read_with(cx, |view, _| {
        assert_eq!(rows(shown(view).unwrap()), ["+ Side"]);
    });
    redraw(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "created", "text": "Side"}))
    );
    assert_closed(&view, cx);

    // Without a create row, Enter on an emptied list answers nothing and the
    // picker stays for the query to be corrected.
    deliver(&view, 0, open("pick-2", false), cx);
    redraw(cx);
    cx.simulate_input("Side");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut first), Sent::nothing());
    view.read_with(cx, |view, _| {
        let picker = shown(view).unwrap();
        assert!(picker.list.rows().is_empty());
        assert_eq!(pick_id(picker), "pick-2");
    });
    redraw(cx);
    assert!(cx.debug_bounds("picker-empty").is_some());
    cx.simulate_keystrokes("backspace backspace backspace backspace w");
    view.read_with(cx, |view, cx| {
        let picker = shown(view).unwrap();
        assert_eq!(picker.search.read(cx).text(), "w");
        assert_eq!(rows(picker), ["Work"]);
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-2", json!({"type": "picked", "id": "work"}))
    );
}

#[gpui::test]
fn clicking_a_row_picks_it(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    let mut request = open("pick-1", true);
    request.items[0].detail = Some("3 spaces".into());
    request.items[0].badge = Some("1".into());
    deliver(&view, 0, request, cx);
    redraw(cx);
    let row = cx.debug_bounds("picker-row-0").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "picked", "id": "work"}))
    );
    assert_closed(&view, cx);

    // The create row is a row like the others.
    deliver(&view, 0, open("pick-2", true), cx);
    redraw(cx);
    cx.simulate_input("o");
    redraw(cx);
    let create = cx.debug_bounds("picker-row-2").unwrap();
    cx.simulate_click(create.center(), gpui::Modifiers::none());
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-2", json!({"type": "created", "text": "o"}))
    );
}

#[gpui::test]
fn an_answer_too_long_to_send_is_reported_and_the_plugin_is_cancelled(
    cx: &mut gpui::TestAppContext,
) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    let search = view.read_with(cx, |view, _| shown(view).unwrap().search.clone());
    cx.update(|window, cx| {
        search.update(cx, |input, cx| {
            use gpui::EntityInputHandler;
            // More than the daemon takes in one command, though half of
            // what fits a frame: sent, it would cost the connection.
            let limit = herdr_client::protocol::clients::MAX_ENDPOINT_COMMAND_BYTES;
            input.replace_text_in_range(None, &"x".repeat(limit), window, cx)
        })
    });
    press(&view, "enter", cx);
    // The created text does not fit one daemon command, so the plugin hears
    // `cancelled` instead of nothing, and the window says why.
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "cancelled"}))
    );
    assert_closed(&view, cx);
    view.read_with(cx, |view, _| {
        let error = view.local_error.as_deref().unwrap();
        assert!(error.starts_with("Picker: "), "{error}");
    });
}

#[gpui::test]
fn a_stopped_connection_is_owed_no_answer_and_the_picker_still_closes(
    cx: &mut gpui::TestAppContext,
) {
    let (first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", false), cx);
    // The worker was told to stop; the daemon cancels the picker itself.
    first.client.handle.disconnect();
    press(&view, "enter", cx);
    assert_closed(&view, cx);
    let unreported = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.local_error
                .as_deref()
                .is_none_or(|error| !error.starts_with("Picker: "))
        })
    };
    assert!(unreported(cx));

    // So does one whose handle the window has already let go of.
    deliver(&view, 0, open("pick-2", false), cx);
    view.update(cx, |view, _| view.endpoints[0].connection.handle = None);
    press(&view, "escape", cx);
    assert_closed(&view, cx);
    assert!(unreported(cx));
}
