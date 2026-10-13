//! The gjermundgaraba/herdr fork's traffic on a connection: the named
//! controls this client forwards to its owner, and the fork's lane methods.

use super::*;

mod controls;
mod jumps;
mod picks;
mod reports;
mod request_size;

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
