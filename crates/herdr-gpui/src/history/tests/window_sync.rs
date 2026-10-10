//! What the window records as it ticks: a visit per presented snapshot whose
//! focused pane changed, and nothing for a host it no longer holds.

use super::*;

#[gpui::test]
fn a_visit_is_recorded_once_per_snapshot_whose_focused_pane_changed(cx: &mut gpui::TestAppContext) {
    let (_local, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    assert_eq!(visits(&view, cx), (vec![local("p1")], 0));

    // Ticks without a new snapshot record nothing.
    for _ in 0..5 {
        view.update(cx, |view, _| view.sync_history());
    }
    // Nor does a snapshot that leaves the focus where it was.
    visit(&view, "p1", cx);
    assert_eq!(visits(&view, cx), (vec![local("p1")], 0));

    // Focus changes that never reached a tick are not visits: only the
    // snapshot the window holds when it ticks is.
    view.update(cx, |view, _| {
        for pane in ["p2", "p3"] {
            let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
            next.focused_pane_id = Some(pane.into());
            show(view, next);
        }
        view.sync_history();
    });
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p3")], 1));
}

#[gpui::test]
fn presenting_another_host_visits_its_focused_pane(cx: &mut gpui::TestAppContext) {
    let (_local, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());

    present(&view, 1, cx);
    assert_eq!(visits(&view, cx), (vec![local("p1"), build("r1")], 1));
    view.read_with(cx, |view, _| {
        let visit = &view.history.history.visits[1];
        assert_eq!(visit.boot_id, BOOT);
    });
    present(&view, 0, cx);
    assert_eq!(
        visits(&view, cx),
        (vec![local("p1"), build("r1"), local("p1")], 2)
    );
}

#[gpui::test]
fn a_closed_pane_and_a_restart_prune_a_host_that_is_not_presented(cx: &mut gpui::TestAppContext) {
    let (_local, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    present(&view, 1, cx);
    visit(&view, "r2", cx);
    present(&view, 0, cx);
    visit(&view, "p2", cx);
    let recorded = vec![
        local("p1"),
        build("r1"),
        build("r2"),
        local("p1"),
        local("p2"),
    ];
    assert_eq!(visits(&view, cx), (recorded, 4));

    // Build closes `r2` while Local is presented.
    view.update(cx, |view, _| {
        let mut next = snapshot(&["r1"], "r1");
        next.revision = 9;
        view.endpoints[1].live.snapshot = Some(Arc::new(next));
        view.sync_history();
    });
    assert_eq!(
        visits(&view, cx),
        (vec![local("p1"), build("r1"), local("p1"), local("p2")], 3)
    );

    // Build restarts: its pane ids may name other panes now.
    view.update(cx, |view, _| {
        let mut next = snapshot(&["r1"], "r1");
        next.boot_id = "boot-v2".into();
        view.endpoints[1].live.snapshot = Some(Arc::new(next));
        view.sync_history();
    });
    // The two Local visits around it became neighbours and merged.
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p2")], 1));
}

#[gpui::test]
fn a_host_that_is_switched_off_or_removed_is_forgotten(cx: &mut gpui::TestAppContext) {
    let (_local, _remote, view, cx) = window(cx);
    view.update(cx, |view, _| view.sync_history());
    present(&view, 1, cx);
    present(&view, 0, cx);
    visit(&view, "p2", cx);
    assert_eq!(visits(&view, cx).0.len(), 4);

    // A host that is only reconnecting keeps its visits.
    view.update(cx, |view, _| {
        let live = &mut view.endpoints[1].live;
        live.status = ConnectionStatus::Disconnected;
        live.error = Some("connection lost".into());
        view.sync_history();
    });
    assert_eq!(visits(&view, cx).0.len(), 4);

    view.update(cx, |view, _| {
        view.endpoints[1].enabled = false;
        view.sync_history();
    });
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p2")], 1));

    // Switched on again and visited, then removed from the devices list.
    view.update(cx, |view, _| {
        view.endpoints[1].enabled = true;
        let live = &mut view.endpoints[1].live;
        live.status = ConnectionStatus::Connected;
        live.error = None;
    });
    present(&view, 1, cx);
    assert_eq!(visits(&view, cx).0.last(), Some(&build("r1")));
    view.update(cx, |view, _| {
        view.selected_endpoint = 0;
        view.live = view.endpoints[0].live.clone();
        view.endpoints.truncate(1);
        view.sync_history();
    });
    assert_eq!(visits(&view, cx), (vec![local("p1"), local("p2")], 1));
}
