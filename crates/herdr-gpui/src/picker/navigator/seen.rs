//! What a navigator's rows are built from, cheap to take on every tick, so
//! an open navigator lists its rows again only when the hosts changed.

use crate::HerdrWindow;
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

/// The window's scalars the rows read, hashed, and each host's snapshot by
/// identity.
#[derive(Debug)]
pub(super) struct Fingerprint {
    scalars: u64,
    /// Every accepted snapshot is a new allocation, and a held `Weak` keeps
    /// its allocation, so a newer snapshot never reuses the address of one
    /// held here.
    snapshots: Vec<Option<Weak<ClientShellSnapshot>>>,
}

/// Equal when nothing the rows are built from changed: the same scalars and
/// the very same snapshots.
impl PartialEq for Fingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.scalars == other.scalars
            && self.snapshots.len() == other.snapshots.len()
            && self
                .snapshots
                .iter()
                .zip(&other.snapshots)
                .all(|pair| match pair {
                    (Some(left), Some(right)) => left.ptr_eq(right),
                    (left, right) => left.is_none() && right.is_none(),
                })
    }
}

impl HerdrWindow {
    /// Changes whenever `navigator_rows` may return something else. No row
    /// is built and nothing is sorted, so the window can afford it on every
    /// tick.
    pub(super) fn navigator_fingerprint(&self) -> Fingerprint {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        // Which host draws from the window's own live state.
        self.selected_endpoint.hash(&mut hash);
        let mut snapshots = Vec::with_capacity(self.endpoints.len());
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            endpoint.id.hash(&mut hash);
            endpoint.label.hash(&mut hash);
            endpoint.availability().hash(&mut hash);
            let snapshot = self.host_live(index).snapshot.as_ref();
            snapshots.push(snapshot.map(Arc::downgrade));
        }
        Fingerprint {
            scalars: hash.finish(),
            snapshots,
        }
    }
}
