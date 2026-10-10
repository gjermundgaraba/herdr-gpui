//! One test of whether a host is online, as the Herdr fork's terminal client
//! has: a host whose connection is up but that the user cannot act on, such
//! as one whose daemon needs an update, is ordered, stepped, reported, and
//! jumped to as not online everywhere.

use super::*;
use crate::client_contract::jump::jump_target;

#[gpui::test]
fn a_connected_host_that_is_not_online_is_not_online_anywhere(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, |view| {
        two_machines(view);
        // Build's connection is up, its snapshot current, but its daemon is
        // refused until it is updated.
        assert!(view.endpoints[1].live.status.is_connected());
        view.endpoints[1].version_mismatch = Some(herdr_client::VersionMismatch::DaemonOutdated {
            server_version: Some("0.1.0".into()),
        });
    });

    view.read_with(cx, |view, _| {
        let build = host_id(1);
        // Its lifted blocked agent follows every online host's agents.
        assert_eq!(
            view.sidebar_agents(),
            [(0, "local-blocked"), (0, "local-idle"), (1, "build-lifted"),]
        );
        // The agent keys never land on it.
        assert_eq!(
            view.stepped_agents(),
            [(0, "local-blocked"), (0, "local-idle")]
        );
        // The report lists it as needing the user.
        let report = report_of(view);
        assert_eq!(report.endpoints[1].endpoint_id, build);
        assert_eq!(report.endpoints[1].status, ClientEndpointStatus::Attention);
        // And a jump to it is refused.
        let jump = EndpointClientJump {
            jump_id: "jump-1".into(),
            endpoint_id: build,
            boot_id: "build-boot".into(),
            pane_id: "build-lifted".into(),
        };
        assert_eq!(
            jump_target(&view.contract_hosts(), &jump),
            Err(ClientJumpOutcome::Unavailable)
        );
    });
}
