//! The rows the agent picker and the workspace list offer, from the retained
//! snapshots alone, with no window in it.
//!
//! It mirrors the Navigator of the Herdr fork's terminal client:
//! `navigator_rows` and `navigator_selected_index` in its
//! `src/client/shell/aggregate_navigation.rs`, and `agent_navigator_rows` in
//! its `src/client/shell/aggregate_navigation/agent_picker.rs`, for its
//! `Agents` and `Workspaces` layouts. A row is found by the same text,
//! labelled the same way, and carries the same target, so either client
//! opens the same pane for the same search. The cases are ported in
//! `tests/rows_agents.rs` and `tests/rows_workspaces.rs`.

use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellPane, ClientShellSnapshot, ClientShellTab,
};
use std::collections::HashMap;

/// Which rows a navigator lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Every agent pane across hosts as a flat list in priority order.
    Agents,
    /// Workspaces alone; their panes appear only while searching.
    Workspaces,
}

/// One agent status the list is narrowed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StatusFilter {
    Blocked,
    Working,
    Idle,
    Done,
}

impl StatusFilter {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Working => "working",
            Self::Idle => "idle",
            Self::Done => "done",
        }
    }

    fn matches(filter: Option<Self>, status: AgentStatus) -> bool {
        match filter {
            Some(Self::Blocked) => status == AgentStatus::Blocked,
            Some(Self::Working) => status == AgentStatus::Working,
            Some(Self::Idle) => status == AgentStatus::Idle,
            Some(Self::Done) => status == AgentStatus::Done,
            None => true,
        }
    }
}

/// Where choosing a row leads. Hosts are named by endpoint id, which stays
/// the same while the devices list changes under an open navigator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Host { endpoint: String },
    Workspace { endpoint: String, workspace: String },
    Pane { endpoint: String, pane: String },
}

impl Target {
    pub(super) fn endpoint(&self) -> &str {
        match self {
            Self::Host { endpoint }
            | Self::Workspace { endpoint, .. }
            | Self::Pane { endpoint, .. } => endpoint,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Row {
    /// How far the row is indented: a host, its workspaces, their panes.
    pub(super) depth: u8,
    pub(super) label: String,
    /// The pane's directory, or the workspace's branch.
    pub(super) meta: String,
    /// The agent's status; `None` for a host or a workspace.
    pub(super) status: Option<AgentStatus>,
    /// The host is not online: the row shows what it last sent.
    pub(super) stale: bool,
    pub(super) target: Target,
}

/// One host the navigator lists.
#[derive(Clone, Copy, Debug)]
pub(super) struct Host<'a> {
    pub(super) id: &'a str,
    pub(super) label: &'a str,
    /// The first host is the local one, which a label never names.
    pub(super) local: bool,
    pub(super) online: bool,
    pub(super) snapshot: Option<&'a ClientShellSnapshot>,
}

/// What the user asked the navigator to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct View {
    pub(super) layout: Layout,
    pub(super) query: String,
    pub(super) filter: Option<StatusFilter>,
    /// The selected row, by where it leads. `None` selects the first pane,
    /// or the first row when no pane is listed.
    pub(super) selected: Option<Target>,
}

impl View {
    pub(super) fn new(layout: Layout) -> Self {
        Self {
            layout,
            query: String::new(),
            filter: None,
            selected: None,
        }
    }
}

/// The rows `view` lists. `agents` are every host's agents in the agents
/// panel's priority order, each with its host's place in `hosts`.
pub(super) fn rows(
    hosts: &[Host<'_>],
    agents: &[(usize, &ClientShellAgent)],
    view: &View,
) -> Vec<Row> {
    match view.layout {
        Layout::Agents => agent_rows(hosts, agents, view),
        Layout::Workspaces => workspace_rows(hosts, view),
    }
}

/// What a pane row is called: one pane alone in its tab takes the tab's
/// place, and several are told apart by their position.
fn pane_label(
    workspace_label: &str,
    tab: &ClientShellTab,
    multiple_tabs: bool,
    tab_panes: usize,
    index: usize,
    pane: &ClientShellPane,
    agent: Option<&ClientShellAgent>,
) -> String {
    let agent_kind =
        agent.and_then(|agent| agent.agent.as_deref().or(agent.display_agent.as_deref()));
    let name = pane
        .label
        .as_deref()
        .or_else(|| agent.and_then(|agent| agent.name.as_deref()));
    let title = agent.and_then(|agent| {
        agent
            .title
            .as_deref()
            .or(agent.terminal_title_stripped.as_deref())
    });
    let tab_name =
        (tab.custom_label || tab.label.parse::<usize>().is_err()).then_some(tab.label.as_str());
    if tab_panes == 1 {
        return match name.or(tab_name).or(title) {
            Some(label) => label.to_owned(),
            None if multiple_tabs => {
                format!("{} · {}", agent_kind.unwrap_or("terminal"), tab.label)
            }
            None => workspace_label.to_owned(),
        };
    }
    let pane_name = name.or(title).or(agent_kind).unwrap_or("terminal");
    match tab_name {
        Some(tab_name) if tab_name != pane_name => {
            format!("{tab_name} · {pane_name} · {}", index + 1)
        }
        _ => format!("{pane_name} · {}", index + 1),
    }
}

/// Whether `value` holds every one of `words`, which are lowercased, in any
/// order and ignoring case; no words match everything. Both layouts search
/// this way, as the fork's navigator does since upstream Herdr's #4535.
fn matches_words(words: &[&str], value: &str) -> bool {
    if words.is_empty() {
        return true;
    }
    let value = value.to_lowercase();
    words.iter().all(|word| value.contains(word))
}

/// Hosts, their workspaces, and, while a search or a status filter is
/// active, the panes that match under them.
fn workspace_rows(hosts: &[Host<'_>], view: &View) -> Vec<Row> {
    let query = view.query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let text = |value: &str| matches_words(&words, value);
    let filtering = view.filter.is_some() || !query.is_empty();
    let federated = hosts.len() > 1;
    let depth_offset = u8::from(federated);
    let mut rows = Vec::new();

    for host in hosts {
        let stale = !host.online;
        let host_query_matches = !query.is_empty() && text(host.label);
        let mut host_rows = Vec::new();
        if let Some(snapshot) = host.snapshot {
            let agents: HashMap<&str, &ClientShellAgent> = snapshot
                .agents
                .iter()
                .map(|agent| (agent.pane_id.as_str(), agent))
                .collect();
            // Each bucket is walked in snapshot order, so interleaved tabs
            // and panes keep the order the daemon lists them in.
            let mut tabs_by_workspace: HashMap<&str, Vec<&ClientShellTab>> = HashMap::new();
            for tab in &snapshot.tabs {
                tabs_by_workspace
                    .entry(tab.workspace_id.as_str())
                    .or_default()
                    .push(tab);
            }
            let mut panes_by_tab: HashMap<&str, Vec<&ClientShellPane>> = HashMap::new();
            for pane in &snapshot.panes {
                panes_by_tab
                    .entry(pane.tab_id.as_str())
                    .or_default()
                    .push(pane);
            }
            for workspace in &snapshot.workspaces {
                let workspace_matches = host_query_matches
                    || text(&workspace.label)
                    || workspace.branch.as_deref().is_some_and(text);
                let mut children = Vec::new();
                let workspace_tabs = tabs_by_workspace
                    .get(workspace.workspace_id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let multiple_tabs = workspace_tabs.len() > 1;
                for tab in workspace_tabs {
                    let tab_matches = workspace_matches || text(&tab.label);
                    let tab_panes = panes_by_tab
                        .get(tab.tab_id.as_str())
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    for (index, pane) in tab_panes.iter().enumerate() {
                        let agent = agents.get(pane.pane_id.as_str()).copied();
                        let status = agent.map_or(AgentStatus::Unknown, |agent| agent.agent_status);
                        if !StatusFilter::matches(view.filter, status) {
                            continue;
                        }
                        let agent_kind = agent.and_then(|agent| {
                            agent.agent.as_deref().or(agent.display_agent.as_deref())
                        });
                        let title = agent.and_then(|agent| {
                            agent
                                .title
                                .as_deref()
                                .or(agent.terminal_title_stripped.as_deref())
                        });
                        let label = pane_label(
                            &workspace.label,
                            tab,
                            multiple_tabs,
                            tab_panes.len(),
                            index,
                            pane,
                            agent,
                        );
                        let meta = pane
                            .foreground_cwd
                            .as_deref()
                            .or(pane.cwd.as_deref())
                            .unwrap_or_default();
                        if tab_matches
                            || text(&label)
                            || text(meta)
                            || pane.cwd.as_deref().is_some_and(text)
                            || agent_kind.is_some_and(text)
                            || title.is_some_and(text)
                            || agent
                                .and_then(|agent| agent.display_agent.as_deref())
                                .is_some_and(text)
                            || text(&pane.pane_id)
                        {
                            children.push(Row {
                                depth: 1 + depth_offset,
                                label,
                                meta: meta.to_owned(),
                                status: Some(status),
                                stale,
                                target: Target::Pane {
                                    endpoint: host.id.to_owned(),
                                    pane: pane.pane_id.clone(),
                                },
                            });
                        }
                    }
                }
                if !filtering
                    || !children.is_empty()
                    || (view.filter.is_none() && !query.is_empty() && workspace_matches)
                {
                    host_rows.push(Row {
                        depth: depth_offset,
                        label: workspace.label.clone(),
                        meta: workspace.branch.clone().unwrap_or_default(),
                        status: None,
                        stale,
                        target: Target::Workspace {
                            endpoint: host.id.to_owned(),
                            workspace: workspace.workspace_id.clone(),
                        },
                    });
                    // The list keeps panes hidden until a search needs them.
                    if filtering {
                        host_rows.extend(children);
                    }
                }
            }
        }
        if !filtering || host_query_matches || !host_rows.is_empty() {
            if federated {
                rows.push(Row {
                    depth: 0,
                    label: host.label.to_owned(),
                    meta: String::new(),
                    status: None,
                    stale,
                    target: Target::Host {
                        endpoint: host.id.to_owned(),
                    },
                });
            }
            rows.extend(host_rows);
        }
    }
    rows
}

/// Every agent pane across hosts as one flat list in priority order.
fn agent_rows(hosts: &[Host<'_>], agents: &[(usize, &ClientShellAgent)], view: &View) -> Vec<Row> {
    let query = view.query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let text = |value: &str| matches_words(&words, value);
    let federated = hosts.len() > 1;
    agents
        .iter()
        .filter_map(|&(index, agent)| {
            let host = hosts.get(index)?;
            let snapshot = host.snapshot?;
            if !StatusFilter::matches(view.filter, agent.agent_status) {
                return None;
            }
            let pane = snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == agent.pane_id);
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == agent.workspace_id)
                .map_or(agent.workspace_id.as_str(), |workspace| {
                    workspace.label.as_str()
                });
            let name = pane
                .and_then(|pane| pane.label.as_deref())
                .or(agent.name.as_deref())
                .or(agent.display_agent.as_deref())
                .or(agent.title.as_deref())
                .unwrap_or(&agent.pane_id);
            let label = if federated && !host.local {
                format!("{} · {workspace}: {name}", host.label)
            } else {
                format!("{workspace}: {name}")
            };
            let meta = pane
                .and_then(|pane| pane.foreground_cwd.as_deref().or(pane.cwd.as_deref()))
                .unwrap_or_default();
            if !(text(&label) || text(meta)) {
                return None;
            }
            Some(Row {
                depth: 0,
                label,
                meta: meta.to_owned(),
                status: Some(agent.agent_status),
                stale: !host.online,
                target: Target::Pane {
                    endpoint: host.id.to_owned(),
                    pane: agent.pane_id.clone(),
                },
            })
        })
        .collect()
}

/// The row `selected` names, or the default: the first pane, else the first
/// row. A selection that is no longer listed selects nothing.
pub(super) fn selected_index(rows: &[Row], selected: Option<&Target>) -> Option<usize> {
    match selected {
        Some(target) => rows.iter().position(|row| row.target == *target),
        None => rows
            .iter()
            .position(|row| matches!(row.target, Target::Pane { .. }))
            .or_else(|| (!rows.is_empty()).then_some(0)),
    }
}

/// Where Left and Right move the selection: the first pane of the nearest
/// workspace before or after the selected row's that lists any.
pub(super) fn workspace_jump(rows: &[Row], selected: usize, forward: bool) -> Option<usize> {
    let section = rows
        .get(..=selected)?
        .iter()
        .rposition(|row| !matches!(row.target, Target::Pane { .. }))
        .unwrap_or(selected);
    let mut destinations = rows.windows(2).enumerate().filter(|(index, pair)| {
        matches!(pair[0].target, Target::Workspace { .. })
            && matches!(pair[1].target, Target::Pane { .. })
            && if forward {
                *index > section
            } else {
                *index < section
            }
    });
    let destination = if forward {
        destinations.next()
    } else {
        destinations.next_back()
    };
    destination.map(|(index, _)| index + 1)
}
