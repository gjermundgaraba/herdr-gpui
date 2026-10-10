//! A plugin may close its picker while the window's answer is on its way.
//! The daemon then replies `pick_not_found`: the expected crossing, kept out
//! of the status bar. Any other error for an answer is shown there.

use super::*;

/// Replies to the request `peer` reads next, which must be `method`, with
/// `error`, and hands the reply to the connection's mailboxes as its reader
/// does.
fn refuse(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    method: &str,
    error: Value,
    cx: &mut VisualTestContext,
) {
    let request = peer.request();
    assert_eq!(request["method"], method);
    let id = request["id"].as_str().unwrap();
    let reply = peer.respond(BOOT, id, &json!({"id": id, "error": error}));
    view.update(cx, |view, _| {
        view.endpoints[0].connection.mailboxes().deliver(reply);
    });
}

/// The daemon error the first connection's live state holds for the status bar.
fn shown_error(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        let state = view.endpoints[0].connection.inbox.lock().unwrap();
        state.error.clone()
    })
}

fn not_found() -> Value {
    json!({"code": "pick_not_found", "message": "no open picker with that id on this client"})
}

#[gpui::test]
fn an_answer_that_crossed_the_plugins_close_is_not_an_error(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    assert_eq!(shown_error(&view, cx), None);

    // Chosen and cancelled alike: the plugin closed the picker first.
    for (pick, keystroke) in [("pick-1", "enter"), ("pick-2", "escape")] {
        deliver(&view, 0, open(pick, false), cx);
        press(&view, keystroke, cx);
        assert_closed(&view, cx);
        refuse(&mut first, &view, "ui.pick.resolve", not_found(), cx);
        assert_eq!(shown_error(&view, cx), None);
    }
}

#[gpui::test]
fn any_other_error_for_an_answer_is_shown(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", false), cx);
    press(&view, "enter", cx);
    refuse(
        &mut first,
        &view,
        "ui.pick.resolve",
        json!({"code": "invalid_params", "message": "the outcome cannot be read"}),
        cx,
    );
    let error = shown_error(&view, cx).unwrap();
    assert!(error.contains("the outcome cannot be read"), "{error}");
}

#[gpui::test]
fn the_code_is_expected_only_for_a_picker_answer(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    first.client.handle.focus_pane(BOOT, "w1:p1").unwrap();
    refuse(&mut first, &view, "pane.focus", not_found(), cx);
    let error = shown_error(&view, cx).unwrap();
    assert!(error.contains("no open picker with that id"), "{error}");
}

#[gpui::test]
fn the_oldest_of_many_answers_still_queued_is_expected_to_cross(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    // The connection sends one request at a time, so every answer waits
    // behind the first. The plugin closes each picker before its answer
    // arrives, which lets it open the next one.
    for index in 0..9 {
        let id = format!("pick-{index}");
        deliver(&view, 0, open(&id, false), cx);
        press(&view, "enter", cx);
        view.update(cx, |view, _| {
            feed(&view.endpoints[0].connection, close(&id))
        });
    }
    refuse(&mut first, &view, "ui.pick.resolve", not_found(), cx);
    assert_eq!(shown_error(&view, cx), None);
}
