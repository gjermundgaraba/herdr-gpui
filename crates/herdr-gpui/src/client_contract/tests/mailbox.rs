//! One connection's contract mailbox: what it learns from the connection's
//! events, and which answers it keeps from the live state.

use super::*;
use herdr_client::{Error as ClientError, protocol::endpoint::EndpointServerWelcome};
use serde_json::json;

pub(super) fn welcome(methods: &[&str]) -> ClientEvent {
    let mut welcome: EndpointServerWelcome = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome
        .methods
        .extend(methods.iter().map(|method| (*method).to_owned()));
    ClientEvent::Connected(welcome)
}

pub(super) fn snapshot(boot: &str) -> ClientEvent {
    ClientEvent::Snapshot(Arc::new(machine(boot, Vec::new())))
}

pub(super) fn response(id: &str, body: serde_json::Value) -> ClientEvent {
    ClientEvent::Response {
        request_id: id.into(),
        response: body,
    }
}

/// A mailbox whose daemon takes reports and has sent a snapshot.
fn ready() -> Inbox {
    let mut inbox = Inbox::default();
    assert!(inbox.apply(welcome(&["client.report"])).is_some());
    assert!(inbox.apply(snapshot("boot-1")).is_some());
    inbox
}

#[test]
fn a_report_needs_a_daemon_that_takes_one_and_a_boot_to_name() {
    let mut inbox = Inbox::default();
    assert_eq!(inbox.report_boot(), None);
    // Every event the mailbox learns from still reaches the live state.
    assert!(matches!(
        inbox.apply(welcome(&["client.report"])),
        Some(ClientEvent::Connected(_))
    ));
    assert_eq!(inbox.report_boot(), None, "no boot id to name yet");
    assert!(matches!(
        inbox.apply(snapshot("boot-1")),
        Some(ClientEvent::Snapshot(_))
    ));
    assert_eq!(inbox.report_boot(), Some("boot-1"));

    let mut silent = Inbox::default();
    silent.apply(welcome(&["pane.focus"]));
    silent.apply(snapshot("boot-1"));
    assert_eq!(silent.report_boot(), None, "the daemon takes no reports");
}

#[test]
fn the_answer_to_a_report_is_kept_from_the_live_state() {
    let mut inbox = ready();
    inbox.report_sent("gpui-7".into());
    assert!(inbox.report_pending());

    // Another request's answer is not this mailbox's.
    assert!(
        inbox
            .apply(response("gpui-6", json!({"id": "gpui-6", "result": {}})))
            .is_some()
    );
    assert!(inbox.report_pending());

    assert!(
        inbox
            .apply(response(
                "gpui-7",
                json!({"id": "gpui-7", "result": {"type": "ok"}})
            ))
            .is_none()
    );
    assert!(!inbox.report_pending());
    // Answered once: a late duplicate is an ordinary response again.
    assert!(
        inbox
            .apply(response("gpui-7", json!({"id": "gpui-7", "result": {}})))
            .is_some()
    );
}

#[test]
fn a_rejected_report_is_kept_too_and_ends_the_wait() {
    let rejected = json!({
        "id": "gpui-7",
        "error": {"code": "invalid_params", "message": "a report lists at most 64 endpoints"},
    });
    let mut inbox = ready();
    inbox.report_sent("gpui-7".into());
    assert!(inbox.apply(response("gpui-7", rejected)).is_none());
    assert!(!inbox.report_pending());

    // A report the worker refused to send never reaches the daemon at all.
    inbox.report_sent("gpui-8".into());
    assert!(
        inbox
            .apply(ClientEvent::CommandRejected {
                request_id: Some("gpui-8".into()),
                reason: ClientError::CommandBoot,
            })
            .is_none()
    );
    assert!(!inbox.report_pending());
    // Another command's rejection is the window's to show.
    assert!(
        inbox
            .apply(ClientEvent::CommandRejected {
                request_id: Some("gpui-9".into()),
                reason: ClientError::CommandBoot,
            })
            .is_some()
    );
}

#[test]
fn a_lost_connection_forgets_everything() {
    let mut inbox = ready();
    inbox.report_sent("gpui-7".into());
    assert!(
        inbox
            .apply(ClientEvent::Disconnected {
                reason: "gone".into(),
                ssh: None,
            })
            .is_some()
    );
    assert_eq!(inbox.report_boot(), None);
    assert!(!inbox.report_pending());
}

#[test]
fn a_jump_is_kept_for_the_window_and_one_this_client_cannot_read_is_dropped() {
    const JUMP: &str =
        include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-client-jump-v1.json");
    let control = |kind: &str, data: &str| ClientEvent::Control {
        kind: kind.into(),
        data: data.into(),
    };
    let mut inbox = ready();
    for data in [
        "not json",
        "",
        "{}",
        r#"{"jump_id":"j","endpoint_id":"local"}"#,
        r#"{"jump_id":7,"endpoint_id":"local","boot_id":"b","pane_id":"p"}"#,
    ] {
        assert!(
            inbox.apply(control(CLIENT_JUMP_KIND, data)).is_none(),
            "{data}"
        );
    }
    assert_eq!(inbox.take_jump(), None);
    assert!(!inbox.owes_jump_answer());
    // A neighbouring control is not the contract's: it is passed on.
    assert!(matches!(
        inbox.apply(control("endpoint.client-jump.v2", JUMP)),
        Some(ClientEvent::Control { .. })
    ));
    assert_eq!(inbox.take_jump(), None);
    assert!(inbox.apply(control(CLIENT_JUMP_KIND, JUMP)).is_none());
    assert_eq!(inbox.take_jump(), Some(decode_jump(JUMP).unwrap()));
}
