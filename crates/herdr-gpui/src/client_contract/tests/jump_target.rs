//! Where a jump leads, or why it cannot be performed: the checks of the
//! Herdr fork's `src/client/shell/client_jump.rs`, in its order, with the
//! cases of its `tests/client_jump.rs` that need no window.

use super::*;
use crate::client_contract::{
    jump::{NAVIGATION_SETTLE_DEADLINE, jump_target},
    report::Host,
};
use std::time::Duration;

fn jump(endpoint_id: &str, boot_id: &str, pane_id: &str) -> EndpointClientJump {
    EndpointClientJump {
        jump_id: "jump-1".into(),
        endpoint_id: endpoint_id.into(),
        boot_id: boot_id.into(),
        pane_id: pane_id.into(),
    }
}

fn host<'a>(
    id: &'a str,
    status: ClientEndpointStatus,
    snapshot: Option<&'a ClientShellSnapshot>,
) -> Host<'a> {
    Host {
        id,
        label: id,
        status,
        snapshot,
    }
}

/// Local with `pane_1` and `pane_2`, and Build with `pane_7` and a `pane_1`
/// of its own.
fn machines() -> (ClientShellSnapshot, ClientShellSnapshot) {
    (
        machine(
            "boot-1",
            vec![
                agent("pane_1", AgentStatus::Idle, 1),
                agent("pane_2", AgentStatus::Done, 2),
            ],
        ),
        machine(
            "build-boot",
            vec![
                agent("pane_7", AgentStatus::Blocked, 3),
                agent("pane_1", AgentStatus::Idle, 1),
            ],
        ),
    )
}

const BUILD: &str = "ssh:00000000000000000000000000000001";

#[test]
fn a_pane_on_an_online_host_leads_to_that_host() {
    let (local, build) = machines();
    let hosts = [
        host("local", ClientEndpointStatus::Online, Some(&local)),
        host(BUILD, ClientEndpointStatus::Online, Some(&build)),
    ];
    assert_eq!(
        jump_target(&hosts, &jump("local", "boot-1", "pane_2")),
        Ok(0)
    );
    // Both machines have a `pane_1`; the endpoint id says whose is meant.
    assert_eq!(
        jump_target(&hosts, &jump(BUILD, "build-boot", "pane_1")),
        Ok(1)
    );
}

#[test]
fn a_pane_id_read_under_another_boot_is_stale() {
    let (local, build) = machines();
    let mut hosts = [
        host("local", ClientEndpointStatus::Online, Some(&local)),
        host(BUILD, ClientEndpointStatus::Online, Some(&build)),
    ];
    assert_eq!(
        jump_target(&hosts, &jump("local", "boot-0", "pane_2")),
        Err(ClientJumpOutcome::StaleBoot)
    );
    assert_eq!(
        jump_target(&hosts, &jump(BUILD, "boot-1", "pane_7")),
        Err(ClientJumpOutcome::StaleBoot),
        "Local's boot does not vouch for Build's panes"
    );
    // A restarted machine is stale before it is anything else.
    hosts[1].status = ClientEndpointStatus::Reconnecting;
    assert_eq!(
        jump_target(&hosts, &jump(BUILD, "old-boot", "pane_7")),
        Err(ClientJumpOutcome::StaleBoot)
    );
}

#[test]
fn an_unknown_pane_or_endpoint_is_not_found() {
    let (local, build) = machines();
    let hosts = [
        host("local", ClientEndpointStatus::Online, Some(&local)),
        host(BUILD, ClientEndpointStatus::Online, Some(&build)),
    ];
    assert_eq!(
        jump_target(&hosts, &jump("local", "boot-1", "pane_7")),
        Err(ClientJumpOutcome::NotFound),
        "pane_7 is Build's"
    );
    assert_eq!(
        jump_target(&hosts, &jump(BUILD, "build-boot", "pane_2")),
        Err(ClientJumpOutcome::NotFound)
    );
    assert_eq!(
        jump_target(
            &hosts,
            &jump("ssh:ffffffffffffffffffffffffffffffff", "boot-1", "pane_1")
        ),
        Err(ClientJumpOutcome::NotFound),
        "no such machine in this client"
    );
    assert_eq!(
        jump_target(&hosts, &jump("nostr:relay", "boot-1", "pane_1")),
        Err(ClientJumpOutcome::NotFound)
    );
}

#[test]
fn an_endpoint_that_is_not_online_is_unavailable() {
    let (local, build) = machines();
    for status in [
        ClientEndpointStatus::Connecting,
        ClientEndpointStatus::Reconnecting,
        ClientEndpointStatus::Attention,
        ClientEndpointStatus::Disabled,
    ] {
        let hosts = [
            host("local", ClientEndpointStatus::Online, Some(&local)),
            host(BUILD, status, Some(&build)),
        ];
        assert_eq!(
            jump_target(&hosts, &jump(BUILD, "build-boot", "pane_7")),
            Err(ClientJumpOutcome::Unavailable),
            "{status:?}"
        );
    }
    // Lab is online but has never sent a snapshot.
    let hosts = [
        host("local", ClientEndpointStatus::Online, Some(&local)),
        host("ssh:lab", ClientEndpointStatus::Online, None),
    ];
    assert_eq!(
        jump_target(&hosts, &jump("ssh:lab", "lab-boot", "pane_1")),
        Err(ClientJumpOutcome::Unavailable)
    );
}

#[test]
fn a_refused_focus_request_names_the_jumps_outcome() {
    for (refusal, expected) in [
        (Refusal::PaneNotFound, ClientJumpOutcome::NotFound),
        (Refusal::StaleBoot, ClientJumpOutcome::StaleBoot),
        (Refusal::Unsent, ClientJumpOutcome::Unavailable),
        (
            Refusal::Other {
                message: "the pane cannot take focus".into(),
            },
            ClientJumpOutcome::Failed {
                message: "the pane cannot take focus".into(),
            },
        ),
    ] {
        assert_eq!(ClientJumpOutcome::from(refusal), expected);
    }
}

#[test]
fn a_client_gives_up_before_the_daemon_does() {
    assert!(NAVIGATION_SETTLE_DEADLINE < Duration::from_secs(20));
}
