//! The Herdr fork's agent order (`agents/priority.rs`). The cases follow the
//! Herdr fork's `src/client/shell/tests/priority_order.rs`: daemons send each
//! agent's `priority_key`, and the client only compares keys and merges
//! hosts. The priority mark is the agent picker's, and its case is in
//! `picker/tests/navigator_agents.rs`.

use super::*;
use crate::{
    LiveState,
    preferences::AgentSort,
    sidebar::agents::{panel_agents, sorted_agents},
    state::ConnectionStatus,
};
use herdr_client::ClientEvent;
use std::sync::Arc;

/// A key shaped like the fork's daemon writes one. Only the tests know the
/// shape: `<tier>.<rank>.<age>`, tier 0 for a lifted waiting agent, rank
/// counting down from blocked, and the age as 16 hex digits.
fn key(tier: u8, rank: u8, age: u64) -> String {
    format!("{tier}.{rank}.{age:016x}")
}

const LIFTED: u8 = 0;
const NORMAL: u8 = 1;
const BLOCKED: u8 = 0;
const DONE: u8 = 1;
const WORKING: u8 = 2;
const IDLE: u8 = 3;

/// A connected host whose daemon lists `(pane, status, priority_key)`
/// agents, each a later change than the one before it, so upstream's order
/// alone would differ from the keys'.
fn listed(agents: &[(&str, AgentStatus, &str)]) -> LiveState {
    let mut snapshot = layout_tests::snapshot(1);
    let template = snapshot.agents[0].clone();
    snapshot.agents = (1..)
        .zip(agents)
        .map(
            |(state_change_seq, &(pane, agent_status, priority_key))| ClientShellAgent {
                pane_id: pane.into(),
                agent_status,
                state_change_seq,
                priority_key: priority_key.into(),
                ..template.clone()
            },
        )
        .collect();
    let mut live = LiveState::default();
    live.apply(ClientEvent::Snapshot(Arc::new(snapshot)));
    live
}

/// [`listed`], every agent idle.
fn host(agents: &[(&str, &str)]) -> LiveState {
    let agents: Vec<_> = agents
        .iter()
        .map(|&(pane, key)| (pane, AgentStatus::Idle, key))
        .collect();
    listed(&agents)
}

fn order<'a>(hosts: &[&'a LiveState], sort: AgentSort) -> Vec<(usize, &'a str)> {
    panel_agents(hosts.iter().copied().enumerate(), sort)
        .into_iter()
        .map(|(index, agent)| (index, agent.pane_id.as_str()))
        .collect()
}

fn panes(host: &LiveState, sort: AgentSort) -> Vec<&str> {
    order(&[host], sort)
        .into_iter()
        .map(|(_, pane)| pane)
        .collect()
}

#[test]
fn one_host_lists_agents_by_key() {
    let host = host(&[
        ("idle-old", &key(NORMAL, IDLE, u64::MAX - 1)),
        ("working", &key(NORMAL, WORKING, u64::MAX - 6)),
        ("done-new", &key(NORMAL, DONE, 5)),
        ("idle-new", &key(NORMAL, IDLE, u64::MAX - 7)),
        ("blocked", &key(NORMAL, BLOCKED, 4)),
        ("lifted-done", &key(LIFTED, DONE, 8)),
        ("done-old", &key(NORMAL, DONE, 2)),
        ("lifted-idle", &key(NORMAL, IDLE, u64::MAX - 3)),
    ]);
    // The daemon's order, read off the keys alone: lifted waiting agents
    // first; then blocked, done oldest first, working, and idle newest first.
    let expected = [
        "lifted-done",
        "blocked",
        "done-old",
        "done-new",
        "working",
        "idle-new",
        "lifted-idle",
        "idle-old",
    ];
    assert_eq!(panes(&host, AgentSort::Priority), expected);
    // Upstream's Devices layout lists a host's agents in the same order.
    let devices: Vec<_> = sorted_agents(host.snapshot.as_deref().unwrap(), AgentSort::Priority)
        .into_iter()
        .map(|agent| agent.pane_id.as_str())
        .collect();
    assert_eq!(devices, expected);
}

#[test]
fn keys_are_compared_as_bytes_without_being_read() {
    // None of these is a key the fork's daemon would send. The order is plain
    // byte order all the same: no field is parsed, no number is read. Only
    // the empty key is special: it is no key, and follows every key, even
    // when its agent wants attention.
    let host = listed(&[
        ("lower-z", AgentStatus::Blocked, "z"),
        ("ten", AgentStatus::Idle, "10"),
        ("nine", AgentStatus::Idle, "9"),
        ("upper-z", AgentStatus::Done, "Z"),
        ("empty", AgentStatus::Blocked, ""),
        ("non-ascii", AgentStatus::Idle, "\u{e9}"),
    ]);
    assert_eq!(
        panes(&host, AgentSort::Priority),
        ["ten", "nine", "upper-z", "lower-z", "non-ascii", "empty"]
    );
}

#[test]
fn equal_keys_on_one_host_keep_the_snapshot_order() {
    // No ordering event since the daemon started: every age is zero. The
    // statuses and changes that upstream ranks by do not break the tie.
    let (idle, done) = (key(NORMAL, IDLE, u64::MAX), key(NORMAL, DONE, 0));
    let host = listed(&[
        ("c", AgentStatus::Idle, &idle),
        ("a", AgentStatus::Blocked, &idle),
        ("first-done", AgentStatus::Done, &done),
        ("b", AgentStatus::Working, &idle),
        ("second-done", AgentStatus::Done, &done),
    ]);
    assert_eq!(
        panes(&host, AgentSort::Priority),
        ["first-done", "second-done", "c", "a", "b"]
    );
}

#[test]
fn hosts_merge_by_key_where_grouped_keeps_them_apart() {
    let local = host(&[
        ("local-done", &key(NORMAL, DONE, 20)),
        ("local-idle", &key(NORMAL, IDLE, u64::MAX - 5)),
    ]);
    let build = host(&[
        ("build-idle", &key(NORMAL, IDLE, u64::MAX - 9)),
        ("build-done", &key(NORMAL, DONE, 10)),
        ("build-lifted", &key(LIFTED, BLOCKED, 40)),
    ]);
    let lab = host(&[
        ("lab-blocked", &key(NORMAL, BLOCKED, 30)),
        ("lab-done", &key(NORMAL, DONE, 15)),
    ]);
    // One queue over all three hosts: the keys interleave them.
    assert_eq!(
        order(&[&local, &build, &lab], AgentSort::Priority),
        [
            (1, "build-lifted"),
            (2, "lab-blocked"),
            (1, "build-done"),
            (2, "lab-done"),
            (0, "local-done"),
            (1, "build-idle"),
            (0, "local-idle"),
        ]
    );
    // Grouped ignores the keys: each host in turn, as its daemon lists it.
    assert_eq!(
        order(&[&local, &build, &lab], AgentSort::Grouped),
        [
            (0, "local-done"),
            (0, "local-idle"),
            (1, "build-idle"),
            (1, "build-done"),
            (1, "build-lifted"),
            (2, "lab-blocked"),
            (2, "lab-done"),
        ]
    );
}

#[test]
fn equal_keys_across_hosts_keep_host_order_then_snapshot_order() {
    let (done, idle) = (key(NORMAL, DONE, 7), key(NORMAL, IDLE, u64::MAX));
    let tied = |first, second, third| host(&[(first, &done), (second, &idle), (third, &done)]);
    // Seen in another order than the hosts are listed in, so the most recent
    // change, which upstream ranks by, would put the build host's first.
    let lab = tied("lab-1", "lab-idle", "lab-2");
    let local = tied("local-1", "local-idle", "local-2");
    let build = tied("build-1", "build-idle", "build-2");
    assert_eq!(
        order(&[&local, &build, &lab], AgentSort::Priority),
        [
            (0, "local-1"),
            (0, "local-2"),
            (1, "build-1"),
            (1, "build-2"),
            (2, "lab-1"),
            (2, "lab-2"),
            (0, "local-idle"),
            (1, "build-idle"),
            (2, "lab-idle"),
        ]
    );
    // The sidebar's host order decides, whichever way the hosts are listed.
    assert_eq!(
        order(&[&lab, &local], AgentSort::Priority)[..2],
        [(0, "lab-1"), (0, "lab-2")]
    );
}

#[test]
fn a_host_that_sends_no_keys_follows_every_keyed_agent_in_upstreams_order() {
    let local = listed(&[
        (
            "local-idle",
            AgentStatus::Idle,
            &key(NORMAL, IDLE, u64::MAX),
        ),
        ("local-unkeyed", AgentStatus::Working, ""),
    ]);
    // A daemon without `priority_key`, as stock Herdr, decodes to empty keys.
    let stock = listed(&[
        ("stock-idle", AgentStatus::Idle, ""),
        ("stock-blocked", AgentStatus::Blocked, ""),
        ("stock-working", AgentStatus::Working, ""),
    ]);
    let build = listed(&[("build-done", AgentStatus::Done, &key(NORMAL, DONE, 3))]);
    // Keyless agents come after every keyed one, even an idle one, and rank
    // among themselves as upstream does: attention, then the most recent
    // change on any host. The terminal client keeps them in listing order.
    assert_eq!(
        order(&[&local, &stock, &build], AgentSort::Priority),
        [
            (2, "build-done"),
            (0, "local-idle"),
            (1, "stock-blocked"),
            (1, "stock-working"),
            (0, "local-unkeyed"),
            (1, "stock-idle"),
        ]
    );
    // One such host alone is in upstream's order too.
    assert_eq!(
        panes(&stock, AgentSort::Priority),
        ["stock-blocked", "stock-working", "stock-idle"]
    );
}

#[test]
fn a_disconnected_host_lists_last_whatever_its_keys_say() {
    let local = host(&[("local-idle", &key(NORMAL, IDLE, u64::MAX - 1))]);
    let mut build = host(&[
        ("build-lifted", &key(LIFTED, BLOCKED, 2)),
        ("build-idle", &key(NORMAL, IDLE, u64::MAX - 3)),
    ]);
    build.status = ConnectionStatus::Detached;
    let lab = host(&[("lab-done", &key(NORMAL, DONE, 4))]);
    // This client's own rule applies before any key: the last-known rows
    // follow every connected row, still ordered by key among themselves.
    assert_eq!(
        order(&[&local, &build, &lab], AgentSort::Priority),
        [
            (2, "lab-done"),
            (0, "local-idle"),
            (1, "build-lifted"),
            (1, "build-idle"),
        ]
    );
}

#[test]
fn a_plugin_view_keeps_every_host_in_its_own_order() {
    let mut viewed = host(&[
        ("idle", &key(NORMAL, IDLE, u64::MAX)),
        ("hidden", &key(LIFTED, DONE, 1)),
    ]);
    if let Some(snapshot) = viewed.snapshot.as_mut() {
        let snapshot = Arc::make_mut(snapshot);
        snapshot.agent_view_label = Some("review".into());
        snapshot.agent_order = vec!["idle".into()];
    }
    let other = host(&[
        ("idle", &key(NORMAL, IDLE, u64::MAX)),
        ("blocked", &key(NORMAL, BLOCKED, 0)),
    ]);
    // The view's host lists only what the view orders; the other host orders
    // its own agents by key, below it, as before hosts merged.
    assert_eq!(
        order(&[&viewed, &other], AgentSort::Priority),
        [(0, "idle"), (1, "blocked"), (1, "idle")]
    );
    assert_eq!(
        order(&[&other, &viewed], AgentSort::Priority),
        [(0, "blocked"), (0, "idle"), (1, "idle")]
    );
}
