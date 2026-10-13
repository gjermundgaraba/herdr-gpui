//! The gjermundgaraba/herdr fork's traffic on a connection: the named
//! controls this client forwards to its owner, and the fork's lane methods.

use super::*;
use crate::{
    method::ForkMethod,
    protocol::pick::{UiPickOutcome, UiPickResolveParams},
};

mod controls;
mod reports;

/// A named endpoint control, as the daemon sends one after the handshake.
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
fn a_fork_method_is_written_when_offered_and_rejected_when_not() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["ui.pick.resolve"]);
    // Refused before it is written, like any method the daemon did not offer.
    let refused = client
        .handle
        .request(
            "boot-v1",
            Method::Fork(ForkMethod::ClientJumpResolve),
            json!({"jump_id": "jump-3", "outcome": {"type": "timeout"}}),
        )
        .unwrap();
    assert!(matches!(
        event(&client),
        ClientEvent::CommandRejected { request_id: Some(request_id), reason: Error::UnsupportedMethod }
            if request_id == refused
    ));

    let params = UiPickResolveParams {
        pick_id: "pick-7".into(),
        outcome: UiPickOutcome::Picked { id: "t2".into() },
    };
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::Fork(ForkMethod::UiPickResolve),
            serde_json::to_value(&params).unwrap(),
        )
        .unwrap();
    // The first request the daemon reads.
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
