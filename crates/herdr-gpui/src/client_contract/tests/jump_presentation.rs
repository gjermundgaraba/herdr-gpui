//! A jump is `focused` only once the window presents the pane: its host is
//! selected and its surface is active and drawn from the snapshot that shows
//! the pane focused. Selecting a host shows the snapshot it last sent at
//! once, and that snapshot's focus says nothing about the screen while the
//! surface is still being activated, or after its activation failed.

use super::jumps::{answer_of, answers, arrive, asked, client, focus, jump, turn};
use super::wiring::BOOT;
use super::*;
use crate::{client_contract::jump::NAVIGATION_SETTLE_DEADLINE, state::SurfaceActivation};
use std::time::{Duration, Instant};

/// Build, selected for a jump, has its surface asked for: a frame of its
/// latest snapshot is drawn, the daemon has not acknowledged the activation.
fn activating(view: &mut HerdrWindow) {
    presented(view);
    view.live.activation = Some(SurfaceActivation {
        request: "activate".into(),
        boot: BOOT.into(),
        revision: None,
        failed: false,
        focus: None,
        active: true,
    });
}

#[gpui::test]
fn a_selected_host_is_not_focused_before_its_surface_is_active(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    let build = host_id(1);
    // Build last showed `pane_7` focused, before the window ever presented it.
    focus(&view, 1, "pane_7", cx);

    arrive(&view, 0, jump("jump-1", &build, BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 1));
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    view.update(cx, |view, _| activating(view));
    turn(&view, start + Duration::from_millis(40), cx);
    assert!(
        answers(&asked(&mut local, &view, 0, cx)).is_empty(),
        "the activation is not acknowledged yet"
    );

    view.update(cx, |view, _| {
        let revision = view.live.snapshot.as_ref().unwrap().revision;
        if let Some(activation) = &mut view.live.activation {
            activation.revision = Some(revision);
        }
    });
    turn(&view, start + Duration::from_millis(80), cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Focused)
    );
}

#[gpui::test]
fn a_failed_activation_is_never_focused(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();
    focus(&view, 1, "pane_7", cx);

    arrive(&view, 0, jump("jump-1", &host_id(1), BOOT, "pane_7"), cx);
    turn(&view, start, cx);
    view.update(cx, |view, _| {
        activating(view);
        if let Some(activation) = &mut view.live.activation {
            activation.failed = true;
        }
    });
    turn(&view, start + Duration::from_secs(1), cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    turn(&view, start + NAVIGATION_SETTLE_DEADLINE, cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Timeout)
    );
}
