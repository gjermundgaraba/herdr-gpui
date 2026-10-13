//! The fork's views of the hosts' agents beside the panel's own: every
//! enabled host's, whatever the device filter shows, for the client report
//! and the agent picker, and only the online hosts' for the agent keys. And
//! the live state the window draws a host from.

use super::Row;
use crate::{HerdrWindow, sidebar::agents::panel_agents, state::LiveState};
use herdr_client::protocol::ClientShellAgent;

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

    /// Each enabled host, with the live state it draws from, whatever the
    /// device filter shows.
    fn enabled_lives(&self) -> impl Iterator<Item = (usize, &LiveState)> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| endpoint.enabled)
            .map(|(index, _)| (index, self.host_live(index)))
    }

    /// Every enabled host's agents in the panel's order under its current
    /// sort, as the panel would paint them with the device filter at All
    /// Devices. The client report lists these: the filter is a view of this
    /// window, and does not narrow what tools read. The report leaves out the
    /// rows it cannot describe (see `client_contract::report`).
    pub(crate) fn reported_agents(&self) -> Vec<(usize, &ClientShellAgent)> {
        panel_agents(self.enabled_lives(), self.agent_sort)
    }

    /// Every enabled host's agents in the panel's priority order, whatever
    /// the panel's own sort and the device filter show. The agent picker
    /// lists these.
    pub(crate) fn fleet_agents(&self) -> Vec<(usize, &ClientShellAgent)> {
        panel_agents(
            self.enabled_lives(),
            crate::preferences::AgentSort::Priority,
        )
    }

    /// The rows the next, previous, and numbered agent keys step through:
    /// the panel's, without the agents of a host that is not online. Those
    /// are what the host last sent, and a key must not land on a pane that
    /// cannot be shown. As the Herdr fork's `online_agent_rows`.
    pub(crate) fn stepped_agents(&self) -> Vec<Row<'_>> {
        self.sidebar_agents()
            .into_iter()
            .filter(|(host, _)| self.endpoints[*host].online())
            .collect()
    }
}
