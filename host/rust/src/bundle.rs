//! Where a host keeps the files of the bundles it has verified.
//!
//! The rules of a bundle (what a manifest may say, which digests a host lacks, how a file is
//! verified) are the wire's (`pencere_wire::bundle`) and are re-exported here; this module adds
//! the one thing that is the terminal's own, the [`Store`] behind which it keeps them.

use std::collections::HashMap;
use std::sync::Arc;

pub use pencere_wire::bundle::*;

/// Where a host keeps the files it has verified, by digest. How much it keeps and
/// for how long is its own policy: it may drop any entry at any time, and a
/// program sends every file of its manifest at every open it makes.
pub trait Store {
    /// Whether the store holds the file with this digest.
    fn contains(&self, digest: &Digest) -> bool;
    /// Keeps a verified file.
    fn insert(&mut self, file: Verified);
    /// The file with this digest.
    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>>;
}

/// A store in memory that keeps everything it is given.
#[derive(Debug, Default)]
pub struct MemoryStore {
    files: HashMap<Digest, Arc<[u8]>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many files it holds.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Store for MemoryStore {
    fn contains(&self, digest: &Digest) -> bool {
        self.files.contains_key(digest)
    }

    fn insert(&mut self, file: Verified) {
        let (digest, bytes) = file.into_parts();
        self.files.insert(digest, bytes.into());
    }

    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>> {
        self.files.get(digest).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_store_keeps_a_file_under_its_digest() {
        let mut store = MemoryStore::new();
        assert!(store.is_empty());
        store.insert(Verified::from_bytes(b"two".to_vec()));
        assert!(store.contains(&Digest::of(b"two")));
        assert!(!store.contains(&Digest::of(b"one")));
        assert_eq!(store.get(&Digest::of(b"two")).as_deref(), Some(&b"two"[..]));
        assert_eq!(store.len(), 1);
    }
}
