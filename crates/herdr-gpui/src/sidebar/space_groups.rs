//! Space groups in the spaces list: which rows one host shows, in what
//! order, and where a header or a divider starts a run.
//!
//! Mirrors the Herdr fork's `src/client/shell/space_groups.rs`, with the group
//! of a worktree family from its `src/space_order.rs` (`anchor`, `group`);
//! their cases are ported in `space_groups/tests/rows.rs`.
//!
//! A space joins a group through the `space_group` workspace token, which the
//! daemon's `workspace.set_group` sets. The sidebar never reorders spaces: it
//! shows the server order and starts a collapsible header wherever the group
//! changes, with a divider where ungrouped spaces follow a group. A worktree
//! family takes its parent checkout's group. Runs of one group share a
//! collapse key, so collapsing either collapses both.
//!
//! The Herdr fork's terminal client also draws pinned sections above the
//! list; this client does not show them.
//!
//! Collapse state is this client's own, in memory, per host: the keys here
//! live in the same set as the collapsed worktree repositories, namespaced
//! apart from them.
//!
//! Upstream's spaces loop in `render.rs` lists [`sidebar_rows`], drawing each
//! marker through `group_row` and previewing a drop through `drop_preview`.
//! `sidebar.rs` imports [`visible_workspace_entries`] in place of upstream's,
//! so navigation and the collapsed rail step through the same spaces.

use super::{agents::status_priority, workspaces};
use herdr_client::protocol::{AgentStatus, ClientShellWorkspace};
use std::collections::HashSet;

mod drop_preview;
mod group_row;

#[cfg(test)]
mod tests;

/// Workspace metadata key that names a space's group.
pub(super) const GROUP_TOKEN: &str = "space_group";

/// Collapse state key of a group, namespaced apart from worktree repo keys.
pub(super) fn collapse_key(name: &str) -> String {
    format!("group:{name}")
}

/// What grouping needs to know about one space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Space<'a> {
    /// Repo key and whether the space is a linked worktree.
    pub(super) worktree: Option<(&'a str, bool)>,
    /// The space's own `space_group` token.
    pub(super) group: Option<&'a str>,
}

impl<'a> Space<'a> {
    fn of(workspace: &'a ClientShellWorkspace) -> Self {
        Self {
            worktree: workspace
                .worktree
                .as_ref()
                .map(|worktree| (worktree.key.as_str(), worktree.is_linked_worktree)),
            group: workspace
                .tokens
                .iter()
                .find(|(key, _)| key == GROUP_TOKEN)
                .map(|(_, name)| name.as_str()),
        }
    }
}

/// The index of the space whose group `index` shares: the first non-linked
/// checkout of a repo with at least two open spaces, or `index` itself.
pub(super) fn anchor(spaces: &[Space<'_>], index: usize) -> usize {
    let Some((key, _)) = spaces[index].worktree else {
        return index;
    };
    let mut checkout = None;
    let mut count = 0;
    for (member, space) in spaces.iter().enumerate() {
        let Some((other, linked)) = space.worktree else {
            continue;
        };
        if other != key {
            continue;
        }
        count += 1;
        if checkout.is_none() && !linked {
            checkout = Some(member);
        }
    }
    match checkout {
        Some(checkout) if count >= 2 => checkout,
        _ => index,
    }
}

/// The group of the family `index` belongs to.
pub(super) fn group<'a>(spaces: &[Space<'a>], index: usize) -> Option<&'a str> {
    spaces[anchor(spaces, index)]
        .group
        .filter(|group| !group.is_empty())
}

/// A group header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Group<'a> {
    pub(super) name: &'a str,
    pub(super) collapsed: bool,
}

/// Starts a run: a group header, or with `group: None` the divider before
/// ungrouped spaces that follow a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Marker<'a> {
    pub(super) group: Option<Group<'a>>,
    /// Every member's workspace index in expanded order, including members
    /// hidden by a collapsed group or worktree family.
    pub(super) members: Vec<usize>,
}

impl Marker<'_> {
    /// The one status a collapsed header shows for its members: the most
    /// urgent of theirs.
    pub(super) fn status(&self, workspaces: &[ClientShellWorkspace]) -> AgentStatus {
        self.members
            .iter()
            .map(|&index| workspaces[index].agent_status)
            .max_by_key(|status| status_priority(*status))
            .unwrap_or(AgentStatus::Unknown)
    }
}

/// A space's own row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    /// Position in the snapshot's `workspaces`.
    pub(super) index: usize,
    /// A linked worktree under its repository's parent row.
    pub(super) indented: bool,
    /// The repository this row folds, on every parent of a worktree group.
    pub(super) fold: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SidebarRow<'a> {
    Marker(Marker<'a>),
    Workspace(Entry),
}

impl SidebarRow<'_> {
    /// A linked worktree's row under its repository's parent row; never a
    /// marker.
    pub(super) fn indented(&self) -> bool {
        matches!(self, Self::Workspace(entry) if entry.indented)
    }

    /// Whether the configured row gap separates `self` from `previous`, the
    /// row before it: a marker stays attached to the space below it, and a
    /// worktree child to its parent.
    pub(super) fn gap_after(&self, previous: &Self) -> bool {
        match (previous, self) {
            (Self::Marker(_), _) => false,
            (_, Self::Workspace(entry)) if entry.indented => false,
            _ => true,
        }
    }
}

/// Consecutive top-level units that share a group, or share having none.
struct Run<'a> {
    name: Option<&'a str>,
    members: Vec<usize>,
}

/// The snapshot's spaces as runs, in the order the sidebar shows them. A unit
/// is a top-level row with the worktree children under it.
fn runs(workspaces: &[ClientShellWorkspace]) -> Vec<Run<'_>> {
    let spaces: Vec<_> = workspaces.iter().map(Space::of).collect();
    let mut runs = Vec::<Run<'_>>::new();
    for (index, child) in workspaces::workspace_entries(workspaces) {
        let name = group(&spaces, index);
        match runs.last_mut() {
            // A child follows its parent, and a family shares one group.
            Some(run) if child || run.name == name => run.members.push(index),
            _ => runs.push(Run {
                name,
                members: vec![index],
            }),
        }
    }
    runs
}

/// Sidebar rows for one host: its spaces in sidebar order with a marker
/// starting each run after the first ungrouped one. Without groups this is
/// the flat list, worktree groups aside.
///
/// `collapsed` holds this host's collapsed worktree repositories and groups.
pub(super) fn sidebar_rows<'a>(
    workspaces: &'a [ClientShellWorkspace],
    collapsed: &HashSet<String>,
) -> Vec<SidebarRow<'a>> {
    let mut rows = Vec::new();
    let runs = runs(workspaces);
    let mut run_of = vec![0; workspaces.len()];
    for (index, run) in runs.iter().enumerate() {
        for member in &run.members {
            run_of[*member] = index;
        }
    }
    let mut shown: Vec<Vec<Entry>> = vec![Vec::new(); runs.len()];
    for (index, indented, fold) in workspaces::visible_workspace_entries(workspaces, collapsed) {
        shown[run_of[index]].push(Entry {
            index,
            indented,
            fold,
        });
    }
    for (index, (run, entries)) in runs.into_iter().zip(shown).enumerate() {
        let collapsed = run
            .name
            .is_some_and(|name| collapsed.contains(&collapse_key(name)));
        if run.name.is_some() || index > 0 {
            rows.push(SidebarRow::Marker(Marker {
                group: run.name.map(|name| Group { name, collapsed }),
                members: run.members,
            }));
        }
        if collapsed {
            // A collapsed group still shows its focused space, as a top-level row.
            rows.extend(
                entries
                    .into_iter()
                    .find(|entry| workspaces[entry.index].focused)
                    .map(|entry| {
                        SidebarRow::Workspace(Entry {
                            index: entry.index,
                            indented: false,
                            fold: None,
                        })
                    }),
            );
        } else {
            rows.extend(entries.into_iter().map(SidebarRow::Workspace));
        }
    }
    rows
}

/// Each space's own row in sidebar order, as upstream's
/// [`workspaces::visible_workspace_entries`] gives them: `(index, child,
/// group)`. The sidebar imports this one under that name, so navigation and
/// the collapsed rail step through what the spaces list shows, one mark per
/// space; group headers and dividers have none.
pub(super) fn visible_workspace_entries(
    workspaces: &[ClientShellWorkspace],
    collapsed: &HashSet<String>,
) -> Vec<(usize, bool, Option<String>)> {
    sidebar_rows(workspaces, collapsed)
        .into_iter()
        .filter_map(|row| match row {
            SidebarRow::Workspace(entry) => Some((entry.index, entry.indented, entry.fold)),
            SidebarRow::Marker(_) => None,
        })
        .collect()
}
