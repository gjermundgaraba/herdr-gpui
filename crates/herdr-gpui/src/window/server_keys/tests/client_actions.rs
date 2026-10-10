//! The client's own actions while a device's keybinding profile applies:
//! they keep this machine's `[keys]`, as in the Herdr fork.

use super::*;
use crate::keymap::DaemonKeys;

const HOST_KEYS: &str = "[keys]\nprefix = \"cmd+j\"\nagent_picker = \"alt+p\"\n\
                         history_back = \"alt+b\"\nmark_unread = \"alt+m\"\n";

fn local_keys(text: &str) -> DaemonKeys {
    let table: toml::Table = text.parse().unwrap();
    DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table))
}

#[gpui::test]
fn a_devices_profile_leaves_the_clients_own_actions_on_local_keys(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        view.active = true;
        view.config.daemon_keys = local_keys(
            "[keys]\nagent_picker = 'alt+a'\nworkspace_list = 'prefix+shift+l'\n\
             history_back = 'alt+o'\nhistory_forward = 'alt+i'\nmark_unread = 'prefix+u'\n",
        );
        opt_in(view, KeybindingSource::Server);
        select(view, 1, Some(HOST_KEYS), cx);
        let keymap = view.keymap();
        // The host's keys apply: its prefix starts a chord.
        assert!(keymap.is_prefix(&key("cmd-j")));
        for (command, typed) in [
            (Command::AgentPicker, "alt-a"),
            (Command::Back, "alt-o"),
            (Command::Forward, "alt-i"),
        ] {
            assert!(keymap.triggers(command, &key(typed), false), "{command:?}");
        }
        assert_eq!(keymap.chord(&key("shift-l")), Some(Command::WorkspaceList));
        assert_eq!(keymap.chord(&key("u")), Some(Command::MarkUnread));
        // What the host says of them is not read.
        for (command, typed) in [
            (Command::AgentPicker, "alt-p"),
            (Command::Back, "alt-b"),
            (Command::MarkUnread, "alt-m"),
        ] {
            assert!(!keymap.triggers(command, &key(typed), false), "{command:?}");
        }
    });
}

#[gpui::test]
fn a_devices_prefix_keeps_its_key_from_a_local_client_action(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        view.active = true;
        // The key this machine gives the agent picker is the host's prefix.
        view.config.daemon_keys = local_keys("[keys]\nagent_picker = 'cmd+j'\n");
        opt_in(view, KeybindingSource::Server);
        select(view, 1, Some(HOST_KEYS), cx);
        let keymap = view.keymap();
        // The prefix is placed first: the key starts a chord, and the action
        // is left without it. The terminal client runs the action instead.
        assert!(keymap.is_prefix(&key("cmd-j")));
        assert!(!keymap.triggers(Command::AgentPicker, &key("cmd-j"), false));
    });
}

#[gpui::test]
fn a_reloaded_local_table_rebuilds_the_devices_keymap(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        opt_in(view, KeybindingSource::Server);
        select(view, 1, Some(HOST_KEYS), cx);
        assert!(!view.keymap().triggers(Command::Back, &key("alt-o"), false));

        // The same host and profile, but this machine's table changed.
        view.config.daemon_keys = local_keys("[keys]\nhistory_back = 'alt+o'\n");
        view.sync_server_keymap(cx);
        assert!(view.keymap().triggers(Command::Back, &key("alt-o"), false));
        assert!(view.keymap().is_prefix(&key("cmd-j")));
    });
}
