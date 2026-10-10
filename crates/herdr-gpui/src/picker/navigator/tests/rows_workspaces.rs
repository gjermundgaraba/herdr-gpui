//! The workspace list's rows. The cases follow the Navigator tests of the
//! Herdr fork's `src/client/shell/tests/copy.rs` and `tests/endpoints.rs`
//! for what its `Workspaces` layout shares with the tree: the search, the
//! status filters, the labels, and the host rows.

use super::*;

#[test]
fn only_workspaces_are_listed_until_a_search_needs_their_panes() {
    let mut snapshot = snapshot();
    snapshot.agents = vec![agent("w1:p1", "reviewer", AgentStatus::Blocked)];
    let hosts = local(&snapshot);

    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "", None));
    assert_eq!(labels(&listed), ["repo"]);
    assert_eq!(listed[0].target, workspace("local", "w1"));
    assert_eq!(listed[0].meta, "main", "a workspace shows its branch");
    assert_eq!((listed[0].depth, listed[0].status), (0, None));

    // A search lists the matching panes under their workspace.
    let searched = rows(&hosts, &[], &view(Layout::Workspaces, "shell", None));
    assert_eq!(labels(&searched), ["repo", "shell"]);
    assert_eq!(searched[1].target, pane("local", "w1:p1"));
    assert_eq!(searched[1].depth, 1);
    assert_eq!(searched[1].status, Some(AgentStatus::Blocked));
    assert_eq!(searched[1].meta, "/repo");

    // So does a status filter, with no search at all.
    let filtered = rows(
        &hosts,
        &[],
        &view(Layout::Workspaces, "", Some(StatusFilter::Blocked)),
    );
    assert_eq!(labels(&filtered), ["repo", "shell"]);
    let none = rows(
        &hosts,
        &[],
        &view(Layout::Workspaces, "", Some(StatusFilter::Working)),
    );
    assert!(none.is_empty());
}

#[test]
fn a_search_matches_non_adjacent_words_without_losing_the_pane_target() {
    let mut snapshot = snapshot();
    snapshot.panes[0].label = Some("alpha beta gamma".into());
    let hosts = local(&snapshot);
    for (query, matches) in [
        ("alpha gamma", true),
        ("  ALP\tGAM  ", true),
        ("gamma alpha", true),
        ("beta gamma", true),
        ("alpha missing", false),
        ("alphagamma", false),
    ] {
        let view = view(Layout::Workspaces, query, None);
        let listed = rows(&hosts, &[], &view);
        assert_eq!(
            chosen(&listed, &view),
            matches.then(|| pane("local", "w1:p1")),
            "query={query:?}"
        );
    }
}

#[test]
fn a_search_reads_the_panes_ancestors_and_keeps_split_panes_apart() {
    let mut snapshot = snapshot();
    snapshot.tabs[0].label = "review".into();
    snapshot.tabs[0].custom_label = true;
    let mut second = snapshot.panes[0].clone();
    second.pane_id = "w1:p2".into();
    second.label = None;
    second.cwd = Some("/repo/api".into());
    second.foreground_cwd = None;
    second.focused = false;
    snapshot.panes.push(second);
    snapshot.agents = vec![agent("w1:p2", "helper", AgentStatus::Working)];
    let hosts = local(&snapshot);
    let search = |query: &str| {
        let listed = rows(&hosts, &[], &view(Layout::Workspaces, query, None));
        listed.into_iter().map(|row| row.label).collect::<Vec<_>>()
    };

    // The tab's name, the workspace's, and its branch each bring both panes.
    for query in ["review", "repo", "main"] {
        assert_eq!(
            search(query),
            ["repo", "review · shell · 1", "review · helper · 2"],
            "{query}"
        );
    }
    // A pane's own directory, agent, or id brings that pane alone.
    for query in ["api", "helper", "pi", "w1:p2"] {
        assert_eq!(search(query), ["repo", "review · helper · 2"], "{query}");
    }
}

#[test]
fn unnamed_terminals_on_numbered_tabs_are_told_apart() {
    let mut snapshot = snapshot();
    snapshot.tabs[0].label = "1".into();
    snapshot.panes[0].label = None;
    add_tab(&mut snapshot, 2, "2", false);
    add_tab(&mut snapshot, 3, "logs", true);
    let hosts = local(&snapshot);

    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "repo", None));

    assert_eq!(
        labels(&listed),
        ["repo", "terminal · 1", "terminal · 2", "logs"]
    );
}

#[test]
fn an_empty_workspace_stays_searchable_without_a_status_filter() {
    let mut snapshot = snapshot();
    snapshot.tabs.clear();
    snapshot.panes.clear();
    let hosts = local(&snapshot);
    for (query, filter, expected) in [
        ("", None, true),
        ("repo", None, true),
        ("main", None, true),
        ("missing", None, false),
        ("main", Some(StatusFilter::Idle), false),
        ("", Some(StatusFilter::Working), false),
    ] {
        let view = view(Layout::Workspaces, query, filter);
        let listed = rows(&hosts, &[], &view);
        assert_eq!(
            listed.len(),
            usize::from(expected),
            "query={query:?}, filter={filter:?}"
        );
        assert_eq!(
            chosen(&listed, &view),
            expected.then(|| workspace("local", "w1"))
        );
    }
}

#[test]
fn hosts_get_a_row_only_when_there_are_several() {
    let local_snapshot = snapshot();
    let mut build = snapshot();
    build.workspaces[0].label = "release".into();
    let hosts = federated(&local_snapshot, Some(&build));

    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "", None));
    assert_eq!(labels(&listed), ["Local", "repo", "Build", "release"]);
    assert_eq!(
        listed[2].target,
        Target::Host {
            endpoint: BUILD.into()
        }
    );
    assert_eq!(listed[3].target, workspace(BUILD, "w1"));
    assert_eq!(
        listed.iter().map(|row| row.depth).collect::<Vec<_>>(),
        [0, 1, 0, 1]
    );
    // A search nests the panes one step further, under their own host
    // although both have a `w1:p1`.
    let searched = rows(&hosts, &[], &view(Layout::Workspaces, "shell", None));
    assert_eq!(
        searched
            .iter()
            .map(|row| (row.label.as_str(), row.depth, row.target.endpoint()))
            .collect::<Vec<_>>(),
        [
            ("Local", 0, "local"),
            ("repo", 1, "local"),
            ("shell", 2, "local"),
            ("Build", 0, BUILD),
            ("release", 1, BUILD),
            ("shell", 2, BUILD),
        ]
    );
    // A host's name finds everything on it.
    let by_host = rows(&hosts, &[], &view(Layout::Workspaces, "build", None));
    assert_eq!(labels(&by_host), ["Build", "release", "shell"]);

    let alone = rows(
        &local(&local_snapshot),
        &[],
        &view(Layout::Workspaces, "", None),
    );
    assert!(
        alone
            .iter()
            .all(|row| !matches!(row.target, Target::Host { .. }))
    );
}

#[test]
fn a_saved_host_is_listed_before_its_first_snapshot_and_a_stale_one_is_marked() {
    let local_snapshot = snapshot();
    let mut hosts = federated(&local_snapshot, None);
    hosts[1].online = false;

    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "", None));
    assert_eq!(labels(&listed), ["Local", "repo", "Build"]);
    assert!(listed[2].stale && !listed[0].stale && !listed[1].stale);

    // What a host last sent is still listed while it is away, marked stale.
    let build = snapshot();
    hosts[1].snapshot = Some(&build);
    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "shell", None));
    assert_eq!(
        listed
            .iter()
            .filter(|row| row.target.endpoint() == BUILD)
            .map(|row| row.stale)
            .collect::<Vec<_>>(),
        [true, true, true]
    );
}

#[test]
fn the_default_selection_is_the_first_pane_and_a_lost_one_selects_nothing() {
    let snapshot = snapshot();
    let hosts = local(&snapshot);
    let searched = rows(&hosts, &[], &view(Layout::Workspaces, "shell", None));
    assert_eq!(selected_index(&searched, None), Some(1));
    assert_eq!(
        selected_index(&searched, Some(&workspace("local", "w1"))),
        Some(0)
    );
    assert_eq!(
        selected_index(&searched, Some(&pane("local", "gone"))),
        None
    );
    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "", None));
    assert_eq!(selected_index(&listed, None), Some(0), "no pane is listed");
    assert_eq!(selected_index(&[], None), None);
}

#[test]
fn left_and_right_jump_to_the_first_pane_of_the_next_workspace_with_panes() {
    let mut snapshot = snapshot();
    for (id, label) in [("w2", "empty"), ("w3", "api")] {
        let mut workspace = snapshot.workspaces[0].clone();
        workspace.workspace_id = id.into();
        workspace.label = label.into();
        workspace.branch = Some("main".into());
        snapshot.workspaces.push(workspace);
    }
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w3:t1".into();
    tab.workspace_id = "w3".into();
    snapshot.tabs.push(tab);
    for id in ["w3:p1", "w3:p2"] {
        let mut pane = snapshot.panes[0].clone();
        pane.pane_id = id.into();
        pane.tab_id = "w3:t1".into();
        snapshot.panes.push(pane);
    }
    let hosts = local(&snapshot);
    // Every space is on `main`, so the search lists all of them.
    let listed = rows(&hosts, &[], &view(Layout::Workspaces, "main", None));
    assert_eq!(
        labels(&listed),
        [
            "repo",
            "shell",
            "empty",
            "api",
            "main · shell · 1",
            "main · shell · 2"
        ]
    );

    assert_eq!(workspace_jump(&listed, 1, true), Some(4));
    assert_eq!(workspace_jump(&listed, 0, true), Some(4));
    assert_eq!(workspace_jump(&listed, 5, false), Some(1));
    assert_eq!(workspace_jump(&listed, 4, true), None, "nothing further");
    assert_eq!(workspace_jump(&listed, 1, false), None);
    assert_eq!(workspace_jump(&listed, 9, true), None, "no such row");
}
