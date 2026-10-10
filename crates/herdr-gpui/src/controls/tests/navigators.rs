//! The agent picker and the workspace list as named commands.

use super::*;

#[test]
fn the_navigators_are_named_as_the_daemons_keys_table_names_them() {
    for (command, name, label) in [
        (Command::AgentPicker, "agent_picker", "Agent Picker"),
        (Command::WorkspaceList, "workspace_list", "Workspace List"),
    ] {
        let info = COMMANDS
            .iter()
            .find(|info| info.command == command)
            .unwrap();
        assert_eq!((info.name, info.label), (name, label));
        assert!(
            info.default_shortcuts().is_empty(),
            "unset by default, as in Herdr"
        );
        // Client-side lists: opening one asks the daemon for nothing.
        assert_eq!(request(command, &snapshot()), None);
    }
}
