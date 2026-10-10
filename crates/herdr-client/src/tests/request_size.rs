//! A request is one daemon command: the daemon closes a connection that
//! sends a longer one, so the handle refuses it and writes nothing.

use super::*;
use crate::protocol::{
    clients::MAX_ENDPOINT_COMMAND_BYTES,
    pick::{UiPickOutcome, UiPickResolveParams},
};

/// `ui.pick.resolve` for created text of `bytes` bytes.
fn created(bytes: usize) -> Value {
    serde_json::to_value(UiPickResolveParams {
        pick_id: "pick-7".into(),
        outcome: UiPickOutcome::Created {
            text: "x".repeat(bytes),
        },
    })
    .unwrap()
}

/// The request the daemon read next, as written.
fn written(server: &mut Stream) -> String {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = receive(server) else {
        panic!("expected a request")
    };
    request
}

#[test]
fn a_request_longer_than_one_command_is_refused_and_nothing_is_written() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["ui.pick.resolve"]);
    let request = |bytes| {
        client
            .handle
            .request("boot-v1", Method::UiPickResolve, created(bytes))
    };
    // What the request is besides its text: measured, not assumed.
    let first = request(0).unwrap();
    let envelope = written(&mut server).len();
    let room = MAX_ENDPOINT_COMMAND_BYTES - envelope;
    // The daemon answers, so the connection takes its next request.
    send(
        &mut server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: "boot-v1".into(),
            request_id: first.clone(),
            final_chunk: true,
            data: json!({"id": first, "result": {"type": "ok"}})
                .to_string()
                .into_bytes(),
        },
    );

    // One byte over the daemon's limit, though well within a frame.
    assert!(matches!(request(room + 1), Err(Error::RequestTooLarge)));

    // Exactly the limit is sent, and is the next request the daemon reads.
    let id = request(room).unwrap();
    assert_eq!(
        id.len(),
        first.len(),
        "the envelope was measured with {first}"
    );
    let sent = written(&mut server);
    assert_eq!(sent.len(), MAX_ENDPOINT_COMMAND_BYTES);
    assert!(sent.starts_with(&format!(r#"{{"id":"{id}""#)), "{id}");
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}
