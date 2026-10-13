//! A report is measured as the request it becomes: one too long for one
//! daemon command loses rows from the end of its list, and one that does not
//! fit without rows is not sent.

use super::wiring::BOOT;
use super::*;
use crate::{
    client_contract::report::{Error, one_command},
    window::MockPeer,
};
use herdr_client::{ForkMethod, Method, protocol::ClientMessage};
use serde_json::Value;

const REPORT: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-client-report-v1.json");

/// The fixture's report.
fn fixture() -> ClientReport {
    let request: Value = serde_json::from_str(REPORT).unwrap();
    serde_json::from_value(request["params"].clone()).unwrap()
}

/// The length of the `client.report` request `params` become.
fn request_len(params: &Value) -> usize {
    crate::fork::lane_request_len(ForkMethod::ClientReport, params)
}

#[test]
fn a_report_too_long_for_one_command_loses_its_last_rows() {
    // Every row at the daemon's text limits: the row and text limits alone
    // do not bound a report to one command.
    let mut full = fixture();
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

    let params = one_command(full.clone()).unwrap();

    assert!(request_len(&params) <= MAX_ENDPOINT_COMMAND_BYTES);
    let sent: ClientReport = serde_json::from_value(params).unwrap();
    assert!(!sent.agents.is_empty() && sent.agents.len() < full.agents.len());
    // The rows kept are the head of the list, in order; nothing else changed.
    assert_eq!(sent.agents[..], full.agents[..sent.agents.len()]);
    assert_eq!(sent.endpoints, full.endpoints);
    assert_eq!(sent.focused, full.focused);
    assert_eq!(sent.active_endpoint_id, full.active_endpoint_id);
}

#[test]
fn a_report_that_fits_is_sent_as_given() {
    assert_eq!(
        one_command(fixture()).unwrap(),
        serde_json::to_value(fixture()).unwrap()
    );
}

#[test]
fn a_report_that_does_not_fit_without_rows_is_not_sent() {
    let mut huge = fixture();
    huge.endpoints[0].label = "x".repeat(MAX_ENDPOINT_COMMAND_BYTES);

    assert!(matches!(one_command(huge), Err(Error::TooLarge)));
}

#[test]
fn the_measure_bounds_the_request_the_connection_writes() {
    let mut peer = MockPeer::advertising(&["client.report"]);
    let params = serde_json::to_value(fixture()).unwrap();
    let measured = request_len(&params);

    let method = Method::Fork(ForkMethod::ClientReport);
    let id = peer.client.handle.request(BOOT, method, params).unwrap();

    let written = loop {
        if let ClientMessage::ClientShellEndpointRequest { request, .. } = peer.receive() {
            break request;
        }
    };
    // The same request but for its id, which the measure takes at its
    // longest.
    let longest = format!("gpui-{}", u64::MAX);
    assert_eq!(measured - written.len(), longest.len() - id.len());
}
