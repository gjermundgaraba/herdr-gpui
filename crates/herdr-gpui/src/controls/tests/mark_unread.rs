//! The `agent.mark_unread` request, against the Herdr fork's fixture.

use super::*;

/// The fork's canonical request, copied into the protocol crate.
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-agent-mark-unread-v1.json"
    ))
    .unwrap()
}

#[test]
fn mark_unread_names_the_focused_pane() {
    let mut snapshot = snapshot();
    snapshot.focused_pane_id = Some("w1:p1".into());
    let (method, params) = mark_unread(&snapshot).unwrap();
    assert_eq!(method, Method::AgentMarkUnread);
    assert_eq!(params, json!({"pane_id": "w1:p1"}));
    // Method and parameters as the fork's own client sends them.
    let fixture = fixture();
    assert_eq!(fixture["method"], method.as_str());
    assert_eq!(fixture["params"], params);
}

#[test]
fn mark_unread_leaves_agents_and_their_status_to_the_daemon() {
    let mut snapshot = snapshot();
    // Working, blocked, still badged, or no agent at all: the request is the
    // same, and the daemon answers `agent_not_idle` or `agent_not_found`.
    for status in [
        herdr_client::protocol::AgentStatus::Working,
        herdr_client::protocol::AgentStatus::Blocked,
        herdr_client::protocol::AgentStatus::Done,
        herdr_client::protocol::AgentStatus::Idle,
    ] {
        snapshot.agents[0].agent_status = status;
        assert!(mark_unread(&snapshot).is_some(), "{status:?}");
    }
    snapshot.agents.clear();
    assert_eq!(
        mark_unread(&snapshot).map(|(_, params)| params),
        Some(json!({"pane_id": snapshot.focused_pane_id}))
    );
    // Without a focused pane there is nothing to name.
    snapshot.focused_pane_id = None;
    assert_eq!(mark_unread(&snapshot), None);
}

#[test]
fn mark_unread_is_a_named_command_the_window_runs() {
    let info = COMMANDS
        .iter()
        .find(|info| info.command == Command::MarkUnread)
        .unwrap();
    // The config key matches the daemon's `[keys]` action name.
    assert_eq!(info.name, "mark_unread");
    assert_eq!(info.label, "Mark Agent Unread");
    assert!(info.default_shortcuts().is_empty());
    // It changes no focus, so it is not one of the fenced snapshot requests.
    assert_eq!(request(Command::MarkUnread, &snapshot()), None);
}
