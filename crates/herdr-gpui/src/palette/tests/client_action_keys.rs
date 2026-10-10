//! Go To under a host's keybinding profile. The profile's
//! `navigate_workspace_*` keys move the selection, but a keystroke this
//! machine gives one of the client's own actions stays that action's, and no
//! command runs while a menu page is open.

use super::*;
use crate::{
    keymap::{DaemonKeys, Keymap, PaneKeys},
    window::ActiveServerKeymap,
};
use std::collections::BTreeMap;

fn keys(text: &str) -> DaemonKeys {
    let table: toml::Table = text.parse().unwrap();
    DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table))
}

#[gpui::test]
fn a_client_actions_direct_key_does_not_move_the_go_to_selection(cx: &mut TestAppContext) {
    let profile = keys("[keys]\nnavigate_workspace_down = ['ctrl+n', 'ctrl+j']\n");
    let local = keys("[keys]\nhistory_back = 'ctrl+n'\n");
    let keymap = Keymap::with_overrides(
        &BTreeMap::new(),
        &PaneKeys::new(),
        &profile.with_client_actions(&local),
    )
    .unwrap();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.live.snapshot = Some(Arc::new(snapshot()));
            view.config.keybindings = keymap.clone();
        });
        // As for the active window under a device's profile.
        cx.set_global(ActiveServerKeymap(Some(keymap)));
        crate::bind_keys(cx);
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Navigation, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    let selected = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::Palette));
            let palette = view.menu.palette.as_ref().unwrap();
            assert!(palette.filtered.len() > 1);
            palette.selected
        })
    };
    assert_eq!(selected(cx), 0);

    // The profile's own navigate key moves the selection, as an arrow does.
    cx.simulate_keystrokes("ctrl-j");
    assert_eq!(selected(cx), 1);

    // The one this machine bound to Back is bound, so the list never sees
    // it; and Back does not run under a menu page.
    cx.simulate_keystrokes("ctrl-n");
    assert_eq!(selected(cx), 1);
    view.read_with(cx, |view, _| assert_eq!(view.local_error, None));
}
