//! A daemon forwards a tool's jump to the client that carries the window's
//! report, which an editor group may have parked since. The answer goes back
//! on the connection the jump arrived on, parked or not.

use super::*;
use herdr_client::{
    ClientEvent,
    protocol::{
        clients::{
            CLIENT_JUMP_KIND, ClientJumpOutcome, ClientJumpResolveParams, EndpointClientJump,
        },
        endpoint::EndpointServerWelcome,
    },
};

const RESOLVE: &str = "client.jump.resolve";

/// What a connection's reader hands its contract mailbox once the daemon has
/// welcomed the client and sent its first snapshot.
fn greet(connection: &ConnectionBridge) {
    let mut welcome: EndpointServerWelcome = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome.methods.push(RESOLVE.into());
    let mut fork = connection.fork.lock().unwrap();
    fork.clients.apply(ClientEvent::Connected(welcome));
    fork.clients
        .apply(ClientEvent::Snapshot(Arc::new(fixture_snapshot())));
}

/// The jump answers `peer` was sent; a sentinel typed into the connection
/// marks the end. Any other request is accepted, so the connection, which
/// holds one request at a time, takes the next.
fn answers(peer: &mut MockPeer) -> Vec<ClientJumpResolveParams> {
    peer.client
        .handle
        .send_input(
            "boot-v1",
            "sentinel",
            [herdr_client::protocol::ClientPaneInputEvent::TextCommit(
                "sentinel".into(),
            )],
        )
        .unwrap();
    let mut answers = Vec::new();
    loop {
        match peer.receive() {
            ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "sentinel" => {
                return answers;
            }
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                let request: serde_json::Value = serde_json::from_str(&request).unwrap();
                if request["method"] == RESOLVE {
                    answers.push(serde_json::from_value(request["params"].clone()).unwrap());
                } else {
                    let id = request["id"].as_str().unwrap();
                    let accepted = serde_json::json!({"id": id, "result": {"type": "ok"}});
                    peer.respond("boot-v1", id, &accepted);
                }
            }
            _ => {}
        }
    }
}

#[gpui::test]
fn a_jump_forwarded_to_a_parked_connection_is_answered_on_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let left = view.read_with(cx, |view, _| view.group_slots()[0].id);
    let mut own = MockPeer::advertising(&[RESOLVE]);
    let mut group = MockPeer::advertising(&[RESOLVE]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints[0].connection.handle = Some(own.client.handle.clone());
            greet(&view.endpoints[0].connection);
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &group, snapshot(40));
            greet(&parked.connection);
            // The pane id was read under a boot the daemon no longer runs,
            // so the jump is refused without navigating anywhere.
            parked
                .connection
                .mailboxes()
                .deliver(jumping(&EndpointClientJump {
                    jump_id: "jump-9".into(),
                    endpoint_id: crate::endpoint::LOCAL.into(),
                    boot_id: "an-older-boot".into(),
                    pane_id: "w1:p1".into(),
                }));
            view.browser.terminals.parked.push(parked);
            view.tick_client_contract(Instant::now(), cx);
        })
    });

    assert_eq!(
        answers(&mut group),
        [ClientJumpResolveParams {
            jump_id: "jump-9".into(),
            outcome: ClientJumpOutcome::StaleBoot,
        }]
    );
    assert!(answers(&mut own).is_empty());
}

/// The window's snapshot with two more panes, in another space, `w1`.
fn elsewhere(view: &HerdrWindow) -> ClientShellSnapshot {
    let mut snapshot = view.live.snapshot.as_deref().unwrap().clone();
    for id in ["w1:p8", "w1:p9"] {
        let mut pane = fixture_snapshot().panes[0].clone();
        pane.pane_id = id.into();
        pane.workspace_id = "w1".into();
        snapshot.panes.push(pane);
    }
    snapshot
}

/// A window whose parked connection is forwarded `jump-1`, to `w1:p9`, and
/// has asked its own daemon for the pane. Returns the window's own peer, the
/// parked one, the parked connection's mailboxes, and when the jump arrived.
fn jump_to_another_space(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> (MockPeer, MockPeer, crate::fork::Mailboxes, Instant) {
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let left = view.read_with(cx, |view, _| view.group_slots()[0].id);
    let own = MockPeer::advertising(&[RESOLVE, "pane.focus"]);
    let group = MockPeer::advertising(&[RESOLVE]);
    let start = Instant::now();
    let mailboxes = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let listed = Arc::new(elsewhere(view));
            view.live.snapshot = Some(listed.clone());
            view.endpoints[0].live.snapshot = Some(listed);
            view.endpoints[0].connection.handle = Some(own.client.handle.clone());
            greet(&view.endpoints[0].connection);
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &group, snapshot(40));
            greet(&parked.connection);
            let mailboxes = parked.connection.mailboxes();
            mailboxes.deliver(jumping(&jump("jump-1")));
            view.browser.terminals.parked.push(parked);
            view.tick_client_contract(start, cx);
            mailboxes
        })
    });
    (own, group, mailboxes, start)
}

/// The daemon's control forwarding `jump`, as the connection hands it on.
fn jumping(jump: &EndpointClientJump) -> ClientEvent {
    crate::fork::control(CLIENT_JUMP_KIND, jump)
}

fn jump(id: &str) -> EndpointClientJump {
    EndpointClientJump {
        jump_id: id.into(),
        endpoint_id: crate::endpoint::LOCAL.into(),
        boot_id: "layout-test".into(),
        pane_id: "w1:p9".into(),
    }
}

/// The window's daemon moves it to `w1` with `pane` focused. Leaving the
/// workspace retires the parked connections; the groups settle first, as a
/// window tick orders them.
fn arrive_in_the_space(view: &mut HerdrWindow, pane: &str, cx: &mut Context<HerdrWindow>) {
    let mut moved = view.live.snapshot.as_deref().unwrap().clone();
    moved.revision += 1;
    moved.focused_workspace_id = Some("w1".into());
    moved.focused_pane_id = Some(pane.into());
    view.live.surface = Some(surface(&moved, pane));
    view.live.snapshot = Some(Arc::new(moved));
    view.endpoints[0].live = view.live.clone();
    view.endpoints[0].assume_surface_requested();
    view.reconcile_group_terminals(cx);
    assert!(view.browser.terminals.parked.is_empty());
}

/// A jump to another space leaves the workspace the parked connections
/// belong to, which retires them. The one the jump arrived on still owes its
/// answer, so it outlives that until the answer is sent and acknowledged.
#[gpui::test]
fn a_jump_that_leaves_the_workspace_is_answered_on_its_parked_connection(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let (mut own, mut group, _mailboxes, start) = jump_to_another_space(&view, cx);
    // The window navigates; its daemon moves it to `w1` and shows the pane.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            arrive_in_the_space(view, "w1:p9", cx);
            view.tick_client_contract(start + Duration::from_millis(40), cx);
        })
    });

    assert_eq!(
        answers(&mut group),
        [ClientJumpResolveParams {
            jump_id: "jump-1".into(),
            outcome: ClientJumpOutcome::Focused,
        }]
    );
    assert!(answers(&mut own).is_empty());
}

/// The retired connection carries the window's report until another does, so
/// its daemon can forward it a second jump late in the time it is kept for
/// the first. It is then kept for as long as the daemon waits for that one.
#[gpui::test]
fn a_later_jump_on_a_retired_connection_keeps_it_for_its_own_answer(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let (_own, mut group, mailboxes, start) = jump_to_another_space(&view, cx);
    let at = |seconds| start + Duration::from_secs(seconds);
    let kept = |view: &HerdrWindow| view.connections().count() - view.endpoints.len();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // The window arrives in `w1` on another pane, and the pane the
            // jump asked for does not show: the jump stays open, and the
            // connection it arrived on is kept for its answer.
            arrive_in_the_space(view, "w1:p8", cx);
            view.tick_client_contract(at(1), cx);
            assert_eq!(kept(view), 1);

            // Eleven seconds on, the daemon forwards a newer jump there,
            // which takes the first one's place and has its own time.
            mailboxes.deliver(jumping(&jump("jump-2")));
            view.tick_client_contract(at(11), cx);
            // Twenty seconds after the connection retired, the newer jump
            // is still open, so the connection is still here.
            view.tick_client_contract(at(22), cx);
            assert_eq!(kept(view), 1);
            // The jump runs out of its time and says so.
            view.tick_client_contract(at(24), cx);
        })
    });
    assert_eq!(
        answers(&mut group),
        [ClientJumpResolveParams {
            jump_id: "jump-2".into(),
            outcome: ClientJumpOutcome::Timeout,
        }]
    );

    // Once the daemon has stopped waiting for that one, the connection goes.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_client_contract(at(30), cx);
            assert_eq!(kept(view), 1);
            view.tick_client_contract(at(32), cx);
            assert_eq!(kept(view), 0);
        })
    });
}

/// Closing the group that holds the window's connection gives the window
/// the connection of the group that is left. The one it held carries the
/// window's report and a jump still open, so it is kept, out of every group,
/// until that jump is answered on it.
#[gpui::test]
fn a_jump_open_when_the_primary_group_closes_is_answered_on_its_connection(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let [left, right] = view.read_with(cx, |view, _| {
        let slots = view.group_slots();
        [slots[0].id, slots[1].id]
    });
    let mut own = MockPeer::advertising(&[RESOLVE, "pane.focus"]);
    let group = MockPeer::advertising(&[RESOLVE]);
    let start = Instant::now();
    let kept = |view: &HerdrWindow| view.connections().count() - view.endpoints.len();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let listed = elsewhere(view);
            // The left group shows `t1` through a connection of its own.
            let mut on_t1 = listed.clone();
            on_t1.focused_tab_id = Some("t1".into());
            let listed = Arc::new(listed);
            view.live.snapshot = Some(listed.clone());
            view.endpoints[0].live.snapshot = Some(listed);
            view.endpoints[0].connection.handle = Some(own.client.handle.clone());
            greet(&view.endpoints[0].connection);
            terminals_side_by_side(view, left, right);
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &group, on_t1);
            greet(&parked.connection);
            view.browser.terminals.parked.push(parked);
            // The daemon forwards a jump on the window's own connection.
            view.endpoints[0]
                .connection
                .mailboxes()
                .deliver(jumping(&jump("jump-1")));
            view.tick_client_contract(start, cx);
            assert_eq!(kept(view), 1, "the parked connection");

            // The user closes the group that holds the window's connection.
            view.close_group(right, window, cx);
            assert_eq!(view.primary_group(), Some(left));
            assert_eq!(view.focused_herdr_tab(), Some("t1"));
            assert!(view.browser.terminals.parked.is_empty());
            assert_eq!(kept(view), 1, "the connection that owes the answer");
        })
    });
    // Nothing is answered yet: the pane has not shown.
    assert!(answers(&mut own).is_empty());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let late = start + crate::client_contract::jump::NAVIGATION_SETTLE_DEADLINE;
            view.tick_client_contract(late, cx);
        })
    });
    assert_eq!(
        answers(&mut own),
        [ClientJumpResolveParams {
            jump_id: "jump-1".into(),
            outcome: ClientJumpOutcome::Timeout,
        }]
    );
}
