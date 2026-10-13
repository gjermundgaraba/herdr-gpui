//! An open navigator lists its rows again whenever any host's snapshot
//! changes. That must not take the list from where the user scrolled it: the
//! list is asked to scroll only when the selected row moved.

use super::navigator_fixture::*;
use super::*;
use crate::controls::Command;
use crate::controls::ForkCommand;
use gpui::{Context, Window};

const DEPLOYER: &str = "Second · release: deployer";
const REVIEWER: &str = "repo: shell";
const HELPER: &str = "repo: helper";

/// The row `act` made the list ask to scroll to. The list keeps the request
/// only until it is next drawn, so it is read in the update that made it.
fn scrolled_by(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    act: impl FnOnce(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>),
) -> Option<usize> {
    let asked = |view: &HerdrWindow| {
        let picker = shown(view).expect("an open picker");
        picker.scroll.0.borrow_mut().deferred_scroll_to_item.take()
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            asked(view);
            act(view, window, cx);
            asked(view).map(|asked| asked.item_index)
        })
    })
}

/// A host's next snapshot, changed by `change`.
fn next(
    snapshot: &mut Option<Arc<ClientShellSnapshot>>,
    change: impl FnOnce(&mut ClientShellSnapshot),
) {
    let mut next = (**snapshot.as_ref().unwrap()).clone();
    next.revision += 1;
    change(&mut next);
    *snapshot = Some(Arc::new(next));
}

#[gpui::test]
fn a_host_change_scrolls_only_when_the_selected_row_moved(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::Fork(ForkCommand::AgentPicker), cx);
    // The step itself brings its row into view.
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        view.picker_key(&key("j"), window, cx);
    });
    assert_eq!(asked, Some(1));
    assert_eq!(listed(&view, cx), [DEPLOYER, REVIEWER, HELPER]);
    assert_eq!(selected(&view, cx), REVIEWER);

    // Snapshots that leave the selected row where it is: another host's
    // with nothing new to list, and one that changes a row below it.
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        next(&mut view.endpoints[1].live.snapshot, |_| {});
        view.poll_picker(window, cx);
    });
    assert_eq!(asked, None);
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        next(&mut view.live.snapshot, |snapshot| {
            snapshot.agents.retain(|agent| agent.pane_id != "w1:p2");
        });
        view.poll_picker(window, cx);
    });
    assert_eq!(asked, None);
    assert_eq!(listed(&view, cx), [DEPLOYER, REVIEWER]);
    assert_eq!(selected(&view, cx), REVIEWER);

    // The row above it goes away: the selected row moved, and is followed.
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        next(&mut view.endpoints[1].live.snapshot, |snapshot| {
            snapshot.agents.clear();
        });
        view.poll_picker(window, cx);
    });
    assert_eq!(asked, Some(0));
    assert_eq!(listed(&view, cx), [REVIEWER]);
    assert_eq!(selected(&view, cx), REVIEWER);

    // The selected row itself goes away: there is nothing to scroll to.
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        next(&mut view.live.snapshot, |snapshot| snapshot.agents.clear());
        view.poll_picker(window, cx);
    });
    assert_eq!(asked, None);
    assert_eq!(state(&view, cx).unwrap().selected, None);
}

#[gpui::test]
fn a_filter_the_user_sets_brings_the_selection_into_view(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::Fork(ForkCommand::AgentPicker), cx);
    assert_eq!(state(&view, cx).unwrap().selected, Some(0));

    // The filter selects its first row, which is row 0 as before. The list
    // is the user's own doing, so it is scrolled there all the same.
    let asked = scrolled_by(&view, cx, |view, window, cx| {
        view.picker_key(&key("i"), window, cx);
    });
    assert_eq!(asked, Some(0));
    assert_eq!(listed(&view, cx), [HELPER]);
}
