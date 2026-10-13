//! A jump that arrives during a host switch the user started. This window
//! selects the new host at the click, so a first jump to the pane the host
//! it left was showing navigates back and is answered `focused` once that
//! host's surface is active again. The Herdr fork's terminal client lets the
//! switch finish and answers `timeout`
//! (`a_first_jump_to_the_pane_shown_times_out_when_the_switch_leaves_it` in
//! its `src/client/shell/tests/client_jump.rs`).

use super::jumps::{answer_of, answers, arrive, asked, client, jump, turn};
use super::wiring::BOOT;
use super::*;
use crate::navigation::NavigationTarget;
use std::time::{Duration, Instant};

#[gpui::test]
fn a_first_jump_to_the_pane_left_by_a_user_switch_navigates_back(cx: &mut gpui::TestAppContext) {
    let (mut local, _remote, view, cx) = client(cx);
    let start = Instant::now();

    // The user clicks Build; its surface is not active yet.
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint(&host_id(1), cx))
    });
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 1));

    // A tool asks for the pane Local was showing.
    arrive(&view, 0, jump("jump-1", "local", BOOT, "pane_1"), cx);
    turn(&view, start, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0, "the jump takes the window back");
        assert_eq!(
            view.pending_navigation
                .as_ref()
                .map(|pending| pending.as_deref()),
            Some(NavigationTarget::Pane("pane_1"))
        );
        assert!(!view.surface_activation_ready());
    });
    assert!(
        answers(&asked(&mut local, &view, 0, cx)).is_empty(),
        "not focused while Local's surface is not active"
    );
    turn(&view, start + Duration::from_millis(40), cx);
    assert!(answers(&asked(&mut local, &view, 0, cx)).is_empty());

    view.update(cx, |view, _| presented(view));
    turn(&view, start + Duration::from_millis(80), cx);
    assert_eq!(
        answers(&asked(&mut local, &view, 0, cx)),
        answer_of(ClientJumpOutcome::Focused)
    );
}
