//! The actions the client performs itself stay on the local table under a
//! host's profile, as in the Herdr fork's
//! `src/config/keybinds/client_actions.rs`.

use super::{DaemonKeys, Target, Trigger};
use crate::controls::{Command, ForkCommand};

/// The targets of the fork's own `[keys]` actions.
pub(super) const MARK_UNREAD: Target = Target::Command(Command::Fork(ForkCommand::MarkUnread));
pub(super) const AGENT_PICKER: Target = Target::Command(Command::Fork(ForkCommand::AgentPicker));
pub(super) const WORKSPACE_LIST: Target =
    Target::Command(Command::Fork(ForkCommand::WorkspaceList));

/// The actions this client performs itself, which never reach a daemon: the
/// two navigators, Back and Forward, and the request for the unread hold.
/// The Herdr fork reads them from the local config even while a host's
/// keybinding profile is in effect (`Keybinds::copy_client_actions`), and so
/// does this client, through [`DaemonKeys::with_client_actions`].
///
/// A keystroke the local config gives one of them is that action's over the
/// profile's command for it, except the profile's prefix key: a keymap
/// places its prefixes before any binding, so a direct key of a client
/// action that equals the profile's prefix arms the prefix, and the action
/// is left without that key. The profile's `navigate_workspace_up` and
/// `_down` are not commands: they move the Go To palette's selection, from
/// its key handler, which a bound keystroke never reaches. So in Go To a
/// direct key of a client action does not move the selection, and it does
/// not run either, because no command runs while a menu page is open; a
/// prefix chord closes the page and runs.
///
/// The terminal client gives the local action the keystroke in the same
/// way, with three differences. In its Navigate mode, which this client
/// does not have, the profile's navigate keys are matched before the client
/// actions. It matches a direct binding before the prefix, so there a
/// client action on the profile's prefix key runs and the prefix cannot be
/// armed. And when the local `config.toml` stops parsing while it runs, it
/// keeps the five bindings it last read; this client reads no `[keys]` from
/// a file that does not parse, so none of the five has a `[keys]` binding
/// until it does.
pub(super) const CLIENT_ACTIONS: &[Command] = &[
    Command::Fork(ForkCommand::AgentPicker),
    Command::Fork(ForkCommand::WorkspaceList),
    Command::Back,
    Command::Forward,
    Command::Fork(ForkCommand::MarkUnread),
];

impl DaemonKeys {
    /// A host's profile with `local`'s bindings for the actions the client
    /// performs itself, in place of whatever the profile says of them. They
    /// come first, so a keystroke the user gave one of them here keeps it
    /// over a command the host binds to the same keystroke, though not over
    /// the host's prefix key; a prefix chord is typed after the profile's
    /// prefix, like every other chord while it applies.
    pub(crate) fn with_client_actions(mut self, local: &Self) -> Self {
        let client = |(command, _): &(Command, Trigger)| CLIENT_ACTIONS.contains(command);
        self.bindings.retain(|binding| !client(binding));
        self.bindings.splice(
            0..0,
            local
                .bindings
                .iter()
                .filter(|binding| client(binding))
                .cloned(),
        );
        self
    }
}
