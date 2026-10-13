//! `client.jump`'s answer is the `client.jump.resolve` method on the
//! connection the jump arrived on.

use super::*;
use crate::{method::ForkMethod, protocol::clients::*};

const JUMP: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-client-jump-v1.json");

#[test]
fn the_answer_reaches_the_daemon_as_client_jump_resolve() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["client.jump.resolve"]);
    let params = ClientJumpResolveParams {
        jump_id: decode_jump(JUMP).unwrap().jump_id,
        outcome: ClientJumpOutcome::Focused,
    };
    let id = client
        .handle
        .request(
            "boot-v1",
            Method::Fork(ForkMethod::ClientJumpResolve),
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
            Method::Fork(ForkMethod::ClientJumpResolve),
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
