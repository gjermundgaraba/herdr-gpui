//! A navigator whose selected row goes away selects nothing, as the Herdr
//! fork's Navigator does (`navigator_selected_index` in its
//! `src/client/shell/aggregate_navigation.rs`): Enter then opens nothing,
//! rather than whichever row took the lost one's place.

use super::navigator_fixture::*;
use super::*;
use crate::controls::Command;

const DEPLOYER: &str = "Second · release: deployer";
const REVIEWER: &str = "repo: shell";
const HELPER: &str = "repo: helper";

/// Local's next snapshot, changed by `change`, as the window's poll takes it.
fn local(
    view: &Entity<HerdrWindow>,
    change: impl FnOnce(&mut ClientShellSnapshot),
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, _| {
        let mut local = (**view.live.snapshot.as_ref().unwrap()).clone();
        local.revision += 1;
        change(&mut local);
        view.live.snapshot = Some(Arc::new(local));
    });
    poll(view, cx);
}

#[gpui::test]
fn a_lost_selection_selects_nothing_and_enter_opens_nothing(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window(cx);
    ready(&view, cx);
    sent(&mut first);
    run(&view, Command::AgentPicker, cx);
    ready(&view, cx);
    cx.simulate_keystrokes("j");
    assert_eq!(selected(&view, cx), REVIEWER);

    // The selected agent goes away while the picker is open.
    let mut gone = None;
    local(
        &view,
        |snapshot| {
            let index = snapshot
                .agents
                .iter()
                .position(|agent| agent.pane_id == "w1:p1")
                .unwrap();
            gone = Some(snapshot.agents.remove(index));
        },
        cx,
    );
    let state = state(&view, cx).unwrap();
    assert_eq!(state.rows, [DEPLOYER, HELPER]);
    assert_eq!(state.selected, None, "no row takes the lost one's place");

    cx.simulate_keystrokes("enter");

    assert!(view.read_with(cx, |view, _| shown(view).is_some()));
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(view.pending_navigation, None);
    });
    assert_eq!(sent(&mut first), Sent::nothing());
    assert_eq!(sent(&mut second), Sent::nothing());

    // The row it named is selected again when it comes back.
    local(&view, |snapshot| snapshot.agents.extend(gone), cx);
    assert_eq!(selected(&view, cx), REVIEWER);
}

#[gpui::test]
fn a_step_from_nothing_selected_starts_at_the_first_row(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);
    cx.simulate_keystrokes("j j");
    assert_eq!(selected(&view, cx), HELPER);
    local(
        &view,
        |snapshot| snapshot.agents.retain(|agent| agent.pane_id != "w1:p2"),
        cx,
    );
    assert_eq!(state(&view, cx).unwrap().selected, None);

    cx.simulate_keystrokes("j");

    assert_eq!(selected(&view, cx), DEPLOYER);
}
