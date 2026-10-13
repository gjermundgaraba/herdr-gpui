//! A connection's picker mailbox: what it hands the window, once, however
//! the events between two polls were interleaved, and that it stays bounded.

use super::*;
use core::prelude::v1::test;

fn ready() -> Inbox {
    let mut inbox = Inbox::default();
    assert!(inbox.apply(snapshot(BOOT)).is_some());
    inbox
}

fn opened(inbox: &mut Inbox, pick_id: &str) {
    assert!(inbox.apply(opening(open(pick_id, false))).is_none());
}

fn closed(inbox: &mut Inbox, pick_id: &str) {
    assert!(inbox.apply(close(pick_id)).is_none());
}

fn request(pick_id: &str) -> Option<Request> {
    Some(Request {
        boot: BOOT.into(),
        open: open(pick_id, false),
    })
}

#[test]
fn a_picker_is_handed_over_once_with_the_boot_it_was_asked_under() {
    let mut inbox = ready();
    assert_eq!(inbox.take(), Update::default());
    opened(&mut inbox, "pick-1");
    assert_eq!(
        inbox.take(),
        Update {
            closed: None,
            open: request("pick-1"),
        }
    );
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn a_picker_before_any_snapshot_cannot_be_answered_and_is_dropped() {
    let mut inbox = Inbox::default();
    opened(&mut inbox, "pick-1");
    assert_eq!(inbox.take(), Update::default());
    // It is not held back for a snapshot that arrives later.
    assert!(inbox.apply(snapshot(BOOT)).is_some());
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn a_picker_closed_before_it_was_handed_over_is_never_shown() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    closed(&mut inbox, "pick-1");
    assert_eq!(inbox.take(), Update::default());
    // A close for another picker leaves the waiting one alone.
    opened(&mut inbox, "pick-2");
    closed(&mut inbox, "pick-1");
    assert_eq!(inbox.take().open, request("pick-2"));
}

#[test]
fn closing_the_handed_over_picker_is_reported_once_and_only_for_its_id() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    inbox.take();
    closed(&mut inbox, "other");
    assert_eq!(inbox.take(), Update::default());
    closed(&mut inbox, "pick-1");
    closed(&mut inbox, "pick-1");
    assert_eq!(
        inbox.take(),
        Update {
            closed: Some("pick-1".into()),
            open: None,
        }
    );
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn a_close_and_the_next_picker_between_two_polls_arrive_together_in_order() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    inbox.take();
    closed(&mut inbox, "pick-1");
    opened(&mut inbox, "pick-2");
    assert_eq!(
        inbox.take(),
        Update {
            closed: Some("pick-1".into()),
            open: request("pick-2"),
        }
    );
    // The window answered the first itself, so no close was owed for it;
    // one that still arrives late is not mistaken for the second's.
    closed(&mut inbox, "pick-1");
    assert_eq!(inbox.take(), Update::default());
    closed(&mut inbox, "pick-2");
    assert_eq!(inbox.take().closed.as_deref(), Some("pick-2"));
}

#[test]
fn a_lost_connection_forgets_a_waiting_picker_and_closes_the_shown_one() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    assert!(inbox.apply(disconnected()).is_some());
    assert_eq!(inbox.take(), Update::default());

    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    inbox.take();
    opened(&mut inbox, "pick-2");
    assert!(inbox.apply(disconnected()).is_some());
    assert_eq!(
        inbox.take(),
        Update {
            closed: Some("pick-1".into()),
            open: None,
        }
    );
    // Nothing is accepted again until a snapshot names a boot.
    opened(&mut inbox, "pick-3");
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn a_new_daemon_boot_closes_the_shown_picker_and_fences_the_next_one() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-1");
    inbox.take();
    // The same boot's later snapshots change nothing.
    assert!(inbox.apply(snapshot(BOOT)).is_some());
    assert_eq!(inbox.take(), Update::default());
    opened(&mut inbox, "pick-2");
    assert!(inbox.apply(snapshot("boot-v2")).is_some());
    assert_eq!(
        inbox.take(),
        Update {
            closed: Some("pick-1".into()),
            open: None,
        }
    );
    opened(&mut inbox, "pick-1");
    let update = inbox.take();
    assert_eq!(update.open.unwrap().boot, "boot-v2");
}

#[test]
fn the_mailbox_holds_one_picker_and_one_close_whatever_the_peer_sends() {
    let mut inbox = ready();
    opened(&mut inbox, "pick-0");
    inbox.take();
    for index in 1..2000 {
        opened(&mut inbox, &format!("pick-{index}"));
        closed(&mut inbox, &format!("unknown-{index}"));
    }
    closed(&mut inbox, "pick-0");
    // Only the newest picker and the close of the shown one remain.
    assert_eq!(
        inbox.take(),
        Update {
            closed: Some("pick-0".into()),
            open: request("pick-1999"),
        }
    );
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn every_other_event_passes_through_untouched() {
    let mut inbox = ready();
    for event in [
        ClientEvent::Response {
            request_id: "gpui-1".into(),
            response: json!({"id": "gpui-1", "error": {"code": "pick_not_found"}}),
        },
        ClientEvent::CommandRejected {
            request_id: Some("gpui-2".into()),
            reason: herdr_client::Error::UnsupportedMethod,
        },
        ClientEvent::Message(herdr_client::protocol::ServerMessage::TerminalBell { count: 1 }),
    ] {
        let name = format!("{event:?}");
        assert_eq!(
            inbox.apply(event).map(|event| format!("{event:?}")),
            Some(name)
        );
    }
    assert_eq!(inbox.take(), Update::default());
}

#[test]
fn a_picker_control_this_client_cannot_read_is_dropped() {
    let mut inbox = ready();
    let control = |kind: &str, data: &str| ClientEvent::Control {
        kind: kind.into(),
        data: data.into(),
    };
    for (kind, data) in [
        (PICK_OPEN_KIND, "not json"),
        (PICK_OPEN_KIND, ""),
        (PICK_OPEN_KIND, r#"{"pick_id":"p","title":"t"}"#),
        (
            PICK_OPEN_KIND,
            r#"{"pick_id":"p","title":"t","items":[{"id":"a"}]}"#,
        ),
        (PICK_CLOSE_KIND, "{}"),
        (PICK_CLOSE_KIND, r#"{"pick_id":7}"#),
    ] {
        assert!(inbox.apply(control(kind, data)).is_none(), "{kind} {data}");
    }
    assert_eq!(inbox.take(), Update::default());
    // Neighbouring controls are not the picker's: they are passed on.
    let open = serde_json::to_string(&open("pick-1", false)).unwrap();
    for kind in ["endpoint.pick.open.v2", "endpoint.agent-completions.v1"] {
        assert!(matches!(
            inbox.apply(control(kind, &open)),
            Some(ClientEvent::Control { .. })
        ));
    }
    assert_eq!(inbox.take(), Update::default());
    // The next well-formed one is read as ever.
    opened(&mut inbox, "pick-1");
    assert_eq!(inbox.take().open, request("pick-1"));
}
