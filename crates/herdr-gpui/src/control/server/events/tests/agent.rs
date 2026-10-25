use super::*;

/// A tick at which `window` is front, with `agent` in its focused pane.
fn showing(window: u64, agent: Option<&str>) -> Seen {
    Seen {
        windows: vec![1, 2],
        front: Some((window, agent.map(str::to_owned))),
        ..Seen::default()
    }
}

fn agent_events(log: &Log) -> Vec<(String, Option<String>)> {
    log.after(0, &[EventKind::Agent])
        .into_iter()
        .map(|event| (event.subject, event.detail))
        .collect()
}

#[test]
fn the_front_window_stays_front_while_another_app_is_active() {
    let open = [1, 2];
    // A main window that is active is front.
    assert_eq!(front(Some(2), Some(1), &open), Some(2));
    // macOS reports no main window while another app is active: the last
    // front window stays front.
    assert_eq!(front(None, Some(2), &open), Some(2));
    // Another of the app's windows, such as Settings, is not a main window.
    assert_eq!(front(Some(9), Some(2), &open), Some(2));
    // A front window that closed gives way to an open one.
    assert_eq!(front(None, Some(7), &open), Some(1));
    assert_eq!(front(None, None, &open), Some(1));
    assert_eq!(front(None, Some(2), &[]), None);
}

#[test]
fn a_change_of_front_window_or_its_agent_is_an_event() {
    let mut log = Log::default();
    // An agent starts in the focused pane.
    compare(&mut log, &showing(1, None), &showing(1, Some("claude")));
    // Another window comes to the front, with no agent in its pane.
    compare(&mut log, &showing(1, Some("claude")), &showing(2, None));
    // Focus moves to a pane running another agent.
    compare(&mut log, &showing(2, None), &showing(2, Some("codex")));
    // The agent exits.
    compare(&mut log, &showing(2, Some("codex")), &showing(2, None));
    assert_eq!(
        agent_events(&log),
        [
            ("1".to_owned(), Some("claude".to_owned())),
            ("2".to_owned(), None),
            ("2".to_owned(), Some("codex".to_owned())),
            ("2".to_owned(), None),
        ]
    );
}

#[test]
fn the_same_window_and_agent_is_no_event() {
    let mut log = Log::default();
    // Other things change, the front window's agent does not.
    let mut later = showing(1, Some("claude"));
    later.notes = 4;
    compare(&mut log, &showing(1, Some("claude")), &later);
    assert!(agent_events(&log).is_empty());
    // Two windows running the same agent are still two fronts.
    compare(&mut log, &showing(1, Some("pi")), &showing(2, Some("pi")));
    assert_eq!(
        agent_events(&log),
        [("2".to_owned(), Some("pi".to_owned()))]
    );
}

#[test]
fn no_front_window_is_no_event() {
    let mut log = Log::default();
    compare(&mut log, &showing(1, Some("claude")), &Seen::default());
    assert!(agent_events(&log).is_empty());
}

#[test]
fn every_kind_is_named_once_and_parses_back() {
    assert!(EventKind::ALL.contains(&EventKind::Agent));
    for (index, kind) in EventKind::ALL.iter().enumerate() {
        assert_eq!(kind.name().parse::<EventKind>(), Ok(*kind));
        assert!(!EventKind::ALL[..index].contains(kind), "{kind:?}");
    }
}

#[test]
fn an_agent_event_names_the_window_and_the_agent() {
    let mut log = Log::default();
    let windows = Seen {
        windows: vec![1, 2],
        ..Seen::default()
    };
    compare(&mut log, &windows, &showing(2, Some("claude")));
    compare(&mut log, &showing(2, Some("claude")), &showing(2, None));
    let [named, none] = log.after(0, &[]).try_into().unwrap();
    let json = |event: Event| {
        let mut value = serde_json::to_value(event).unwrap();
        value["at"] = 0.into();
        value
    };
    assert_eq!(
        json(named),
        serde_json::json!({"seq": 1, "at": 0, "kind": "agent", "subject": "2", "detail": "claude"})
    );
    assert_eq!(
        json(none),
        serde_json::json!({"seq": 2, "at": 0, "kind": "agent", "subject": "2", "detail": null})
    );
}
