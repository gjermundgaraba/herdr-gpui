//! The agent picker in a window: what `agent_picker` opens, which keys are
//! the list's and which the search's, and where Enter leads. The keys are
//! the Herdr fork's Navigator's (`src/client/shell/overlay_input.rs`).

use super::keys::type_at_the_terminal;
use super::navigator_fixture::*;
use super::*;
use crate::{controls::Command, navigation::NavigationTarget, state::ConnectionStatus};
use herdr_client::protocol::AgentStatus;

const DEPLOYER: &str = "Second · release: deployer";
const REVIEWER: &str = "repo: shell";
const HELPER: &str = "repo: helper";

#[gpui::test]
fn it_lists_every_hosts_agents_by_priority_with_the_top_row_selected(
    cx: &mut gpui::TestAppContext,
) {
    let (_first, _second, view, cx) = window(cx);

    run(&view, Command::AgentPicker, cx);

    assert_eq!(
        state(&view, cx),
        Some(Shown {
            title: "Agents".into(),
            // Priority order across hosts, not each host's listing order.
            rows: vec![DEPLOYER.into(), REVIEWER.into(), HELPER.into()],
            // The highest-priority agent, not the focused one.
            selected: Some(0),
            // The keyboard is on the rows, so `j` and `k` move at once.
            searching: false,
            query: String::new(),
        })
    );
    view.read_with(cx, |view, _| {
        let picker = shown(view).unwrap();
        let item = |index: usize| picker.list.item(index).unwrap();
        // The priority mark stands before the label.
        assert_eq!(item(0).badge.as_ref(), "★");
        assert_eq!(item(1).badge.as_ref(), "");
        assert_eq!(
            [item(0).status, item(1).status, item(2).status],
            [
                Some(AgentStatus::Working),
                Some(AgentStatus::Blocked),
                Some(AgentStatus::Idle)
            ]
        );
        assert_eq!(item(2).detail.as_ref(), "/repo/api");
        assert!(!item(0).dimmed);
    });
}

#[gpui::test]
fn the_panels_own_sort_and_the_device_filter_do_not_change_the_list(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    view.update(cx, |view, _| {
        view.agent_sort = crate::preferences::AgentSort::Grouped;
        view.device_filter = Some(crate::endpoint::LOCAL.into());
    });

    run(&view, Command::AgentPicker, cx);

    assert_eq!(listed(&view, cx), [DEPLOYER, REVIEWER, HELPER]);
}

#[gpui::test]
fn the_list_keys_move_and_none_of_them_reaches_the_pane(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window(cx);
    type_at_the_terminal(&view, cx);
    sent(&mut first);
    run(&view, Command::AgentPicker, cx);

    // Through the window's real focus and key dispatch.
    for (keystrokes, expected) in [
        ("j", REVIEWER),
        ("j", HELPER),
        ("j", HELPER),
        ("k", REVIEWER),
        ("up", DEPLOYER),
        ("up", DEPLOYER),
        ("down", REVIEWER),
        ("shift-g", HELPER),
        ("home", DEPLOYER),
        ("end", HELPER),
        ("ctrl-u", DEPLOYER),
        ("ctrl-d", HELPER),
        ("ctrl-p", REVIEWER),
        ("ctrl-n", HELPER),
        // Keys the rows have no use for change nothing.
        ("x", HELPER),
        ("tab", HELPER),
        ("cmd-j", HELPER),
    ] {
        cx.simulate_keystrokes(keystrokes);
        assert_eq!(selected(&view, cx), expected, "{keystrokes}");
    }
    let state = state(&view, cx).unwrap();
    assert_eq!(state.query, "", "nothing was typed into the search");
    assert!(!state.searching);
    // Even handed straight to the terminal's handlers, nothing is typed.
    type_at_the_terminal(&view, cx);
    assert_eq!(sent(&mut first), Sent::nothing());
    assert_eq!(sent(&mut second), Sent::nothing());
}

#[gpui::test]
fn slash_searches_and_escape_leaves_the_search_before_it_closes(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);

    cx.simulate_keystrokes("/");
    assert!(state(&view, cx).unwrap().searching);
    // In the search the list letters are text.
    cx.simulate_input("j");
    assert_eq!(state(&view, cx).unwrap().query, "j");
    assert!(listed(&view, cx).is_empty());
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("repo");
    assert_eq!(listed(&view, cx), [REVIEWER, HELPER]);
    // A changed search selects its first row; the arrows still move.
    assert_eq!(selected(&view, cx), REVIEWER);
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&view, cx), HELPER);
    cx.simulate_keystrokes("ctrl-p");
    assert_eq!(selected(&view, cx), REVIEWER);

    // Escape leaves the search and keeps what it found.
    cx.simulate_keystrokes("escape");
    let left = state(&view, cx).expect("still open");
    assert!(!left.searching);
    assert_eq!(left.query, "repo");
    assert_eq!(left.rows, [REVIEWER, HELPER]);
    cx.simulate_keystrokes("j");
    assert_eq!(selected(&view, cx), HELPER);

    cx.simulate_keystrokes("escape");
    assert_closed(&view, cx);
    assert!(cx.update(|window, cx| view.read(cx).focus.is_focused(window)));
    assert_eq!(sent(&mut first), Sent::nothing());
}

#[gpui::test]
fn the_status_letters_narrow_the_list_and_a_search_starts_over(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);

    for (keystroke, expected) in [
        ("b", vec![REVIEWER]),
        ("w", vec![DEPLOYER]),
        ("i", vec![HELPER]),
        ("d", vec![]),
        ("a", vec![DEPLOYER, REVIEWER, HELPER]),
        ("i", vec![HELPER]),
        ("backspace", vec![DEPLOYER, REVIEWER, HELPER]),
    ] {
        press(&view, keystroke, cx);
        assert_eq!(listed(&view, cx), expected, "{keystroke}");
    }

    // A status letter drops the search, and a search drops the status.
    cx.simulate_keystrokes("/");
    cx.simulate_input("repo");
    cx.simulate_keystrokes("escape");
    press(&view, "w", cx);
    let filtered = state(&view, cx).unwrap();
    assert_eq!(filtered.query, "");
    assert_eq!(filtered.rows, [DEPLOYER]);
    cx.simulate_keystrokes("/");
    assert_eq!(
        listed(&view, cx),
        [DEPLOYER, REVIEWER, HELPER],
        "searching starts from every status"
    );
}

#[gpui::test]
fn enter_opens_the_agents_pane_on_the_presented_host(cx: &mut gpui::TestAppContext) {
    let (mut first, mut second, view, cx) = window(cx);
    ready(&view, cx);
    sent(&mut first);
    run(&view, Command::AgentPicker, cx);
    ready(&view, cx);

    cx.simulate_keystrokes("j j enter");

    assert_closed(&view, cx);
    let requests = sent(&mut first).requests;
    assert_eq!(
        requests.first(),
        Some(&json!({"method": "pane.focus", "params": {"pane_id": "w1:p2"}})),
        "{requests:?}"
    );
    assert_eq!(sent(&mut second), Sent::nothing());
    view.read_with(cx, |view, _| assert_eq!(view.selected_endpoint, 0));
}

#[gpui::test]
fn enter_on_another_hosts_agent_presents_that_host_with_the_pane_as_target(
    cx: &mut gpui::TestAppContext,
) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);

    cx.simulate_keystrokes("enter");

    assert_closed(&view, cx);
    // The window's own cross-host navigation, as a click on the row starts.
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Pane("w1:p1".into()))
        );
    });
}

#[gpui::test]
fn a_host_that_is_away_is_dimmed_and_not_opened(cx: &mut gpui::TestAppContext) {
    let (_first, mut second, view, cx) = window(cx);
    view.update(cx, |view, _| {
        let live = &mut view.endpoints[1].live;
        live.status = ConnectionStatus::Disconnected;
        live.error = Some("connection lost".into());
    });
    run(&view, Command::AgentPicker, cx);
    // Its last-known agent follows the connected hosts', as in the panel.
    assert_eq!(listed(&view, cx), [REVIEWER, HELPER, DEPLOYER]);
    view.read_with(cx, |view, _| {
        let picker = shown(view).unwrap();
        assert!(picker.list.item(2).unwrap().dimmed);
        assert!(!picker.list.item(0).unwrap().dimmed);
    });

    cx.simulate_keystrokes("shift-g enter");

    // The navigator stays for another choice and says why.
    assert_eq!(selected(&view, cx), DEPLOYER);
    assert_eq!(notice(&view, cx).as_deref(), Some("Second is not ready"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(view.pending_navigation, None);
    });
    assert_eq!(sent(&mut second), Sent::nothing());
    // The next key takes the notice away.
    cx.simulate_keystrokes("k");
    assert_eq!(notice(&view, cx), None);
}

#[gpui::test]
fn the_rows_follow_the_hosts_while_the_picker_is_open(cx: &mut gpui::TestAppContext) {
    let (_first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);
    cx.simulate_keystrokes("j");
    assert_eq!(selected(&view, cx), REVIEWER);

    // Second's agent goes away and Local's helper is renamed.
    view.update(cx, |view, _| {
        let mut remote = (**view.endpoints[1].live.snapshot.as_ref().unwrap()).clone();
        remote.revision += 1;
        remote.agents.clear();
        view.endpoints[1].live.snapshot = Some(Arc::new(remote));
        let mut local = (**view.live.snapshot.as_ref().unwrap()).clone();
        local.revision += 1;
        local.agents[0].name = Some("renamed".into());
        view.live.snapshot = Some(Arc::new(local));
    });
    poll(&view, cx);

    assert_eq!(listed(&view, cx), [REVIEWER, "repo: renamed"]);
    assert_eq!(
        selected(&view, cx),
        REVIEWER,
        "the selection stays on its row"
    );
}

#[gpui::test]
fn clicking_a_row_opens_it(cx: &mut gpui::TestAppContext) {
    let (mut first, _second, view, cx) = window(cx);
    ready(&view, cx);
    sent(&mut first);
    run(&view, Command::AgentPicker, cx);
    ready(&view, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.choose_picker_row(1, window, cx));
    });

    assert_closed(&view, cx);
    assert_eq!(
        sent(&mut first).requests.first(),
        Some(&json!({"method": "pane.focus", "params": {"pane_id": "w1:p1"}}))
    );
}

#[gpui::test]
fn an_open_navigator_refuses_a_plugins_picker_and_another_menu_closes_it(
    cx: &mut gpui::TestAppContext,
) {
    let (mut first, _second, view, cx) = window(cx);
    run(&view, Command::AgentPicker, cx);

    // A plugin must not take over the list the user is in.
    deliver(&view, 0, open("pick-1", false), cx);
    assert_eq!(state(&view, cx).unwrap().title, "Agents");
    assert_eq!(
        sent(&mut first),
        Sent::resolve("pick-1", json!({"type": "cancelled"}))
    );

    // Nobody waits on a navigator, so another page simply replaces it.
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_menu(window, cx)));
    view.read_with(cx, |view, _| assert!(view.picker.is_none()));
    assert_eq!(sent(&mut first), Sent::nothing());
}
