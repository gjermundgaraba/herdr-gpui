//! `client.report` as the window builds it: the Agents list it shows, read
//! from its retained snapshots. The cases are the Herdr fork's, from
//! `src/client/shell/tests/client_report.rs`. Its
//! `rows_carry_each_daemons_agent_record_unchanged` is
//! `rows_carry_the_snapshots_agent_verbatim` here: both clients report the
//! daemon's status as sent.

use super::*;

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(endpoint, pane)| ((*endpoint).to_owned(), (*pane).to_owned()))
        .collect()
}

#[gpui::test]
fn the_report_lists_every_machine_and_the_agents_as_displayed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);
    let remote = host_id(1);

    let report = view.read_with(cx, |view, _| report_of(view));

    assert_eq!(
        report.focused,
        Some(false),
        "the fixture window is inactive"
    );
    assert_eq!(report.active_endpoint_id.as_deref(), Some("local"));
    assert_eq!(
        report.endpoints,
        [
            ClientReportEndpoint {
                endpoint_id: "local".into(),
                label: "Local".into(),
                status: ClientEndpointStatus::Online,
                boot_id: Some("boot-1".into()),
            },
            ClientReportEndpoint {
                endpoint_id: remote.clone(),
                label: "Build".into(),
                status: ClientEndpointStatus::Online,
                boot_id: Some("build-boot".into()),
            },
        ]
    );
    assert_eq!(
        reported(&report),
        pairs(&[
            (&remote, "build-lifted"),
            ("local", "local-blocked"),
            ("local", "local-idle"),
        ])
    );

    let lifted = &report.agents[0];
    assert_eq!(lifted.workspace_label, "release");
    assert_eq!(lifted.workspace_number, 3);
    assert_eq!(lifted.agent.agent_status, AgentStatus::Blocked);
    assert_eq!(lifted.agent.priority_mark.as_deref(), Some("★"));
    assert_eq!(
        lifted.agent.priority_key,
        priority_key(true, AgentStatus::Blocked, 40)
    );
    let blocked = &report.agents[1];
    assert_eq!(blocked.cwd.as_deref(), Some("/repo/api"));
    assert_eq!(blocked.foreground_cwd.as_deref(), Some("/repo/api/src"));
    assert_eq!(blocked.workspace_label, "repo");
}

#[gpui::test]
fn rows_carry_the_snapshots_agent_verbatim(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);

    view.read_with(cx, |view, _| {
        let report = report_of(view);
        for row in &report.agents {
            let index = (0..view.endpoints.len())
                .find(|index| view.endpoints[*index].id == row.endpoint_id)
                .unwrap();
            let snapshot = retained(view, index);
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == row.agent.pane_id)
                .unwrap();
            assert_eq!(&row.agent, agent);
        }
    });
}

#[gpui::test]
fn rows_follow_the_displayed_order_under_either_sort(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);
    let ids = |view: &HerdrWindow| {
        let panel: Vec<_> = view
            .sidebar_agents()
            .into_iter()
            .map(|(_, pane)| pane.to_owned())
            .collect();
        let rows: Vec<_> = reported(&report_of(view))
            .into_iter()
            .map(|(_, pane)| pane)
            .collect();
        assert_eq!(rows, panel, "the report lists what the panel paints");
        rows
    };

    view.update(cx, |view, _| {
        assert_eq!(ids(view), ["build-lifted", "local-blocked", "local-idle"]);
        view.agent_sort = AgentSort::Grouped;
        assert_eq!(ids(view), ["local-idle", "local-blocked", "build-lifted"]);
    });
}

#[gpui::test]
fn the_device_filter_narrows_the_panel_and_not_the_report(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);
    let panel = |view: &HerdrWindow| {
        view.sidebar_agents()
            .into_iter()
            .map(|(_, pane)| pane.to_owned())
            .collect::<Vec<_>>()
    };
    let rows = |view: &HerdrWindow| {
        reported(&report_of(view))
            .into_iter()
            .map(|(_, pane)| pane)
            .collect::<Vec<_>>()
    };

    view.update(cx, |view, _| {
        let unfiltered = (view.client_report_fingerprint(), report_of(view));

        view.device_filter = Some("local".into());
        assert_eq!(panel(view), ["local-blocked", "local-idle"]);
        // Every enabled host's agents, in the order the panel has unfiltered.
        assert_eq!(rows(view), ["build-lifted", "local-blocked", "local-idle"]);
        assert_eq!(report_of(view), unfiltered.1);
        assert_eq!(
            view.client_report_fingerprint(),
            unfiltered.0,
            "a view of the window is nothing to report"
        );

        view.device_filter = Some(host_id(1));
        assert_eq!(panel(view), ["build-lifted"]);
        assert_eq!(report_of(view), unfiltered.1);

        view.agent_sort = AgentSort::Grouped;
        assert_eq!(rows(view), ["local-idle", "local-blocked", "build-lifted"]);
    });
}

#[gpui::test]
fn only_the_presented_machines_focused_pane_is_the_focused_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);
    let focused = |view: &HerdrWindow| {
        report_of(view)
            .agents
            .iter()
            .filter(|row| row.focused)
            .map(|row| (row.endpoint_id.clone(), row.agent.pane_id.clone()))
            .collect::<Vec<_>>()
    };

    view.update(cx, |view, _| {
        // Build also has a focused pane of its own, but it is not presented.
        assert_eq!(focused(view), pairs(&[("local", "local-idle")]));

        present(view, 1);
        assert_eq!(focused(view), pairs(&[(&host_id(1), "build-lifted")]));
        assert_eq!(report_of(view).active_endpoint_id, Some(host_id(1)));
    });
}

#[gpui::test]
fn window_focus_is_reported(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);

    view.update(cx, |view, _| {
        view.active = false;
        assert_eq!(report_of(view).focused, Some(false));
        // The focused row says where the client is, focused window or not.
        assert_eq!(
            report_of(view)
                .agents
                .iter()
                .filter(|row| row.focused)
                .count(),
            1
        );

        view.active = true;
        assert_eq!(report_of(view).focused, Some(true));
    });
}

#[gpui::test]
fn a_machine_without_a_snapshot_is_listed_without_boot_or_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, |view| {
        fleet(view, &["Build", "Lab"]);
        accept(
            view,
            0,
            machine("boot-1", vec![agent("local-idle", AgentStatus::Idle, 1)]),
        );
        accept(
            view,
            2,
            machine("lab-boot", vec![agent("lab-done", AgentStatus::Done, 2)]),
        );
        view.endpoints[1].live.status = ConnectionStatus::Connecting;
        reconnecting(view, 2);
    });

    let report = view.read_with(cx, |view, _| report_of(view));

    assert_eq!(
        report
            .endpoints
            .iter()
            .map(|endpoint| (
                endpoint.label.as_str(),
                endpoint.status,
                endpoint.boot_id.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            ("Local", ClientEndpointStatus::Online, Some("boot-1")),
            ("Build", ClientEndpointStatus::Connecting, None),
            ("Lab", ClientEndpointStatus::Reconnecting, Some("lab-boot")),
        ]
    );
    // The reconnecting machine's retained rows are still shown, after the
    // online ones.
    assert_eq!(
        reported(&report),
        pairs(&[("local", "local-idle"), (&host_id(2), "lab-done")])
    );
}

#[gpui::test]
fn every_way_a_host_can_be_down_has_a_reported_status(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, |view| {
        fleet(view, &["Off", "Outdated", "Locked"]);
        accept(view, 0, machine("boot-1", Vec::new()));
        view.endpoints[1].enabled = false;
        view.endpoints[2].version_mismatch = Some(herdr_client::VersionMismatch::DaemonOutdated {
            server_version: Some("0.1.0".into()),
        });
        let locked = &mut view.endpoints[3].live;
        locked.status = ConnectionStatus::Disconnected;
        locked.error = Some("permission denied".into());
        locked.ssh_failure = Some(herdr_client::SshFailure::Auth);
    });

    let statuses = view.read_with(cx, |view, _| {
        report_of(view)
            .endpoints
            .into_iter()
            .map(|endpoint| (endpoint.label, endpoint.status))
            .collect::<Vec<_>>()
    });

    assert_eq!(
        statuses,
        [
            ("Local".to_owned(), ClientEndpointStatus::Online),
            ("Off".to_owned(), ClientEndpointStatus::Disabled),
            ("Outdated".to_owned(), ClientEndpointStatus::Attention),
            ("Locked".to_owned(), ClientEndpointStatus::Attention),
        ]
    );
}

#[gpui::test]
fn the_report_stays_within_the_daemons_limits(cx: &mut gpui::TestAppContext) {
    let labels = (0..MAX_CLIENT_REPORT_ENDPOINTS + 3)
        .map(|index| format!("machine-{index}"))
        .collect::<Vec<_>>();
    let (view, cx) = window(cx, |view| {
        let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
        fleet(view, &labels);
        let mut local = machine(
            "boot-1",
            (0..MAX_CLIENT_REPORT_AGENTS + 10)
                .map(|index| agent(&format!("pane_{index:04}"), AgentStatus::Done, index as u64))
                .collect(),
        );
        local.workspaces[0].label = "é".repeat(MAX_CLIENT_REPORT_TEXT_BYTES);
        // An id the daemon would reject the whole report for.
        local.agents[1].pane_id = "p".repeat(MAX_CLIENT_REPORT_ID_BYTES + 1);
        accept(view, 0, local);
        // The last machine is beyond the endpoint limit, so its rows go too.
        let beyond = view.endpoints.len() - 1;
        accept(
            view,
            beyond,
            machine(
                "far-boot",
                vec![lifted_agent("far-lifted", AgentStatus::Blocked, 1)],
            ),
        );
    });

    let (report, beyond) = view.read_with(cx, |view, _| {
        (report_of(view), view.endpoints.last().unwrap().id.clone())
    });

    assert_eq!(report.endpoints.len(), MAX_CLIENT_REPORT_ENDPOINTS);
    assert!(
        report
            .endpoints
            .iter()
            .all(|endpoint| endpoint.endpoint_id != beyond)
    );
    assert_eq!(report.agents.len(), MAX_CLIENT_REPORT_AGENTS);
    assert!(report.agents.iter().all(|row| row.endpoint_id == "local"));
    // The first rows of the list are the ones kept, less the rejected id.
    assert_eq!(report.agents[0].agent.pane_id, "pane_0000");
    assert_eq!(report.agents[1].agent.pane_id, "pane_0002");
    assert_eq!(
        report.agents[0].workspace_label.len(),
        MAX_CLIENT_REPORT_TEXT_BYTES
    );
    let mut again = report.clone();
    assert_eq!(again.normalize(), Ok(()));
    assert_eq!(again, report);
}

#[gpui::test]
fn presenting_a_machine_beyond_the_limit_reports_no_active_endpoint(cx: &mut gpui::TestAppContext) {
    let labels = (0..MAX_CLIENT_REPORT_ENDPOINTS)
        .map(|index| format!("machine-{index}"))
        .collect::<Vec<_>>();
    let (view, cx) = window(cx, |view| {
        let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
        fleet(view, &labels);
        accept(view, 0, machine("boot-1", Vec::new()));
        let beyond = view.endpoints.len() - 1;
        accept(
            view,
            beyond,
            machine("far-boot", vec![agent("far", AgentStatus::Idle, 1)]),
        );
        present(view, beyond);
    });

    let report = view.read_with(cx, |view, _| report_of(view));

    assert_eq!(report.active_endpoint_id, None);
    assert!(report.agents.is_empty());
}

#[gpui::test]
fn an_agent_whose_space_is_gone_has_no_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, |view| {
        fleet(view, &[]);
        let mut local = machine(
            "boot-1",
            vec![
                agent("kept", AgentStatus::Done, 1),
                agent("orphan", AgentStatus::Done, 2),
            ],
        );
        local.agents[1].workspace_id = "closed".into();
        accept(view, 0, local);
    });

    let report = view.read_with(cx, |view, _| report_of(view));

    assert_eq!(reported(&report), pairs(&[("local", "kept")]));
}

#[gpui::test]
fn the_fingerprint_follows_everything_the_report_reads(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);

    view.update(cx, |view, _| {
        let mut seen = vec![view.client_report_fingerprint()];
        let mut changed = |view: &HerdrWindow, what: &str| {
            let fingerprint = view.client_report_fingerprint();
            assert!(!seen.contains(&fingerprint), "{what} left it unchanged");
            seen.push(fingerprint);
        };

        view.active = true;
        changed(view, "window focus");

        view.agent_sort = AgentSort::Grouped;
        changed(view, "the sort");

        reconnecting(view, 1);
        changed(view, "a machine's status");

        update(view, 0, |snapshot| {
            snapshot.agents[0].title = Some("renamed".into());
        });
        changed(view, "a snapshot");

        view.endpoints[1].label = "Builder".into();
        changed(view, "a machine's label");

        present(view, 1);
        changed(view, "the presented machine");
    });
}

#[gpui::test]
fn the_fingerprint_is_stable_while_nothing_reported_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, two_machines);
    let before = view.read_with(cx, |view, _| {
        (view.client_report_fingerprint(), report_of(view))
    });

    // Drawing, scrolling, and an error line report nothing new.
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    view.update(cx, |view, cx| {
        view.local_error = Some("Input not sent".into());
        view.sidebar_visible = false;
        cx.notify();
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });

    view.read_with(cx, |view, _| {
        assert_eq!(view.client_report_fingerprint(), before.0);
        assert_eq!(report_of(view), before.1);
    });
}
