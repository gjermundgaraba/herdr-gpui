//! The order the sidebar lists workspaces and agents in, which the previous,
//! next, and numbered shortcuts step through, as Herdr's TUI steps through
//! its own sidebar: every listed host's rows, top to bottom.

use super::{agents::panel_agents, space_groups::workspace_rows};
use crate::{HerdrWindow, controls::Command, state::LiveState};
use herdr_client::protocol::{ClientShellAgent, ClientShellSnapshot};
use std::collections::HashSet;

/// A listed row: its host, by endpoint index, and the workspace or agent
/// pane it shows.
pub(crate) type Row<'a> = (usize, &'a str);

impl HerdrWindow {
    /// The live state the window draws host `index` from: its own copy for
    /// the selected host, which carries what the window stamped since the
    /// last poll, and the endpoint's for every other.
    pub(crate) fn host_live(&self, index: usize) -> &LiveState {
        if index == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[index].live
        }
    }

    /// Each host the sidebar lists, with the live state it draws from.
    fn listed_lives(&self) -> impl Iterator<Item = (usize, &LiveState)> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| endpoint.enabled && self.device_visible(&endpoint.id))
            .map(|(index, _)| (index, self.host_live(index)))
    }

    /// Each host the sidebar lists, with the snapshot it draws from.
    fn listed_hosts(&self) -> impl Iterator<Item = (usize, &ClientShellSnapshot)> {
        self.listed_lives()
            .filter_map(|(index, live)| live.snapshot.as_deref().map(|snapshot| (index, snapshot)))
    }

    /// What a host has folded away: its collapsed worktree repositories,
    /// space groups, and pinned sections, each under its own kind of key.
    pub(super) fn collapsed_for(&self, endpoint: usize) -> &HashSet<String> {
        if endpoint == 0 {
            &self.collapsed_repos
        } else {
            &self.endpoints[endpoint].collapsed_repos
        }
    }

    /// The workspace rows the sidebar shows: a folded group or host hides
    /// its rows here too, and a pinned section's mirrors are left out, so
    /// each space is stepped to once.
    pub(crate) fn sidebar_workspaces(&self) -> Vec<Row<'_>> {
        let multi = self.endpoints.len() > 1;
        self.listed_hosts()
            .filter(|(index, _)| !(multi && self.endpoints[*index].collapsed))
            .flat_map(|(index, snapshot)| {
                workspace_rows(snapshot, self.collapsed_for(index))
                    .into_iter()
                    .map(move |(row, _)| (index, snapshot.workspaces[row].workspace_id.as_str()))
            })
            .collect()
    }

    /// The agents the panel paints, by host index, in its order. Painting,
    /// stepping, and the client report all read it, so a shortcut lands on
    /// the row below and tools read the list that is on screen.
    pub(crate) fn panel_agents(&self) -> Vec<(usize, &ClientShellAgent)> {
        panel_agents(self.listed_lives(), self.agent_sort)
    }

    /// The agent panel's rows, whether or not the panel is shown, as Herdr
    /// steps through its agents either way.
    pub(crate) fn sidebar_agents(&self) -> Vec<Row<'_>> {
        self.panel_agents()
            .into_iter()
            .map(|(index, agent)| (index, agent.pane_id.as_str()))
            .collect()
    }
}

/// Which row a sidebar step lands on. `current` is the focused row, if the
/// sidebar lists it; `selected` is the selected host. Previous and next wrap
/// around, and start from an end when nothing listed is focused. A numbered
/// workspace counts only the selected host's rows and a numbered agent every
/// host's, as Herdr numbers them.
pub(crate) fn step(
    rows: &[Row<'_>],
    current: Option<usize>,
    selected: usize,
    command: Command,
) -> Option<usize> {
    let len = rows.len();
    if len == 0 {
        return None;
    }
    match command {
        Command::PreviousWorkspace | Command::PreviousAgent => {
            Some(current.map_or(len - 1, |index| (index + len - 1) % len))
        }
        Command::NextWorkspace | Command::NextAgent => {
            Some(current.map_or(0, |index| (index + 1) % len))
        }
        Command::WorkspaceNumber(number) => rows
            .iter()
            .enumerate()
            .filter(|(_, (host, _))| *host == selected)
            .nth(usize::from(number).checked_sub(1)?)
            .map(|(index, _)| index),
        Command::AgentNumber(number) => {
            let index = usize::from(number).checked_sub(1)?;
            (index < len).then_some(index)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wrap_and_start_from_an_end() {
        let rows = [(0, "a"), (0, "b"), (1, "c")];
        for (command, current, expected) in [
            (Command::NextWorkspace, Some(0), Some(1)),
            (Command::NextWorkspace, Some(2), Some(0)),
            (Command::NextWorkspace, None, Some(0)),
            (Command::PreviousWorkspace, Some(0), Some(2)),
            (Command::PreviousWorkspace, Some(2), Some(1)),
            (Command::PreviousWorkspace, None, Some(2)),
            (Command::NextAgent, Some(1), Some(2)),
            (Command::PreviousAgent, None, Some(2)),
        ] {
            assert_eq!(step(&rows, current, 0, command), expected, "{command:?}");
        }
        for command in [Command::NextWorkspace, Command::PreviousAgent] {
            assert_eq!(step(&[], None, 0, command), None);
        }
        assert_eq!(step(&rows, Some(0), 0, Command::Tab), None);
    }

    #[test]
    fn numbers_count_from_one() {
        let rows = [(0, "a"), (1, "b"), (0, "c"), (1, "d")];
        // A numbered workspace counts the selected host's rows only.
        assert_eq!(step(&rows, None, 0, Command::WorkspaceNumber(2)), Some(2));
        assert_eq!(step(&rows, None, 1, Command::WorkspaceNumber(1)), Some(1));
        assert_eq!(step(&rows, None, 1, Command::WorkspaceNumber(3)), None);
        assert_eq!(step(&rows, None, 0, Command::WorkspaceNumber(0)), None);
        // A numbered agent counts every host's.
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(4)), Some(3));
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(5)), None);
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(0)), None);
    }
}
