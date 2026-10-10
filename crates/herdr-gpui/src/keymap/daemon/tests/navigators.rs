//! `agent_picker` and `workspace_list`, the Herdr fork's `[keys]` actions
//! for its two client-side navigators.

use super::*;

#[test]
fn the_navigators_are_unset_until_the_daemon_config_binds_them() {
    // The fork ships both without a binding.
    for command in [Command::AgentPicker, Command::WorkspaceList] {
        assert!(bound(&DaemonKeys::default(), command).is_empty());
    }
}

#[test]
fn the_navigators_read_the_bindings_the_daemon_config_gives_them() {
    // The bindings the fork's documentation suggests.
    let keys = keys("[keys]\nagent_picker = 'prefix+a'\nworkspace_list = 'prefix+shift+l'\n");
    assert_eq!(bound(&keys, Command::AgentPicker), [prefixed("a")]);
    assert_eq!(bound(&keys, Command::WorkspaceList), [prefixed("shift-l")]);
    // Go To keeps its own two actions beside them.
    assert_eq!(
        bound(&keys, Command::WorkspacePicker),
        [prefixed("w"), prefixed("g")]
    );
}
