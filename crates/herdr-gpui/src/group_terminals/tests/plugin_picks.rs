//! A plugin's `ui.pick` may be addressed to any client, and each editor
//! group's parked connection is one: its picker is shown in the window and
//! answered on that connection, wherever the window keeps it meanwhile.

use super::*;
use herdr_client::{
    ClientEvent,
    protocol::pick::{EndpointPickOpen, PICK_OPEN_KIND, UiPickItem},
};
use serde_json::json;

fn pick(pick_id: &str) -> ClientEvent {
    let open = EndpointPickOpen {
        pick_id: pick_id.into(),
        title: "Move pane to".into(),
        items: vec![UiPickItem {
            id: "t2".into(),
            label: "build".into(),
            detail: None,
            badge: None,
        }],
        selected: None,
        create: None,
    };
    crate::fork::control(PICK_OPEN_KIND, &open)
}

fn press(keystroke: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke::parse(keystroke).unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

/// The next request on `peer`, which must be the answer to `pick_id`.
fn answered(peer: &mut MockPeer, pick_id: &str, outcome: serde_json::Value) {
    let request = peer.request();
    assert_eq!(request["method"], "ui.pick.resolve");
    assert_eq!(
        request["params"],
        json!({"pick_id": pick_id, "outcome": outcome})
    );
    let id = request["id"].as_str().unwrap();
    peer.respond("boot-v1", id, &json!({"id": id, "result": {"type": "ok"}}));
}

#[gpui::test]
fn a_parked_connections_picker_is_shown_and_answered_on_that_connection(cx: &mut TestAppContext) {
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
    let mut peer = MockPeer::advertising(&["ui.pick.resolve"]);
    let shown = |view: &HerdrWindow| {
        view.fork.picker.is_some() && view.menu.page == Some(crate::menu::Page::Picker)
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            terminals_side_by_side(view, left, right);
            let mut on_t1 = snapshot(40);
            on_t1.focused_workspace_id = Some("w0".into());
            on_t1.focused_tab_id = Some("t1".into());
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &peer, on_t1);
            if let Ok(mut inbox) = parked.connection.inbox.lock() {
                *inbox = parked.live.clone();
            }
            // As the connection's reader does: the fork mailbox learns the
            // peer's boot from its snapshot, then keeps the picker control.
            {
                let mut fork = parked.connection.fork.lock().unwrap();
                fork.apply(ClientEvent::Snapshot(Arc::new(fixture_snapshot())));
                assert!(fork.apply(pick("pick-1")).is_none());
            }
            view.browser.terminals.parked.push(parked);
            view.poll_picker(window, cx);
            assert!(shown(view));
            view.picker_key(&press("enter"), window, cx);
            assert!(view.fork.picker.is_none() && view.menu.page.is_none());
        })
    });
    answered(&mut peer, "pick-1", json!({"type": "picked", "id": "t2"}));

    // A parked connection that goes away takes its picker with it, unanswered.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let parked = &view.browser.terminals.parked[0];
            assert!(
                parked
                    .connection
                    .fork
                    .lock()
                    .unwrap()
                    .apply(pick("gone"))
                    .is_none()
            );
            view.poll_picker(window, cx);
            assert!(shown(view));
            let parked = view.browser.terminals.parked.pop().unwrap();
            view.poll_picker(window, cx);
            assert!(view.fork.picker.is_none() && view.menu.page.is_none());
            view.browser.terminals.parked.push(parked);
        })
    });

    // Using the parked group trades its connection for the window's own.
    // The open picker goes with the connection, not with the slot it was in.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let parked = &view.browser.terminals.parked[0];
            assert!(
                parked
                    .connection
                    .fork
                    .lock()
                    .unwrap()
                    .apply(pick("pick-2"))
                    .is_none()
            );
            view.poll_picker(window, cx);
            assert!(shown(view));
            view.activate_group(left, window, cx);
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.primary_group(), Some(left));
            assert_eq!(view.browser.terminals.parked[0].group, right);
            view.poll_picker(window, cx);
            assert!(shown(view));
            view.picker_key(&press("escape"), window, cx);
            assert!(view.fork.picker.is_none() && view.menu.page.is_none());
        })
    });
    // The first thing the peer hears after its first answer is this one:
    // nothing was sent for the picker that went away with its connection.
    answered(&mut peer, "pick-2", json!({"type": "cancelled"}));
}
