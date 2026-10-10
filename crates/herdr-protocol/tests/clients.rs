#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The client contract of the Herdr fork, as far as a client takes part in
//! it: the snapshot additions, the client report, the jump control and its
//! answer, and the unread hold. Every fixture is a verbatim copy of the
//! fork's `tests/fixtures/` file of the same name, so a shape that changes
//! on one side fails here. Where the fork's own test pins the canonical
//! encoding (`src/protocol/contract_fixtures.rs`), so does this one. The
//! socket API shapes tools read (`client.list`, `client.changed`,
//! `client.jump`) are no client's, and have no copy here.
//!
//! The lane request envelope is not a type of this crate, so the tests spell
//! it out.

use herdr_protocol::{
    AgentStatus, ClientShellPinnedSection, ClientShellSnapshot, Error, clients::*,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const SNAPSHOT: &str = include_str!("fixtures/endpoint-snapshot-shared-v1.json");
const REPORT: &str = include_str!("fixtures/endpoint-client-report-v1.json");
const JUMP: &str = include_str!("fixtures/endpoint-client-jump-v1.json");
const JUMP_RESOLVE: &str = include_str!("fixtures/endpoint-client-jump-resolve-v1.json");
const MARK_UNREAD: &str = include_str!("fixtures/endpoint-agent-mark-unread-v1.json");

const REMOTE: &str = "ssh:0123456789abcdef0123456789abcdef";

/// A request on the client's command lane.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct LaneRequest<P> {
    id: String,
    method: String,
    params: P,
}

fn value(fixture: &str) -> Value {
    serde_json::from_str(fixture).expect("fixture is JSON")
}

/// The fixture decodes, and what it decoded to encodes to exactly the
/// fixture's JSON: no field more, none missing.
fn canonical<T: Serialize + DeserializeOwned>(json: &Value) -> T {
    let decoded: T = serde_json::from_value(json.clone()).expect("fixture decodes");
    assert_eq!(
        serde_json::to_value(&decoded).expect("value encodes"),
        *json
    );
    decoded
}

#[test]
fn snapshot_fixture_carries_pinned_sections_and_agent_priority() {
    let snapshot: ClientShellSnapshot = canonical(&value(SNAPSHOT));
    assert_eq!(
        snapshot.pinned_sections,
        [
            ClientShellPinnedSection {
                title: "Priority".into(),
                fg: Some("#ff8800".into()),
                workspace_ids: vec!["w1".into()],
            },
            ClientShellPinnedSection {
                title: "Platform".into(),
                fg: None,
                workspace_ids: vec!["w1".into(), "w2".into()],
            },
        ]
    );
    let keys: Vec<_> = snapshot
        .agents
        .iter()
        .map(|agent| (agent.priority_key.as_str(), agent.priority_mark.as_deref()))
        .collect();
    assert_eq!(
        keys,
        [
            ("0.1.00065d7128fb2820", Some("high")),
            ("1.0.00065d7125600000", None),
            ("1.3.fff9a28ed378f1ff", None),
        ]
    );
    // Sorting by key gives the daemon's own priority order, and the lifted
    // done agent leads although a blocked agent is waiting elsewhere.
    let mut by_key: Vec<_> = snapshot.agents.iter().collect();
    by_key.sort_by(|left, right| {
        left.priority_key
            .as_bytes()
            .cmp(right.priority_key.as_bytes())
    });
    let order: Vec<_> = by_key.iter().map(|agent| agent.pane_id.as_str()).collect();
    assert_eq!(order, snapshot.agent_order);
    assert_eq!(by_key[0].agent_status, AgentStatus::Done);
    assert_eq!(by_key[1].agent_status, AgentStatus::Blocked);
}

#[test]
fn a_snapshot_without_the_additions_still_decodes() {
    let mut json = value(SNAPSHOT);
    json.as_object_mut().unwrap().remove("pinned_sections");
    for agent in json["agents"].as_array_mut().unwrap() {
        agent.as_object_mut().unwrap().remove("priority_key");
        agent.as_object_mut().unwrap().remove("priority_mark");
    }
    let decoded: ClientShellSnapshot = serde_json::from_value(json).unwrap();
    assert!(decoded.pinned_sections.is_empty());
    assert!(
        decoded
            .agents
            .iter()
            .all(|agent| agent.priority_key.is_empty() && agent.priority_mark.is_none())
    );
}

#[test]
fn malformed_snapshot_additions_are_rejected() {
    for (path, bad) in [
        ("pinned_sections", json!({})),
        ("pinned_sections", json!([{"title": "t", "fg": null}])),
        (
            "pinned_sections",
            json!([{"fg": null, "workspace_ids": []}]),
        ),
        (
            "pinned_sections",
            json!([{"title": "t", "fg": 7, "workspace_ids": []}]),
        ),
        (
            "pinned_sections",
            json!([{"title": "t", "fg": null, "workspace_ids": [1]}]),
        ),
    ] {
        let mut json = value(SNAPSHOT);
        json[path] = bad.clone();
        assert!(
            serde_json::from_value::<ClientShellSnapshot>(json).is_err(),
            "{bad}"
        );
    }
    for (field, bad) in [("priority_key", json!(3)), ("priority_mark", json!(["x"]))] {
        let mut json = value(SNAPSHOT);
        json["agents"][0][field] = bad.clone();
        assert!(
            serde_json::from_value::<ClientShellSnapshot>(json).is_err(),
            "{field}: {bad}"
        );
    }
}

#[test]
fn client_report_fixture_is_the_lane_request() {
    let request: LaneRequest<ClientReport> = canonical(&value(REPORT));
    assert_eq!(request.id, "client-shell:7");
    assert_eq!(request.method, "client.report");
    let report = request.params;
    assert_eq!(report.focused, Some(true));
    assert_eq!(report.active_endpoint_id.as_deref(), Some("local"));
    assert_eq!(
        report.endpoints,
        [
            ClientReportEndpoint {
                endpoint_id: "local".into(),
                label: "Local".into(),
                status: ClientEndpointStatus::Online,
                boot_id: Some("boot-a".into()),
            },
            ClientReportEndpoint {
                endpoint_id: REMOTE.into(),
                label: "Build".into(),
                status: ClientEndpointStatus::Online,
                boot_id: Some("boot-b".into()),
            },
            ClientReportEndpoint {
                endpoint_id: "ssh:fedcba9876543210fedcba9876543210".into(),
                label: "Staging".into(),
                status: ClientEndpointStatus::Reconnecting,
                boot_id: None,
            },
        ]
    );
    // Display order: the keys of both endpoints, merged.
    let rows: Vec<_> = report
        .agents
        .iter()
        .map(|row| {
            (
                row.endpoint_id.as_str(),
                row.agent.pane_id.as_str(),
                row.focused,
                row.agent.focused,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("local", "w1:p1", false, false),
            ("local", "w2:p1", true, true),
            // Focused inside its own endpoint, which the client does not present.
            (REMOTE, "w1:p4", false, true),
            ("local", "w2:p2", false, false),
        ]
    );
    assert!(
        report
            .agents
            .windows(2)
            .all(|pair| pair[0].agent.priority_key.as_bytes()
                <= pair[1].agent.priority_key.as_bytes())
    );
    let row = &report.agents[2];
    assert_eq!(
        (row.workspace_label.as_str(), row.workspace_number),
        ("infra", 1)
    );
    assert_eq!(
        (row.cwd.as_deref(), row.foreground_cwd.as_deref()),
        (Some("/work/infra"), None)
    );
    // The row's record is the snapshot's own agent, the additions included.
    let snapshot: ClientShellSnapshot = serde_json::from_str(SNAPSHOT).unwrap();
    assert_eq!(report.agents[0].agent, snapshot.agents[0]);
    assert!(report.endpoints.len() <= MAX_CLIENT_REPORT_ENDPOINTS);
    assert!(report.agents.len() <= MAX_CLIENT_REPORT_AGENTS);
}

#[test]
fn every_endpoint_status_has_the_daemons_spelling() {
    for (status, name) in [
        (ClientEndpointStatus::Connecting, "connecting"),
        (ClientEndpointStatus::Online, "online"),
        (ClientEndpointStatus::Reconnecting, "reconnecting"),
        (ClientEndpointStatus::Attention, "attention"),
        (ClientEndpointStatus::Disabled, "disabled"),
    ] {
        assert_eq!(serde_json::to_value(status).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<ClientEndpointStatus>(json!(name)).unwrap(),
            status
        );
    }
    assert!(serde_json::from_value::<ClientEndpointStatus>(json!("offline")).is_err());
}

#[test]
fn report_limits_are_the_daemons() {
    assert_eq!(
        (
            MAX_CLIENT_REPORT_ENDPOINTS,
            MAX_CLIENT_REPORT_AGENTS,
            MAX_CLIENT_REPORT_ID_BYTES,
            MAX_CLIENT_REPORT_TEXT_BYTES,
            MAX_CLIENT_REPORT_PAIRS,
        ),
        (64, 512, 128, 512, 32)
    );
}

fn jump() -> EndpointClientJump {
    EndpointClientJump {
        jump_id: "jump-3".into(),
        endpoint_id: REMOTE.into(),
        boot_id: "boot-b".into(),
        pane_id: "w1:p4".into(),
    }
}

#[test]
fn client_jump_fixture_is_the_control_payload() {
    assert_eq!(CLIENT_JUMP_KIND, "endpoint.client-jump.v1");
    assert_eq!(canonical::<EndpointClientJump>(&value(JUMP)), jump());
    assert_eq!(decode_jump(JUMP).unwrap(), jump());
    // Field order, as the daemon writes the control.
    assert_eq!(
        serde_json::to_string(&jump()).unwrap(),
        format!(
            r#"{{"jump_id":"jump-3","endpoint_id":"{REMOTE}","boot_id":"boot-b","pane_id":"w1:p4"}}"#
        )
    );
    // Generation 1 grows by optional fields; a newer daemon's are ignored.
    let newer = format!(
        r#"{{"jump_id":"j","endpoint_id":"{REMOTE}","boot_id":"b","pane_id":"p","reason":"tool"}}"#
    );
    assert_eq!(decode_jump(&newer).unwrap().jump_id, "j");
}

#[test]
fn malformed_jump_payloads_are_typed_errors() {
    use std::error::Error as _;

    for data in [
        "",
        "not json",
        "[]",
        "null",
        r#"{"endpoint_id":"e","boot_id":"b","pane_id":"p"}"#,
        r#"{"jump_id":"j","boot_id":"b","pane_id":"p"}"#,
        r#"{"jump_id":"j","endpoint_id":"e","pane_id":"p"}"#,
        r#"{"jump_id":"j","endpoint_id":"e","boot_id":"b"}"#,
        r#"{"jump_id":3,"endpoint_id":"e","boot_id":"b","pane_id":"p"}"#,
        r#"{"jump_id":"j","endpoint_id":"e","boot_id":null,"pane_id":"p"}"#,
        r#"{"jump_id":"j","endpoint_id":"e","boot_id":"b","pane_id":"p"} trailing"#,
    ] {
        let error = decode_jump(data).expect_err(data);
        assert!(matches!(error, Error::ClientJumpPayload(_)), "{data}");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            error
                .source()
                .is_some_and(|source| source.is::<serde_json::Error>()),
            "{data}"
        );
    }
}

#[test]
fn client_jump_resolve_fixture_lists_every_outcome() {
    let requests: Vec<LaneRequest<ClientJumpResolveParams>> = canonical(&value(JUMP_RESOLVE));
    let expected: Vec<_> = [
        ClientJumpOutcome::Focused,
        ClientJumpOutcome::StaleBoot,
        ClientJumpOutcome::NotFound,
        ClientJumpOutcome::Unavailable,
        ClientJumpOutcome::Timeout,
        ClientJumpOutcome::Failed {
            message: "activation was cancelled".into(),
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, outcome)| LaneRequest {
        id: format!("client-shell:{}", index + 8),
        method: "client.jump.resolve".to_owned(),
        params: ClientJumpResolveParams {
            jump_id: "jump-3".into(),
            outcome,
        },
    })
    .collect();
    assert_eq!(requests, expected);
    assert_eq!(
        serde_json::to_string(&requests[0].params).unwrap(),
        r#"{"jump_id":"jump-3","outcome":{"type":"focused"}}"#
    );
    assert_eq!(
        serde_json::to_string(&requests[5].params).unwrap(),
        r#"{"jump_id":"jump-3","outcome":{"type":"failed","message":"activation was cancelled"}}"#
    );
    for outcome in [
        json!({"type": "failed"}),
        json!({"type": "cancelled"}),
        json!({"message": "x"}),
        json!("focused"),
    ] {
        assert!(
            serde_json::from_value::<ClientJumpOutcome>(outcome.clone()).is_err(),
            "{outcome}"
        );
    }
}

#[test]
fn agent_mark_unread_fixture_is_the_request() {
    assert_eq!(
        value(MARK_UNREAD),
        json!({
            "id": "client-shell:14",
            "method": "agent.mark_unread",
            "params": {"pane_id": "w1:p1"},
        })
    );
}
