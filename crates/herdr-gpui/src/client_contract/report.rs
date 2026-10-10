//! `client.report`: what this window's Agents list shows, built for the
//! local daemon so tools can read it through `client.list` and
//! `client.changed`.
//!
//! The report is built only from the hosts' retained snapshots and the rows
//! the agents panel paints at All Devices, in its order: the sidebar's device
//! filter narrows what the window shows, never what tools read. A row needs
//! its space's label and number, so an agent whose space the snapshot no
//! longer lists is left out, although the panel still paints it; every other
//! row is reported. Nothing here draws or sends; the connection measures and sends
//! the request. This is mirrored client logic: the counterpart is
//! `src/client/shell/client_report.rs` in the Herdr fork, and its cases are
//! ported in `tests/report_rows.rs`.
//!
//! Every row's `agent` is the snapshot's record verbatim, in either client:
//! the status a tool reads is the daemon's, and no client keeps a record of
//! completions of its own.

use herdr_client::protocol::{
    ClientShellAgent, ClientShellSnapshot,
    clients::{
        ClientEndpointStatus, ClientReport, ClientReportAgent, ClientReportEndpoint,
        ClientReportError, MAX_CLIENT_REPORT_AGENTS, MAX_CLIENT_REPORT_ENDPOINTS,
        MAX_CLIENT_REPORT_ID_BYTES,
    },
};

/// One host this window is attached to, as the report describes it.
pub(crate) struct Host<'a> {
    pub(crate) id: &'a str,
    pub(crate) label: &'a str,
    pub(crate) status: ClientEndpointStatus,
    /// The snapshot the window draws the host from, if it has one.
    pub(crate) snapshot: Option<&'a ClientShellSnapshot>,
}

/// Whether the daemon accepts `id` in a report: it rejects the whole report
/// for one id beyond the limit.
fn within_id_limit(id: &str) -> bool {
    id.len() <= MAX_CLIENT_REPORT_ID_BYTES
}

/// The report for `hosts`, within the daemon's limits: hosts and rows beyond
/// the counts are left out, as is anything whose id the daemon would reject,
/// and display text is cut.
///
/// `rows` are the agents panel's, each with its host's index in `hosts`, in
/// the order the panel shows them. `active` is the host the window presents;
/// its focused agent is the one focused row. `focused` is whether the window
/// is the active OS window, which this client always knows.
pub(crate) fn build<'a>(
    hosts: &[Host<'a>],
    active: usize,
    focused: bool,
    rows: impl IntoIterator<Item = (usize, &'a ClientShellAgent)>,
) -> Result<ClientReport, ClientReportError> {
    let listed: Vec<usize> = hosts
        .iter()
        .enumerate()
        .filter(|(_, host)| {
            !host.id.is_empty()
                && within_id_limit(host.id)
                && host
                    .snapshot
                    .is_none_or(|snapshot| within_id_limit(&snapshot.boot_id))
        })
        .map(|(index, _)| index)
        .take(MAX_CLIENT_REPORT_ENDPOINTS)
        .collect();
    let is_listed = |index: usize| listed.contains(&index);
    let mut focused_reported = false;
    let agents = rows
        .into_iter()
        .filter(|(index, _)| is_listed(*index))
        .filter_map(|(index, agent)| {
            let host = hosts.get(index)?;
            let snapshot = host.snapshot?;
            if agent.pane_id.is_empty()
                || ![&agent.pane_id, &agent.workspace_id, &agent.tab_id]
                    .into_iter()
                    .all(|id| within_id_limit(id))
            {
                return None;
            }
            // A row needs its space's label and number; an agent whose space
            // the snapshot no longer lists has neither.
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
            let pane = snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == agent.pane_id);
            let focused = !focused_reported && index == active && agent.focused;
            focused_reported |= focused;
            Some(ClientReportAgent {
                endpoint_id: host.id.to_owned(),
                focused,
                workspace_label: workspace.label.clone(),
                workspace_number: workspace.number,
                cwd: pane.and_then(|pane| pane.cwd.clone()),
                foreground_cwd: pane.and_then(|pane| pane.foreground_cwd.clone()),
                agent: agent.clone(),
            })
        })
        .take(MAX_CLIENT_REPORT_AGENTS)
        .collect();
    let mut report = ClientReport {
        focused: Some(focused),
        active_endpoint_id: hosts
            .get(active)
            .filter(|_| is_listed(active))
            .map(|host| host.id.to_owned()),
        endpoints: listed
            .iter()
            .filter_map(|index| hosts.get(*index))
            .map(|host| ClientReportEndpoint {
                endpoint_id: host.id.to_owned(),
                label: host.label.to_owned(),
                status: host.status,
                boot_id: host.snapshot.map(|snapshot| snapshot.boot_id.clone()),
            })
            .collect(),
        agents,
    };
    report.normalize()?;
    Ok(report)
}
