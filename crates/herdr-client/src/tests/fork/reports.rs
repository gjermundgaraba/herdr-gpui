//! `client.report`: the request is measured as it is written, and a report
//! too long for one daemon command loses rows from the end of its list.

use super::*;
use crate::protocol::clients::*;

const REPORT: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-client-report-v1.json");

/// The fixture's report.
fn report() -> ClientReport {
    let request: Value = serde_json::from_str(REPORT).unwrap();
    serde_json::from_value(request["params"].clone()).unwrap()
}

/// The daemon accepts request `id`.
fn accept(server: &mut Stream, id: &str) {
    send(
        server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: "boot-v1".into(),
            request_id: id.into(),
            final_chunk: true,
            data: json!({"id": id, "result": {"type": "ok"}})
                .to_string()
                .into_bytes(),
        },
    );
}

/// The `client.report` request the daemon read, as written.
fn reported(server: &mut Stream) -> String {
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = receive(server) else {
        panic!("expected the report")
    };
    assert_eq!(boot_id, "boot-v1");
    request
}

#[test]
fn a_report_too_long_for_one_command_loses_its_last_rows() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["client.report"]);
    // Every row at the daemon's text limits: the row and text limits alone
    // do not bound a report to one command.
    let mut full = report();
    let template = full.agents[0].clone();
    let token = "t".repeat(MAX_CLIENT_REPORT_TEXT_BYTES);
    full.agents = (0..MAX_CLIENT_REPORT_AGENTS)
        .map(|index| {
            let mut row = template.clone();
            row.focused = false;
            row.agent.pane_id = format!("w1:p{index}");
            row.agent.tokens = (0..8)
                .map(|key| (format!("k{key}"), token.clone()))
                .collect();
            row
        })
        .collect();
    assert_eq!(full.normalize(), Ok(()));
    assert!(serde_json::to_string(&full).unwrap().len() > MAX_ENDPOINT_COMMAND_BYTES);

    let id = client.handle.report("boot-v1", full.clone()).unwrap();

    let request = reported(&mut server);
    assert!(request.len() <= MAX_ENDPOINT_COMMAND_BYTES);
    let request: Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["id"], id.as_str());
    assert_eq!(request["method"], "client.report");
    let sent: ClientReport = serde_json::from_value(request["params"].clone()).unwrap();
    assert!(!sent.agents.is_empty() && sent.agents.len() < full.agents.len());
    // The rows kept are the head of the list, in order; nothing else changed.
    assert_eq!(sent.agents[..], full.agents[..sent.agents.len()]);
    assert_eq!(sent.endpoints, full.endpoints);
    // The daemon takes one request at a time.
    accept(&mut server, &id);
    assert!(matches!(event(&client), ClientEvent::Response { .. }));

    // A report that fits is sent exactly as given.
    let id = client.handle.report("boot-v1", report()).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&reported(&mut server)).unwrap(),
        json!({
            "id": id,
            "method": "client.report",
            "params": serde_json::to_value(report()).unwrap(),
        })
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_report_that_does_not_fit_without_rows_is_not_sent() {
    let (client, mut server, worker) = test_client();
    handshake_offering(&client, &mut server, &["client.report", "pane.focus"]);
    let mut huge = report();
    huge.endpoints[0].label = "x".repeat(MAX_ENDPOINT_COMMAND_BYTES);

    assert!(matches!(
        client.handle.report("boot-v1", huge),
        Err(Error::ClientReportTooLarge)
    ));

    // Nothing was written: the next request is the first the daemon reads.
    let id = client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
    let request: Value = serde_json::from_str(&reported(&mut server)).unwrap();
    assert_eq!(request["id"], id.as_str());
    assert_eq!(request["method"], "pane.focus");
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}
