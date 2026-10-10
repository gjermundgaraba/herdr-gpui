//! `history_back` and `history_forward`, the Herdr fork's `[keys]` actions
//! for stepping through visited panes.

use super::*;

#[test]
fn back_and_forward_are_unset_until_the_daemon_config_binds_them() {
    for command in [Command::Back, Command::Forward] {
        assert!(bound(&DaemonKeys::default(), command).is_empty());
    }
}

#[test]
fn back_and_forward_read_the_bindings_the_daemon_config_gives_them() {
    // The bindings the fork's documentation suggests: direct keys, no prefix.
    let keys = keys("[keys]\nhistory_back = 'alt+left'\nhistory_forward = 'alt+right'\n");
    assert_eq!(
        bound(&keys, Command::Back),
        [Trigger::Direct(parsed("alt-left"))]
    );
    assert_eq!(
        bound(&keys, Command::Forward),
        [Trigger::Direct(parsed("alt-right"))]
    );
}
