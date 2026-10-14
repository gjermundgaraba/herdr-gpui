//! A connection's mailbox follows the requests the window registered to the
//! daemon's answer, and passes every event on to the live state.

use super::*;
use herdr_client::Error as ClientError;
use serde_json::json;

fn response(id: &str, body: Value) -> ClientEvent {
    ClientEvent::Response {
        request_id: id.into(),
        response: body,
    }
}

fn accepted(id: &str) -> ClientEvent {
    response(id, json!({"id": id, "result": {"type": "ok"}}))
}

fn refused(id: &str, code: &str) -> ClientEvent {
    response(
        id,
        json!({"id": id, "error": {"code": code, "message": "refused"}}),
    )
}

fn rejected(id: &str, reason: ClientError) -> ClientEvent {
    ClientEvent::CommandRejected {
        request_id: Some(id.into()),
        reason,
    }
}

#[test]
fn a_followed_request_keeps_its_acceptance_for_the_window() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    assert!(!inbox.take_accepted(), "not answered yet");

    inbox.observe(&accepted("gpui-3"));
    assert!(inbox.take_accepted());
    assert!(!inbox.take_accepted(), "collected once");

    // A refusal is shown like any other, and accepts nothing.
    inbox.follow("gpui-4".into());
    inbox.observe(&refused("gpui-4", "agent_not_idle"));
    assert!(!inbox.take_accepted());
}

#[test]
fn a_request_the_window_does_not_follow_accepts_nothing() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    inbox.observe(&accepted("gpui-2"));
    assert!(!inbox.take_accepted());

    // A followed request is answered once: a late duplicate is not followed.
    inbox.observe(&accepted("gpui-3"));
    assert!(inbox.take_accepted());
    inbox.observe(&accepted("gpui-3"));
    assert!(!inbox.take_accepted());
}

#[test]
fn a_request_the_worker_would_not_send_is_not_accepted() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    inbox.observe(&rejected("gpui-3", ClientError::UnsupportedMethod));
    assert!(!inbox.take_accepted());
    // The rejection ended the following: a late answer accepts nothing.
    inbox.observe(&accepted("gpui-3"));
    assert!(!inbox.take_accepted());
}

#[test]
fn a_new_boot_still_knows_the_requests_the_old_one_was_sent() {
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut inbox = Inbox::default();
    inbox.follow("unread-1".into());
    // The worker refuses what was queued for the old boot only after the
    // snapshot that names the new one, each request by its id.
    inbox.observe(&ClientEvent::Snapshot(std::sync::Arc::new(snapshot)));
    inbox.observe(&rejected("unread-1", ClientError::CommandBoot));
    assert!(!inbox.take_accepted());
    inbox.observe(&accepted("unread-1"));
    assert!(
        !inbox.take_accepted(),
        "the request was known until refused"
    );
}

#[test]
fn a_lost_connection_takes_its_followed_requests_with_it() {
    let mut inbox = Inbox::default();
    inbox.follow("gpui-3".into());
    inbox.observe(&accepted("gpui-3"));
    inbox.follow("gpui-4".into());

    inbox.observe(&ClientEvent::Disconnected {
        reason: "gone".into(),
        ssh: None,
    });

    assert!(
        !inbox.take_accepted(),
        "an acceptance goes with its connection"
    );
    // A request of the old connection is no longer followed.
    inbox.observe(&accepted("gpui-4"));
    assert!(!inbox.take_accepted());
}
