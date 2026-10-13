//! The actions the client performs itself under a host's keybinding
//! profile. The Herdr fork copies them from the local config
//! (`Keybinds::copy_client_actions` in its
//! `src/config/keybinds/client_actions.rs`), so the same keys open the
//! navigators, step through the history, and mark an agent unread whichever
//! host is selected.

use super::*;
use crate::controls::ForkCommand;
use crate::keymap::{Keymap, PaneKeys, daemon::client_actions::CLIENT_ACTIONS};
use std::collections::BTreeMap;

const LOCAL: &str = r#"
    [keys]
    prefix = "ctrl+a"
    agent_picker = "alt+a"
    workspace_list = "prefix+shift+l"
    history_back = "alt+left"
    history_forward = "alt+right"
    mark_unread = "prefix+u"
    new_tab = "alt+t"
"#;

const PROFILE: &str = r#"
    [keys]
    prefix = "cmd+j"
    agent_picker = "alt+p"
    history_back = "alt+b"
    mark_unread = "alt+m"
    new_tab = "alt+n"
"#;

#[test]
fn a_profile_takes_this_machines_bindings_for_the_clients_own_actions() {
    let (local, profile) = (keys(LOCAL), keys(PROFILE));
    let merged = profile.clone().with_client_actions(&local);
    for command in CLIENT_ACTIONS {
        assert_eq!(
            bound(&merged, command),
            bound(&local, command),
            "{command:?}"
        );
    }
    assert_eq!(bound(&merged, Command::Back).len(), 1);
    // Everything else is the host's, prefix included.
    assert_eq!(merged.prefixes, profile.prefixes);
    assert_eq!(
        bound(&merged, Command::Tab),
        [Trigger::Direct(parsed("alt-n"))]
    );
    assert_eq!(
        bound(&merged, Command::ToggleSidebar),
        bound(&profile, Command::ToggleSidebar)
    );
}

#[test]
fn an_action_this_machine_leaves_unset_is_unset_under_a_profile_too() {
    // The host binds all five; this machine none.
    let profile = keys(
        "[keys]\nagent_picker = 'alt+p'\nworkspace_list = 'alt+w'\nhistory_back = 'alt+b'\n\
         history_forward = 'alt+f'\nmark_unread = 'alt+m'\n",
    );
    let merged = profile.with_client_actions(&DaemonKeys::default());
    for command in CLIENT_ACTIONS {
        assert!(bound(&merged, command).is_empty(), "{command:?}");
    }
}

#[test]
fn a_local_binding_keeps_its_keystroke_over_the_hosts_use_of_it() {
    let local = keys("[keys]\nagent_picker = 'alt+n'\nmark_unread = 'prefix+c'\n");
    let merged = keys(PROFILE).with_client_actions(&local);
    let keymap = Keymap::with_overrides(&BTreeMap::new(), &PaneKeys::new(), &merged).unwrap();
    // The host gave alt+n to new_tab, and prefix+c is Herdr's default for it.
    assert!(keymap.triggers(
        Command::Fork(ForkCommand::AgentPicker),
        &parsed("alt-n"),
        false
    ));
    assert!(!keymap.triggers(Command::Tab, &parsed("alt-n"), false));
    assert_eq!(
        keymap.chord(&parsed("c")),
        Some(Command::Fork(ForkCommand::MarkUnread))
    );
    // A chord is typed after the host's prefix, like every other chord
    // while its profile applies.
    assert!(keymap.is_prefix(&parsed("cmd-j")));
    assert!(!keymap.is_prefix(&parsed("ctrl-b")));
}
