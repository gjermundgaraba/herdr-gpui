//! Every request the contract mailbox was told of stays known until its
//! answer or its rejection is filed, however many wait. The connection sends
//! one request at a time, so many can wait behind a slow one, and the oldest
//! is the next to be answered.

use super::jumps::{arrive, client, jump, turn};
use super::mailbox::{response, snapshot, welcome};
use super::wiring::{BOOT, answer};
use super::*;
use herdr_client::Error as ClientError;
use serde_json::json;
use std::time::Instant;

/// What one connection can hold unanswered: the 64 requests of its command
/// queue, one its worker holds back, and the one in flight.
const OUTSTANDING: usize = 66;

fn refused(id: &str, code: &str) -> ClientEvent {
    response(
        id,
        json!({"id": id, "error": {"code": code, "message": "no longer there"}}),
    )
}

#[test]
fn the_oldest_of_many_jump_answers_still_queued_is_kept_from_the_live_state() {
    let mut inbox = Inbox::default();
    for index in 0..OUTSTANDING {
        inbox.resolution_sent(format!("resolution-{index}"));
    }
    // The daemon gave up on the first jump long before its answer was sent.
    assert!(
        inbox
            .apply(refused("resolution-0", "jump_not_found"))
            .is_none()
    );
    let newest = format!("resolution-{}", OUTSTANDING - 1);
    assert!(inbox.apply(refused(&newest, "jump_not_found")).is_none());
}

#[test]
fn the_oldest_of_many_followed_requests_still_gets_its_answer() {
    let mut inbox = Inbox::default();
    for index in 0..OUTSTANDING {
        inbox.follow(format!("unread-{index}"));
    }
    assert!(inbox.follows("unread-0"));
    assert!(inbox.apply(refused("unread-0", "agent_not_idle")).is_some());
    assert_eq!(inbox.take_answer("unread-0"), Some(false));
    assert!(inbox.follows(&format!("unread-{}", OUTSTANDING - 1)));
}

#[test]
fn a_new_boot_still_knows_the_requests_the_old_one_was_sent() {
    let mut inbox = Inbox::default();
    inbox.apply(welcome(&["client.jump.resolve"]));
    inbox.apply(snapshot("boot-1"));
    inbox.resolution_sent("resolution-1".into());
    inbox.follow("unread-1".into());

    // The worker refuses what was queued for the old boot only after the
    // snapshot that names the new one, each request by its id.
    inbox.apply(snapshot("boot-2"));
    let rejected = |request: &str| ClientEvent::CommandRejected {
        request_id: Some(request.into()),
        reason: ClientError::CommandBoot,
    };
    assert!(inbox.apply(rejected("resolution-1")).is_none());
    assert!(inbox.apply(rejected("unread-1")).is_some());
    assert_eq!(inbox.take_answer("unread-1"), Some(false));
}

#[gpui::test]
fn a_window_shows_no_error_for_the_oldest_of_many_jump_answers(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let now = Instant::now();
    // Each jump asks for the pane already shown, so it is answered at once.
    // The answers wait behind the first, which the daemon has not replied
    // to, and the daemon gave up on each jump when it forwarded the next.
    for index in 0..33 {
        let id = format!("jump-{index}");
        arrive(&view, 0, jump(&id, "local", BOOT, "pane_1"), cx);
        turn(&view, now, cx);
    }

    let first = local.request();
    assert_eq!(first["method"], "client.jump.resolve");
    assert_eq!(first["params"]["jump_id"], "jump-0");
    answer(
        &mut local,
        &view,
        0,
        &first,
        json!({"error": {"code": "jump_not_found", "message": "no such jump"}}),
        cx,
    );
    turn(&view, now, cx);

    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        assert_eq!(inbox.error, None);
        assert_eq!(view.local_error, None);
    });
}
