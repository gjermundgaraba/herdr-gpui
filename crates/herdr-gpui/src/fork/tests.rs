//! The fork mailbox: each named control goes to the feature that reads it,
//! and every event neither keeps reaches the live state.

use super::*;
use herdr_client::protocol::{
    ClientShellSnapshot,
    clients::{CLIENT_JUMP_KIND, decode_jump},
    endpoint::EndpointServerWelcome,
    pick::PICK_OPEN_KIND,
};

const JUMP: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-client-jump-v1.json");
const PICK_OPEN: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");

fn raw(kind: &str, data: &str) -> ClientEvent {
    ClientEvent::Control {
        kind: kind.into(),
        data: data.into(),
    }
}

/// A mailbox whose daemon offers the jump's answer and has sent a snapshot.
fn ready() -> Inbox {
    let mut welcome: EndpointServerWelcome = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome.methods.push("client.jump.resolve".into());
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut inbox = Inbox::default();
    assert!(inbox.apply(ClientEvent::Connected(welcome)).is_some());
    assert!(
        inbox
            .apply(ClientEvent::Snapshot(Arc::new(snapshot)))
            .is_some()
    );
    inbox
}

#[test]
fn each_control_reaches_the_feature_that_reads_it() {
    let mut inbox = ready();
    assert!(inbox.apply(raw(PICK_OPEN_KIND, PICK_OPEN)).is_none());
    assert!(inbox.apply(raw(CLIENT_JUMP_KIND, JUMP)).is_none());
    assert!(inbox.picks.take().open.is_some());
    assert_eq!(inbox.clients.take_jump(), Some(decode_jump(JUMP).unwrap()));
}

#[test]
fn a_control_neither_reads_reaches_the_live_state_unread() {
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
    assert_eq!(inbox.clients.take_jump(), None);
}
