use super::*;
use herdr_client::protocol::pick::{PICK_CLOSE_KIND, PICK_OPEN_KIND};

const PICK_OPEN: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-pick-open-v1.json");
const PICK_CLOSE: &str =
    include_str!("../../../../herdr-protocol/tests/fixtures/endpoint-pick-close-v1.json");

fn key(keystroke: &str) -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(keystroke).unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

fn control(server: &mut Server, kind: &str, data: &str) {
    write_message(
        &mut server.stream,
        &ServerMessage::EndpointControl {
            kind: kind.into(),
            data: data.into(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
}

fn shown(view: &HerdrWindow) -> bool {
    view.fork.picker.is_some() && view.menu.page == Some(crate::menu::Page::Picker)
}

/// The daemon's picker controls over a real socket and the connection's own
/// reader thread: shown by the window, answered on that connection with
/// `ui.pick.resolve`, and dismissed without an answer when the daemon closes
/// it or the connection ends. Nothing typed meanwhile reaches the pane.
#[gpui::test]
fn a_daemons_picker_is_shown_answered_and_closed_over_a_real_connection(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("picker");
    cx.update(|_, cx| view.update(cx, |view, cx| prepare_mouse(view, endpoint, cx)));

    control(&mut server, PICK_OPEN_KIND, PICK_OPEN);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.poll_picker(window, cx);
                shown(view)
            });
            // The pane is ready for input, and still gets none of this.
            assert!(view.input_ready());
            view.key_down(&key("a"), window, cx);
            view.send(ClientPaneInputEvent::TextCommit("leaked".into()), cx);
            // Enter chooses the item the plugin preselected.
            view.picker_key(&key("enter"), window, cx);
            assert!(!shown(view) && view.fork.picker.is_none());
            view.send(ClientPaneInputEvent::TextCommit("after".into()), cx);
        })
    });
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = server.receive() else {
        panic!("the first frame after the picker opened must be its answer");
    };
    assert_eq!(boot_id, snapshot().boot_id);
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "ui.pick.resolve");
    assert_eq!(
        request["params"],
        serde_json::json!({"pick_id": "pick-7", "outcome": {"type": "picked", "id": "t2"}})
    );
    server.respond(&request);
    let typed = |text: &str| ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events: vec![ClientPaneInputEvent::TextCommit(text.into())],
    };
    assert_eq!(server.receive(), typed("after"));

    // The daemon closes the next one itself: dismissed, and not answered.
    control(&mut server, PICK_OPEN_KIND, PICK_OPEN);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.poll_picker(window, cx);
                shown(view)
            });
        })
    });
    control(&mut server, PICK_CLOSE_KIND, PICK_CLOSE);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.poll_picker(window, cx);
                view.fork.picker.is_none()
            });
            assert!(view.menu.page.is_none());
            view.send(ClientPaneInputEvent::TextCommit("closed".into()), cx);
        })
    });
    assert_eq!(server.receive(), typed("closed"));

    // The connection ends under an open picker: dismissed, nothing to answer.
    control(&mut server, PICK_OPEN_KIND, PICK_OPEN);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.poll_picker(window, cx);
                shown(view)
            });
        })
    });
    server.stream.shutdown(std::net::Shutdown::Both).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.poll_picker(window, cx);
                view.fork.picker.is_none()
            });
            assert!(view.menu.page.is_none());
            assert!(
                view.local_error
                    .as_deref()
                    .is_none_or(|error| !error.starts_with("Picker: "))
            );
        })
    });
}
