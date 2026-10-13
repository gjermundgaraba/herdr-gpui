//! The workspace list in a window: what `workspace_list` opens, how typing
//! searches it, and where Enter leads.

use super::navigator_fixture::*;
use super::*;
use crate::{controls::Command, navigation::NavigationTarget};
use herdr_client::protocol::AgentStatus;

/// The fixture's local endpoint is labelled by its host.
fn local_label(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.endpoints[0].label.clone())
}

#[gpui::test]
fn it_lists_workspaces_alone_with_the_search_focused(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    let local = local_label(&view, cx);

    run(&view, Command::WorkspaceList, cx);

    assert_eq!(
        state(&view, cx),
        Some(Shown {
            title: "Workspaces".into(),
            // Each host, then its workspaces; no pane until a search.
            rows: vec![local, "repo".into(), "Second".into(), "release".into()],
            // The focused workspace of the presented host.
            selected: Some(1),
            searching: true,
            query: String::new(),
        })
    );
    view.read_with(cx, |view, _| {
        let picker = shown(view).unwrap();
        let item = |index: usize| picker.list.item(index).unwrap();
        assert_eq!([item(0).indent, item(1).indent], [0, 1]);
        assert_eq!(
            item(1).detail.as_ref(),
            "main",
            "a workspace shows its branch"
        );
        assert_eq!(item(1).status, None);
    });
}

#[gpui::test]
fn typing_searches_and_matching_panes_appear_under_their_workspaces(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window(cx);
    let local = local_label(&view, cx);
    run(&view, Command::WorkspaceList, cx);

    cx.simulate_input("helper");

    let found = state(&view, cx).unwrap();
    assert_eq!(found.query, "helper");
    assert_eq!(found.rows, [local.as_str(), "repo", "main · helper · 2"]);
    // A search selects its first pane.
    assert_eq!(found.selected, Some(2));
    view.read_with(cx, |view, _| {
        let pane = shown(view).unwrap().list.item(2).unwrap();
        assert_eq!(pane.indent, 2);
        assert_eq!(pane.status, Some(AgentStatus::Idle));
        assert_eq!(pane.detail.as_ref(), "/repo/api");
    });

    // The list keys of the search: arrows and Control-N/P. Letters are text.
    cx.simulate_keystrokes("up");
    assert_eq!(selected(&view, cx), "repo");
    cx.simulate_keystrokes("ctrl-n");
    assert_eq!(selected(&view, cx), "main · helper · 2");
    cx.simulate_keystrokes("ctrl-p ctrl-p");
    assert_eq!(selected(&view, cx), local);
    cx.simulate_keystrokes("down down down");
    assert_eq!(selected(&view, cx), "main · helper · 2");
    cx.simulate_input("s");
    assert_eq!(state(&view, cx).unwrap().query, "helpers");
    assert!(listed(&view, cx).is_empty());
    assert_eq!(sent(&mut first), Sent::nothing());
}

#[gpui::test]
fn escape_leaves_the_search_and_then_closes(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::WorkspaceList, cx);
    // Both spaces are on `main`, so the search lists every pane.
    cx.simulate_input("main");
    assert!(state(&view, cx).unwrap().searching);

    cx.simulate_keystrokes("escape");
    let left = state(&view, cx).expect("still open");
    assert!(!left.searching);
    assert_eq!(left.query, "main");
    assert_eq!(
        left.rows[2..],
        [
            "main · shell · 1",
            "main · helper · 2",
            "Second",
            "release",
            "deployer"
        ]
    );
    // On the rows, Left and Right step between the workspaces that list
    // panes, landing on the first pane of each.
    assert_eq!(left.selected, Some(2));
    cx.simulate_keystrokes("right");
    assert_eq!(selected(&view, cx), "deployer");
    cx.simulate_keystrokes("right left");
    assert_eq!(selected(&view, cx), "main · shell · 1");

    cx.simulate_keystrokes("escape");
    assert_closed(&view, cx);
}

#[gpui::test]
fn enter_opens_the_selected_workspace_or_pane(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window(cx);
    ready(&view, cx);
    sent(&mut first);
    run(&view, Command::WorkspaceList, cx);
    ready(&view, cx);

    cx.simulate_keystrokes("enter");

    assert_closed(&view, cx);
    assert_eq!(
        sent(&mut first).requests.first(),
        Some(&json!({"method": "workspace.focus", "params": {"workspace_id": "w1"}}))
    );

    // A pane found by searching is opened as a pane.
    let (mut first, _second, view, cx) = window(cx);
    ready(&view, cx);
    sent(&mut first);
    run(&view, Command::WorkspaceList, cx);
    cx.simulate_input("helper");
    ready(&view, cx);
    cx.simulate_keystrokes("enter");
    assert_closed(&view, cx);
    let requests = sent(&mut first).requests;
    assert!(
        requests.contains(&json!({"method": "pane.focus", "params": {"pane_id": "w1:p2"}})),
        "{requests:?}"
    );
}

#[gpui::test]
fn enter_on_a_host_or_its_workspace_presents_that_host(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::WorkspaceList, cx);

    // Second's row: the host itself, with nothing to focus on it.
    cx.simulate_keystrokes("down enter");
    assert_closed(&view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(view.pending_navigation, None);
    });

    // Back from Second, to a workspace on Local.
    run(&view, Command::WorkspaceList, cx);
    assert_eq!(selected(&view, cx), "release");
    cx.simulate_keystrokes("up up enter");
    assert_closed(&view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Workspace("w1".into()))
        );
    });
}
