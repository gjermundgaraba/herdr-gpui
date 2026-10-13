//! The contract mailbox's part in a jump, the jumps a daemon forwarded and
//! the acknowledgement of the answer the window sent, and in the requests the
//! window follows.

use super::mailbox::{response, snapshot, welcome};
use super::*;
use herdr_client::Error as ClientError;
use serde_json::json;

fn jump(id: &str) -> EndpointClientJump {
    EndpointClientJump {
        jump_id: id.into(),
        endpoint_id: "local".into(),
        boot_id: "boot-1".into(),
        pane_id: "pane_2".into(),
    }
}

fn refused(id: &str, error: serde_json::Value) -> ClientEvent {
    response(id, json!({"id": id, "error": error}))
}

#[test]
fn a_forwarded_jump_waits_for_the_window_and_a_newer_one_replaces_it() {
    let mut inbox = Inbox::default();
    assert!(
        inbox
            .apply(ClientEvent::ClientJump(jump("jump-1")))
            .is_none()
    );
    assert!(inbox.owes_jump_answer());
    assert_eq!(inbox.take_jump(), Some(jump("jump-1")));
    assert_eq!(inbox.take_jump(), None, "taken once");
    assert!(!inbox.owes_jump_answer());

    // The daemon gave up on the older jump when it forwarded the newer.
    for index in 2..100 {
        inbox.apply(ClientEvent::ClientJump(jump(&format!("jump-{index}"))));
    }
    assert_eq!(inbox.take_jump(), Some(jump("jump-99")));
    assert_eq!(inbox.take_jump(), None);
}

#[test]
fn an_answer_needs_a_daemon_that_takes_one_and_a_boot_to_name() {
    let mut inbox = Inbox::default();
    inbox.apply(welcome(&["client.jump.resolve"]));
    assert_eq!(inbox.resolve_boot(), None, "no boot id to name yet");
    inbox.apply(snapshot("boot-1"));
    assert_eq!(inbox.resolve_boot(), Some("boot-1"));
    assert_eq!(inbox.report_boot(), None, "it takes answers, not reports");

    let mut silent = Inbox::default();
    silent.apply(welcome(&["client.report"]));
    silent.apply(snapshot("boot-1"));
    assert_eq!(silent.resolve_boot(), None);
}

#[test]
fn a_followed_request_keeps_its_answer_for_the_window() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    assert!(inbox.follows("gpui-3"));
    assert_eq!(inbox.take_answer("gpui-3"), None, "not answered yet");

    // The live state sees the answer too.
    assert!(
        inbox
            .apply(response(
                "gpui-3",
                json!({"id": "gpui-3", "result": {"type": "ok"}})
            ))
            .is_some()
    );
    assert_eq!(inbox.take_answer("gpui-3"), Some(true));
    assert!(!inbox.follows("gpui-3"), "collected once");
    assert_eq!(inbox.take_answer("gpui-3"), None);

    // A refusal is shown like any other.
    inbox.follow("gpui-4".into());
    assert!(
        inbox
            .apply(refused(
                "gpui-4",
                json!({"code": "agent_not_idle", "message": "the agent is working"})
            ))
            .is_some()
    );
    assert_eq!(inbox.take_answer("gpui-4"), Some(false));
}

#[test]
fn a_request_the_worker_would_not_send_is_not_accepted() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    assert!(
        inbox
            .apply(ClientEvent::CommandRejected {
                request_id: Some("gpui-3".into()),
                reason: ClientError::UnsupportedMethod,
            })
            .is_some(),
        "the user's own request still reports its failure"
    );
    assert_eq!(inbox.take_answer("gpui-3"), Some(false));
}

#[test]
fn the_acknowledgement_of_a_jumps_answer_is_kept_from_the_live_state() {
    let mut inbox = Inbox::default();
    inbox.resolution_sent("gpui-5".into());
    inbox.resolution_sent("gpui-6".into());
    inbox.resolution_sent("gpui-7".into());

    assert!(
        inbox
            .apply(response(
                "gpui-5",
                json!({"id": "gpui-5", "result": {"type": "ok"}})
            ))
            .is_none()
    );
    // A newer jump replaced it, or the daemon stopped waiting, before the
    // answer came: expected, and never shown.
    assert!(
        inbox
            .apply(refused(
                "gpui-6",
                json!({"code": "jump_not_found", "message": "no such jump"})
            ))
            .is_none()
    );
    assert!(
        inbox
            .apply(ClientEvent::CommandRejected {
                request_id: Some("gpui-7".into()),
                reason: ClientError::CommandBoot,
            })
            .is_none()
    );
    // Each acknowledged once: a late duplicate is an ordinary response again.
    assert!(
        inbox
            .apply(response("gpui-5", json!({"id": "gpui-5", "result": {}})))
            .is_some()
    );
}

#[test]
fn a_lost_connection_takes_its_jumps_and_followed_requests_with_it() {
    let mut inbox = Inbox::default();
    inbox.apply(welcome(&["client.jump.resolve"]));
    inbox.apply(snapshot("boot-1"));
    inbox.apply(ClientEvent::ClientJump(jump("jump-1")));
    inbox.follow("gpui-3".into());

    inbox.apply(ClientEvent::Disconnected {
        reason: "gone".into(),
        ssh: None,
    });

    assert_eq!(inbox.take_jump(), None);
    assert!(!inbox.follows("gpui-3"));
    assert_eq!(inbox.resolve_boot(), None);
}
