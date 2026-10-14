// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::{
    Page, Picker, Source,
    list::{Choice, Item, List, Row},
    plugin::{Inbox, Request, Update},
};
use crate::{
    HerdrWindow, connection::ConnectionBridge, endpoint::Endpoint,
    sidebar::layout_tests::fixture_window, terminal::InputTarget, window::MockPeer,
};
use gpui::{Entity, KeyDownEvent, Keystroke, VisualTestContext};
use herdr_client::{
    ClientEvent, ConnectTarget,
    protocol::{
        ClientMessage, ClientPaneInputEvent, ClientShellSnapshot,
        pick::{
            EndpointPickClose, EndpointPickOpen, PICK_CLOSE_KIND, PICK_OPEN_KIND, UiPickCreate,
            UiPickItem,
        },
    },
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

mod backpressure;
mod crossed;
mod dismissal;
mod inbox;
mod keys;
mod list;
mod navigator_agents;
mod navigator_fixture;
mod navigator_scroll;
mod navigator_selection;
mod navigator_workspaces;
mod outcomes;
mod presented;

impl MockPeer {
    /// Stalls the connection behind a request this peer leaves unanswered,
    /// then fills its command queue behind that for good: until the worker
    /// has taken the one command it keeps beside the request in flight, it
    /// frees a slot whenever it runs. Returns the stalled request.
    pub(super) fn stall_commands(&mut self, boot_id: &str) -> Value {
        /// 64 queued commands and the one the worker keeps, as `herdr-client`
        /// documents its bound.
        const HELD_COMMANDS: usize = 65;
        let queue = |peer: &Self| {
            peer.client.handle.request(
                boot_id,
                herdr_client::Method::PaneFocus,
                json!({"pane_id": "pane_1"}),
            )
        };
        let stalled = queue(self).unwrap();
        let blocker = self.request();
        assert_eq!(blocker["id"], stalled.as_str());
        let mut held = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while held < HELD_COMMANDS {
            match queue(self) {
                Ok(_) => held += 1,
                Err(_) => {
                    assert!(
                        Instant::now() < deadline,
                        "the worker never took its command"
                    );
                    std::thread::yield_now();
                }
            }
        }
        assert!(matches!(queue(self), Err(herdr_client::Error::Full)));
        blocker
    }
}

fn snapshot(boot: &str) -> ClientEvent {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    snapshot.boot_id = boot.into();
    ClientEvent::Snapshot(Arc::new(snapshot))
}

/// The boot of the snapshot fixture every mock peer sends.
const BOOT: &str = "boot-v1";

fn pick_item(id: &str, label: &str) -> UiPickItem {
    UiPickItem {
        id: id.into(),
        label: label.into(),
        detail: None,
        badge: None,
    }
}

/// The picker Herdr's own client tests open: two groups, the second one
/// preselected, and optionally a create row.
fn open(pick_id: &str, create: bool) -> EndpointPickOpen {
    EndpointPickOpen {
        pick_id: pick_id.into(),
        title: "Space group".into(),
        items: vec![pick_item("work", "Work"), pick_item("home", "Home")],
        selected: Some("home".into()),
        create: create.then(|| UiPickCreate {
            label: "New group".into(),
        }),
    }
}

/// The daemon's control asking for `open`, as the connection forwards it.
fn opening(open: EndpointPickOpen) -> ClientEvent {
    crate::fork::control(PICK_OPEN_KIND, &open)
}

fn close(pick_id: &str) -> ClientEvent {
    let close = EndpointPickClose {
        pick_id: pick_id.into(),
    };
    crate::fork::control(PICK_CLOSE_KIND, &close)
}

/// Hands `event` to `connection` the way its reader does: the fork mailbox
/// sees it before the live state, and keeps the picker controls.
fn feed(connection: &ConnectionBridge, event: ClientEvent) {
    let passed = connection.fork.lock().unwrap().apply(event);
    let mut state = connection.inbox.lock().unwrap();
    match passed {
        Some(event) => state.apply(event),
        None => state.dirty = true,
    }
}

/// Gives the window a second connection of its own, over `peer`.
fn add_connection(view: &mut HerdrWindow, peer: &MockPeer) -> usize {
    let mut endpoint = Endpoint::new(
        "second".into(),
        "Second".into(),
        ConnectTarget::Socket("/unused-picker-test.sock".into()),
        true,
    );
    endpoint.connection.handle = Some(peer.client.handle.clone());
    feed(&endpoint.connection, snapshot(BOOT));
    view.endpoints.push(endpoint);
    view.endpoints.len() - 1
}

/// A window whose first connection is `peer`, ready for terminal input.
fn connect(view: &mut HerdrWindow, peer: &MockPeer, cx: &mut gpui::Context<HerdrWindow>) {
    peer.prepare(view, cx);
    let local = &mut view.endpoints[0];
    local.live = view.live.clone();
    *local.connection.inbox.lock().unwrap() = view.live.clone();
    feed(&local.connection, snapshot(BOOT));
}

fn key(keystroke: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke::parse(keystroke).unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

/// What a connection was sent that matters here.
#[derive(Debug, Default, PartialEq)]
struct Sent {
    /// Endpoint requests, as `{"method", "params"}`.
    requests: Vec<Value>,
    /// Pane or popup input.
    input: Vec<ClientMessage>,
}

impl Sent {
    fn nothing() -> Self {
        Self::default()
    }

    fn resolve(pick_id: &str, outcome: Value) -> Self {
        Self {
            requests: vec![json!({
                "method": "ui.pick.resolve",
                "params": {"pick_id": pick_id, "outcome": outcome},
            })],
            input: Vec::new(),
        }
    }
}

/// Everything `peer` was sent since the last call, in order. A sentinel typed
/// straight into the connection marks the end, so "nothing" is a positive
/// observation and not a timeout. Each request is answered, because the
/// daemon holds one request per connection at a time.
fn sent(peer: &mut MockPeer) -> Sent {
    ConnectionBridge::send_input(
        &peer.client.handle,
        BOOT,
        &InputTarget::Pane("w1:p1".into()),
        ClientPaneInputEvent::TextCommit("sentinel".into()),
    )
    .unwrap();
    let mut sent = Sent::default();
    loop {
        match peer.receive() {
            ClientMessage::ClientShellPaneInput { events, .. }
                if events == [ClientPaneInputEvent::TextCommit("sentinel".into())] =>
            {
                return sent;
            }
            ClientMessage::ClientShellEndpointRequest { boot_id, request } => {
                assert_eq!(boot_id, BOOT);
                let request: Value = serde_json::from_str(&request).unwrap();
                let id = request["id"].as_str().unwrap().to_owned();
                peer.respond(BOOT, &id, &json!({"id": id, "result": {"type": "ok"}}));
                sent.requests
                    .push(json!({"method": request["method"], "params": request["params"]}));
            }
            message @ (ClientMessage::ClientShellPaneInput { .. }
            | ClientMessage::ClientShellPopupInput { .. }) => sent.input.push(message),
            // The resize, focus and theme traffic a drawn window sends on its own.
            _ => {}
        }
    }
}

/// Delivers a plugin picker on connection `index` and lets the window see it.
fn deliver(
    view: &Entity<HerdrWindow>,
    index: usize,
    open: EndpointPickOpen,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            feed(&view.endpoints[index].connection, opening(open));
            view.poll_picker(window, cx);
        })
    });
}

/// Two peers that both offer `ui.pick.resolve`, and a drawn window holding a
/// connection to each: its own, ready for terminal input, and a second one.
fn window_with_two_connections(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    let first = MockPeer::advertising(&["ui.pick.resolve"]);
    let second = MockPeer::advertising(&["ui.pick.resolve"]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        connect(&mut view, &first, cx);
        add_connection(&mut view, &second);
        view
    });
    // The keyboard starts in the terminal, as it does in a real window.
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    redraw(cx);
    (first, second, view, cx)
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

/// Asserts no picker is on screen and none is still owed an answer.
fn assert_closed(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert!(view.fork.picker.is_none());
        assert_ne!(view.menu.page, Some(Page::Picker));
    });
}

fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_picker(window, cx)));
}

fn press(view: &Entity<HerdrWindow>, keystroke: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.picker_key(&key(keystroke), window, cx))
    });
}

/// The rows on screen, as their labels; the create row reads as `+ text`.
fn rows(picker: &Picker) -> Vec<String> {
    picker
        .list
        .rows()
        .iter()
        .map(|row| match row {
            Row::Item(index) => picker.list.item(*index).unwrap().label.to_string(),
            Row::Create => format!("+ {}", picker.list.create_text()),
        })
        .collect()
}

fn shown(view: &HerdrWindow) -> Option<&Picker> {
    view.fork
        .picker
        .as_ref()
        .filter(|_| view.menu.page == Some(Page::Picker))
}

fn pick_id(picker: &Picker) -> &str {
    match &picker.source {
        Source::Plugin(pick) => pick.pick_id(),
        Source::Navigator(_) => panic!("a navigator answers no plugin"),
    }
}
