//! A window on two hosts for the history's tests: Local over a mock peer,
//! presented and ready for input, with panes `p1` to `p3`, and Build with
//! `r1` and `r2`.

use super::{History, Step, Visit};
use crate::{
    HerdrWindow, connection::ConnectionBridge, controls::Command, endpoint::Endpoint,
    sidebar::layout_tests::fixture_window, state::ConnectionStatus, terminal::InputTarget,
    window::MockPeer,
};
use gpui::{Entity, VisualTestContext};
use herdr_client::{
    ClientEvent, ConnectTarget, Method,
    protocol::{
        ClientMessage, ClientPaneInputEvent, ClientShellSnapshot, FrameData, PaneSurfaceFrame,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

mod arrivals;
mod repeats;
mod stepping;
mod traversal;
mod visits;
mod window_sync;

/// The boot of the snapshot every mock peer sends its client.
const BOOT: &str = "boot-v1";
const BUILD: &str = "ssh:00000000000000000000000000000001";

/// A host's snapshot with one pane per id, `focused` among them.
fn snapshot(panes: &[&str], focused: &str) -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let template = snapshot.panes[0].clone();
    snapshot.panes = panes
        .iter()
        .map(|id| {
            let mut pane = template.clone();
            pane.pane_id = (*id).into();
            pane.focused = *id == focused;
            pane
        })
        .collect();
    snapshot.agents.clear();
    snapshot.agent_order.clear();
    snapshot.focused_pane_id = Some(focused.into());
    snapshot
}

fn attach(
    view: &mut HerdrWindow,
    local: &MockPeer,
    remote: &MockPeer,
    cx: &mut gpui::Context<HerdrWindow>,
) {
    view.endpoints.truncate(1);
    local.prepare(view, cx);
    view.live.supports_surface = true;
    view.live.error = None;
    let mut build = Endpoint::new(
        BUILD.into(),
        "Build".into(),
        ConnectTarget::Socket("/unused-history-test.sock".into()),
        true,
    );
    build.connection.handle = Some(remote.client.handle.clone());
    build.live.status = ConnectionStatus::Connected;
    build.live.snapshot = Some(Arc::new(snapshot(&["r1", "r2"], "r1")));
    view.endpoints.push(build);
    show(view, snapshot(&["p1", "p2", "p3"], "p1"));
}

/// A window attached after its first frame, which measures the terminal and
/// would otherwise leave the prepared surface the wrong size.
fn window(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    let local = MockPeer::new();
    let remote = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| attach(view, &local, &remote, cx));
    (local, remote, view, cx)
}

/// The presented host accepts `next` and its surface, as a navigation that
/// settled leaves the window: ready for input again.
fn show(view: &mut HerdrWindow, next: ClientShellSnapshot) {
    surface_comes(view, next);
    view.pending_navigation = None;
    // Only Local is over a prepared connection.
    assert!(view.selected_endpoint != 0 || view.input_ready());
}

/// The presented host accepts `next` and its surface. A navigation that
/// waits for the surface is still the window's to send, on its next poll.
fn surface_comes(view: &mut HerdrWindow, mut next: ClientShellSnapshot) {
    let previous = view.live.snapshot.as_ref().map_or(0, |held| held.revision);
    next.revision = previous + 1;
    // A fence takes the surface away until the daemon sends the next one.
    let size = view.options.surface_size;
    view.live.surface = Some(Arc::new(PaneSurfaceFrame {
        boot_id: next.boot_id.clone(),
        projection_revision: next.revision,
        surface_revision: 1,
        frame: FrameData {
            width: size.cols,
            height: size.rows,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    }));
    view.live.snapshot = Some(Arc::new(next));
    view.live.activation = None;
    let endpoint = &mut view.endpoints[view.selected_endpoint];
    endpoint.live = view.live.clone();
    *endpoint.connection.inbox.lock().unwrap() = view.live.clone();
}

/// The presented host's next snapshot shows `pane` focused, and the window
/// observes it on its next tick.
fn visit(view: &Entity<HerdrWindow>, pane: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.focused_pane_id = Some(pane.into());
        show(view, next);
        view.sync_history();
    });
}

/// Presents host `index` as it stands, as selecting it does.
fn present(view: &Entity<HerdrWindow>, index: usize, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        view.selected_endpoint = index;
        view.live = view.endpoints[index].live.clone();
        view.sync_history();
    });
}

/// The presented host shows `pane` focused, with its surface, and the window
/// polls and observes as on a tick: a navigation that waited for the surface
/// goes out.
fn arrive(view: &Entity<HerdrWindow>, pane: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.focused_pane_id = Some(pane.into());
        surface_comes(view, next);
        view.poll_endpoints(cx);
        view.sync_history();
    });
}

/// Hands the window the answers `asked` holds, as the connection's reader
/// does, and has it collect them.
fn answer(view: &Entity<HerdrWindow>, index: usize, asked: Vec<Asked>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let mailboxes = view.endpoints[index].connection.mailboxes();
        for asked in asked {
            mailboxes.deliver(asked.answer);
        }
        view.tick_client_contract(std::time::Instant::now(), cx);
    });
}

fn step(view: &Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
}

/// The visits the window holds, as `(endpoint id, pane id)`, and its cursor.
fn visits(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> (Vec<(String, String)>, usize) {
    view.read_with(cx, |view, _| {
        let history = &view.history.history;
        (
            history
                .visits
                .iter()
                .map(|visit| (visit.endpoint_id.clone(), visit.pane_id.clone()))
                .collect(),
            history.cursor,
        )
    })
}

fn local(pane: &str) -> (String, String) {
    (crate::endpoint::LOCAL.to_owned(), pane.to_owned())
}

fn build(pane: &str) -> (String, String) {
    (BUILD.to_owned(), pane.to_owned())
}

/// One endpoint request a peer was sent, and the event its answer became.
struct Asked {
    method: Value,
    params: Value,
    answer: ClientEvent,
}

/// The endpoint requests `peer` was sent since the last call, each answered
/// as it is read, which lets the one queued behind it reach the wire: a
/// `pane.focus` with the error code `refuse` when one is given, everything
/// else as accepted. A sentinel typed straight into the connection marks the
/// end, so "nothing" is a positive observation and not a timeout.
fn exchange(peer: &mut MockPeer, refuse: Option<&str>) -> Vec<Asked> {
    ConnectionBridge::send_input(
        &peer.client.handle,
        BOOT,
        &InputTarget::Pane("sentinel".into()),
        ClientPaneInputEvent::TextCommit("sentinel".into()),
    )
    .unwrap();
    let mut asked = Vec::new();
    loop {
        let request: Value = match peer.receive() {
            ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "sentinel" => {
                return asked;
            }
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                serde_json::from_str(&request).unwrap()
            }
            _ => continue,
        };
        let id = request["id"].as_str().unwrap();
        let body = match refuse.filter(|_| request["method"] == "pane.focus") {
            Some(code) => json!({
                "id": id,
                "error": {"code": code, "message": "the host said no"},
            }),
            None if request["method"] == Method::ClientShellSurfaceSet.as_str() => json!({
                "id": id,
                "result": {
                    "type": "client_shell_surface_set",
                    "active": request["params"]["active"],
                    "projection_revision": 1,
                },
            }),
            None => json!({"id": id, "result": {"type": "ok"}}),
        };
        asked.push(Asked {
            answer: peer.respond(BOOT, id, &body),
            method: request["method"].clone(),
            params: request["params"].clone(),
        });
    }
}

fn requests(peer: &mut MockPeer) -> Vec<Asked> {
    exchange(peer, None)
}

/// The panes `peer` was asked to focus since the last call.
fn focused(peer: &mut MockPeer) -> Vec<String> {
    requests(peer)
        .into_iter()
        .filter(|asked| asked.method == "pane.focus")
        .filter_map(|asked| asked.params["pane_id"].as_str().map(str::to_owned))
        .collect()
}
