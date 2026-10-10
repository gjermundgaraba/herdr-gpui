//! `client.jump`: the daemon's jump control becomes an ordered client event,
//! and the answer is the `client.jump.resolve` method on the same connection.

use super::*;
use crate::protocol::clients::*;

const JUMP: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-client-jump-v1.json");

#[test]
fn a_jump_arrives_as_an_ordered_event() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    send(&mut server, ServerMessage::TerminalBell { count: 1 });
    send(&mut server, control(CLIENT_JUMP_KIND, JUMP));
    // An event after it proves nothing was reordered or held back.
    send(&mut server, ServerMessage::TerminalBell { count: 2 });

    assert!(matches!(
        event(&client),
        ClientEvent::Message(ServerMessage::TerminalBell { count: 1 })
    ));
    let ClientEvent::ClientJump(jump) = event(&client) else {
        panic!("expected the jump")
    };
    assert_eq!(jump, decode_jump(JUMP).unwrap());
    assert_eq!(jump.jump_id, "jump-3");
    assert_eq!(jump.endpoint_id, "ssh:0123456789abcdef0123456789abcdef");
    assert_eq!(jump.boot_id, "boot-b");
    assert_eq!(jump.pane_id, "w1:p4");
    assert!(matches!(
        event(&client),
        ClientEvent::Message(ServerMessage::TerminalBell { count: 2 })
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_malformed_jump_is_ignored_and_the_connection_continues() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    for (kind, data) in [
        (CLIENT_JUMP_KIND, "not json"),
        (CLIENT_JUMP_KIND, ""),
        (CLIENT_JUMP_KIND, "{}"),
        (CLIENT_JUMP_KIND, r#"{"jump_id":"j","endpoint_id":"local"}"#),
        (
            CLIENT_JUMP_KIND,
            r#"{"jump_id":7,"endpoint_id":"local","boot_id":"b","pane_id":"p"}"#,
        ),
        // A neighbouring control this client has no use for.
        ("endpoint.client-jump.v2", JUMP),
    ] {
        send(&mut server, control(kind, data));
    }
    // The first event after them is the next well-formed message: none of
    // the malformed controls produced an event or ended the session.
    send(&mut server, control(CLIENT_JUMP_KIND, JUMP));
    assert!(matches!(
        event(&client),
        ClientEvent::ClientJump(jump) if jump.jump_id == "jump-3"
    ));
    assert!(!client.handle.is_disconnected());
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn session_decodes_a_jump_without_touching_projection_state() {
    let mut session = ready_session();
    let revision = session.snapshot.as_ref().unwrap().revision;
    let mut events = Vec::new();
    for message in [
        control(CLIENT_JUMP_KIND, "{"),
        control(CLIENT_JUMP_KIND, JUMP),
    ] {
        session
            .handle_message(message, |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();
    }
    assert!(matches!(events.as_slice(), [ClientEvent::ClientJump(_)]));
    assert_eq!(session.snapshot.as_ref().unwrap().revision, revision);
    assert!(session.pending.is_none());
}

#[test]
fn a_jump_before_the_welcome_is_still_a_handshake_error() {
    let mut session = Session::new(true, false);
    assert!(matches!(
        session.handle_message(control(CLIENT_JUMP_KIND, JUMP), |_| Ok(())),
        Err(Error::WelcomeKind)
    ));
}

#[test]
fn the_answer_reaches_the_daemon_as_client_jump_resolve() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["client.jump.resolve"]);
    send(&mut server, control(CLIENT_JUMP_KIND, JUMP));
    let ClientEvent::ClientJump(jump) = event(&client) else {
        panic!("expected the jump")
    };
    let params = ClientJumpResolveParams {
        jump_id: jump.jump_id,
        outcome: ClientJumpOutcome::Focused,
    };
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::ClientJumpResolve,
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
            "method": "client.jump.resolve",
            "params": {"jump_id": "jump-3", "outcome": {"type": "focused"}},
        })
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn an_answer_is_rejected_when_the_daemon_does_not_offer_it() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["pane.focus"]);
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::ClientJumpResolve,
            json!({"jump_id": "jump-3", "outcome": {"type": "timeout"}}),
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
