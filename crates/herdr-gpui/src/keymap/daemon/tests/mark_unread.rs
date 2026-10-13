//! `mark_unread`, the Herdr fork's `[keys]` action for the unread hold.

use super::*;
use crate::controls::ForkCommand;

#[test]
fn mark_unread_is_unset_until_the_daemon_config_binds_it() {
    // The fork ships the action without a binding, as its config template says.
    assert!(
        bound(
            &DaemonKeys::default(),
            Command::Fork(ForkCommand::MarkUnread)
        )
        .is_empty()
    );
    assert!(
        bound(
            &keys("[keys]\nmark_unread = ''\n"),
            Command::Fork(ForkCommand::MarkUnread)
        )
        .is_empty()
    );
}

#[test]
fn mark_unread_reads_the_bindings_the_daemon_config_gives_it() {
    let prefix = keys("[keys]\nmark_unread = 'prefix+u'\n");
    assert_eq!(
        bound(&prefix, Command::Fork(ForkCommand::MarkUnread)),
        [prefixed("u")]
    );
    // Binding it leaves every other action on Herdr's default.
    assert_eq!(bound(&prefix, Command::SplitRight), [prefixed("v")]);

    let both = keys("[keys]\nprefix = 'ctrl+a'\nmark_unread = ['prefix+u', 'alt+u']\n");
    assert_eq!(both.prefixes, [parsed("ctrl-a")]);
    assert_eq!(
        bound(&both, Command::Fork(ForkCommand::MarkUnread)),
        [prefixed("u"), Trigger::Direct(parsed("alt-u"))]
    );
    // An entry this client cannot express is skipped, not turned into an error.
    assert!(
        bound(
            &keys("[keys]\nmark_unread = 'hyper+u'\n"),
            Command::Fork(ForkCommand::MarkUnread)
        )
        .is_empty()
    );
    assert!(
        bound(
            &keys("[keys]\nmark_unread = 7\n"),
            Command::Fork(ForkCommand::MarkUnread)
        )
        .is_empty()
    );
}
