//! A picker's answer that the connection's full command queue will not take
//! is kept and sent on a later poll, so the plugin waiting on it is never
//! left hanging by backpressure.

use super::*;

/// What `MockPeer::stall_commands` leaves held behind the stalled request.
const HELD_COMMANDS: usize = 65;

/// The requests `peer` reads until a `ui.pick.resolve`, each answered; the
/// window polls after every answer, which is when a freed slot takes the
/// owed answer. Returns the answer's params.
fn drain_until_answered(
    peer: &mut MockPeer,
    first: Value,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> Value {
    let mut next = first;
    for _ in 0..100 {
        if next["method"] == "ui.pick.resolve" {
            return next["params"].clone();
        }
        let id = next["id"].as_str().unwrap().to_owned();
        peer.respond(BOOT, &id, &json!({"id": id, "result": {"type": "ok"}}));
        poll(view, cx);
        next = peer.request();
    }
    panic!("the answer never reached the daemon");
}

#[gpui::test]
fn a_cancellation_a_full_queue_refused_is_sent_once_it_drains(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    let blocker = first.stall_commands(BOOT);

    cx.simulate_keystrokes("escape");
    // The picker is gone from the screen, its answer still owed.
    assert_closed(&view, cx);
    poll(&view, cx);
    poll(&view, cx);

    assert_eq!(
        drain_until_answered(&mut first, blocker, &view, cx),
        json!({"pick_id": "pick-1", "outcome": {"type": "cancelled"}})
    );
    // Answered once, and only on the connection that asked.
    assert_eq!(sent(&mut first), Sent::nothing());
    assert_eq!(sent(&mut second), Sent::nothing());
}

#[gpui::test]
fn a_cancellation_on_arrival_a_full_queue_refused_is_sent_once_it_drains(
    cx: &mut gpui::TestAppContext,
) {
    let (mut first, mut second, view, cx) = window_with_two_connections(cx);
    let blocker = second.stall_commands(BOOT);

    // A host the window does not present asks: never shown, and the answer
    // it is owed at once has to wait for its queue.
    deliver(&view, 1, open("pick-9", true), cx);
    assert_closed(&view, cx);

    assert_eq!(
        drain_until_answered(&mut second, blocker, &view, cx),
        json!({"pick_id": "pick-9", "outcome": {"type": "cancelled"}})
    );
    assert_eq!(sent(&mut second), Sent::nothing());
    assert_eq!(sent(&mut first), Sent::nothing());
}

#[gpui::test]
fn a_choice_a_full_queue_refused_is_sent_as_chosen(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", false), cx);
    let blocker = first.stall_commands(BOOT);

    cx.simulate_keystrokes("enter");
    assert_closed(&view, cx);

    assert_eq!(
        drain_until_answered(&mut first, blocker, &view, cx),
        json!({"pick_id": "pick-1", "outcome": {"type": "picked", "id": "home"}})
    );
}

#[gpui::test]
fn an_owed_answer_goes_with_a_restarted_daemon(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window_with_two_connections(cx);
    deliver(&view, 0, open("pick-1", true), cx);
    let blocker = first.stall_commands(BOOT);
    cx.simulate_keystrokes("escape");

    // The daemon restarted meanwhile and forgot the picker.
    view.read_with(cx, |view, _| {
        feed(&view.endpoints[0].connection, snapshot("boot-v2"));
    });
    // The stalled request and every command held behind it drain, with the
    // window polling all the while; none of them is the answer.
    let mut next = blocker;
    for drained in 0..=HELD_COMMANDS {
        assert_eq!(next["method"], "pane.focus", "{next}");
        let id = next["id"].as_str().unwrap().to_owned();
        first.respond(BOOT, &id, &json!({"id": id, "result": {"type": "ok"}}));
        poll(&view, cx);
        if drained < HELD_COMMANDS {
            next = first.request();
        }
    }
    poll(&view, cx);
    assert_eq!(sent(&mut first), Sent::nothing());
}
