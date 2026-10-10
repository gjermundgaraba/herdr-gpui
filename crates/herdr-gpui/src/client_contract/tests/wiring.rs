//! The window's side of the contract over real connections: what
//! `sync_client_contract` sends to which daemon, tick by tick. The cases
//! follow the Herdr fork's `src/client/client_contract/tests.rs`.

use super::mailbox::{snapshot, welcome};
use super::*;
use crate::{connection::ConnectionBridge, terminal::InputTarget, window::MockPeer};
use herdr_client::protocol::{ClientMessage, ClientPaneInputEvent};
use serde_json::{Value, json};

/// The boot of the snapshot every mock peer sends its client.
const BOOT: &str = "boot-v1";
const REPORT: &str = "client.report";

/// Puts host `index` on a connection to `peer`, as its reader would have
/// left it once the daemon's welcome and first snapshot arrived.
fn connect(view: &mut HerdrWindow, index: usize, peer: &MockPeer, methods: &[&str]) {
    let connection = &mut view.endpoints[index].connection;
    // A new set of mailboxes, as a reconnect makes one. The handle it had is
    // only let go of: the test may still speak to that peer.
    connection.handle = None;
    connection.detach(false);
    connection.handle = Some(peer.client.handle.clone());
    let mailboxes = connection.mailboxes();
    mailboxes.deliver(welcome(methods));
    mailboxes.deliver(snapshot(BOOT));
}

/// Local over `local` and Build over `remote`, both taking reports, with the
/// agents of the fork's contract tests. Local is presented.
fn attach(view: &mut HerdrWindow, local: &MockPeer, remote: &MockPeer) {
    fleet(view, &["Build"]);
    connect(view, 0, local, &[REPORT]);
    connect(view, 1, remote, &[REPORT]);
    accept(
        view,
        0,
        machine(
            BOOT,
            vec![
                agent("pane_1", AgentStatus::Idle, 1),
                agent("pane_2", AgentStatus::Done, 2),
            ],
        ),
    );
    accept(
        view,
        1,
        machine(BOOT, vec![agent("pane_7", AgentStatus::Blocked, 3)]),
    );
}

fn client(
    cx: &mut gpui::TestAppContext,
) -> (
    MockPeer,
    MockPeer,
    Entity<HerdrWindow>,
    &mut VisualTestContext,
) {
    let local = MockPeer::advertising(&[REPORT]);
    let remote = MockPeer::advertising(&[REPORT]);
    let (view, cx) = window(cx, |view| attach(view, &local, &remote));
    (local, remote, view, cx)
}

/// One window tick's worth of contract work.
fn turn(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.sync_client_contract());
}

/// The lane requests `peer` was sent since the last call, as `(boot id,
/// request)`. A sentinel typed straight into the connection marks the end,
/// so "nothing" is a positive observation and not a timeout.
fn received(peer: &mut MockPeer) -> Vec<(String, Value)> {
    ConnectionBridge::send_input(
        &peer.client.handle,
        BOOT,
        &InputTarget::Pane("sentinel".into()),
        ClientPaneInputEvent::TextCommit("sentinel".into()),
    )
    .unwrap();
    let mut requests = Vec::new();
    loop {
        match peer.receive() {
            ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "sentinel" => {
                return requests;
            }
            ClientMessage::ClientShellEndpointRequest { boot_id, request } => {
                requests.push((boot_id, serde_json::from_str(&request).unwrap()));
            }
            _ => {}
        }
    }
}

/// Answers `request` on `peer` with `body` and hands the answer to host
/// `index`'s mailboxes, as that connection's reader does.
fn answer(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    index: usize,
    request: &Value,
    body: Value,
    cx: &mut VisualTestContext,
) {
    let id = request["id"].as_str().unwrap();
    let mut envelope = body;
    envelope["id"] = json!(id);
    let event = peer.respond(BOOT, id, &envelope);
    view.read_with(cx, |view, _| {
        view.endpoints[index].connection.mailboxes().deliver(event);
    });
}

/// The reports `peer` was sent since the last call, each answered as accepted.
fn reports(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    index: usize,
    cx: &mut VisualTestContext,
) -> Vec<ClientReport> {
    received(peer)
        .into_iter()
        .map(|(boot_id, request)| {
            assert_eq!(boot_id, BOOT, "the lane request names this daemon's boot");
            assert_eq!(request["method"], REPORT);
            answer(
                peer,
                view,
                index,
                &request,
                json!({"result": {"type": "ok"}}),
                cx,
            );
            serde_json::from_value(request["params"].clone()).expect("a report")
        })
        .collect()
}

fn panes(report: &ClientReport) -> Vec<(&str, &str)> {
    report
        .agents
        .iter()
        .map(|row| (row.endpoint_id.as_str(), row.agent.pane_id.as_str()))
        .collect()
}

#[gpui::test]
fn the_first_turn_reports_to_the_local_daemon(cx: &mut gpui::TestAppContext) {
    let (mut local, mut remote, view, cx) = client(cx);

    turn(&view, cx);

    let sent = reports(&mut local, &view, 0, cx);
    assert_eq!(sent.len(), 1);
    let report = &sent[0];
    assert_eq!(report.active_endpoint_id.as_deref(), Some("local"));
    assert_eq!(report.endpoints.len(), 2);
    let build = host_id(1);
    assert_eq!(
        panes(report),
        [
            (build.as_str(), "pane_7"),
            ("local", "pane_2"),
            ("local", "pane_1"),
        ]
    );
    assert!(received(&mut remote).is_empty());
}

#[gpui::test]
fn a_turn_without_changes_sends_nothing(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);

    for _ in 0..5 {
        turn(&view, cx);
    }

    assert!(received(&mut local).is_empty());
}

#[gpui::test]
fn every_change_within_one_turn_becomes_one_report(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);

    // Three events arrive before the window ticks again.
    view.update(cx, |view, _| {
        update(view, 0, |snapshot| {
            snapshot.agents[1].title = Some("first".into());
        });
        update(view, 1, |snapshot| {
            snapshot.agents[0].agent_status = AgentStatus::Working;
        });
        update(view, 0, |snapshot| {
            snapshot.agents[1].title = Some("second".into());
        });
        view.active = true;
    });
    turn(&view, cx);

    let sent = reports(&mut local, &view, 0, cx);
    assert_eq!(sent.len(), 1);
    let latest = &sent[0];
    assert_eq!(latest.focused, Some(true));
    let row = |pane: &str| latest.agents.iter().find(|row| row.agent.pane_id == pane);
    assert_eq!(
        row("pane_2").and_then(|row| row.agent.title.as_deref()),
        Some("second")
    );
    assert_eq!(
        row("pane_7").map(|row| row.agent.agent_status),
        Some(AgentStatus::Working)
    );
}

#[gpui::test]
fn a_report_waits_for_the_answer_to_the_one_before(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    let first = received(&mut local);
    assert_eq!(first.len(), 1);

    // The daemon has not answered yet: changes are held, however many ticks pass.
    for title in ["first", "second"] {
        view.update(cx, |view, _| {
            update(view, 0, |snapshot| {
                snapshot.agents[1].title = Some(title.into());
            });
        });
        turn(&view, cx);
        turn(&view, cx);
    }
    assert!(received(&mut local).is_empty());

    answer(
        &mut local,
        &view,
        0,
        &first[0].1,
        json!({"result": {"type": "ok"}}),
        cx,
    );
    turn(&view, cx);

    let sent = reports(&mut local, &view, 0, cx);
    assert_eq!(
        sent.len(),
        1,
        "one report for everything that changed meanwhile"
    );
    assert_eq!(
        sent[0]
            .agents
            .iter()
            .find(|row| row.agent.pane_id == "pane_2")
            .and_then(|row| row.agent.title.as_deref()),
        Some("second")
    );
}

#[gpui::test]
fn window_focus_changes_are_reported(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let mut focused = Vec::new();
    for active in [false, true, false, false] {
        view.update(cx, |view, _| view.active = active);
        turn(&view, cx);
        focused.extend(
            reports(&mut local, &view, 0, cx)
                .into_iter()
                .map(|report| report.focused),
        );
    }

    assert_eq!(focused, [Some(false), Some(true), Some(false)]);
}

#[gpui::test]
fn a_snapshot_that_changes_nothing_reported_is_not_sent_again(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);

    // A new revision whose agents, hosts, and focus read the same.
    view.update(cx, |view, _| {
        let before = view.client_report_fingerprint();
        update(view, 0, |snapshot| {
            snapshot.tab_bar_right_separator = " / ".into();
        });
        assert_ne!(view.client_report_fingerprint(), before);
    });
    turn(&view, cx);

    assert!(received(&mut local).is_empty());
}

#[gpui::test]
fn the_report_goes_to_local_while_another_host_is_presented(cx: &mut gpui::TestAppContext) {
    let (mut local, mut remote, view, cx) = client(cx);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);

    // The user switches to Build.
    view.update(cx, |view, _| present(view, 1));
    turn(&view, cx);
    let switched = reports(&mut local, &view, 0, cx);
    assert_eq!(switched.len(), 1, "the switch itself is reported");
    assert_eq!(switched[0].active_endpoint_id, Some(host_id(1)));
    assert!(
        switched[0]
            .agents
            .iter()
            .any(|row| row.focused && row.agent.pane_id == "pane_7")
    );

    // Build's agent changes while Build is presented.
    view.update(cx, |view, _| {
        update(view, 1, |snapshot| {
            snapshot.agents[0].agent_status = AgentStatus::Working;
        });
    });
    turn(&view, cx);
    let changed = reports(&mut local, &view, 0, cx);
    assert_eq!(changed.len(), 1);
    assert_eq!(
        changed[0]
            .agents
            .iter()
            .find(|row| row.agent.pane_id == "pane_7")
            .map(|row| row.agent.agent_status),
        Some(AgentStatus::Working)
    );
    assert!(
        received(&mut remote).is_empty(),
        "a daemon that is not the local one is never reported to"
    );
}

#[gpui::test]
fn a_client_without_a_local_connection_reports_to_nobody(cx: &mut gpui::TestAppContext) {
    let (mut local, mut remote, view, cx) = client(cx);
    view.update(cx, |view, _| view.endpoints[0].connection.handle = None);

    turn(&view, cx);
    view.update(cx, |view, _| view.active = true);
    turn(&view, cx);

    assert!(received(&mut local).is_empty());
    assert!(received(&mut remote).is_empty());
}

#[gpui::test]
fn a_local_daemon_that_does_not_take_reports_gets_none(cx: &mut gpui::TestAppContext) {
    let mut local = MockPeer::new();
    let remote = MockPeer::advertising(&[REPORT]);
    let (view, cx) = window(cx, |view| {
        attach(view, &local, &remote);
        connect(view, 0, &local, &["pane.focus"]);
    });

    turn(&view, cx);

    assert!(received(&mut local).is_empty());
}

#[gpui::test]
fn nothing_is_reported_before_local_has_sent_a_snapshot(cx: &mut gpui::TestAppContext) {
    let mut local = MockPeer::advertising(&[REPORT]);
    let (view, cx) = window(cx, |view| {
        fleet(view, &[]);
        let connection = &mut view.endpoints[0].connection;
        connection.detach(false);
        connection.handle = Some(local.client.handle.clone());
        connection.mailboxes().deliver(welcome(&[REPORT]));
    });

    turn(&view, cx);
    assert!(received(&mut local).is_empty(), "no boot id to name yet");

    view.update(cx, |view, _| {
        view.endpoints[0]
            .connection
            .mailboxes()
            .deliver(snapshot(BOOT));
        accept(view, 0, machine(BOOT, Vec::new()));
    });
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);
}

#[gpui::test]
fn a_new_local_connection_is_reported_to_afresh(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    let before = reports(&mut local, &view, 0, cx);

    // Local reconnects: the daemon dropped the report with the old connection.
    let mut reconnected = MockPeer::advertising(&[REPORT]);
    view.update(cx, |view, _| connect(view, 0, &reconnected, &[REPORT]));
    turn(&view, cx);
    turn(&view, cx);

    assert_eq!(reports(&mut reconnected, &view, 0, cx), before);
    assert!(
        received(&mut local).is_empty(),
        "the connection it replaced hears nothing more"
    );
}

#[gpui::test]
fn a_restarted_local_daemon_is_reported_to_under_its_new_boot(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);

    // Only the boot the connection names changes; what the window shows does
    // not. The mock peer's worker still fences on its own boot, so the report
    // the window queues under the new one is refused before the wire, which
    // is how the test sees that it was queued at all.
    view.read_with(cx, |view, _| {
        view.endpoints[0]
            .connection
            .clients
            .lock()
            .unwrap()
            .apply(snapshot("boot-2"));
    });
    turn(&view, cx);

    let refused = local
        .client
        .events
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert!(
        matches!(
            refused,
            ClientEvent::CommandRejected {
                request_id: Some(_),
                reason: herdr_client::Error::CommandBoot,
            }
        ),
        "{refused:?}"
    );
    assert!(received(&mut local).is_empty());
}

#[gpui::test]
fn a_rejected_report_is_not_shown_and_not_retried(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    turn(&view, cx);
    let sent = received(&mut local);
    assert_eq!(sent.len(), 1);

    answer(
        &mut local,
        &view,
        0,
        &sent[0].1,
        json!({"error": {"code": "invalid_params", "message": "an agent names an endpoint the report omits"}}),
        cx,
    );
    for _ in 0..3 {
        turn(&view, cx);
    }

    assert!(received(&mut local).is_empty(), "nothing is retried");
    view.read_with(cx, |view, _| {
        let inbox = view.endpoints[0].connection.inbox.lock().unwrap();
        assert_eq!(inbox.error, None, "the rejection is logged, not shown");
    });

    // The next change is reported as usual.
    view.update(cx, |view, _| view.active = true);
    turn(&view, cx);
    assert_eq!(reports(&mut local, &view, 0, cx).len(), 1);
}

#[gpui::test]
fn a_full_command_queue_leaves_the_report_owed(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    // Stall the worker behind a request the peer leaves unanswered, then fill
    // the queue behind it, so the report would not slip in.
    let blocker = local.stall_commands(BOOT);

    turn(&view, cx);
    view.read_with(cx, |view, _| {
        let mailbox = view.endpoints[0].connection.clients.lock().unwrap();
        assert!(!mailbox.report_pending(), "nothing was queued");
    });

    // The queue drains: every held request is answered in turn, and the
    // report follows on the next tick.
    let mut next = blocker;
    loop {
        let id = next["id"].as_str().unwrap().to_owned();
        local.respond(BOOT, &id, &json!({"id": id, "result": {"type": "ok"}}));
        turn(&view, cx);
        next = local.request();
        if next["method"] == REPORT {
            break;
        }
    }
    assert_eq!(next["params"]["active_endpoint_id"], "local");
}
