//! The Herdr fork's agent order under the Priority sort, laid over
//! upstream's. Mirrors `priority_order` in the Herdr fork's
//! `src/client/shell/aggregate_navigation.rs`.
//!
//! The fork's daemon sends each agent's place in its queue as `priority_key`,
//! with the `space_priority` lift applied, and keys from different daemons
//! compare the same way. The panel compares them as plain byte strings,
//! smallest first, and never reads one. Equal keys tie, so the stable sort
//! keeps them in host order, then in their snapshot's order, as the terminal
//! client's endpoint index and position do. A keyless agent, as a daemon that
//! computes no key sends, follows every keyed one and keeps upstream's order
//! among the keyless: attention, then the most recent change. The terminal
//! client keeps those in listing order instead.

use herdr_client::protocol::ClientShellAgent;

/// The sort key that ranks `agent` in the daemon's order. `upstream` is
/// upstream's own sort key for it, which decides only between agents that
/// have no priority key.
pub(super) fn rank<T>(agent: &ClientShellAgent, upstream: T) -> (bool, &[u8], Option<T>) {
    let key = agent.priority_key.as_bytes();
    (key.is_empty(), key, key.is_empty().then_some(upstream))
}
