//! Where verified bundle files are kept: in memory, or in a directory so that
//! a second run finds what the first one verified, as a terminal that remembers
//! files between runs does.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pencere::bundle::{Digest, MemoryStore, Store, Verified};

use crate::log;

/// Files named by their digest, in one directory.
pub struct DirStore {
    dir: PathBuf,
}

impl DirStore {
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    fn path(&self, digest: &Digest) -> PathBuf {
        self.dir.join(digest.to_string())
    }
}

impl Store for DirStore {
    fn contains(&self, digest: &Digest) -> bool {
        self.get(digest).is_some()
    }

    fn insert(&mut self, file: Verified) {
        let (digest, bytes) = file.into_parts();
        // Written whole under a name of its own, then put in place: a file
        // that is there is a file that is complete.
        let part = self.dir.join(format!("{digest}.part"));
        let written = fs::write(&part, &bytes).and_then(|()| fs::rename(&part, self.path(&digest)));
        if let Err(error) = written {
            log::line("host", format!("could not keep {digest}: {error}"));
        }
    }

    /// A file is given only if it still has the digest it is named by: the
    /// directory is the person's, and a file changed on disk must not reach a view
    /// as if the program had sent it.
    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>> {
        let bytes = fs::read(self.path(digest)).ok()?;
        (Digest::of(&bytes) == *digest).then(|| Arc::from(bytes))
    }
}

/// The store the host was asked for.
pub enum Keep {
    Memory(MemoryStore),
    Directory(DirStore),
}

impl Store for Keep {
    fn contains(&self, digest: &Digest) -> bool {
        match self {
            Self::Memory(store) => store.contains(digest),
            Self::Directory(store) => store.contains(digest),
        }
    }

    fn insert(&mut self, file: Verified) {
        match self {
            Self::Memory(store) => store.insert(file),
            Self::Directory(store) => store.insert(file),
        }
    }

    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>> {
        match self {
            Self::Memory(store) => store.get(digest),
            Self::Directory(store) => store.get(digest),
        }
    }
}
