//! Snapshots for the navigator's row logic, built from the protocol's
//! snapshot fixture: one space `repo` on branch `main`, one tab `main`, one
//! pane `w1:p1` labelled `shell`, focused.

// Not a glob of the parent: its `gpui::*` would shadow `#[test]`.
use super::rows::{
    Host, Layout, Row, StatusFilter, Target, View, rows, selected_index, workspace_jump,
};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};

mod rows_agents;
mod rows_workspaces;

const BUILD: &str = "ssh:00000000000000000000000000000001";

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    snapshot.agent_view_label = None;
    snapshot.agent_order.clear();
    snapshot.agents.clear();
    snapshot
}

fn agent(pane_id: &str, name: &str, status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "w1".into(),
        tab_id: "w1:t1".into(),
        name: Some(name.into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
        priority_key: String::new(),
        priority_mark: None,
    }
}

/// Adds a tab to the fixture's space, with one pane in it.
fn add_tab(snapshot: &mut ClientShellSnapshot, number: usize, label: &str, custom: bool) {
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = format!("w1:t{number}");
    tab.number = number;
    tab.label = label.into();
    tab.custom_label = custom;
    tab.focused = false;
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = format!("w1:p{number}");
    pane.tab_id = tab.tab_id.clone();
    pane.label = None;
    pane.focused = false;
    snapshot.tabs.push(tab);
    snapshot.panes.push(pane);
}

/// Local alone, online, with `snapshot`.
fn local(snapshot: &ClientShellSnapshot) -> [Host<'_>; 1] {
    [Host {
        id: "local",
        label: "Local",
        local: true,
        online: true,
        snapshot: Some(snapshot),
    }]
}

/// Local and Build, both online.
fn federated<'a>(
    local: &'a ClientShellSnapshot,
    build: Option<&'a ClientShellSnapshot>,
) -> [Host<'a>; 2] {
    [
        Host {
            id: "local",
            label: "Local",
            local: true,
            online: true,
            snapshot: Some(local),
        },
        Host {
            id: BUILD,
            label: "Build",
            local: false,
            online: true,
            snapshot: build,
        },
    ]
}

fn view(layout: Layout, query: &str, filter: Option<StatusFilter>) -> View {
    View {
        query: query.into(),
        filter,
        ..View::new(layout)
    }
}

fn labels(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.label.as_str()).collect()
}

fn pane(endpoint: &str, pane: &str) -> Target {
    Target::Pane {
        endpoint: endpoint.into(),
        pane: pane.into(),
    }
}

fn workspace(endpoint: &str, workspace: &str) -> Target {
    Target::Workspace {
        endpoint: endpoint.into(),
        workspace: workspace.into(),
    }
}

/// Where Enter would lead.
fn chosen(rows: &[Row], view: &View) -> Option<Target> {
    selected_index(rows, view.selected.as_ref()).map(|index| rows[index].target.clone())
}
