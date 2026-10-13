//! Space groups as rows. The cases follow the Herdr
//! fork's `src/client/shell/tests/space_groups.rs` and the `anchor` and
//! `group` cases of its `src/space_order.rs`. Where the fork's tests click and
//! compose a frame, these read the rows the frame would be built from; the
//! painted geometry is in `painted.rs`.

use crate::sidebar::layout_tests;
use crate::sidebar::space_groups::{
    Entry, GROUP_TOKEN, Group, Marker, SidebarRow, Space, anchor, collapse_key, group,
    sidebar_rows, visible_workspace_entries,
};
use herdr_client::protocol::{
    AgentStatus, ClientShellSnapshot, ClientShellWorkspace, ClientShellWorktree,
};
use std::collections::HashSet;

/// Spaces `ws_1..`, each with its tokens, the first one focused.
fn spaces(tokens: &[&[(&str, &str)]]) -> ClientShellSnapshot {
    let mut snapshot = layout_tests::snapshot(1);
    let template = snapshot.workspaces[0].clone();
    snapshot.workspaces = tokens
        .iter()
        .enumerate()
        .map(|(index, tokens)| ClientShellWorkspace {
            workspace_id: format!("ws_{}", index + 1),
            number: index + 1,
            label: format!("space-{}", index + 1),
            focused: index == 0,
            tokens: tokens
                .iter()
                .map(|&(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            ..template.clone()
        })
        .collect();
    snapshot
}

/// Spaces in the given groups, `None` for an ungrouped one.
fn grouped(groups: &[Option<&str>]) -> ClientShellSnapshot {
    let tokens: Vec<Vec<(&str, &str)>> = groups
        .iter()
        .map(|group| {
            group
                .map(|group| (GROUP_TOKEN, group))
                .into_iter()
                .collect()
        })
        .collect();
    let tokens: Vec<&[(&str, &str)]> = tokens.iter().map(Vec::as_slice).collect();
    spaces(&tokens)
}

/// Makes `ws_{parent}` a checkout of `repo` and each of `linked` a linked
/// worktree of it. Numbers count from one, as the ids do.
fn family(snapshot: &mut ClientShellSnapshot, parent: usize, linked: &[usize]) {
    let members = [(parent, false)]
        .into_iter()
        .chain(linked.iter().map(|&index| (index, true)));
    for (number, is_linked_worktree) in members {
        snapshot.workspaces[number - 1].worktree = Some(ClientShellWorktree {
            key: "repo".into(),
            label: "repo".into(),
            is_linked_worktree,
        });
    }
}

fn collapsed(keys: &[String]) -> HashSet<String> {
    keys.iter().cloned().collect()
}

fn id(snapshot: &ClientShellSnapshot, index: usize) -> &str {
    &snapshot.workspaces[index].workspace_id
}

/// The spaces navigation steps through, in order.
fn navigation<'a>(snapshot: &'a ClientShellSnapshot, collapsed: &HashSet<String>) -> Vec<&'a str> {
    visible_workspace_entries(&snapshot.workspaces, collapsed)
        .into_iter()
        .map(|(index, _, _)| id(snapshot, index))
        .collect()
}

/// Every header as `(collapse key, member ids)`, in order.
fn headers<'a>(
    snapshot: &'a ClientShellSnapshot,
    collapsed: &HashSet<String>,
) -> Vec<(String, Vec<&'a str>)> {
    sidebar_rows(&snapshot.workspaces, collapsed)
        .into_iter()
        .filter_map(|row| match row {
            SidebarRow::Marker(Marker {
                group: Some(group),
                members,
            }) => Some((
                collapse_key(group.name),
                members.iter().map(|&index| id(snapshot, index)).collect(),
            )),
            _ => None,
        })
        .collect()
}

/// The rows in order, spelled so a list reads at a glance: `▾ key` or
/// `▸ key` for a header, `---` for the divider, a space's id for its own row
/// (`  id` when indented).
fn drawn(snapshot: &ClientShellSnapshot, collapsed: &HashSet<String>) -> Vec<String> {
    sidebar_rows(&snapshot.workspaces, collapsed)
        .into_iter()
        .map(|row| match row {
            SidebarRow::Marker(Marker { group: None, .. }) => "---".to_owned(),
            SidebarRow::Marker(Marker {
                group: Some(group), ..
            }) => format!(
                "{} {}",
                if group.collapsed {
                    '\u{25b8}'
                } else {
                    '\u{25be}'
                },
                collapse_key(group.name)
            ),
            SidebarRow::Workspace(Entry {
                index, indented, ..
            }) => format!(
                "{}{}",
                if indented { "  " } else { "" },
                id(snapshot, index)
            ),
        })
        .collect()
}

#[test]
fn a_linked_child_resolves_to_its_checkout_family() {
    let space = |repo| Space {
        worktree: repo,
        group: None,
    };
    let spaces = [
        space(Some(("repo", true))),
        space(None),
        space(Some(("repo", false))),
        space(Some(("solo", true))),
    ];
    assert_eq!(anchor(&spaces, 0), 2);
    assert_eq!(anchor(&spaces, 1), 1);
    assert_eq!(anchor(&spaces, 2), 2);
    // A linked worktree whose checkout is not open stands alone.
    assert_eq!(anchor(&spaces, 3), 3);
    // So does a checkout that is its repo's only open space.
    assert_eq!(anchor(&[space(Some(("repo", false)))], 0), 0);
}

#[test]
fn a_family_takes_its_checkouts_group() {
    let space = |group, repo| Space {
        worktree: repo,
        group,
    };
    let spaces = [
        space(Some("a"), Some(("repo", false))),
        space(Some("b"), Some(("repo", true))),
        space(Some(""), None),
    ];
    assert_eq!(group(&spaces, 0), Some("a"));
    assert_eq!(group(&spaces, 1), Some("a"));
    // An empty token names no group.
    assert_eq!(group(&spaces, 2), None);
}

#[test]
fn runs_render_in_server_order_with_a_divider_before_ungrouped_spaces() {
    let snapshot = grouped(&[Some("work"), Some("work"), Some("play"), None]);
    let none = HashSet::new();
    assert_eq!(
        navigation(&snapshot, &none),
        ["ws_1", "ws_2", "ws_3", "ws_4"]
    );
    assert_eq!(
        drawn(&snapshot, &none),
        [
            "\u{25be} group:work",
            "ws_1",
            "ws_2",
            "\u{25be} group:play",
            "ws_3",
            "---",
            "ws_4",
        ]
    );
    assert_eq!(
        headers(&snapshot, &none),
        [
            ("group:work".to_owned(), vec!["ws_1", "ws_2"]),
            ("group:play".to_owned(), vec!["ws_3"]),
        ]
    );
    // The divider's run is the ungrouped spaces after it.
    assert_eq!(
        sidebar_rows(&snapshot.workspaces, &none)[5],
        SidebarRow::Marker(Marker {
            group: None,
            members: vec![3],
        })
    );
}

#[test]
fn without_group_tokens_the_sidebar_stays_flat() {
    let snapshot = grouped(&[None, None, None]);
    let none = HashSet::new();
    assert_eq!(navigation(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
    assert_eq!(drawn(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
    // An empty token, or another token, groups nothing either.
    let snapshot = spaces(&[&[(GROUP_TOKEN, "")], &[("team", "platform")]]);
    assert_eq!(drawn(&snapshot, &none), ["ws_1", "ws_2"]);
}

#[test]
fn leading_ungrouped_spaces_need_no_divider() {
    let snapshot = grouped(&[None, Some("a"), None]);
    assert_eq!(
        drawn(&snapshot, &HashSet::new()),
        ["ws_1", "\u{25be} group:a", "ws_2", "---", "ws_3"]
    );
}

#[test]
fn a_split_group_shows_a_header_for_each_run_without_reordering() {
    let snapshot = grouped(&[Some("work"), None, Some("work")]);
    let none = HashSet::new();
    assert_eq!(navigation(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
    assert_eq!(
        headers(&snapshot, &none),
        [
            ("group:work".to_owned(), vec!["ws_1"]),
            ("group:work".to_owned(), vec!["ws_3"]),
        ]
    );
    // Both runs share the collapse key, so collapsing either collapses both.
    // The first run keeps its focused space; the second shows only a header.
    assert_eq!(
        drawn(&snapshot, &collapsed(&[collapse_key("work")])),
        [
            "\u{25b8} group:work",
            "ws_1",
            "---",
            "ws_2",
            "\u{25b8} group:work",
        ]
    );
}

#[test]
fn a_collapsed_group_keeps_its_focused_member_and_shows_its_most_urgent_status() {
    let mut snapshot = grouped(&[Some("work"), Some("work"), Some("play"), None]);
    snapshot.workspaces[0].agent_status = AgentStatus::Idle;
    snapshot.workspaces[1].agent_status = AgentStatus::Blocked;
    let work = collapsed(&[collapse_key("work")]);
    assert_eq!(navigation(&snapshot, &work), ["ws_1", "ws_3", "ws_4"]);
    assert_eq!(
        drawn(&snapshot, &work),
        [
            "\u{25b8} group:work",
            "ws_1",
            "\u{25be} group:play",
            "ws_3",
            "---",
            "ws_4",
        ]
    );
    // The header still counts every member, hidden ones included, and stands
    // in for them with the status that most wants attention.
    let rows = sidebar_rows(&snapshot.workspaces, &work);
    let SidebarRow::Marker(header) = &rows[0] else {
        panic!("a header leads the list");
    };
    assert_eq!(header.members, [0, 1]);
    assert_eq!(header.status(&snapshot.workspaces), AgentStatus::Blocked);
    assert_eq!(
        header.group,
        Some(Group {
            name: "work",
            collapsed: true,
        })
    );

    // A collapsed group without the focused space shows its header alone.
    let both = collapsed(&[collapse_key("work"), collapse_key("play")]);
    assert_eq!(navigation(&snapshot, &both), ["ws_1", "ws_4"]);
}

#[test]
fn worktree_children_follow_their_parent_group() {
    // The linked worktree carries another group's token and sits after an
    // ungrouped space in server order; its checkout's group decides.
    let mut snapshot = grouped(&[Some("work"), None, Some("play")]);
    family(&mut snapshot, 1, &[3]);
    let none = HashSet::new();
    assert_eq!(navigation(&snapshot, &none), ["ws_1", "ws_3", "ws_2"]);
    assert_eq!(
        headers(&snapshot, &none),
        [("group:work".to_owned(), vec!["ws_1", "ws_3"])]
    );
    assert_eq!(
        drawn(&snapshot, &none),
        ["\u{25be} group:work", "ws_1", "  ws_3", "---", "ws_2"]
    );
    // Folding the repository hides the child and keeps the group's header.
    let folded = collapsed(&["repo".to_owned()]);
    assert_eq!(
        drawn(&snapshot, &folded),
        ["\u{25be} group:work", "ws_1", "---", "ws_2"]
    );
    assert_eq!(headers(&snapshot, &folded)[0].1, ["ws_1", "ws_3"]);
    // The parent row still offers the repository's fold inside a group.
    let rows = sidebar_rows(&snapshot.workspaces, &none);
    assert_eq!(
        rows[1],
        SidebarRow::Workspace(Entry {
            index: 0,
            indented: false,
            fold: Some("repo".into()),
        })
    );
}

#[test]
fn a_collapsed_group_shows_a_focused_worktree_child_as_a_top_level_row() {
    let mut snapshot = grouped(&[Some("work"), Some("work")]);
    family(&mut snapshot, 1, &[2]);
    snapshot.workspaces[0].focused = false;
    snapshot.workspaces[1].focused = true;
    let rows = sidebar_rows(&snapshot.workspaces, &collapsed(&[collapse_key("work")]));
    assert_eq!(
        rows[1..],
        [SidebarRow::Workspace(Entry {
            index: 1,
            indented: false,
            fold: None,
        })]
    );
}

#[test]
fn a_group_and_a_repository_with_one_name_collapse_separately() {
    let mut snapshot = grouped(&[Some("repo"), Some("repo"), None]);
    family(&mut snapshot, 1, &[2]);
    // The repository's key folds its worktree child, not the group.
    assert_eq!(
        drawn(&snapshot, &collapsed(&["repo".to_owned()])),
        ["\u{25be} group:repo", "ws_1", "---", "ws_3"]
    );
    // The group's key folds the group, not the repository.
    assert_eq!(
        drawn(&snapshot, &collapsed(&[collapse_key("repo")])),
        ["\u{25b8} group:repo", "ws_1", "---", "ws_3"]
    );
}

#[test]
fn gaps_attach_a_marker_to_the_row_below_and_a_child_to_its_parent() {
    let mut snapshot = grouped(&[Some("work"), Some("work"), None]);
    family(&mut snapshot, 1, &[2]);
    let rows = sidebar_rows(&snapshot.workspaces, &HashSet::new());
    let gaps: Vec<_> = rows
        .windows(2)
        .map(|pair| pair[1].gap_after(&pair[0]))
        .collect();
    // header work, ws_1, child ws_2, divider, ws_3
    assert_eq!(
        gaps,
        [
            false, // A header's first row hugs it.
            false, // A worktree child hugs its parent.
            true,  // The divider clears the group's last row.
            false, // The ungrouped space hugs the divider.
        ]
    );
}
