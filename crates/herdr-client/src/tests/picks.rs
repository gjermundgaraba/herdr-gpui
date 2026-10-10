//! `ui.pick`: the daemon's two picker controls become ordered client events,
//! and the answer is the `ui.pick.resolve` method on the same connection.

use super::*;
use crate::protocol::pick::*;

const PICK_OPEN: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");
const PICK_CLOSE: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-pick-close-v1.json");

fn control(kind: &str, data: &str) -> ServerMessage {
    ServerMessage::EndpointControl {
        kind: kind.into(),
        data: data.into(),
    }
}

/// A handshake whose welcome advertises `methods`, with its two events read.
fn handshake_offering(client: &Client, server: &mut Stream, methods: &[&str]) {
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    server
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    assert!(matches!(
        receive(server),
        ClientMessage::EndpointControl { kind, .. } if kind == ENDPOINT_HELLO_KIND
    ));
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(methods);
    send(server, control(ENDPOINT_WELCOME_KIND, &welcome.to_string()));
    send(server, control(ENDPOINT_SNAPSHOT_KIND, SNAPSHOT));
    assert!(matches!(event(client), ClientEvent::Connected(_)));
    assert!(matches!(event(client), ClientEvent::Snapshot(_)));
}

#[test]
fn pick_open_then_close_arrive_as_ordered_events() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    send(&mut server, control(PICK_OPEN_KIND, PICK_OPEN));
    send(&mut server, control(PICK_CLOSE_KIND, PICK_CLOSE));
    // An event after them proves nothing was reordered or held back.
    send(&mut server, ServerMessage::TerminalBell { count: 1 });

    let ClientEvent::PickOpen(open) = event(&client) else {
        panic!("expected the picker to open")
    };
    assert_eq!(open, decode_open(PICK_OPEN).unwrap());
    assert_eq!(open.pick_id, "pick-7");
    assert_eq!(open.items.len(), 2);
    assert_eq!(open.selected.as_deref(), Some("t2"));
    let ClientEvent::PickClose(close) = event(&client) else {
        panic!("expected the picker to close")
    };
    assert_eq!(close.pick_id, "pick-7");
    assert!(matches!(
        event(&client),
        ClientEvent::Message(ServerMessage::TerminalBell { count: 1 })
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_malformed_pick_control_is_ignored_and_the_connection_continues() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
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
        // Neighbouring optional controls this client has no use for.
        ("endpoint.pick.open.v2", PICK_OPEN),
        ("endpoint.agent-completions.v1", "not json"),
    ] {
        send(&mut server, control(kind, data));
    }
    // The first event after them is the next well-formed message: none of
    // the malformed controls produced an event or ended the session.
    send(&mut server, control(PICK_CLOSE_KIND, PICK_CLOSE));
    assert!(matches!(
        event(&client),
        ClientEvent::PickClose(close) if close.pick_id == "pick-7"
    ));
    assert!(!client.handle.is_disconnected());
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn session_decodes_pick_controls_without_touching_projection_state() {
    let mut session = ready_session();
    let revision = session.snapshot.as_ref().unwrap().revision;
    let mut events = Vec::new();
    for message in [
        control(PICK_OPEN_KIND, PICK_OPEN),
        control(PICK_OPEN_KIND, "{"),
        control(PICK_CLOSE_KIND, PICK_CLOSE),
    ] {
        session
            .handle_message(message, |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();
    }
    assert!(matches!(
        events.as_slice(),
        [ClientEvent::PickOpen(_), ClientEvent::PickClose(_)]
    ));
    assert_eq!(session.snapshot.as_ref().unwrap().revision, revision);
    assert!(session.pending.is_none());
}

#[test]
fn a_pick_before_the_welcome_is_still_a_handshake_error() {
    let mut session = Session::new(true, false);
    assert!(matches!(
        session.handle_message(control(PICK_OPEN_KIND, PICK_OPEN), |_| Ok(())),
        Err(Error::WelcomeKind)
    ));
}

#[test]
fn resolve_reaches_the_daemon_as_ui_pick_resolve() {
    assert_eq!(Method::UiPickResolve.as_str(), "ui.pick.resolve");
    assert_eq!(Method::UiPickResolve.to_string(), "ui.pick.resolve");
    assert!(!Method::UiPickResolve.advertised_in(&["ui.pick".into(), "ui.pick.close".into()]));

    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["ui.pick.resolve"]);
    send(&mut server, control(PICK_OPEN_KIND, PICK_OPEN));
    let ClientEvent::PickOpen(open) = event(&client) else {
        panic!("expected the picker to open")
    };
    let params = UiPickResolveParams {
        pick_id: open.pick_id,
        outcome: UiPickOutcome::Picked { id: "t2".into() },
    };
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::UiPickResolve,
            serde_json::to_value(&params).unwrap(),
        )
        .unwrap();
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = receive(&mut server)
    else {
        panic!("expected the resolve request")
    };
    assert_eq!(boot_id, "boot-v1");
    assert_eq!(
        serde_json::from_str::<Value>(&request).unwrap(),
        json!({
            "id": id,
            "method": "ui.pick.resolve",
            "params": {"pick_id": "pick-7", "outcome": {"type": "picked", "id": "t2"}},
        })
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn resolve_is_rejected_when_the_daemon_does_not_offer_it() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["pane.focus"]);
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::UiPickResolve,
            json!({"pick_id": "pick-7", "outcome": {"type": "cancelled"}}),
        )
        .unwrap();
    assert!(matches!(
        event(&client),
        ClientEvent::CommandRejected { request_id: Some(request_id), reason: Error::UnsupportedMethod }
            if request_id == id
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}
