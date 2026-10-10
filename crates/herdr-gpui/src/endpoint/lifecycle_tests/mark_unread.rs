//! `mark_unread` over a real connection: the one request it sends, on the
//! host that owns the focused pane, where the daemon's refusal shows, and the
//! `Marked unread` it says once the daemon accepted, as the Herdr fork's
//! terminal client does (`src/client/shell/actions.rs`), and what a press
//! that cannot be sent says.

use super::*;

const REFUSAL: &str = "only an acknowledged completion, shown as idle, can be marked unread";

fn refuse(server: &mut Server, request: &serde_json::Value) {
    let id = request["id"].as_str().unwrap();
    write_message(
        &mut server.stream,
        &ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: snapshot().boot_id,
            request_id: id.into(),
            final_chunk: true,
            data: serde_json::to_vec(&serde_json::json!({
                "id": id,
                "error": {"code": "agent_not_idle", "message": REFUSAL},
            }))
            .unwrap(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
}

#[gpui::test]
fn mark_unread_asks_the_selected_host_about_its_focused_pane(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:fixture");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            // A queued mark is no news yet, so an unrelated error stays shown.
            view.local_error = Some("Input not sent: earlier".into());
            let before = view.live.snapshot.clone().unwrap();
            view.command(Command::MarkUnread, window, cx);
            assert_eq!(view.local_error.as_deref(), Some("Input not sent: earlier"));
            view.local_error = None;
            // It is no focus change: input is not fenced, and nothing about
            // the agent is recorded here. The badge is the snapshot's to show.
            assert!(view.input_ready());
            assert!(Arc::ptr_eq(view.live.snapshot.as_ref().unwrap(), &before));
            view.send(ClientPaneInputEvent::TextCommit("after".into()), cx);
        })
    });
    // The host that owns the pane gets the request, before what was typed next.
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = server.receive() else {
        panic!("the first frame must be the mark");
    };
    assert_eq!(boot_id, snapshot().boot_id);
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "agent.mark_unread");
    assert_eq!(request["params"], serde_json::json!({"pane_id": "w1:p1"}));
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("after".into())],
        }
    );

    // The daemon refuses a working agent; its reason reaches the status line.
    refuse(&mut server, &request);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "the refusal", |view| view.live.error.is_some());
            assert_eq!(view.live.error.as_deref(), Some(REFUSAL));
            // The answer is followed, and a refusal confirms nothing.
            view.tick_client_contract(Instant::now(), cx);
            assert!(view.flash.is_none());
            assert_eq!(
                view.live.status_text(view.local_error.as_deref()),
                format!("Connected: {REFUSAL}")
            );
        })
    });

    // A second press is a second request, not a client-side toggle.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(Command::MarkUnread, window, cx))
    });
    let ClientMessage::ClientShellEndpointRequest {
        request: second, ..
    } = server.receive()
    else {
        panic!("a second mark must be sent as well");
    };
    let second: serde_json::Value = serde_json::from_str(&second).unwrap();
    assert_eq!(second["method"], "agent.mark_unread");
    assert_eq!(second["params"], request["params"]);
    assert_ne!(second["id"], request["id"]);
    // Sent is not accepted: nothing is said until the daemon answers.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_client_contract(Instant::now(), cx);
            assert!(view.flash.is_none());
        })
    });
    server.respond(&second);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while view.flash.is_none() {
                assert!(Instant::now() < deadline, "the acceptance was never said");
                std::thread::sleep(Duration::from_millis(1));
                view.tick_client_contract(Instant::now(), cx);
            }
            let said = view.flash.as_ref().map(|(flash, _)| flash.clone());
            assert_eq!(said, Some(crate::window::Flash::success("Marked unread")));
            // Said once: the answer is not read again on the next tick.
            view.flash = None;
            view.tick_client_contract(Instant::now(), cx);
            assert!(view.flash.is_none());
        })
    });
}

#[gpui::test]
fn mark_unread_without_a_connection_or_under_a_menu_sends_nothing(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    // The fixture's own host has a snapshot and no connection: the press
    // says so rather than nothing.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.endpoints[0].connection.handle.is_none());
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id = Some("p0".into());
            view.command(Command::MarkUnread, window, cx);
            assert_eq!(
                view.local_error.take().as_deref(),
                Some("Mark Agent Unread: client is disconnected")
            );
            // Without a snapshot there is no pane to name yet.
            let snapshot = view.live.snapshot.take();
            view.command(Command::MarkUnread, window, cx);
            assert_eq!(
                view.local_error.take().as_deref(),
                Some("Mark Agent Unread: the host has not shown its panes yet.")
            );
            view.live.snapshot = snapshot;
        })
    });
    let (endpoint, mut server) = connected_endpoint("ssh:fixture");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            // No focused pane: nothing to name, and the user is told so.
            let focused = {
                let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.focused_pane_id.take()
            };
            view.command(Command::MarkUnread, window, cx);
            assert_eq!(
                view.local_error.take().as_deref(),
                Some("Mark Agent Unread: no pane is focused.")
            );
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id = focused;
            // An open menu owns the keyboard, as for every command.
            view.menu.page = Some(crate::menu::Page::Workspace);
            view.command(Command::MarkUnread, window, cx);
            view.menu.page = None;
            view.send(ClientPaneInputEvent::TextCommit("only".into()), cx);
        })
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("only".into())],
        }
    );
}
