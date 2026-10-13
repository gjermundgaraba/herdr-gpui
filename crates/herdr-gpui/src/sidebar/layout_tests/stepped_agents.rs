//! The agents the next, previous, and numbered agent keys step through. The
//! panel keeps showing what a host last sent while it reconnects, but a key
//! must not land there: the Herdr fork steps through `online_agent_targets`
//! (`src/client/shell/aggregate_navigation.rs`), and so does this client.

use super::*;
use crate::{controls::Command, state::ConnectionStatus};

const REMOTE: &str = "ssh:test";

/// Local and one remote host, each connected and listing one agent.
fn fleet(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    let mut view = fixture_window(window, cx);
    let mut listing = snapshot(1);
    listing.agents.truncate(1);
    listing.focused_pane_id = Some(listing.agents[0].pane_id.clone());
    view.live.snapshot = Some(Arc::new(listing.clone()));
    view.live.status = ConnectionStatus::Connected;
    view.endpoints[0].live.snapshot = view.live.snapshot.clone();
    view.endpoints[0].live.status = ConnectionStatus::Connected;
    // Leaving a connected host for another needs its surface support.
    view.endpoints[0].live.supports_surface = true;
    let mut remote = crate::endpoint::Endpoint::new(
        REMOTE.into(),
        "Remote".into(),
        ConnectTarget::Ssh {
            target: "unused".into(),
            session: "default".into(),
        },
        true,
    );
    remote.live.snapshot = Some(Arc::new(listing));
    remote.live.status = ConnectionStatus::Connected;
    remote.live.supports_surface = true;
    view.endpoints.push(remote);
    view
}

/// The link dropped: the host keeps its last snapshot and says why.
fn drop_link(view: &mut HerdrWindow) {
    let remote = &mut view.endpoints[1];
    remote.live.status = ConnectionStatus::Disconnected;
    remote.live.error = Some("connection lost".into());
    assert_eq!(remote.status(), "reconnecting");
}

fn hosts(rows: &[(usize, &str)]) -> Vec<usize> {
    rows.iter().map(|&(host, _)| host).collect()
}

#[gpui::test]
fn a_reconnecting_hosts_agents_are_shown_but_not_stepped_to(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fleet);
    view.update(cx, |view, _| {
        assert_eq!(hosts(&view.sidebar_agents()), [0, 1]);
        assert_eq!(hosts(&view.stepped_agents()), [0, 1]);

        drop_link(view);
        // The panel still lists the agent the host last sent.
        assert_eq!(hosts(&view.sidebar_agents()), [0, 1]);
        let rows = view.stepped_agents();
        assert_eq!(hosts(&rows), [0]);
        let step = |command| crate::sidebar::sidebar_step(&rows, Some(0), 0, command);
        // Next and previous wrap within what is online, and a number counts
        // only those rows.
        assert_eq!(step(Command::NextAgent), Some(0));
        assert_eq!(step(Command::PreviousAgent), Some(0));
        assert_eq!(step(Command::AgentNumber(1)), Some(0));
        assert_eq!(step(Command::AgentNumber(2)), None);
    });
}

#[gpui::test]
fn every_other_state_than_online_is_skipped(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fleet);
    view.update(cx, |view, _| {
        // Still dialing, with the snapshot of an earlier connection.
        view.endpoints[1].live.status = ConnectionStatus::Connecting;
        assert_eq!(hosts(&view.stepped_agents()), [0]);
        view.endpoints[1].live.status = ConnectionStatus::Connected;
        assert_eq!(hosts(&view.stepped_agents()), [0, 1]);
        // Local is a host like any other.
        view.endpoints[0].live.status = ConnectionStatus::Disconnected;
        view.endpoints[0].live.error = Some("daemon stopped".into());
        assert_eq!(hosts(&view.stepped_agents()), [1]);
    });
}

#[gpui::test]
fn the_next_agent_key_stays_off_a_reconnecting_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fleet);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            drop_link(view);
            for command in [
                Command::NextAgent,
                Command::PreviousAgent,
                Command::AgentNumber(2),
            ] {
                view.command(command, window, cx);
                assert_eq!(view.selected_endpoint, 0, "{command:?}");
                assert_eq!(view.pending_navigation, None, "{command:?}");
            }

            // Back online, the same key moves to it.
            let remote = &mut view.endpoints[1];
            remote.live.status = ConnectionStatus::Connected;
            remote.live.error = None;
            view.command(Command::NextAgent, window, cx);
            assert_eq!(view.selected_endpoint, 1);
        })
    });
}
