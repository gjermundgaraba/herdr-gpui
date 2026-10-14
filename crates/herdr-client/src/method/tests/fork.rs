//! The fork's client-lane methods, spelled as the Herdr fork's
//! `endpoint-agent-mark-unread-v1.json` fixture and its `ui.pick` docs spell
//! them.

use super::*;

#[test]
fn fork_methods_match_the_forks_spelling() {
    for (method, name) in [
        (ForkMethod::AgentMarkUnread, "agent.mark_unread"),
        (ForkMethod::UiPickResolve, "ui.pick.resolve"),
    ] {
        let method = Method::Fork(method);
        assert_eq!(method.as_str(), name);
        assert_eq!(method.to_string(), name);
        assert!(method.advertised_in(&["pane.focus".into(), name.into()]));
        // Stock Herdr advertises none of them, so each is rejected there
        // before it is sent.
        assert!(!method.advertised_in(&["client_shell.surface.set".into()]));
        assert!(!method.advertised_in(&[format!("{name}.extra")]));
    }
    // The plugin's own `ui.pick` methods are not lane methods.
    let resolve = Method::Fork(ForkMethod::UiPickResolve);
    assert!(!resolve.advertised_in(&["ui.pick".into(), "ui.pick.close".into()]));
}
