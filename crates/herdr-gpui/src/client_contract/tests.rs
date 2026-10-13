//! A window attached to several hosts, for the tests of the client contract.
//! The helpers follow the Herdr fork's `src/client/shell/tests/fleet.rs`, so
//! its cases port with their names.

use super::*;
use crate::{
    LiveState, endpoint::Endpoint, preferences::AgentSort, sidebar::layout_tests::fixture_window,
    state::ConnectionStatus,
};
use gpui::{Entity, VisualTestContext};
use herdr_client::{
    ClientEvent, ConnectTarget,
    protocol::{AgentStatus, ClientShellAgent, ClientShellPane, ClientShellSnapshot, clients::*},
};
use std::sync::Arc;

/// The daemon's control forwarding `jump`, as the connection hands it on.
fn jumping(jump: &EndpointClientJump) -> ClientEvent {
    crate::fork::control(CLIENT_JUMP_KIND, jump)
}

mod followed;
mod jump_presentation;
mod jump_replacement;
mod jump_target;
mod jumps;
mod mailbox;
mod mark_unread;
mod online;
mod outstanding;
mod report_rows;
mod wiring;

/// A key shaped like the fork's daemon writes one. Only the tests know the
/// shape: a lifted waiting agent first, then blocked, done, working, idle,
/// and the stamp as 16 hex digits.
fn priority_key(lifted: bool, status: AgentStatus, stamp: u64) -> String {
    let rank = match status {
        AgentStatus::Blocked => 0,
        AgentStatus::Done => 1,
        AgentStatus::Working => 2,
        AgentStatus::Idle | AgentStatus::Unknown => 3,
    };
    let waiting = matches!(status, AgentStatus::Blocked | AgentStatus::Done);
    let tier = u8::from(!(lifted && waiting));
    format!("{tier}.{rank}.{stamp:016x}")
}

/// An agent as a daemon reports it: `priority_key` already says where it sorts.
fn agent(pane_id: &str, status: AgentStatus, stamp: u64) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "w1".into(),
        tab_id: "w1:t1".into(),
        name: Some(pane_id.into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: stamp,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
        priority_key: priority_key(false, status, stamp),
        priority_mark: None,
    }
}

/// An agent in a space that carries a priority token.
fn lifted_agent(pane_id: &str, status: AgentStatus, stamp: u64) -> ClientShellAgent {
    ClientShellAgent {
        priority_key: priority_key(true, status, stamp),
        priority_mark: Some("★".into()),
        ..agent(pane_id, status, stamp)
    }
}

/// One host's snapshot: a pane per agent, the first one focused, all in the
/// fixture's one space, `repo`, number 1.
fn machine(boot_id: &str, agents: Vec<ClientShellAgent>) -> ClientShellSnapshot {
    let mut machine: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    machine.boot_id = boot_id.into();
    machine.agent_view_label = None;
    machine.agent_order.clear();
    let template = machine.panes[0].clone();
    machine.panes = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| ClientShellPane {
            pane_id: agent.pane_id.clone(),
            focused: index == 0,
            ..template.clone()
        })
        .collect();
    machine.focused_pane_id = agents.first().map(|agent| agent.pane_id.clone());
    machine.agents = agents;
    if let Some(agent) = machine.agents.first_mut() {
        agent.focused = true;
    }
    machine
}

/// The id the window gives the host at `index`: Local, then one saved SSH
/// host per label.
fn host_id(index: usize) -> String {
    match index {
        0 => crate::endpoint::LOCAL.to_owned(),
        index => format!("ssh:{index:032x}"),
    }
}

/// Turns the fixture window into one on the Priority sort that is attached
/// to Local and one host per label, in that sidebar order. Local is selected
/// and no host has a snapshot yet; every remote host is online.
fn fleet(view: &mut HerdrWindow, labels: &[&str]) {
    view.agent_sort = AgentSort::Priority;
    view.live = LiveState::default();
    view.endpoints[0].live = LiveState::default();
    view.endpoints.truncate(1);
    for (index, label) in labels.iter().enumerate() {
        let mut endpoint = Endpoint::new(
            host_id(index + 1),
            (*label).to_owned(),
            ConnectTarget::Socket("/unused-client-contract-test.sock".into()),
            true,
        );
        endpoint.live.status = ConnectionStatus::AwaitingSnapshot;
        view.endpoints.push(endpoint);
    }
}

/// Host `index` accepts `snapshot`, as its reader and the window's poll
/// would deliver it.
fn accept(view: &mut HerdrWindow, index: usize, snapshot: ClientShellSnapshot) {
    let endpoint = &mut view.endpoints[index];
    endpoint
        .live
        .apply(ClientEvent::Snapshot(Arc::new(snapshot)));
    endpoint.live.error = None;
    if index == view.selected_endpoint {
        view.live = endpoint.live.clone();
    }
}

/// Host `index` lost its connection and is being redialled, its last
/// snapshot still on screen.
fn reconnecting(view: &mut HerdrWindow, index: usize) {
    let live = &mut view.endpoints[index].live;
    live.status = ConnectionStatus::Disconnected;
    live.error = Some("connection lost".into());
}

/// The window presents host `index` instead.
fn present(view: &mut HerdrWindow, index: usize) {
    view.selected_endpoint = index;
    view.live = view.endpoints[index].live.clone();
}

/// The snapshot the window retains for host `index`.
fn retained(view: &HerdrWindow, index: usize) -> ClientShellSnapshot {
    view.host_live(index)
        .snapshot
        .as_deref()
        .expect("retained snapshot")
        .clone()
}

/// The next snapshot of host `index`, changed by `change`.
fn update(view: &mut HerdrWindow, index: usize, change: impl FnOnce(&mut ClientShellSnapshot)) {
    let mut next = retained(view, index);
    next.revision += 1;
    change(&mut next);
    accept(view, index, next);
}

/// Local with a waiting and an idle agent, Build with a lifted blocked one.
fn two_machines(view: &mut HerdrWindow) {
    fleet(view, &["Build"]);
    let mut local = machine(
        "boot-1",
        vec![
            agent("local-idle", AgentStatus::Idle, 5),
            agent("local-blocked", AgentStatus::Blocked, 20),
        ],
    );
    local.panes[1].cwd = Some("/repo/api".into());
    local.panes[1].foreground_cwd = Some("/repo/api/src".into());
    accept(view, 0, local);
    let mut build = machine(
        "build-boot",
        vec![lifted_agent("build-lifted", AgentStatus::Blocked, 40)],
    );
    build.workspaces[0].label = "release".into();
    build.workspaces[0].number = 3;
    accept(view, 1, build);
}

/// A fixture window set up by `setup`, outside the render tree's concerns.
fn window(
    cx: &mut gpui::TestAppContext,
    setup: impl FnOnce(&mut HerdrWindow),
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        setup(&mut view);
        view
    })
}

/// The surface the daemon projects for `snapshot`, coherent with it.
fn surface_for(snapshot: &ClientShellSnapshot) -> Arc<herdr_client::protocol::PaneSurfaceFrame> {
    use herdr_client::protocol::{FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect};
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    Arc::new(PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: FrameData {
            width: 80,
            height: 24,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![PaneSurfacePane {
            pane_id: snapshot.focused_pane_id.clone().unwrap_or_default(),
            content_revision: 1,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: None,
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 800,
            pixel_height: 480,
        }],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

/// The presented host's surface is active and shows its latest snapshot, as
/// once the daemon acknowledged the activation and projected a frame.
fn presented(view: &mut HerdrWindow) {
    let index = view.selected_endpoint;
    view.endpoints[index].assume_surface_requested();
    let snapshot = view.live.snapshot.clone().expect("a presented snapshot");
    view.live.surface = Some(surface_for(&snapshot));
    view.live.activation = None;
}

fn report_of(view: &HerdrWindow) -> ClientReport {
    view.client_report().expect("report within limits")
}

/// A report's rows as `(endpoint id, pane id)`.
fn reported(report: &ClientReport) -> Vec<(String, String)> {
    report
        .agents
        .iter()
        .map(|row| (row.endpoint_id.clone(), row.agent.pane_id.clone()))
        .collect()
}
