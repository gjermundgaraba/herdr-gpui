//! The agent picker's rows: the flat list of the Herdr fork's
//! `agent_navigator_rows`, with the cases of its `tests/endpoints.rs` and
//! `tests/priority_order.rs` that concern the list.

use super::*;

/// Local's agents `reviewer` (blocked, in `shell`) and `helper` (idle, in a
/// second space), and Build's `deployer`.
fn machines() -> (ClientShellSnapshot, ClientShellSnapshot) {
    let mut local = snapshot();
    let mut urgent = local.workspaces[0].clone();
    urgent.workspace_id = "w2".into();
    urgent.label = "urgent".into();
    local.workspaces.push(urgent);
    let mut second = local.panes[0].clone();
    second.pane_id = "w2:p1".into();
    second.workspace_id = "w2".into();
    second.tab_id = "w2:t1".into();
    second.label = None;
    second.cwd = Some("/urgent".into());
    second.foreground_cwd = Some("/urgent/src".into());
    second.focused = false;
    local.panes.push(second);
    let mut helper = agent("w2:p1", "helper", AgentStatus::Idle);
    helper.workspace_id = "w2".into();
    helper.priority_mark = Some("★".into());
    local.agents = vec![agent("w1:p1", "reviewer", AgentStatus::Blocked), helper];

    let mut build = snapshot();
    build.workspaces[0].label = "release".into();
    build.panes[0].label = None;
    build.agents = vec![agent("w1:p1", "deployer", AgentStatus::Working)];
    (local, build)
}

#[test]
fn every_agent_is_one_flat_row_in_the_order_given() {
    let (local, build) = machines();
    let hosts = federated(&local, Some(&build));
    // The panel's priority order, each agent with its host's place.
    let order = [
        (1, &build.agents[0]),
        (0, &local.agents[1]),
        (0, &local.agents[0]),
    ];

    let listed = rows(&hosts, &order, &view(Layout::Agents, "", None));

    assert_eq!(
        listed
            .iter()
            .map(|row| (row.label.as_str(), row.mark.as_deref(), row.depth))
            .collect::<Vec<_>>(),
        [
            // Another machine's agent names its machine; Local's do not.
            ("Build · release: deployer", None, 0),
            // The mark is the one the host sent, never worked out here.
            ("urgent: helper", Some("★"), 0),
            // A pane's own label comes before the agent's name.
            ("repo: shell", None, 0),
        ]
    );
    assert_eq!(
        listed
            .iter()
            .map(|row| row.target.clone())
            .collect::<Vec<_>>(),
        [
            pane(BUILD, "w1:p1"),
            pane("local", "w2:p1"),
            pane("local", "w1:p1")
        ]
    );
    assert_eq!(
        listed.iter().map(|row| row.status).collect::<Vec<_>>(),
        [
            Some(AgentStatus::Working),
            Some(AgentStatus::Idle),
            Some(AgentStatus::Blocked)
        ]
    );
    assert_eq!(listed[1].meta, "/urgent/src");
    // The top row is the default selection, whichever pane is focused.
    assert_eq!(selected_index(&listed, None), Some(0));
}

#[test]
fn a_lone_host_is_never_named() {
    let (local_snapshot, _) = machines();
    let hosts = local(&local_snapshot);
    let order = [(0, &local_snapshot.agents[0])];
    let listed = rows(&hosts, &order, &view(Layout::Agents, "", None));
    assert_eq!(labels(&listed), ["repo: shell"]);
}

#[test]
fn the_search_reads_the_label_and_the_directory_and_a_filter_the_status() {
    let (local, build) = machines();
    let hosts = federated(&local, Some(&build));
    let order = [
        (1, &build.agents[0]),
        (0, &local.agents[1]),
        (0, &local.agents[0]),
    ];
    let search = |query: &str, filter| {
        rows(&hosts, &order, &view(Layout::Agents, query, filter))
            .into_iter()
            .map(|row| row.label)
            .collect::<Vec<_>>()
    };

    assert_eq!(search("URGENT", None), ["urgent: helper"]);
    assert_eq!(search("build", None), ["Build · release: deployer"]);
    assert_eq!(search("/urgent/src", None), ["urgent: helper"]);
    assert_eq!(search("", Some(StatusFilter::Blocked)), ["repo: shell"]);
    assert_eq!(
        search("", Some(StatusFilter::Working)),
        ["Build · release: deployer"]
    );
    assert_eq!(search("", Some(StatusFilter::Idle)), ["urgent: helper"]);
    assert!(search("", Some(StatusFilter::Done)).is_empty());
    assert!(search("urgent", Some(StatusFilter::Blocked)).is_empty());
}

#[test]
fn a_search_matches_every_word_in_any_order_within_one_field() {
    let (local, build) = machines();
    let hosts = federated(&local, Some(&build));
    let order = [
        (1, &build.agents[0]),
        (0, &local.agents[1]),
        (0, &local.agents[0]),
    ];
    let search = |query: &str| {
        rows(&hosts, &order, &view(Layout::Agents, query, None))
            .into_iter()
            .map(|row| row.label)
            .collect::<Vec<_>>()
    };

    // As the workspace list and the fork's navigator search: each word on
    // its own, in any order, ignoring case and extra spaces.
    assert_eq!(search("helper urgent"), ["urgent: helper"]);
    assert_eq!(search("  DEPLOYER   build "), ["Build · release: deployer"]);
    assert_eq!(search("urgent src"), ["urgent: helper"]);
    // Every word must be found, and all of them in the label or all in the
    // directory: `helper` is only in the label, `src` only in the directory.
    assert!(search("helper deployer").is_empty());
    assert!(search("helper src").is_empty());
}

#[test]
fn a_host_that_is_away_keeps_its_rows_marked_stale() {
    let (local, build) = machines();
    let mut hosts = federated(&local, Some(&build));
    hosts[1].online = false;
    let order = [(0, &local.agents[0]), (1, &build.agents[0])];

    let listed = rows(&hosts, &order, &view(Layout::Agents, "", None));

    assert_eq!(
        listed
            .iter()
            .map(|row| (row.label.as_str(), row.stale))
            .collect::<Vec<_>>(),
        [("repo: shell", false), ("Build · release: deployer", true)]
    );
}

#[test]
fn an_agent_without_a_listed_space_or_pane_falls_back_to_what_it_has() {
    let mut snapshot = snapshot();
    let mut orphan = agent("gone:p9", "orphan", AgentStatus::Done);
    orphan.workspace_id = "gone".into();
    orphan.name = None;
    snapshot.agents = vec![orphan];
    let hosts = local(&snapshot);
    let order = [(0, &snapshot.agents[0])];

    let listed = rows(&hosts, &order, &view(Layout::Agents, "", None));

    assert_eq!(labels(&listed), ["gone: gone:p9"]);
    assert_eq!(listed[0].meta, "");
}
