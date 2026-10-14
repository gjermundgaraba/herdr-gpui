//! The fork mailbox: each named control goes to the feature that reads it,
//! and every event no feature keeps reaches the live state.

use super::*;
use herdr_client::protocol::{ClientShellSnapshot, pick::PICK_OPEN_KIND};

const PICK_OPEN: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");

fn raw(kind: &str, data: &str) -> ClientEvent {
    ClientEvent::Control {
        kind: kind.into(),
        data: data.into(),
    }
}

/// A mailbox whose daemon has sent a snapshot, which names the boot a
/// picker's answer needs.
fn ready() -> Inbox {
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut inbox = Inbox::default();
    assert!(
        inbox
            .apply(ClientEvent::Snapshot(Arc::new(snapshot)))
            .is_some()
    );
    inbox
}

#[test]
fn the_picker_control_reaches_the_picker() {
    let mut inbox = ready();
    assert!(inbox.apply(raw(PICK_OPEN_KIND, PICK_OPEN)).is_none());
    assert!(inbox.picks.take().open.is_some());
}

#[test]
fn a_control_no_feature_reads_reaches_the_live_state_unread() {
    let mut inbox = ready();
    let passed = inbox.apply(raw("endpoint.agent-completions.v1", "not json"));
    assert!(matches!(
        passed,
        Some(ClientEvent::Control { kind, data })
            if kind == "endpoint.agent-completions.v1" && data == "not json"
    ));
    // The live state ignores it, without asking the window to poll.
    let mut state = crate::state::LiveState::default();
    state.dirty = false;
    state.apply(raw("endpoint.agent-completions.v1", "{}"));
    assert!(!state.dirty);
    assert!(inbox.picks.take().open.is_none());
}

#[test]
fn an_answer_mark_unread_follows_still_reaches_the_live_state() {
    let mut inbox = Inbox::default();
    let answer = ClientEvent::Response {
        request_id: "gpui-3".into(),
        response: json!({"id": "gpui-3", "error": {"code": "agent_not_idle"}}),
    };
    assert!(matches!(
        inbox.apply(answer),
        Some(ClientEvent::Response { request_id, .. }) if request_id == "gpui-3"
    ));
}
