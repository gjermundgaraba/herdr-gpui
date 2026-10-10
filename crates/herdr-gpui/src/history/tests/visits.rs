//! The history itself, with no window: the cases of the Herdr fork's
//! `src/client/shell/history.rs`.

use super::*;
use crate::history::CAPACITY;

pub(super) const LOCAL: &str = "local";
pub(super) const REMOTE: &str = "ssh:0123456789abcdef0123456789abcdef";

pub(super) fn online(_: &str) -> bool {
    true
}

fn local_only(endpoint_id: &str) -> bool {
    endpoint_id == LOCAL
}

pub(super) fn observe_local(history: &mut History, pane_id: &str) {
    history.observe(LOCAL, "boot", Some(pane_id));
}

pub(super) fn panes(history: &History) -> Vec<&str> {
    history
        .visits
        .iter()
        .map(|visit| visit.pane_id.as_str())
        .collect()
}

#[test]
fn visits_coalesce_and_traverse() {
    let mut history = History::default();
    history.observe(LOCAL, "boot", Some("p1"));
    history.observe(LOCAL, "boot", Some("p1"));
    history.observe(LOCAL, "boot", None);
    history.observe(REMOTE, "rboot", Some("p9"));
    assert_eq!(history.visits.len(), 2);

    let back = history.back(online).unwrap();
    assert_eq!(back.visit.pane_id, "p1");
    assert!(history.forward(online).is_some());
    assert!(history.forward(online).is_none());
    let back = history.back(online).unwrap();
    // Observing the traversal target once its focus request is out confirms
    // it without a new visit.
    history.traversal_sent(&back);
    history.observe(LOCAL, "boot", Some("p1"));
    assert_eq!(history.visits.len(), 2);
    assert_eq!(history.cursor, 0);
    // A fresh visit truncates the forward list.
    history.observe(LOCAL, "boot", Some("p2"));
    assert_eq!(history.visits.len(), 2);
    assert!(history.forward(online).is_none());
}

#[test]
fn retirement_and_reboot_prune_entries_and_keep_the_cursor_sane() {
    let mut history = History::default();
    history.observe(REMOTE, "rboot", Some("p9"));
    history.observe(LOCAL, "boot", Some("p1"));
    history.observe(REMOTE, "rboot", Some("p8"));
    history.retain_endpoints(local_only);
    assert_eq!(history.visits.len(), 1);
    assert_eq!(history.cursor, 0);
    history.observe(LOCAL, "boot", Some("p2"));
    history.observe_boot(LOCAL, "boot-2");
    assert!(history.visits.is_empty());
    assert!(history.back(online).is_none());
}

#[test]
fn closed_panes_are_pruned_and_equal_neighbours_merge() {
    let mut history = History::default();
    history.observe(REMOTE, "rboot", Some("p9"));
    for pane_id in ["p1", "p2", "p1", "p3"] {
        observe_local(&mut history, pane_id);
    }
    history.observe_panes(LOCAL, "boot", ["p1", "p3"]);
    // The remote visit is untouched; p1 p2 p1 collapses into one p1.
    assert_eq!(panes(&history), ["p9", "p1", "p3"]);
    assert_eq!(history.cursor, 2);
    assert_eq!(history.back(online).unwrap().visit.pane_id, "p1");

    // Closing the current pane moves the cursor to the nearest earlier survivor.
    observe_local(&mut history, "p1");
    history.observe_panes(LOCAL, "boot", ["p3"]);
    assert_eq!(panes(&history), ["p9", "p3"]);
    assert_eq!(history.cursor, 0);
}

#[test]
fn failed_traversal_rewinds_and_forgets_a_closed_target() {
    let mut history = History::default();
    for pane_id in ["p1", "p2", "p3"] {
        observe_local(&mut history, pane_id);
    }
    let target = history.back(online).unwrap();
    assert_eq!(target.visit.pane_id, "p2");
    history.traversal_failed(&target, false);
    assert_eq!(panes(&history), ["p1", "p2", "p3"]);
    assert_eq!(history.cursor, 2);

    let target = history.back(online).unwrap();
    history.traversal_failed(&target, true);
    assert_eq!(panes(&history), ["p1", "p3"]);
    assert_eq!(history.cursor, 1);
    assert_eq!(history.back(online).unwrap().visit.pane_id, "p1");
}

#[test]
fn snapshots_on_the_traversed_path_do_not_truncate_forward_history() {
    let mut history = History::default();
    for pane_id in ["p1", "p2", "p3"] {
        observe_local(&mut history, pane_id);
    }
    // Two quick steps back, then snapshots that predate and trail the focus.
    history.back(online).unwrap();
    let target = history.back(online).unwrap();
    history.traversal_sent(&target);
    observe_local(&mut history, "p3");
    observe_local(&mut history, "p2");
    observe_local(&mut history, "p1");
    assert_eq!(panes(&history), ["p1", "p2", "p3"]);
    assert_eq!(history.cursor, 0);
    assert!(history.pending.is_none());
    assert_eq!(history.forward(online).unwrap().visit.pane_id, "p2");
}

#[test]
fn offline_endpoints_are_skipped_and_capacity_is_bounded() {
    let mut history = History::default();
    history.observe(REMOTE, "rboot", Some("p9"));
    history.observe(LOCAL, "boot", Some("p1"));
    history.observe(LOCAL, "boot", Some("p2"));
    assert_eq!(history.back(local_only).unwrap().visit.pane_id, "p1");
    assert!(history.back(local_only).is_none());

    let mut history = History::default();
    for index in 0..(CAPACITY + 10) {
        history.observe(LOCAL, "boot", Some(&format!("p{index}")));
    }
    assert_eq!(history.visits.len(), CAPACITY);
    assert_eq!(history.cursor, CAPACITY - 1);
}

#[test]
fn a_visit_names_its_host_boot_and_pane() {
    let mut history = History::default();
    history.observe(REMOTE, "rboot", Some("p9"));
    history.observe(LOCAL, "boot", Some("p1"));
    assert_eq!(
        history.back(online).map(|step| step.visit),
        Some(Visit {
            endpoint_id: REMOTE.into(),
            boot_id: "rboot".into(),
            pane_id: "p9".into(),
        })
    );
}
