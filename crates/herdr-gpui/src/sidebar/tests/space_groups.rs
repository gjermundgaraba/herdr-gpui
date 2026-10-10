//! Space groups and pinned sections as rows. The cases follow the Herdr
//! fork's `src/client/shell/tests/space_groups.rs` and the `anchor` and
//! `group` cases of its `src/space_order.rs`. Where the fork's tests click and
//! compose a frame, these read the rows the frame would be built from; the
//! painted geometry is in `layout_tests/space_groups.rs`.

use super::*;
use crate::sidebar::space_groups::{
    Entry, GROUP_TOKEN, Group, Marker, SidebarRow, Space, anchor, collapse_key, group,
    pinned_collapse_key, sidebar_rows, workspace_rows,
};
use herdr_client::protocol::{ClientShellPinnedSection, ClientShellWorktree};
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

/// What the daemon sends for `[[ui.sidebar.spaces.pinned]]` sections.
fn pin(snapshot: &mut ClientShellSnapshot, sections: &[(&str, Option<&str>, &[&str])]) {
    snapshot.pinned_sections = sections
        .iter()
        .map(|&(title, fg, members)| ClientShellPinnedSection {
            title: title.into(),
            fg: fg.map(Into::into),
            workspace_ids: members.iter().map(|&id| id.into()).collect(),
        })
        .collect();
}

fn collapsed(keys: &[String]) -> HashSet<String> {
    keys.iter().cloned().collect()
}

fn id(snapshot: &ClientShellSnapshot, index: usize) -> &str {
    &snapshot.workspaces[index].workspace_id
}

/// The spaces navigation steps through, in order.
fn navigation<'a>(snapshot: &'a ClientShellSnapshot, collapsed: &HashSet<String>) -> Vec<&'a str> {
    workspace_rows(snapshot, collapsed)
        .into_iter()
        .map(|(index, _)| id(snapshot, index))
        .collect()
}

/// Every header as `(collapse key, member ids)`, in order.
fn headers<'a>(
    snapshot: &'a ClientShellSnapshot,
    collapsed: &HashSet<String>,
) -> Vec<(String, Vec<&'a str>)> {
    sidebar_rows(snapshot, collapsed)
        .into_iter()
        .filter_map(|row| match row {
            SidebarRow::Marker(Marker {
                group: Some(group),
                members,
            }) => Some((
                group.key(),
                members.iter().map(|&index| id(snapshot, index)).collect(),
            )),
            _ => None,
        })
        .collect()
}

/// The rows in order, spelled so a list reads at a glance: `▾ key` or
/// `▸ key` for a header, `---` for the divider, a space's id for its own row
/// (`  id` when indented), and `=id` for a mirror.
fn drawn(snapshot: &ClientShellSnapshot, collapsed: &HashSet<String>) -> Vec<String> {
    sidebar_rows(snapshot, collapsed)
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
                group.key()
            ),
            SidebarRow::Workspace(Entry {
                index, indented, ..
            }) => format!(
                "{}{}",
                if indented { "  " } else { "" },
                id(snapshot, index)
            ),
            SidebarRow::Mirror { index, .. } => format!("={}", id(snapshot, index)),
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
        sidebar_rows(&snapshot, &none)[5],
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
    let rows = sidebar_rows(&snapshot, &work);
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
            pinned: false,
            fg: None,
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
    let rows = sidebar_rows(&snapshot, &none);
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
    let rows = sidebar_rows(&snapshot, &collapsed(&[collapse_key("work")]));
    assert_eq!(
        rows[1..],
        [SidebarRow::Workspace(Entry {
            index: 1,
            indented: false,
            fold: None,
        })]
    );
}

/// Server order: ws_1 is in review, ws_3 is both prioritized and in review,
/// ws_4 is prioritized, ws_2 and ws_5 are in no section. The daemon has
/// already matched tokens against the sections; only its lists are read.
fn mixed_pinned() -> ClientShellSnapshot {
    let mut snapshot = spaces(&[
        &[("stage", "review")],
        &[],
        &[("prio", "1"), ("stage", "review")],
        &[("prio", "2")],
        &[("stage", "draft"), ("prio", "")],
    ]);
    pin(
        &mut snapshot,
        &[
            ("Priority", Some("#e5c07b"), &["ws_3", "ws_4"]),
            ("Review", None, &["ws_1", "ws_3"]),
        ],
    );
    snapshot
}

#[test]
fn pinned_sections_without_members_are_not_shown() {
    let mut snapshot = spaces(&[&[], &[("prio", "")], &[("stage", "draft")]]);
    let none = HashSet::new();
    // The daemon leaves a section no space matches out of the snapshot.
    assert_eq!(drawn(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
    // A section naming only spaces this snapshot does not list shows nothing
    // either, and no divider follows it.
    pin(
        &mut snapshot,
        &[("Gone", None, &["ws_9"]), ("Empty", None, &[])],
    );
    assert_eq!(drawn(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
    // Tokens alone pin nothing: matching them is the daemon's.
    let snapshot = spaces(&[&[("prio", "1")]]);
    assert_eq!(drawn(&snapshot, &none), ["ws_1"]);
}

#[test]
fn pinned_sections_mirror_their_members_in_the_daemon_s_order_above_the_list() {
    let snapshot = mixed_pinned();
    let none = HashSet::new();
    assert_eq!(
        drawn(&snapshot, &none),
        [
            "\u{25be} pinned:Priority",
            "=ws_3",
            "=ws_4",
            "\u{25be} pinned:Review",
            "=ws_1",
            "=ws_3",
            // The regular list follows behind a divider, unchanged.
            "---",
            "ws_1",
            "ws_2",
            "ws_3",
            "ws_4",
            "ws_5",
        ]
    );
    assert_eq!(
        headers(&snapshot, &none),
        [
            ("pinned:Priority".to_owned(), vec!["ws_3", "ws_4"]),
            ("pinned:Review".to_owned(), vec!["ws_1", "ws_3"]),
        ]
    );
    let rows = sidebar_rows(&snapshot, &none);
    let SidebarRow::Marker(priority) = &rows[0] else {
        panic!("the first section leads the list");
    };
    assert_eq!(
        priority.group,
        Some(Group {
            name: "Priority",
            collapsed: false,
            pinned: true,
            fg: Some("#e5c07b"),
        })
    );
    // Each copy names its section, which keeps the two of ws_3 apart.
    assert_eq!(
        rows[1],
        SidebarRow::Mirror {
            index: 2,
            section: 0
        }
    );
    assert_eq!(
        rows[5],
        SidebarRow::Mirror {
            index: 2,
            section: 1
        }
    );
    // Navigation visits every space once, in server order.
    assert_eq!(
        navigation(&snapshot, &none),
        ["ws_1", "ws_2", "ws_3", "ws_4", "ws_5"]
    );
}

#[test]
fn a_member_the_snapshot_does_not_list_is_skipped() {
    let mut snapshot = spaces(&[&[], &[]]);
    pin(&mut snapshot, &[("Priority", None, &["ws_9", "ws_2"])]);
    assert_eq!(
        drawn(&snapshot, &HashSet::new()),
        ["\u{25be} pinned:Priority", "=ws_2", "---", "ws_1", "ws_2"]
    );
    assert_eq!(
        headers(&snapshot, &HashSet::new()),
        [("pinned:Priority".to_owned(), vec!["ws_2"])]
    );
}

#[test]
fn pinned_sections_precede_groups_and_keep_group_headers() {
    let mut snapshot = spaces(&[
        &[(GROUP_TOKEN, "work"), ("prio", "1")],
        &[(GROUP_TOKEN, "work")],
        &[],
    ]);
    pin(&mut snapshot, &[("Priority", None, &["ws_1"])]);
    let none = HashSet::new();
    // A grouped first run starts with its own header, not a divider.
    assert_eq!(
        drawn(&snapshot, &none),
        [
            "\u{25be} pinned:Priority",
            "=ws_1",
            "\u{25be} group:work",
            "ws_1",
            "ws_2",
            "---",
            "ws_3",
        ]
    );
    assert_eq!(navigation(&snapshot, &none), ["ws_1", "ws_2", "ws_3"]);
}

#[test]
fn linked_worktree_mirrors_render_flat() {
    let mut snapshot = spaces(&[&[], &[], &[("prio", "1")]]);
    family(&mut snapshot, 1, &[3]);
    pin(&mut snapshot, &[("Priority", None, &["ws_3"])]);
    let none = HashSet::new();
    assert_eq!(
        drawn(&snapshot, &none),
        [
            "\u{25be} pinned:Priority",
            "=ws_3",
            "---",
            "ws_1",
            "  ws_3",
            "ws_2",
        ]
    );
    assert_eq!(navigation(&snapshot, &none), ["ws_1", "ws_3", "ws_2"]);
    // Folding the repository hides the space's own row, not its mirror.
    assert_eq!(
        drawn(&snapshot, &collapsed(&["repo".to_owned()])),
        ["\u{25be} pinned:Priority", "=ws_3", "---", "ws_1", "ws_2"]
    );
}

#[test]
fn collapsing_a_pinned_section_hides_only_its_mirrors() {
    let mut snapshot = mixed_pinned();
    snapshot.workspaces[3].agent_status = AgentStatus::Blocked;
    snapshot.workspaces[2].focused = true;
    snapshot.workspaces[0].focused = false;
    let priority = collapsed(&[pinned_collapse_key("Priority")]);
    // Even the focused space's mirror hides; the Review section is untouched,
    // and so is the regular list.
    assert_eq!(
        drawn(&snapshot, &priority),
        [
            "\u{25b8} pinned:Priority",
            "\u{25be} pinned:Review",
            "=ws_1",
            "=ws_3",
            "---",
            "ws_1",
            "ws_2",
            "ws_3",
            "ws_4",
            "ws_5",
        ]
    );
    assert_eq!(
        navigation(&snapshot, &priority),
        ["ws_1", "ws_2", "ws_3", "ws_4", "ws_5"]
    );
    let rows = sidebar_rows(&snapshot, &priority);
    let SidebarRow::Marker(header) = &rows[0] else {
        panic!("the collapsed section keeps its header");
    };
    assert_eq!(header.members, [2, 3]);
    assert_eq!(header.status(&snapshot.workspaces), AgentStatus::Blocked);
}

#[test]
fn a_group_and_a_pinned_section_with_one_name_collapse_separately() {
    let mut snapshot = spaces(&[&[(GROUP_TOKEN, "Priority"), ("prio", "1")], &[]]);
    pin(&mut snapshot, &[("Priority", None, &["ws_1"])]);
    assert_ne!(collapse_key("Priority"), pinned_collapse_key("Priority"));
    assert_eq!(
        drawn(&snapshot, &collapsed(&[collapse_key("Priority")])),
        [
            "\u{25be} pinned:Priority",
            "=ws_1",
            "\u{25b8} group:Priority",
            "ws_1",
            "---",
            "ws_2",
        ]
    );
    assert_eq!(
        drawn(&snapshot, &collapsed(&[pinned_collapse_key("Priority")])),
        [
            "\u{25b8} pinned:Priority",
            "\u{25be} group:Priority",
            "ws_1",
            "---",
            "ws_2",
        ]
    );
    // Neither key folds a worktree repository of the same name, nor the
    // other way around.
    assert_eq!(
        drawn(&snapshot, &collapsed(&["Priority".to_owned()])).len(),
        6
    );
}

#[test]
fn gaps_attach_a_marker_to_the_row_below_and_a_child_to_its_parent() {
    let mut snapshot = grouped(&[Some("work"), Some("work"), None]);
    family(&mut snapshot, 1, &[2]);
    pin(&mut snapshot, &[("Priority", None, &["ws_1", "ws_3"])]);
    let rows = sidebar_rows(&snapshot, &HashSet::new());
    let gaps: Vec<_> = rows
        .windows(2)
        .map(|pair| pair[1].gap_after(&pair[0]))
        .collect();
    // header, =ws_1, =ws_3, header work, ws_1, child ws_2, divider, ws_3
    assert_eq!(
        gaps,
        [
            // A header's first row hugs it.
            false, // A mirror clears the mirror above it.
            true,  // The next header clears the last mirror.
            true,  // Its first row hugs it again.
            false, // A worktree child hugs its parent.
            false, // The divider clears the group's last row.
            true,  // The ungrouped space hugs the divider.
            false,
        ]
    );
}
