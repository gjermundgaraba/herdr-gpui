use super::*;

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

#[test]
fn the_agent_is_the_one_in_the_focused_pane() {
    // The fixture focuses `w1:p1`, where `claude` runs.
    let mut snapshot = snapshot();
    assert_eq!(focused_agent(&snapshot).as_deref(), Some("claude"));
    // Another pane is focused: it runs no agent.
    snapshot.focused_pane_id = Some("w1:p2".into());
    assert_eq!(focused_agent(&snapshot), None);
    // An agent starts there.
    let mut started = snapshot.agents[0].clone();
    started.pane_id = "w1:p2".into();
    started.agent = Some("codex".into());
    snapshot.agents.push(started);
    assert_eq!(focused_agent(&snapshot).as_deref(), Some("codex"));
    // A pane Herdr lists without a known agent, and no focused pane.
    snapshot.agents[1].agent = None;
    assert_eq!(focused_agent(&snapshot), None);
    snapshot.focused_pane_id = None;
    assert_eq!(focused_agent(&snapshot), None);
}

#[test]
fn a_window_entry_carries_its_front_flag_and_agent() {
    let info = WindowInfo {
        id: 2,
        title: "Herdr".into(),
        active: false,
        front: true,
        fullscreen: false,
        width: 1200.0,
        height: 800.0,
        workspace: Some("w1".into()),
        agent: Some("claude".into()),
        hosts: vec![HostInfo {
            id: "local".into(),
            label: "This Mac".into(),
            status: "online".into(),
            selected: true,
        }],
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": 2, "title": "Herdr", "active": false, "front": true,
            "fullscreen": false, "width": 1200.0, "height": 800.0,
            "workspace": "w1", "agent": "claude",
            "hosts": [{"id": "local", "label": "This Mac", "status": "online", "selected": true}],
        })
    );
    let none = WindowInfo {
        agent: None,
        ..info
    };
    assert_eq!(
        serde_json::to_value(&none).unwrap()["agent"],
        serde_json::Value::Null
    );
    // An older app's entry, without either field, still reads.
    let mut older = json;
    older.as_object_mut().unwrap().remove("agent");
    older.as_object_mut().unwrap().remove("front");
    let read: WindowInfo = serde_json::from_value(older).unwrap();
    assert!(!read.front);
    assert_eq!(read.agent, None);
}
