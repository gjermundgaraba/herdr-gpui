//! Two hosts for the navigator's window tests, and what the tests read of an
//! open navigator.

use super::*;
use crate::{controls::Command, state::ConnectionStatus};
use herdr_client::protocol::{AgentStatus, ClientShellAgent};

fn agent(pane_id: &str, name: &str, status: AgentStatus, key: &str) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "w1".into(),
        tab_id: "w1:t1".into(),
        name: Some(name.into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
        priority_key: key.into(),
        priority_mark: None,
    }
}

/// Local keeps the fixture's space `repo` with two panes in its one tab:
/// `w1:p1`, labelled `shell`, focused, running the blocked `reviewer`, and
/// `w1:p2` running the idle `helper`. Second, online, has a space `release`
/// whose one pane `w1:p1` runs the working `deployer`, lifted to the top by
/// its priority key and marked `★`.
pub(super) fn fleet(view: &mut HerdrWindow) {
    let local = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    local.agent_view_label = None;
    local.agent_order.clear();
    let mut second = local.panes[0].clone();
    second.pane_id = "w1:p2".into();
    second.label = None;
    second.cwd = Some("/repo/api".into());
    second.foreground_cwd = None;
    second.focused = false;
    local.panes.push(second);
    local.agents = vec![
        agent("w1:p2", "helper", AgentStatus::Idle, "1.3.2"),
        agent("w1:p1", "reviewer", AgentStatus::Blocked, "1.0.1"),
    ];
    let mut remote = local.clone();
    remote.workspaces[0].label = "release".into();
    remote.panes.truncate(1);
    remote.panes[0].label = None;
    remote.panes[0].cwd = Some("/srv/release".into());
    remote.panes[0].foreground_cwd = None;
    let mut deployer = agent("w1:p1", "deployer", AgentStatus::Working, "0.2.3");
    deployer.priority_mark = Some("★".into());
    remote.agents = vec![deployer];

    // Leaving a host for another needs its surface lane.
    view.live.supports_surface = true;
    view.endpoints[0].live = view.live.clone();
    *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
    let second = &mut view.endpoints[1];
    second.live.snapshot = Some(Arc::new(remote));
    second.live.status = ConnectionStatus::Connected;
    second.live.error = None;
}

/// A drawn window on [`fleet`], its keyboard in the terminal.
pub(super) fn window(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    // The fixture's welcome offers `pane.focus` alone.
    let methods = ["ui.pick.resolve", "workspace.focus"];
    let first = MockPeer::advertising(&methods);
    let second = MockPeer::advertising(&methods);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        connect(&mut view, &first, cx);
        add_connection(&mut view, &second);
        view
    });
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    redraw(cx);
    view.update(cx, |view, _| fleet(view));
    (first, second, view, cx)
}

/// Runs `command` as its key binding does.
pub(super) fn run(view: &Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    redraw(cx);
}

/// What an open picker shows.
#[derive(Debug, PartialEq)]
pub(super) struct Shown {
    pub(super) title: String,
    pub(super) rows: Vec<String>,
    /// The selected row; none once the row it was on is gone.
    pub(super) selected: Option<usize>,
    /// Whether the keyboard is in the search field.
    pub(super) searching: bool,
    pub(super) query: String,
}

pub(super) fn state(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Shown> {
    cx.update(|window, cx| {
        let view = view.read(cx);
        let picker = shown(view)?;
        let search = picker.search.read(cx);
        Some(Shown {
            title: picker.title.to_string(),
            rows: rows(picker),
            selected: picker.list.selected(),
            searching: search.focus.is_focused(window),
            query: search.text().to_owned(),
        })
    })
}

/// The rows of the open picker.
pub(super) fn listed(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    state(view, cx).expect("an open picker").rows
}

pub(super) fn selected(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> String {
    let state = state(view, cx).expect("an open picker");
    state.rows[state.selected.expect("a selected row")].clone()
}

/// Why Enter opened nothing, when the navigator says so.
pub(super) fn notice(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| match &shown(view)?.source {
        Source::Navigator(navigator) => navigator.notice().map(ToString::to_string),
        Source::Plugin(_) => None,
    })
}

/// Makes the terminal ready for input, so a navigation is sent at once.
pub(super) fn ready(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        let size = view.options.surface_size;
        let frame = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).frame;
        (frame.width, frame.height) = (size.cols, size.rows);
        assert!(view.input_ready());
    });
}
