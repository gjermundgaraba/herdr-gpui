#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `ClientReport::normalize`: the daemon's check of a report, kept here so a
//! client holds its report to the limits before sending. The cases are the
//! Herdr fork's own, from the tests of `src/api/schema/clients.rs`, starting
//! from the fork's report fixture.

use herdr_protocol::clients::*;

const REPORT: &str = include_str!("fixtures/endpoint-client-report-v1.json");

/// The fixture's report: Local and Build, one focused row on Local.
fn report() -> ClientReport {
    let request: serde_json::Value = serde_json::from_str(REPORT).expect("fixture is JSON");
    serde_json::from_value(request["params"].clone()).expect("fixture decodes")
}

/// Another row of `endpoint_id`, not focused.
fn row(endpoint_id: &str, pane_id: &str) -> ClientReportAgent {
    let mut row = report().agents.remove(0);
    row.endpoint_id = endpoint_id.into();
    row.focused = false;
    row.agent.pane_id = pane_id.into();
    row
}

#[test]
fn a_well_formed_report_is_kept_as_sent() {
    let mut normalized = report();
    assert_eq!(normalized.normalize(), Ok(()));
    assert_eq!(normalized, report());
}

#[test]
fn structural_mistakes_are_rejected() {
    let mut too_many_endpoints = report();
    too_many_endpoints.endpoints = (0..=MAX_CLIENT_REPORT_ENDPOINTS)
        .map(|index| ClientReportEndpoint {
            endpoint_id: format!("ssh:{index}"),
            label: String::new(),
            status: ClientEndpointStatus::Online,
            boot_id: None,
        })
        .collect();
    assert_eq!(
        too_many_endpoints.normalize(),
        Err(ClientReportError::TooManyEndpoints)
    );

    let mut too_many_agents = report();
    too_many_agents.agents = (0..=MAX_CLIENT_REPORT_AGENTS)
        .map(|index| row("local", &format!("w1:p{index}")))
        .collect();
    assert_eq!(
        too_many_agents.normalize(),
        Err(ClientReportError::TooManyAgents)
    );

    let mut duplicate = report();
    duplicate.endpoints.push(duplicate.endpoints[0].clone());
    assert_eq!(
        duplicate.normalize(),
        Err(ClientReportError::DuplicateEndpoint)
    );

    let mut unknown_active = report();
    unknown_active.active_endpoint_id = Some("ssh:gone".into());
    assert_eq!(
        unknown_active.normalize(),
        Err(ClientReportError::UnknownActiveEndpoint)
    );

    let mut unknown_agent_endpoint = report();
    unknown_agent_endpoint.agents.push(row("ssh:gone", "w1:p2"));
    assert_eq!(
        unknown_agent_endpoint.normalize(),
        Err(ClientReportError::UnknownAgentEndpoint)
    );

    let mut two_focused = report();
    two_focused.agents.push(row("local", "w1:p2"));
    for row in &mut two_focused.agents {
        row.focused = true;
    }
    assert_eq!(
        two_focused.normalize(),
        Err(ClientReportError::MultipleFocusedAgents)
    );

    let mut long_id = report();
    long_id.agents[0].agent.pane_id = "p".repeat(MAX_CLIENT_REPORT_ID_BYTES + 1);
    assert_eq!(long_id.normalize(), Err(ClientReportError::IdTooLong));

    let mut long_boot = report();
    long_boot.endpoints[0].boot_id = Some("b".repeat(MAX_CLIENT_REPORT_ID_BYTES + 1));
    assert_eq!(long_boot.normalize(), Err(ClientReportError::IdTooLong));

    let mut empty_id = report();
    empty_id.agents[0].agent.pane_id.clear();
    assert_eq!(empty_id.normalize(), Err(ClientReportError::EmptyId));
}

#[test]
fn display_text_is_cut_on_a_character_boundary() {
    let mut long = report();
    // One byte, then two-byte characters: the byte limit falls inside one.
    long.agents[0].agent.title = Some(format!("a{}", "é".repeat(MAX_CLIENT_REPORT_TEXT_BYTES)));
    long.agents[0].agent.tokens = (0..MAX_CLIENT_REPORT_PAIRS + 5)
        .map(|index| {
            (
                format!("k{index}"),
                "v".repeat(MAX_CLIENT_REPORT_TEXT_BYTES * 2),
            )
        })
        .collect();
    long.endpoints[0].label = "x".repeat(MAX_CLIENT_REPORT_TEXT_BYTES + 1);

    assert_eq!(long.normalize(), Ok(()));

    let title = long.agents[0].agent.title.as_deref().unwrap();
    assert_eq!(title.len(), MAX_CLIENT_REPORT_TEXT_BYTES - 1);
    assert!(title.starts_with('a') && title.chars().skip(1).all(|ch| ch == 'é'));
    assert_eq!(long.agents[0].agent.tokens.len(), MAX_CLIENT_REPORT_PAIRS);
    assert_eq!(
        long.agents[0].agent.tokens[0].1.len(),
        MAX_CLIENT_REPORT_TEXT_BYTES
    );
    assert_eq!(long.endpoints[0].label.len(), MAX_CLIENT_REPORT_TEXT_BYTES);
}

#[test]
fn a_rejection_says_what_the_daemon_says() {
    for (error, message) in [
        (
            ClientReportError::TooManyEndpoints,
            "a report lists at most 64 endpoints",
        ),
        (
            ClientReportError::TooManyAgents,
            "a report lists at most 512 agents",
        ),
        (
            ClientReportError::MultipleFocusedAgents,
            "at most one agent is focused",
        ),
    ] {
        assert_eq!(error.to_string(), message);
    }
    assert_eq!(MAX_ENDPOINT_COMMAND_BYTES, 1024 * 1024);
}
