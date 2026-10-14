//! Mark Agent Unread as a command; its request is tested with the code that
//! sends it, in `mark_unread/tests/request.rs`.

use super::*;

#[test]
fn mark_unread_is_a_named_command_the_window_runs() {
    let info = COMMANDS
        .iter()
        .find(|info| info.command == Command::Fork(ForkCommand::MarkUnread))
        .unwrap();
    // The config key matches the daemon's `[keys]` action name.
    assert_eq!(info.name, "mark_unread");
    assert_eq!(info.label, "Mark Agent Unread");
    assert!(info.default_shortcuts().is_empty());
    // It changes no focus, so it is not one of the fenced snapshot requests.
    assert_eq!(
        request(Command::Fork(ForkCommand::MarkUnread), &snapshot()),
        None
    );
}
