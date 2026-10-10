//! What comes back from `agent.mark_unread`. The daemon keeps the hold, so
//! the only thing this client ever holds is the answer to show. The refusals
//! are the Herdr fork's, from `handle_agent_mark_unread` in its
//! `src/app/unread_hold.rs`.

use super::*;
use herdr_client::protocol::AgentStatus;

fn connected() -> LiveState {
    let mut state = LiveState {
        status: ConnectionStatus::Connected,
        ..LiveState::default()
    };
    state.apply(ClientEvent::Snapshot(snapshot()));
    state
}

#[test]
fn a_refused_mark_is_shown_like_any_other_daemon_error() {
    for (code, message) in [
        (
            "agent_not_idle",
            "only an acknowledged completion, shown as idle, can be marked unread",
        ),
        ("agent_not_found", "pane w1:p1 hosts no agent"),
        ("pane_not_found", "pane w9:p9 not found"),
    ] {
        let mut state = connected();
        state.apply(ClientEvent::Response {
            request_id: "client-shell:14".into(),
            response: serde_json::json!({
                "id": "client-shell:14",
                "error": {"code": code, "message": message},
            }),
        });
        assert_eq!(state.error.as_deref(), Some(message), "{code}");
        assert_eq!(state.status_text(None), format!("Connected: {message}"));
    }
    // A daemon that names only the code still says why.
    let mut state = connected();
    state.apply(ClientEvent::Response {
        request_id: "client-shell:14".into(),
        response: serde_json::json!({"error": {"code": "agent_not_idle"}}),
    });
    assert_eq!(state.error.as_deref(), Some("agent_not_idle"));
}

#[test]
fn a_daemon_without_the_method_rejects_the_mark_before_it_is_sent() {
    let mut state = connected();
    state.apply(ClientEvent::CommandRejected {
        request_id: Some("client-shell:14".into()),
        reason: herdr_client::Error::UnsupportedMethod,
    });
    assert_eq!(
        state.error.as_deref(),
        Some("method not advertised by endpoint")
    );
}

#[test]
fn an_accepted_mark_changes_nothing_until_the_daemon_s_snapshot_does() {
    let mut state = connected();
    let before = state.snapshot.clone().unwrap();
    state.apply(ClientEvent::Response {
        request_id: "client-shell:14".into(),
        response: serde_json::json!({
            "id": "client-shell:14",
            "result": {"type": "agent_info", "agent": {"pane_id": "w1:p1"}},
        }),
    });
    assert_eq!(state.error, None);
    // No local unread record: the agent shows what the snapshot says, and
    // the badge arrives only when the daemon restores the completion.
    assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &before));
    let mut restored = (*before).clone();
    restored.revision += 1;
    restored.agents[0].agent_status = AgentStatus::Done;
    state.apply(ClientEvent::Snapshot(Arc::new(restored)));
    assert_eq!(
        state.snapshot.as_ref().unwrap().agents[0].agent_status,
        AgentStatus::Done
    );
}
