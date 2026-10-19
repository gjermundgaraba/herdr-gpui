use super::*;
use crate::{ConnectTarget, Transport, VersionMismatch, connect_with_connector};

/// Connects through an application connector that returns `transport`, then
/// answers the handshake as a daemon with health checks would.
fn connected(transport: impl FnOnce(Stream) -> Transport + Send + 'static) -> (Client, Stream) {
    let (stream, mut server) = Stream::pair().unwrap();
    let client = connect_with_connector(
        ConnectTarget::Socket("/remote/herdr-client.sock".into()),
        ConnectOptions::default(),
        false,
        move |_, _| Ok(transport(stream)),
    )
    .unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    receive(&mut server);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([
        "surface_interest",
        "presentation_effects_fence",
        "health_check"
    ]);
    for (kind, data) in [
        (ENDPOINT_WELCOME_KIND, welcome.to_string()),
        (ENDPOINT_SNAPSHOT_KIND, SNAPSHOT.to_string()),
    ] {
        send(
            &mut server,
            ServerMessage::EndpointControl {
                kind: kind.into(),
                data,
            },
        );
    }
    assert!(matches!(event(&client), ClientEvent::Connected(_)));
    assert!(matches!(event(&client), ClientEvent::Snapshot(_)));
    (client, server)
}

#[test]
fn a_remote_stream_from_a_connector_answers_check_liveness() {
    let (client, mut server) = connected(Transport::Remote);
    client.handle.check_liveness();
    assert!(matches!(
        receive(&mut server),
        ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1"
    ));
}

#[test]
fn a_local_stream_from_a_connector_never_pings() {
    let (client, mut server) = connected(Transport::Local);
    client.handle.check_liveness();
    server
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut byte = [0; 1];
    let error = server.read(&mut byte).unwrap_err();
    assert!(
        matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ),
        "{error:?}"
    );
}

#[test]
fn a_remote_stream_requires_the_daemon_to_offer_health_checks() {
    let (stream, mut server) = Stream::pair().unwrap();
    let client = connect_with_connector(
        ConnectTarget::Socket("/remote/herdr-client.sock".into()),
        ConnectOptions::default(),
        false,
        move |_, _| Ok(Transport::Remote(stream)),
    )
    .unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    receive(&mut server);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!(["surface_interest", "presentation_effects_fence"]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    // A local socket would accept this daemon; a remote link without health
    // checks could hang unnoticed, so it is refused as outdated.
    assert!(matches!(
        event(&client),
        ClientEvent::VersionMismatch(VersionMismatch::DaemonOutdated { .. })
    ));
    assert!(matches!(event(&client), ClientEvent::Disconnected { .. }));
}
