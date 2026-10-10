//! A window reports to its local daemon on one connection. Each editor
//! group's parked connection is a client of its own to that daemon, and using
//! a group swaps connections, so the report stays with the connection that
//! carries it instead of moving with every swap.

use super::*;
use herdr_client::{ClientEvent, protocol::endpoint::EndpointServerWelcome};
use serde_json::json;

const REPORT: &str = "client.report";

/// What a connection's reader hands its mailboxes once the daemon has
/// welcomed the client and sent its first snapshot.
fn greet(connection: &ConnectionBridge) {
    let mut welcome: EndpointServerWelcome = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome.methods.push(REPORT.into());
    let mut clients = connection.clients.lock().unwrap();
    clients.apply(ClientEvent::Connected(welcome));
    clients.apply(ClientEvent::Snapshot(Arc::new(fixture_snapshot())));
}

/// The requests `peer` was sent since the last call, each answered; a
/// sentinel typed into the connection marks the end.
fn reports(peer: &mut MockPeer, connection: &ConnectionBridge) -> usize {
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
    let mut reports = 0;
    loop {
        match peer.receive() {
            ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "sentinel" => {
                return reports;
            }
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                let request: serde_json::Value = serde_json::from_str(&request).unwrap();
                if request["method"] != REPORT {
                    continue;
                }
                reports += 1;
                let id = request["id"].as_str().unwrap();
                let answer =
                    peer.respond("boot-v1", id, &json!({"id": id, "result": {"type": "ok"}}));
                connection.mailboxes().deliver(answer);
            }
            _ => {}
        }
    }
}

#[gpui::test]
fn the_report_stays_with_its_connection_through_a_group_swap(cx: &mut TestAppContext) {
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
    let mut own = MockPeer::advertising(&[REPORT]);
    let mut group = MockPeer::advertising(&[REPORT]);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0].connection.handle = Some(own.client.handle.clone());
            greet(&view.endpoints[0].connection);
            terminals_side_by_side(view, left, right);
            let mut on_t1 = snapshot(40);
            on_t1.focused_workspace_id = Some("w0".into());
            on_t1.focused_tab_id = Some("t1".into());
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &group, on_t1);
            if let Ok(mut inbox) = parked.connection.inbox.lock() {
                *inbox = parked.live.clone();
            }
            greet(&parked.connection);
            view.browser.terminals.parked.push(parked);
            view.sync_client_contract();
        })
    });
    // The window's own connection reports; the group's is another client of
    // the same daemon and says nothing.
    view.read_with(cx, |view, _| {
        assert_eq!(reports(&mut own, &view.endpoints[0].connection), 1);
        let parked = &view.browser.terminals.parked[0].connection;
        assert_eq!(reports(&mut group, parked), 0);
    });

    // Using the left group trades connections: the window now speaks through
    // the group's, and its old one is parked under the right group.
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_group(left, window, cx)));
    draw(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert_eq!(view.primary_group(), Some(left));
            // What the window shows changed with the swap, so a report is due.
            view.active = !view.active;
            view.sync_client_contract();
        })
    });
    view.read_with(cx, |view, _| {
        let parked = &view.browser.terminals.parked[0].connection;
        assert_eq!(
            reports(&mut own, parked),
            1,
            "the report follows its connection into the parked slot"
        );
        assert_eq!(reports(&mut group, &view.endpoints[0].connection), 0);
    });

    // Once the window lets go of that connection, its own one takes over.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.browser.terminals.parked.clear();
            view.sync_client_contract();
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(reports(&mut group, &view.endpoints[0].connection), 1);
    });
}
