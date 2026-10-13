//! `ui.pick`'s answer is the `ui.pick.resolve` method on the connection the
//! picker arrived on.

use super::*;
use crate::{method::ForkMethod, protocol::pick::*};

const PICK_OPEN: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");

#[test]
fn resolve_reaches_the_daemon_as_ui_pick_resolve() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["ui.pick.resolve"]);
    let params = UiPickResolveParams {
        pick_id: decode_open(PICK_OPEN).unwrap().pick_id,
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
            Method::Fork(ForkMethod::UiPickResolve),
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
