use super::*;

#[test]
fn events_help_names_every_kind() {
    let usage = help::usage(Some("events"));
    for kind in crate::control::EventKind::ALL {
        assert!(usage.contains(kind.name()), "{kind:?}: {usage}");
    }
}
