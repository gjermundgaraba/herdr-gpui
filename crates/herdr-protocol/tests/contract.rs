#![allow(clippy::expect_used)]

//! The Herdr fork's additions a client sends: the unread hold request. The
//! fixture is a verbatim copy of the fork's `tests/fixtures/` file of the
//! same name, so a shape that changes on one side fails here.

use serde_json::{Value, json};

const MARK_UNREAD: &str = include_str!("fixtures/endpoint-agent-mark-unread-v1.json");

fn value(fixture: &str) -> Value {
    serde_json::from_str(fixture).expect("fixture is JSON")
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
