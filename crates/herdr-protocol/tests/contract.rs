#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The Herdr fork's additions a client reads or sends: the snapshot's agent
//! priority and the unread hold request. Every fixture is a verbatim copy of
//! the fork's `tests/fixtures/` file of the same name, so a shape that
//! changes on one side fails here. Where the fork's own test pins the
//! canonical encoding (`src/protocol/contract_fixtures.rs`), so does this
//! one.

use herdr_protocol::{AgentStatus, ClientShellSnapshot};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const SNAPSHOT: &str = include_str!("fixtures/endpoint-snapshot-shared-v1.json");
const MARK_UNREAD: &str = include_str!("fixtures/endpoint-agent-mark-unread-v1.json");

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
fn snapshot_fixture_carries_agent_priority() {
    let snapshot: ClientShellSnapshot = canonical(&value(SNAPSHOT));
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
    for agent in json["agents"].as_array_mut().unwrap() {
        agent.as_object_mut().unwrap().remove("priority_key");
        agent.as_object_mut().unwrap().remove("priority_mark");
    }
    let decoded: ClientShellSnapshot = serde_json::from_value(json).unwrap();
    assert!(
        decoded
            .agents
            .iter()
            .all(|agent| agent.priority_key.is_empty() && agent.priority_mark.is_none())
    );
}

#[test]
fn malformed_snapshot_additions_are_rejected() {
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
