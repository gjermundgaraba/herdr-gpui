//! The Agents panel's order. The cases follow the Herdr fork's
//! `src/client/shell/tests/priority_order.rs`: daemons send each agent's
//! `priority_key`, and the client only compares keys and merges hosts.

use super::*;
use crate::{
    LiveState, preferences::AgentSort, sidebar::agents::panel_agents, state::ConnectionStatus,
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

/// A connected host whose daemon lists `(pane, priority_key)` agents.
fn host(agents: &[(&str, &str)]) -> LiveState {
    let mut snapshot = layout_tests::snapshot(1);
    let template = snapshot.agents[0].clone();
    snapshot.agents = agents
        .iter()
        .map(|&(pane, priority_key)| ClientShellAgent {
            pane_id: pane.into(),
            priority_key: priority_key.into(),
            ..template.clone()
        })
        .collect();
    let mut live = LiveState::default();
    live.apply(ClientEvent::Snapshot(Arc::new(snapshot)));
    live
}

/// The panel's order of `hosts`, each online while its connection is up.
fn order<'a>(hosts: &[&'a LiveState], sort: AgentSort) -> Vec<(usize, &'a str)> {
    let hosts = hosts
        .iter()
        .enumerate()
        .map(|(index, live)| (index, live.status.is_connected(), *live));
    panel_agents(hosts, sort)
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
    assert_eq!(
        panes(&host, AgentSort::Priority),
        [
            "lifted-done",
            "blocked",
            "done-old",
            "done-new",
            "working",
            "idle-new",
            "lifted-idle",
            "idle-old",
        ]
    );
}

#[test]
fn keys_are_compared_as_bytes_without_being_read() {
    // None of these is a key the fork's daemon would send. The order is plain
    // byte order all the same: no field is parsed, no number is read. A stock
    // daemon sends no key at all, which decodes as the empty one and follows
    // every other.
    let host = host(&[
        ("lower-z", "z"),
        ("ten", "10"),
        ("nine", "9"),
        ("upper-z", "Z"),
        ("empty", ""),
        ("non-ascii", "\u{e9}"),
    ]);
    assert_eq!(
        panes(&host, AgentSort::Priority),
        ["ten", "nine", "upper-z", "lower-z", "non-ascii", "empty"]
    );
}

#[test]
fn equal_keys_on_one_host_keep_the_snapshot_order() {
    // No ordering event since the daemon started: every age is zero.
    let (idle, done) = (key(NORMAL, IDLE, u64::MAX), key(NORMAL, DONE, 0));
    let host = host(&[
        ("c", &idle),
        ("a", &idle),
        ("first-done", &done),
        ("b", &idle),
        ("second-done", &done),
    ]);
    assert_eq!(
        panes(&host, AgentSort::Priority),
        ["first-done", "second-done", "c", "a", "b"]
    );
    // Without any key, as against stock Herdr, the listing order stays.
    let keyless = self::host(&[("c", ""), ("a", ""), ("b", "")]);
    assert_eq!(panes(&keyless, AgentSort::Priority), ["c", "a", "b"]);
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
fn an_empty_key_follows_every_other_key() {
    // A daemon that computes no key sends an empty one; its agents come after
    // every keyed agent on any host, still in host and snapshot order, as in
    // the terminal client and the devices.
    let local = host(&[
        ("local-unkeyed", ""),
        ("local-idle", &key(NORMAL, IDLE, u64::MAX)),
        ("local-unkeyed-2", ""),
    ]);
    let stock = host(&[("stock-1", ""), ("stock-2", "")]);
    let build = host(&[("build-blocked", &key(NORMAL, BLOCKED, 3))]);
    assert_eq!(
        order(&[&local, &stock, &build], AgentSort::Priority),
        [
            (2, "build-blocked"),
            (0, "local-idle"),
            (0, "local-unkeyed"),
            (0, "local-unkeyed-2"),
            (1, "stock-1"),
            (1, "stock-2"),
        ]
    );
}

#[test]
fn equal_keys_across_hosts_keep_host_order_then_snapshot_order() {
    let (done, idle) = (key(NORMAL, DONE, 7), key(NORMAL, IDLE, u64::MAX));
    let tied = |first, second, third| host(&[(first, &done), (second, &idle), (third, &done)]);
    let local = tied("local-1", "local-idle", "local-2");
    let build = tied("build-1", "build-idle", "build-2");
    let lab = tied("lab-1", "lab-idle", "lab-2");
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
    // Grouped keeps the sidebar's host order instead.
    assert_eq!(
        order(&[&local, &build, &lab], AgentSort::Grouped),
        [
            (0, "local-idle"),
            (1, "build-lifted"),
            (1, "build-idle"),
            (2, "lab-done"),
        ]
    );
}

#[test]
fn a_host_without_a_snapshot_lists_nothing() {
    let waiting = LiveState::default();
    let connected = host(&[
        ("idle", &key(NORMAL, IDLE, u64::MAX)),
        ("blocked", &key(NORMAL, BLOCKED, 0)),
    ]);
    assert_eq!(
        order(&[&waiting, &connected], AgentSort::Grouped),
        [(1, "idle"), (1, "blocked")]
    );
    assert_eq!(
        order(&[&waiting, &connected], AgentSort::Priority),
        [(1, "blocked"), (1, "idle")]
    );
    assert!(order(&[&waiting], AgentSort::Priority).is_empty());
    assert!(order(&[], AgentSort::Priority).is_empty());
}

/// Shows `view` on `host`, ordering `order`.
fn view(host: &mut LiveState, order: &[&str]) {
    if let Some(snapshot) = host.snapshot.as_mut() {
        let snapshot = Arc::make_mut(snapshot);
        snapshot.agent_view_label = Some("review".into());
        snapshot.agent_order = order.iter().map(|&pane| pane.into()).collect();
    }
}

#[test]
fn a_plugin_view_order_replaces_the_keys_and_filters() {
    let mut host = host(&[
        ("p0", &key(NORMAL, IDLE, u64::MAX)),
        ("p1", &key(NORMAL, BLOCKED, 0)),
    ]);
    // Without a view the order is the keys', even when the daemon sent one.
    if let Some(snapshot) = host.snapshot.as_mut() {
        Arc::make_mut(snapshot).agent_order = vec!["p0".into()];
    }
    assert_eq!(panes(&host, AgentSort::Grouped), ["p0", "p1"]);
    assert_eq!(panes(&host, AgentSort::Priority), ["p1", "p0"]);

    view(&mut host, &["p0", "gone", "p1"]);
    for sort in [AgentSort::Grouped, AgentSort::Priority] {
        // A pane the snapshot no longer lists as an agent is skipped.
        assert_eq!(panes(&host, sort), ["p0", "p1"]);
    }
    view(&mut host, &["p1"]);
    assert_eq!(panes(&host, AgentSort::Priority), ["p1"]);
    view(&mut host, &[]);
    assert!(panes(&host, AgentSort::Grouped).is_empty());
}

#[test]
fn a_plugin_view_keeps_every_host_in_its_own_order() {
    let mut viewed = host(&[
        ("idle", &key(NORMAL, IDLE, u64::MAX)),
        ("hidden", &key(LIFTED, DONE, 1)),
    ]);
    view(&mut viewed, &["idle"]);
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
    assert_eq!(
        order(&[&viewed, &other], AgentSort::Grouped),
        [(0, "idle"), (1, "idle"), (1, "blocked")]
    );
}
