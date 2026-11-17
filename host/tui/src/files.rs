//! The files the person gives the page and the places they choose to save in (specification §8.9
//! and §8.10), on the terminal's side: the handles made of them, and the one thread that reads and
//! writes them for the program.
//!
//! The program that runs this screen is in the same process and on the same machine as the
//! person, so no folder and no pipe of the host's (§8.11) carries the bytes: the program's `read`
//! and `write` come through [`pencere::Conversation`] as [`Reading`] and [`Writing`], and the
//! bytes cross in memory, in pieces of at most a megabyte as they would cross a terminal's stream.
//! The rules are the ones a terminal that hosts the view holds (bateri's):
//!
//! **A file is read where the person left it and written where the person chose, and nowhere
//! else.** A handle names a row of a table that is filled from what the person did (a path they
//! gave in the panel), never from what the page or the program says. A handle is of one of two
//! kinds and does the work of that kind alone: a file that was given is read (never written to) and
//! a place that was chosen is written (never read, and the page cannot draw it by `file:` either).
//! A read finds the file as it was when it was given (its length and the time it changed) or says
//! `gone`. A save goes to a part beside its place, `.<name>.<end of the handle>.pencere-part`, made
//! anew (a part that a process that was killed left is replaced, and a link put there is not
//! written through), and is given the place's name when its last piece is in; a save that ends any
//! other way (a piece fails, the view is shown again, the screen ends) leaves no part behind.
//!
//! **The disk is never waited for on the thread that draws and reads the terminal:** a piece goes to
//! a thread of this module, which is started by the first one, and is answered from there.

use std::collections::HashMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use pencere::{MAX_PIECE, Reading, Writing};

/// The most bytes of a file the page is given to draw (a picture): more is no picture to it.
const LONGEST_FILE: u64 = 64 * 1024 * 1024;

/// The longest a file's name may be on the disks this runs on.
const NAME_MAX: usize = 255;

/// What ends the name of a part.
const PART_SUFFIX: &str = ".pencere-part";

/// A file as it was when the person gave it. A read says `gone` when its length or the time it
/// changed is another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GivenFile {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

/// What a handle names.
enum Row {
    /// A file the person gave.
    Given(GivenFile),
    /// A place the person chose to save in.
    Save(PathBuf),
}

/// The handles of the files the person gave the page and of the places they chose to save in: 32
/// lowercase hex digits each, which are nothing once the screen ends. The page draws a file given as
/// `file:` and the handle, and is answered from here, never from a path of its own.
#[derive(Default)]
pub(crate) struct Handles {
    rows: Mutex<HashMap<String, Row>>,
}

impl Handles {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn rows(&self) -> MutexGuard<'_, HashMap<String, Row>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn add(&self, row: Row) -> String {
        let mut rows = self.rows();
        loop {
            let handle = random_handle();
            if !rows.contains_key(&handle) {
                rows.insert(handle.clone(), row);
                return handle;
            }
        }
    }

    /// A new handle for the file at `path`, which is as `meta` (what `fs::metadata` said of it)
    /// shows it.
    pub fn give(&self, path: &Path, meta: &fs::Metadata) -> String {
        self.add(Row::Given(GivenFile {
            path: path.to_path_buf(),
            size: meta.len(),
            modified: meta.modified().ok(),
        }))
    }

    /// A new handle for the place `target`, which the person chose to save a file in.
    pub fn place(&self, target: PathBuf) -> String {
        self.add(Row::Save(target))
    }

    /// Where the file the person gave under `handle` is: none for a handle that was not made, or
    /// that is a place to save in.
    pub fn given_path(&self, handle: &str) -> Option<PathBuf> {
        match self.rows().get(handle) {
            Some(Row::Given(file)) => Some(file.path.clone()),
            _ => None,
        }
    }

    fn given(&self, handle: &str) -> Option<GivenFile> {
        match self.rows().get(handle) {
            Some(Row::Given(file)) => Some(file.clone()),
            _ => None,
        }
    }

    /// The place the person chose under `handle`: none for a handle that was not made, that is a
    /// file given, or that has had its last write (or failed) already.
    pub fn place_of(&self, handle: &str) -> Option<PathBuf> {
        match self.rows().get(handle) {
            Some(Row::Save(target)) => Some(target.clone()),
            _ => None,
        }
    }

    /// Lets the place of `handle` go: it is saved, or it never will be.
    fn forget(&self, handle: &str) {
        let mut rows = self.rows();
        if matches!(rows.get(handle), Some(Row::Save(_))) {
            rows.remove(handle);
        }
    }

    /// The bytes of the file given under `handle`, for the page to draw: none for a handle that was
    /// not made, that is a place, a file that is gone or one too large to draw.
    pub fn read(&self, handle: &str) -> Option<Vec<u8>> {
        let path = self.given_path(handle)?;
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > LONGEST_FILE {
            return None;
        }
        fs::read(path).ok()
    }
}

/// 32 hex digits no one chose: the keys of two `RandomState`s (the system's randomness), the time
/// and a count.
fn random_handle() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let hash = |salt: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.finish()
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}{:016x}", hash(nanos), hash(count))
}

// --- The thread that reads and writes -------------------------------------------------------

/// What the thread is asked.
enum Job {
    Read(Reading),
    Write(Writing),
    /// The view is shown again: the places that were being saved to are nothing now.
    Abandon,
}

/// The program's reads and writes of the person's files, done on a thread of their own. Dropping it
/// finishes the pieces that wait, deletes every part that is not kept and ends the thread.
pub(crate) struct Disk {
    handles: Arc<Handles>,
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Disk {
    pub fn new(handles: Arc<Handles>) -> Self {
        Self {
            handles,
            jobs: None,
            thread: None,
        }
    }

    /// A piece of a file given, which the program asked for.
    pub fn read(&mut self, reading: Reading) {
        self.send(Job::Read(reading));
    }

    /// A piece of a file for a place chosen.
    pub fn write(&mut self, writing: Writing) {
        self.send(Job::Write(writing));
    }

    /// The saves that are under way are given up (their parts deleted) and their places let go.
    pub fn abandon(&mut self) {
        if self.jobs.is_some() {
            self.send(Job::Abandon);
        }
    }

    fn send(&mut self, job: Job) {
        if self.jobs.is_none() {
            let (sender, jobs) = channel();
            let mut worker = Worker {
                handles: Arc::clone(&self.handles),
                saves: HashMap::new(),
            };
            let started = std::thread::Builder::new()
                .name("pencere-tui-files".into())
                .spawn(move || {
                    for job in jobs {
                        worker.run(job);
                    }
                });
            match started {
                Ok(thread) => {
                    self.jobs = Some(sender);
                    self.thread = Some(thread);
                }
                // A job that is dropped tells the program that no view holds files.
                Err(_) => return,
            }
        }
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

impl Drop for Disk {
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The thread's side: the saves under way, by handle.
struct Worker {
    handles: Arc<Handles>,
    saves: HashMap<String, Part>,
}

impl Worker {
    fn run(&mut self, job: Job) {
        match job {
            Job::Read(reading) => {
                let result = self.read(&reading);
                match result {
                    Ok(bytes) => reading.bytes(bytes),
                    Err(code) => reading.refused(code),
                }
            }
            Job::Write(writing) => match self.write(&writing) {
                Ok(()) => writing.written(),
                Err(code) => {
                    // What was written is thrown away, and what comes after for the handle is
                    // for a handle that is nothing.
                    self.saves.remove(writing.handle());
                    self.handles.forget(writing.handle());
                    writing.refused(code);
                }
            },
            Job::Abandon => {
                for (handle, _) in self.saves.drain() {
                    self.handles.forget(&handle);
                }
            }
        }
    }

    fn read(&self, reading: &Reading) -> Result<Vec<u8>, &'static str> {
        let source = self.handles.given(reading.handle()).ok_or("handle")?;
        if reading.length() == 0 || reading.length() > MAX_PIECE as u64 {
            return Err("range");
        }
        read_range(&source, reading.offset(), reading.length())
    }

    fn write(&mut self, writing: &Writing) -> Result<(), &'static str> {
        let handle = writing.handle();
        if writing.bytes().len() > MAX_PIECE {
            return Err("size");
        }
        let target = self.handles.place_of(handle).ok_or("handle")?;
        if !self.saves.contains_key(handle) {
            // The first piece is where the file starts.
            if writing.offset() != 0 {
                return Err("order");
            }
            let part = Part::create(&target, handle).map_err(write_code)?;
            self.saves.insert(handle.to_owned(), part);
        }
        let part = self.saves.get_mut(handle).ok_or("handle")?;
        // The pieces come in order and leave no gap: a part that is not where the piece is would be
        // a file the program did not write.
        if part.length != writing.offset() {
            return Err("order");
        }
        part.file
            .write_all_at(writing.bytes(), writing.offset())
            .map_err(write_code)?;
        part.length += writing.bytes().len() as u64;
        if writing.end()
            && let Some(part) = self.saves.remove(handle)
        {
            part.keep(&target).map_err(write_code)?;
            self.handles.forget(handle);
        }
        Ok(())
    }
}

/// Up to `length` bytes of `source` from `offset` (fewer where the file ends), if the file is as it
/// was given.
fn read_range(source: &GivenFile, offset: u64, length: u64) -> Result<Vec<u8>, &'static str> {
    let file = fs::File::open(&source.path).map_err(read_code)?;
    // The file that is open is the one looked at: what is checked is what is read.
    let meta = file.metadata().map_err(read_code)?;
    if !meta.is_file() || meta.len() != source.size || meta.modified().ok() != source.modified {
        return Err("gone");
    }
    if offset > source.size {
        return Err("range");
    }
    let wanted = length.min(source.size - offset);
    let mut bytes = vec![0; usize::try_from(wanted).map_err(|_| "range")?];
    let mut filled = 0;
    while let Some(room) = bytes.get_mut(filled..).filter(|room| !room.is_empty()) {
        match file.read_at(room, offset + filled as u64) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(read_code(error)),
        }
    }
    // A file that ends sooner than it said has changed.
    if filled < bytes.len() {
        return Err("gone");
    }
    Ok(bytes)
}

/// A file that is not there or may not be read is gone from the program's point of view.
fn read_code(error: io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => "gone",
        _ => "io",
    }
}

/// What a failure of writing is told to the program as: a disk with no room, or any other.
fn write_code(error: io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => "space",
        _ => "io",
    }
}

/// Where the part of a save goes: beside `target`, hidden, and named for the save as well as for its
/// place (the last of its handle), so that two saves to one place do not share a part, and short
/// enough to be a name wherever the place's own is.
fn part_path(target: &Path, handle: &str) -> io::Result<PathBuf> {
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a place with no name"))?
        .to_string_lossy();
    let tag = handle.get(handle.len().saturating_sub(8)..).unwrap_or("");
    let room = NAME_MAX.saturating_sub(".".len() + ".".len() + tag.len() + PART_SUFFIX.len());
    let mut end = name.len().min(room);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let cut = name.get(..end).unwrap_or("");
    Ok(target.with_file_name(format!(".{cut}.{tag}{PART_SUFFIX}")))
}

/// A save under way: the part, beside the place, which is deleted if it is dropped unkept.
struct Part {
    path: PathBuf,
    file: fs::File,
    length: u64,
    /// The part has been given its place's name: there is nothing to delete.
    kept: bool,
}

impl Part {
    fn create(target: &Path, handle: &str) -> io::Result<Self> {
        let path = part_path(target, handle)?;
        // A part a process that was killed left is ours to replace. What is there now is not
        // followed if it is a link: `create_new` fails for anything that is there (a link too),
        // so one put there meanwhile fails the save rather than be written through.
        let _ = fs::remove_file(&path);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file,
            length: 0,
            kept: false,
        })
    }

    /// Gives the part the name of its place.
    fn keep(mut self, target: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        fs::rename(&self.path, target)?;
        // The name is the place's now: the file closes with the part and nothing is deleted.
        self.kept = true;
        Ok(())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.path);
        }
    }
}
