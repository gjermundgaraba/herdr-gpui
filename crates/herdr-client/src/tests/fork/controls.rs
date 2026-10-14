//! Named controls this client does not handle itself reach the owner as
//! ordered `ClientEvent::Control`s, unread: the fork's picker controls are
//! decoded by the window that receives them.

use super::*;

const PICK: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");

#[test]
fn a_named_control_arrives_as_an_ordered_event_with_its_data_unread() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    send(&mut server, ServerMessage::TerminalBell { count: 1 });
    send(&mut server, control("endpoint.pick.open.v1", PICK));
    // Malformed data is the owner's to judge, not the connection's.
    send(&mut server, control("endpoint.pick.close.v1", "not json"));
    // An event after them proves nothing was reordered or held back.
    send(&mut server, ServerMessage::TerminalBell { count: 2 });

    assert!(matches!(
        event(&client),
        ClientEvent::Message(ServerMessage::TerminalBell { count: 1 })
    ));
    assert!(matches!(
        event(&client),
        ClientEvent::Control { kind, data } if kind == "endpoint.pick.open.v1" && data == PICK
    ));
    assert!(matches!(
        event(&client),
        ClientEvent::Control { kind, data } if kind == "endpoint.pick.close.v1" && data == "not json"
    ));
    assert!(matches!(
        event(&client),
        ClientEvent::Message(ServerMessage::TerminalBell { count: 2 })
    ));
    assert!(!client.handle.is_disconnected());
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn session_forwards_a_control_without_touching_projection_state() {
    let mut session = ready_session();
    let revision = session.snapshot.as_ref().unwrap().revision;
    let mut events = Vec::new();
    for message in [
        control("endpoint.pick.open.v1", "{"),
        control("endpoint.agent-completions.v1", "{}"),
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
        [ClientEvent::Control { .. }, ClientEvent::Control { .. }]
    ));
    assert_eq!(session.snapshot.as_ref().unwrap().revision, revision);
    assert!(session.pending.is_none());
}

#[test]
fn the_health_pong_is_not_forwarded() {
    let mut session = ready_session();
    session
        .handle_message(control("endpoint.health.pong.v1", ""), |_| {
            panic!("health controls must not emit UI events")
        })
        .unwrap();
}

#[test]
fn a_control_before_the_welcome_is_still_a_handshake_error() {
    let mut session = Session::new(true, false);
    assert!(matches!(
        session.handle_message(control("endpoint.pick.open.v1", PICK), |_| Ok(())),
        Err(Error::WelcomeKind)
    ));
}
