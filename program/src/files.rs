//! The files beside a view's bundle (specification §8.8 to §8.10): what the program attaches to
//! its view for it to draw, a file the person gave the view, which the program reads by its
//! handle, and a place the person chose in the Save panel, which the program writes to.
//!
//! The helpers are methods of [`ViewControl`] and are called from any thread but the loop's (a
//! handler's, typically, once the view has passed it a handle): each hands the loop what to write
//! and waits for the terminal's answer. They need a view opened at `lua` `2.1` or `wasm` `3.0`
//! and later; with any other they answer [`FileError::Unsupported`] and nothing is sent.
//!
//! A file being received is written beside its place under a name the person recognises
//! (`.` and the name, then `.pencere-part`) and given its name when the last byte is in. When the
//! view ends, the program is asked to end (⌃C, a hangup) or the receiving is given up, the part is
//! deleted, on every path the terminal is given back on as well.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use pencere_wire::base64;
use pencere_wire::bundle::Digest;
use pencere_wire::frame::{FileParams, Message as Frame};

use crate::Kind;
use crate::control::{Request, ViewControl};
use crate::handlers::Work;
use crate::link::Writer;
use crate::payload::{self, StageOffer, Transfer};
use crate::pipes;
use crate::staged::{self, Proof, Went};

/// The most bytes a `read` asks for and a `write` carries (specification §8.9, §8.10).
pub const MAX_PIECE: usize = 1 << 20;

/// How many reads a receiving keeps out at once: the most the terminal answers (§8.9).
const READS_OUT: usize = 2;

/// The scheme of the name a view draws an attached file by (§9.8).
const ATTACHED: &str = "sha256:";

/// Why a file could not be attached, received or saved.
#[derive(Debug)]
#[non_exhaustive]
pub enum FileError {
    /// No view runs that has files beside its bundle: none is up, or the one that is was opened at
    /// a version that has none (`lua` before `2.1`, `wasm` before `3.0`, a page). Nothing was
    /// sent.
    Unsupported,
    /// The terminal said no, with its code: for an attach `limit`, `view`, `size` or `digest`
    /// (§8.8); for a read `handle`, `range`, `busy`, `gone` or `io` (§8.9); for a save `handle`,
    /// `order`, `size`, `space` or `io` (§8.10).
    Refused(String),
    /// The view ended before the terminal answered, or the receiving was given up: what was
    /// received so far is deleted.
    Ended,
    /// The program's side failed: reading the file to attach or send, writing the one received.
    Io(io::Error),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("no view that has files runs"),
            Self::Refused(code) => write!(f, "the terminal refused the file ({code})"),
            Self::Ended => f.write_str("the view ended before the file was through"),
            Self::Io(why) => write!(f, "the file: {why}"),
        }
    }
}

impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(why) => Some(why),
            _ => None,
        }
    }
}

impl From<io::Error> for FileError {
    fn from(why: io::Error) -> Self {
        Self::Io(why)
    }
}

/// Whether a view of `kind` opened at `version` has files beside its bundle (§5.5): `lua` from
/// `2.1` on and `wasm` from `3.0` on.
pub(crate) fn has_files(kind: Kind, version: &str) -> bool {
    let mut parts = version.split('.').map(str::parse::<u64>);
    let (Some(Ok(major)), minor) = (parts.next(), parts.next()) else {
        return false;
    };
    let minor = minor.and_then(Result::ok).unwrap_or(0);
    match kind {
        Kind::Lua => (major, minor) >= (2, 1),
        Kind::Wasm => major >= 3,
        Kind::Web => false,
    }
}

/// The name a view draws an attached file by: `sha256:` and its digest (§9.8).
fn name_of(digest: &Digest) -> String {
    format!("{ATTACHED}{digest}")
}

/// The name of a file the person gave, made safe to put in a path of the program's: its last
/// part only, with no control character, `/`, `\` or NUL, and never `.` or `..` or nothing
/// (`file` then). The name is the person's and comes through the view, so a program never joins
/// it to a path as it came (§8.9).
#[must_use]
pub fn safe_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = last.chars().filter(|c| !c.is_control()).collect();
    let clean = clean.trim();
    if clean.is_empty() || clean == "." || clean == ".." {
        "file".to_owned()
    } else {
        clean.to_owned()
    }
}

// ---- what the loop keeps -------------------------------------------------------------

/// An attach the terminal has not answered for the last time.
struct Attaching {
    files: Vec<(Digest, Arc<[u8]>)>,
    reply: SyncSender<Result<(), FileError>>,
    /// The missing files are sent: the next `need` is the last answer.
    sent: bool,
}

/// A read that waits for its turn.
struct Queued {
    handle: String,
    offset: u64,
    length: u64,
    reply: SyncSender<Result<Vec<u8>, FileError>>,
}

/// What is known of a place to save in while its writes go out.
#[derive(Default)]
struct Saving {
    /// The terminal said the file failed, with its code: later writes are not sent.
    failed: Option<String>,
    /// The write with `end` is out, and waits for the `wrote`.
    last: Option<SyncSender<Result<(), FileError>>>,
}

/// What the loop tells a helper of a `stage` or a `fetch` it was asked for (§8.11).
#[derive(Debug)]
pub(crate) enum Transferring {
    /// The id the message went out with, which is how the transfer is taken back.
    Id(u32),
    /// What the terminal said of it.
    Told(Transfer),
}

/// A `stage` or a `fetch` that is out: who is told of it.
struct Moving {
    events: Sender<Transferring>,
    /// A `fetch`; a `stage` otherwise.
    fetch: bool,
    /// The program has taken it back with an `unstage`. It stays until the host's last answer,
    /// which is `cancelled`, or the end it had already reached: the helper learns which.
    unstaged: bool,
}

/// The loop's side: what the view that runs has attached, and the answers the helpers wait for.
/// Everything is the view's and goes with it ([`Files::forget`]): a helper that waits is told the
/// view ended.
#[derive(Default)]
pub(crate) struct Files {
    attached: HashSet<Digest>,
    attaches: HashMap<u32, Attaching>,
    reads: HashMap<u32, SyncSender<Result<Vec<u8>, FileError>>>,
    /// Reads that wait for one of those out to be answered: the terminal answers a third `busy`
    /// (§8.9), and the reads of the program's helpers are counted together.
    queued: VecDeque<Queued>,
    /// The handle each write that is out was for.
    writes: HashMap<u32, String>,
    saves: HashMap<String, Saving>,
    /// The `stage`s and `fetch`es that are out, by the id they went out with.
    transfers: HashMap<u32, Moving>,
    /// The folder the terminal offers (§8.11). It is the terminal's, not the view's: it outlives
    /// the view, and a `welcome` sets it again.
    stage: Option<StageOffer>,
}

impl Files {
    /// The view is over: what it had attached is no longer, and every helper that waits is told so
    /// (its answer's way is dropped).
    pub(crate) fn forget(&mut self) {
        let stage = self.stage.take();
        *self = Self {
            stage,
            ..Self::default()
        };
    }

    /// The terminal's `welcome` says where its folder is, or that it has none.
    pub(crate) fn offer(&mut self, stage: Option<StageOffer>) {
        self.stage = stage;
    }

    /// Whether the `need` with this `re` answers an attach.
    pub(crate) fn answers_attach(&self, re: Option<u32>) -> bool {
        re.is_some_and(|re| self.attaches.contains_key(&re))
    }

    /// Does what a helper asked, on a view that has files (`has` is false when none runs).
    pub(crate) fn on_request(&mut self, request: Request, has: bool, writer: &mut Writer) {
        match request {
            Request::Attach { files, reply } => {
                if !has {
                    let _ = reply.send(Err(FileError::Unsupported));
                    return;
                }
                let files: Vec<_> = files
                    .into_iter()
                    .filter(|(digest, _)| !self.attached.contains(digest))
                    .collect();
                if files.is_empty() {
                    let _ = reply.send(Ok(()));
                    return;
                }
                let listed: Vec<(String, u64)> = files
                    .iter()
                    .map(|(digest, bytes)| (digest.to_string(), bytes.len() as u64))
                    .collect();
                match writer.message("attach", None, None, &payload::attach(&listed)) {
                    Ok(id) => {
                        self.attaches.insert(
                            id,
                            Attaching {
                                files,
                                reply,
                                sent: false,
                            },
                        );
                    }
                    Err(_) => {
                        let _ = reply.send(Err(FileError::Ended));
                    }
                }
            }
            Request::Detach { digests } => {
                if !has {
                    return;
                }
                let digests: Vec<String> = digests
                    .into_iter()
                    .filter(|digest| self.attached.remove(digest))
                    .map(|digest| digest.to_string())
                    .collect();
                if !digests.is_empty() {
                    let _ = writer.message("detach", None, None, &payload::detach(&digests));
                }
            }
            Request::Read {
                handle,
                offset,
                length,
                reply,
            } => {
                if !has {
                    let _ = reply.send(Err(FileError::Unsupported));
                    return;
                }
                let read = Queued {
                    handle,
                    offset,
                    length,
                    reply,
                };
                if self.reads.len() < READS_OUT {
                    self.send_read(read, writer);
                } else {
                    self.queued.push_back(read);
                }
            }
            Request::Write {
                handle,
                offset,
                bytes,
                end,
                reply,
            } => {
                if !has {
                    let _ = reply.send(Err(FileError::Unsupported));
                    return;
                }
                let saving = self.saves.entry(handle.clone()).or_default();
                if let Some(code) = &saving.failed {
                    let _ = reply.send(Err(FileError::Refused(code.clone())));
                    return;
                }
                let file = FileParams {
                    handle: Some(&handle),
                    offset: Some(offset),
                    end,
                    error: None,
                };
                match writer.file_message("write", None, file, &base64::encode(&bytes)) {
                    Ok(id) => {
                        self.writes.insert(id, handle);
                        if end {
                            saving.last = Some(reply);
                        } else {
                            let _ = reply.send(Ok(()));
                        }
                    }
                    Err(_) => {
                        let _ = reply.send(Err(FileError::Ended));
                    }
                }
            }
            Request::StageOffer { reply } => {
                let _ = reply.send(if has {
                    Ok(self.stage.clone())
                } else {
                    Err(FileError::Unsupported)
                });
            }
            Request::Stage {
                handle,
                offset,
                events,
            } => {
                let body = payload::stage(&handle, offset);
                self.begin_transfer(has, "stage", &body, events, false, writer);
            }
            Request::Fetch {
                handle,
                size,
                offset,
                events,
            } => {
                let body = payload::fetch(&handle, size, offset);
                self.begin_transfer(has, "fetch", &body, events, true, writer);
            }
            Request::Unstage { id } => {
                if let Some(moving) = self.transfers.get_mut(&id)
                    && !moving.unstaged
                {
                    moving.unstaged = true;
                    let _ = writer.message("unstage", None, None, &payload::unstage(id));
                }
            }
            // The others are the loop's own.
            _ => {}
        }
    }

    /// Sends a `stage` or a `fetch` and tells its helper the id. Where it cannot go the helper's
    /// way is dropped, and it is told the view ended.
    fn begin_transfer(
        &mut self,
        has: bool,
        kind: &str,
        body: &str,
        events: Sender<Transferring>,
        fetch: bool,
        writer: &mut Writer,
    ) {
        if !has {
            return;
        }
        if let Ok(id) = writer.message(kind, None, None, body)
            && events.send(Transferring::Id(id)).is_ok()
        {
            self.transfers.insert(
                id,
                Moving {
                    events,
                    fetch,
                    unstaged: false,
                },
            );
        }
    }

    /// A `staged` or a `fetched` (§8.11): the helper that asked is told, and the last answer ends
    /// the transfer. A helper that is gone has its transfer taken back.
    pub(crate) fn on_transfer(&mut self, message: &Frame, fetch: bool, writer: &mut Writer) {
        let Some(re) = message.re() else {
            return;
        };
        let Some(moving) = self
            .transfers
            .get_mut(&re)
            .filter(|moving| moving.fetch == fetch)
        else {
            return;
        };
        let done = if fetch { "saved" } else { "sent" };
        let Some(told) = payload::transfer(&message.payload, done) else {
            return;
        };
        let last = !matches!(told, Transfer::Pipe(_));
        let heard = moving.events.send(Transferring::Told(told)).is_ok();
        if last {
            self.transfers.remove(&re);
        } else if !heard && !moving.unstaged {
            moving.unstaged = true;
            let _ = writer.message("unstage", None, None, &payload::unstage(re));
        }
    }

    /// A `need` that answers an attach (§8.8): what is refused ends it, what is missing is sent,
    /// and the answer after the files is the last.
    pub(crate) fn on_need(&mut self, message: &Frame, writer: &mut Writer) {
        let Some(re) = message.re() else {
            return;
        };
        let (Some(missing), Some(refused)) = (
            payload::need(&message.payload),
            payload::refused(&message.payload),
        ) else {
            return;
        };
        let Some(mut attaching) = self.attaches.remove(&re) else {
            return;
        };
        if let Some(refusal) = refused.first() {
            let _ = attaching
                .reply
                .send(Err(FileError::Refused(refusal.code.clone())));
            return;
        }
        if attaching.sent || missing.is_empty() {
            self.attached
                .extend(attaching.files.iter().map(|(digest, _)| *digest));
            let _ = attaching.reply.send(Ok(()));
            return;
        }
        for hex in &missing {
            let Some((digest, bytes)) = attaching
                .files
                .iter()
                .find(|(digest, _)| digest.to_string() == *hex)
            else {
                continue;
            };
            let body = base64::encode(bytes);
            if writer
                .message("file", None, Some(&digest.to_string()), &body)
                .is_err()
            {
                let _ = attaching.reply.send(Err(FileError::Ended));
                return;
            }
        }
        attaching.sent = true;
        self.attaches.insert(re, attaching);
    }

    /// A `data` that answers a read (§8.9).
    pub(crate) fn on_data(&mut self, message: &Frame, writer: &mut Writer) {
        let Some(reply) = message.re().and_then(|re| self.reads.remove(&re)) else {
            return;
        };
        let answer = match message.error_code() {
            Some(code) => Err(FileError::Refused(code.to_owned())),
            None => base64::decode(message.payload.as_bytes())
                .ok_or_else(|| FileError::Refused("io".to_owned())),
        };
        let _ = reply.send(answer);
        if let Some(next) = self.queued.pop_front() {
            self.send_read(next, writer);
        }
    }

    fn send_read(&mut self, read: Queued, writer: &mut Writer) {
        let body = payload::read(&read.handle, read.offset, read.length);
        match writer.message("read", None, None, &body) {
            Ok(id) => {
                self.reads.insert(id, read.reply);
            }
            Err(_) => {
                let _ = read.reply.send(Err(FileError::Ended));
            }
        }
    }

    /// A `wrote` (§8.10): the file is saved, or failed at the write it answers.
    pub(crate) fn on_wrote(&mut self, message: &Frame) {
        let Some(handle) = message.re().and_then(|re| self.writes.get(&re).cloned()) else {
            return;
        };
        let Some(code) = payload::wrote(&message.payload) else {
            return;
        };
        self.writes.retain(|_, of| *of != handle);
        let Some(saving) = self.saves.get_mut(&handle) else {
            return;
        };
        let answer = match &code {
            Some(code) => Err(FileError::Refused(code.clone())),
            None => Ok(()),
        };
        if let Some(last) = saving.last.take() {
            let _ = last.send(answer);
        }
        match code {
            Some(code) => saving.failed = Some(code),
            None => {
                self.saves.remove(&handle);
            }
        }
    }
}

// ---- the parts of the files being received -------------------------------------------

/// The parts being written, deleted on the way out if their helper did not.
static PARTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn parts() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    PARTS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Deletes every part a receiving left: the terminal is being given back.
pub(crate) fn discard_parts() {
    for part in parts().drain(..) {
        let _ = fs::remove_file(part);
    }
}

/// A part being written; deleted when it is dropped without being kept.
pub(crate) struct Part {
    path: PathBuf,
    file: fs::File,
    /// It has been given its place's name: dropping it deletes nothing.
    kept: bool,
    /// How many bytes it holds: the file's own from the start, in order.
    len: u64,
}

impl Part {
    /// The name of the part of the file that will be `to`.
    fn path_of(to: &Path) -> io::Result<PathBuf> {
        let name = to
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a place with no name"))?;
        let mut part_name = std::ffi::OsString::from(".");
        part_name.push(name);
        part_name.push(".pencere-part");
        Ok(to.with_file_name(part_name))
    }

    fn create(to: &Path) -> io::Result<Self> {
        let path = Self::path_of(to)?;
        // A part a process that was killed left is ours to replace; what is there now is not
        // followed if it is a link, and one put there meanwhile fails the receiving rather than be
        // written through.
        let _ = fs::remove_file(&path);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        parts().push(path.clone());
        Ok(Self {
            path,
            file,
            kept: false,
            len: 0,
        })
    }

    /// A part that is a copy-on-write clone of the file at `from`, which is `size` bytes long, with
    /// the rights a part made by [`Part::create`] has, where the file system can clone; `None`
    /// where it cannot, or `from` is not what was expected.
    fn cloned(to: &Path, from: &Path, size: u64) -> Option<Self> {
        // The rights of a file of the program's own user are learned from a part made anew: a clone
        // has the rights of the file it was cloned from, and those are not the receiver's to
        // choose.
        let probe = Self::create(to).ok()?;
        let rights = probe.file.metadata().ok()?.permissions().mode() & 0o7777;
        let path = probe.path.clone();
        drop(probe);
        if !pipes::clone_file(from, &path) {
            return None;
        }
        // Not followed if it is a link: it was made a moment ago and is a file, or it is not ours.
        let opened = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path);
        let Ok(file) = opened else {
            let _ = fs::remove_file(&path);
            return None;
        };
        parts().push(path.clone());
        // From here the part is deleted if it is dropped.
        let part = Self {
            path,
            file,
            kept: false,
            len: size,
        };
        let meta = part.file.metadata().ok()?;
        if !meta.is_file() || meta.len() != size {
            return None;
        }
        part.file
            .set_permissions(fs::Permissions::from_mode(rights))
            .ok()?;
        Some(part)
    }

    /// How many bytes the part holds.
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// Appends the next bytes of the file.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)?;
        self.len += bytes.len() as u64;
        Ok(())
    }

    /// Empties the part: what it holds is not the file's.
    pub(crate) fn restart(&mut self) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.len = 0;
        Ok(())
    }

    /// Gives the part the name of its place.
    fn keep(mut self, to: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        fs::rename(&self.path, to)?;
        self.kept = true;
        Ok(())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.path);
        }
        parts().retain(|part| *part != self.path);
    }
}

// ---- the helpers -----------------------------------------------------------------------

impl ViewControl {
    /// Hands the loop a request that is answered on `answer`; `Ended` when no loop runs.
    pub(crate) fn ask<T>(
        &self,
        request: impl FnOnce(SyncSender<Result<T, FileError>>) -> Request,
    ) -> Receiver<Result<T, FileError>> {
        let (reply, answer) = sync_channel(1);
        match self.outbox() {
            Some(outbox) => {
                outbox.send(Work::Control(request(reply)));
            }
            None => {
                let _ = reply.send(Err(FileError::Unsupported));
            }
        }
        answer
    }

    /// Attaches `bytes` to the view that runs and returns the name the view draws them by
    /// (`sha256:` and their digest, §9.8). The digest is made of these bytes and these bytes are
    /// sent, so a file that changes meanwhile is not a mismatch. Returns once the terminal holds
    /// them; a file the view has attached already, or that the terminal still holds, does not
    /// cross the stream again. What is attached goes with the view: a view opened again needs
    /// its files attached again.
    ///
    /// # Errors
    ///
    /// [`FileError::Unsupported`] with no view that has files, [`FileError::Refused`] when the
    /// terminal will not attach them (`limit`, …) and [`FileError::Ended`] when the view ended
    /// first.
    pub fn attach(&self, bytes: impl Into<Arc<[u8]>>) -> Result<String, FileError> {
        let bytes: Arc<[u8]> = bytes.into();
        let digest = Digest::of(&bytes);
        let answer = self.ask(|reply| Request::Attach {
            files: vec![(digest, bytes)],
            reply,
        });
        answer.recv().unwrap_or(Err(FileError::Ended))?;
        Ok(name_of(&digest))
    }

    /// Reads the file at `path` once and attaches what was read, as [`ViewControl::attach`] does.
    ///
    /// # Errors
    ///
    /// As `attach`, and [`FileError::Io`] when the file cannot be read.
    pub fn attach_file(&self, path: &Path) -> Result<String, FileError> {
        let bytes = fs::read(path)?;
        self.attach(bytes)
    }

    /// Lets the view's attached file of `name` go: the terminal forgets it and the view no longer
    /// draws it. A name the view has not attached is nothing.
    pub fn detach(&self, name: &str) {
        let Some(digest) = name.strip_prefix(ATTACHED).and_then(Digest::parse) else {
            return;
        };
        if let Some(outbox) = self.outbox() {
            outbox.send(Work::Control(Request::Detach {
                digests: vec![digest],
            }));
        }
    }

    /// Reads up to `length` bytes (at most [`MAX_PIECE`]) of the file the person gave by
    /// `handle`, from `offset`: fewer than were asked for is the end of the file.
    ///
    /// # Errors
    ///
    /// [`FileError::Refused`] with the terminal's code (`handle`, `range`, `busy`, `gone`, `io`),
    /// and as for `attach`.
    pub fn read(&self, handle: &str, offset: u64, length: usize) -> Result<Vec<u8>, FileError> {
        self.start_read(handle, offset, length)
            .recv()
            .unwrap_or(Err(FileError::Ended))
    }

    fn start_read(
        &self,
        handle: &str,
        offset: u64,
        length: usize,
    ) -> Receiver<Result<Vec<u8>, FileError>> {
        let handle = handle.to_owned();
        let length = length.min(MAX_PIECE) as u64;
        self.ask(|reply| Request::Read {
            handle,
            offset,
            length,
            reply,
        })
    }

    /// Writes `bytes` (at most [`MAX_PIECE`]) at `offset` of the place the person chose to save in
    /// by `handle`; `end` when they are the file's last, which waits until the terminal has saved
    /// the file. The writes of a handle go in order with no gap; one that is not the last does not
    /// wait, and a failure the terminal has told of is the answer of the next write.
    ///
    /// # Errors
    ///
    /// [`FileError::Refused`] with the terminal's code (`handle`, `order`, `size`, `space`, `io`),
    /// and as for `attach`.
    pub fn write(
        &self,
        handle: &str,
        offset: u64,
        bytes: Vec<u8>,
        end: bool,
    ) -> Result<(), FileError> {
        let handle = handle.to_owned();
        self.ask(|reply| Request::Write {
            handle,
            offset,
            bytes,
            end,
            reply,
        })
        .recv()
        .unwrap_or(Err(FileError::Ended))
    }

    /// Receives the file the person gave by `handle` into `to`, with two reads out at a time, and
    /// returns its length. The bytes go to a part beside `to` that is given `to`'s name when the
    /// last of them is in; `progress` is told how many bytes are in so far and returns `false` to
    /// give the receiving up. A part that is not finished — given up, the view ended, the program
    /// asked to end — is deleted. Make `to` of [`safe_name`] of the name the view passed on.
    ///
    /// # Errors
    ///
    /// As `read`, [`FileError::Ended`] when given up, and [`FileError::Io`] when the part cannot
    /// be written.
    pub fn receive(
        &self,
        handle: &str,
        to: &Path,
        mut progress: impl FnMut(u64) -> bool,
    ) -> Result<u64, FileError> {
        self.receive_proven(self.stage_proof(), handle, to, &mut progress)
    }

    /// `receive`, given what the program makes of the terminal's folder. With one that is proved
    /// the bytes cross through a pipe in it (§8.11); they cross in messages where it is not, or
    /// where the pipe stops short, from where the part ends.
    fn receive_proven(
        &self,
        proof: Proof,
        handle: &str,
        to: &Path,
        progress: &mut impl FnMut(u64) -> bool,
    ) -> Result<u64, FileError> {
        let mut part = Part::create(to)?;
        if let Proof::Proven(folder) = proof
            && let Went::Whole(total) = staged::receive(self, &folder, handle, &mut part, progress)?
        {
            part.keep(to)?;
            return Ok(total);
        }
        let mut out = std::collections::VecDeque::new();
        let mut asked: u64 = part.len();
        let mut received: u64 = part.len();
        let mut ended = false;
        let piece = MAX_PIECE as u64;
        loop {
            while !ended && out.len() < READS_OUT {
                out.push_back(self.start_read(handle, asked, MAX_PIECE));
                asked += piece;
            }
            let Some(next) = out.pop_front() else {
                break;
            };
            let bytes = next.recv().unwrap_or(Err(FileError::Ended))?;
            part.push(&bytes)?;
            received += bytes.len() as u64;
            if (bytes.len() as u64) < piece {
                // The end of the file: what else is out is past it.
                ended = true;
                out.clear();
            }
            if !progress(received) {
                return Err(FileError::Ended);
            }
        }
        part.keep(to)?;
        Ok(received)
    }

    /// Receives the file the person gave by `handle` into `to` as [`Self::receive`] does, but
    /// copied from `path` when the file there is the one the view was handed: a terminal hands a
    /// view the path of a file only where the program reads the same files as the person
    /// (specification §8.9), and then not a byte of it need cross the terminal. `path` and `size`
    /// are what the view was handed with the file and passed on. A path that is not absolute or
    /// cannot be read, a file that is no longer `size` bytes, and a copy that fails are received
    /// by the handle instead.
    ///
    /// # Errors
    ///
    /// As `receive`.
    pub fn receive_from(
        &self,
        handle: &str,
        path: &Path,
        size: u64,
        to: &Path,
        mut progress: impl FnMut(u64) -> bool,
    ) -> Result<u64, FileError> {
        let proof = self.stage_proof();
        // A terminal that offers a folder the program cannot prove is the terminal of another
        // machine or account: the path it handed over is not this machine's.
        if !matches!(proof, Proof::Unproven)
            && let Some(copied) = copy_given(path, size, to, &mut progress)
        {
            return copied;
        }
        self.receive_proven(proof, handle, to, &mut progress)
    }

    /// Sends the file at `from` to the place the person chose to save in by `handle`, in pieces of
    /// [`MAX_PIECE`], and returns its length once the terminal has saved it; `progress` is told how
    /// many bytes are out so far.
    ///
    /// # Errors
    ///
    /// As `write`, and [`FileError::Io`] when `from` cannot be read.
    pub fn send(
        &self,
        handle: &str,
        from: &Path,
        mut progress: impl FnMut(u64),
    ) -> Result<u64, FileError> {
        let mut file = fs::File::open(from)?;
        let mut offset: u64 = 0;
        // With a folder the terminal proved, the bytes cross through a pipe in it (§8.11), and in
        // messages from where the pipe stops short.
        if let Proof::Proven(folder) = self.stage_proof() {
            match staged::send(self, &folder, handle, &mut file, &mut progress)? {
                Went::Whole(total) => return Ok(total),
                Went::Rest { from } => {
                    file.seek(SeekFrom::Start(from))?;
                    offset = from;
                }
            }
        }
        let mut piece = vec![0; MAX_PIECE];
        let mut next = read_piece(&mut file, &mut piece)?;
        loop {
            let bytes = piece.get(..next).unwrap_or_default().to_vec();
            let mut ahead = vec![0; MAX_PIECE];
            let after = if next == MAX_PIECE {
                read_piece(&mut file, &mut ahead)?
            } else {
                0
            };
            let end = after == 0;
            let length = bytes.len() as u64;
            self.write(handle, offset, bytes, end)?;
            offset += length;
            progress(offset);
            if end {
                return Ok(offset);
            }
            piece = ahead;
            next = after;
        }
    }
}

/// Clones, where the file system can, or else copies the `size` bytes of the file at `path` to a part
/// beside `to`, which is given `to`'s name at the end, telling `progress` as `receive` does. `None` where the file is not there to be
/// copied whole (see [`ViewControl::receive_from`]): the part is deleted, and the caller receives
/// the file by its handle.
fn copy_given(
    path: &Path,
    size: u64,
    to: &Path,
    progress: &mut impl FnMut(u64) -> bool,
) -> Option<Result<u64, FileError>> {
    if !path.is_absolute() {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() != size {
        return None;
    }
    // A file system that can clone does not copy a byte.
    if let Some(part) = Part::cloned(to, path, size) {
        if !progress(size) {
            return Some(Err(FileError::Ended));
        }
        part.keep(to).ok()?;
        return Some(Ok(size));
    }
    let mut part = Part::create(to).ok()?;
    let mut piece = vec![0; MAX_PIECE];
    let mut copied: u64 = 0;
    while copied < size {
        let filled = read_piece(&mut file, &mut piece).ok()?;
        // Shorter than it was a moment ago: not the file the view was handed.
        let wanted = usize::try_from(size - copied).unwrap_or(usize::MAX);
        let bytes = piece
            .get(..filled.min(wanted))
            .filter(|bytes| !bytes.is_empty())?;
        part.push(bytes).ok()?;
        copied += bytes.len() as u64;
        if !progress(copied) {
            return Some(Err(FileError::Ended));
        }
    }
    part.keep(to).ok()?;
    Some(Ok(copied))
}

/// Fills `piece` from `file` as far as it goes; how much it holds.
pub(crate) fn read_piece(file: &mut fs::File, piece: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while let Some(rest) = piece.get_mut(filled..).filter(|rest| !rest.is_empty()) {
        match file.read(rest) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(why) => return Err(why),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_the_person_gave_is_its_last_part_and_nothing_a_path_could_be_made_of() {
        assert_eq!(safe_name("photo.png"), "photo.png");
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Users\\a\\b.txt"), "b.txt");
        assert_eq!(safe_name("a\u{0}b\nc"), "abc");
        for nothing in ["", ".", "..", "dir/", " "] {
            assert_eq!(safe_name(nothing), "file", "{nothing:?}");
        }
    }

    #[test]
    fn a_part_is_written_anew_and_the_way_out_deletes_one_its_helper_did_not() {
        let dir = std::env::temp_dir().join(format!("pencere-parts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        let to = dir.join("a.txt");
        let part = dir.join(".a.txt.pencere-part");
        // A part left by a process that was killed is replaced, and a link there is not followed.
        let elsewhere = dir.join("elsewhere");
        fs::write(&elsewhere, b"kept").expect("a file");
        std::os::unix::fs::symlink(&elsewhere, &part).expect("a link");
        let mut kept = Part::create(&to).expect("a part");
        kept.file.write_all(b"new").expect("written");
        assert_eq!(fs::read(&elsewhere).expect("the file"), b"kept");
        // The terminal is given back while the helper still holds it: the part goes.
        discard_parts();
        assert!(!part.exists(), "the part is left");
        std::mem::forget(kept);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_third_read_waits_for_one_of_the_two_out_to_be_answered() {
        use pencere_wire::frame::{Direction, Event, Outgoing, Split, StreamDecoder, encode};
        use std::os::fd::AsRawFd;
        let (_read, write) = std::os::unix::net::UnixStream::pair().expect("a pair of sockets");
        let mut writer = Writer::new(write.as_raw_fd());
        let mut files = Files::default();
        let mut answers = Vec::new();
        for offset in 0..3 {
            let (reply, answer) = sync_channel(1);
            answers.push(answer);
            files.on_request(
                Request::Read {
                    handle: "0".repeat(32),
                    offset,
                    length: 1,
                    reply,
                },
                true,
                &mut writer,
            );
        }
        assert_eq!((files.reads.len(), files.queued.len()), (2, 1));
        // The first is answered: the third goes out.
        let first = *files.reads.keys().min().expect("a read");
        let mut bytes = Vec::new();
        encode(
            Direction::ToProgram,
            &Outgoing {
                kind: "data",
                id: 1,
                re: Some(first),
                digest: None,
                file: FileParams::NONE,
                payload: "YQ==",
            },
            Split::Fill,
            &mut bytes,
        )
        .expect("a frame");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToProgram).feed(&bytes, &mut events);
        let Some(Event::Message(data)) = events.pop() else {
            panic!("no message");
        };
        files.on_data(&data, &mut writer);
        assert_eq!((files.reads.len(), files.queued.len()), (2, 0));
        assert_eq!(
            answers[0].try_recv().ok().and_then(Result::ok),
            Some(b"a".to_vec())
        );
    }

    /// A message of the terminal, as the loop reads it.
    fn from_terminal(kind: &str, id: u32, re: u32, payload: &str) -> Frame {
        use pencere_wire::frame::{Direction, Event, Outgoing, Split, StreamDecoder, encode};
        let mut bytes = Vec::new();
        encode(
            Direction::ToProgram,
            &Outgoing {
                kind,
                id,
                re: Some(re),
                digest: None,
                file: FileParams::NONE,
                payload,
            },
            Split::Fill,
            &mut bytes,
        )
        .expect("a frame");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToProgram).feed(&bytes, &mut events);
        let Some(Event::Message(message)) = events.pop() else {
            panic!("no message");
        };
        message
    }

    #[test]
    fn a_transfer_is_told_to_its_helper_until_its_last_answer_and_a_stranger_is_not() {
        use std::os::fd::AsRawFd;
        let (_read, write) = std::os::unix::net::UnixStream::pair().expect("a pair of sockets");
        let mut writer = Writer::new(write.as_raw_fd());
        let mut files = Files::default();
        let name = "0b5d7c1e9a3f4682d0e1c7a95b3f8d24";
        let (events, heard) = std::sync::mpsc::channel();
        files.on_request(
            Request::Stage {
                handle: "0".repeat(32),
                offset: 0,
                events,
            },
            true,
            &mut writer,
        );
        let Ok(Transferring::Id(id)) = heard.try_recv() else {
            panic!("the id");
        };
        // A `fetched` is not the answer to a `stage`, and an id that is not out is nobody's.
        files.on_transfer(
            &from_terminal("fetched", 1, id, &format!(r#"{{"pipe":"{name}"}}"#)),
            true,
            &mut writer,
        );
        files.on_transfer(
            &from_terminal("staged", 2, id + 7, &format!(r#"{{"pipe":"{name}"}}"#)),
            false,
            &mut writer,
        );
        assert!(heard.try_recv().is_err());
        files.on_transfer(
            &from_terminal("staged", 3, id, &format!(r#"{{"pipe":"{name}"}}"#)),
            false,
            &mut writer,
        );
        assert!(matches!(
            heard.try_recv(),
            Ok(Transferring::Told(Transfer::Pipe(told))) if told == name
        ));
        assert_eq!(files.transfers.len(), 1);
        files.on_transfer(
            &from_terminal("staged", 4, id, r#"{"sent":11}"#),
            false,
            &mut writer,
        );
        assert!(matches!(
            heard.try_recv(),
            Ok(Transferring::Told(Transfer::Done(11)))
        ));
        assert!(files.transfers.is_empty());

        // Taken back, a transfer is told no more; one that is not out is nothing to take back.
        let (events, heard) = std::sync::mpsc::channel();
        files.on_request(
            Request::Fetch {
                handle: "0".repeat(32),
                size: 5,
                offset: 0,
                events,
            },
            true,
            &mut writer,
        );
        let Ok(Transferring::Id(id)) = heard.try_recv() else {
            panic!("the id");
        };
        files.on_request(Request::Unstage { id }, true, &mut writer);
        files.on_request(Request::Unstage { id }, true, &mut writer);
        // It stays out until the host's last word: `cancelled`, or the end it had reached already.
        assert_eq!(files.transfers.len(), 1);
        files.on_transfer(
            &from_terminal("fetched", 5, id, r#"{"code":"cancelled"}"#),
            true,
            &mut writer,
        );
        assert!(matches!(
            heard.try_recv(),
            Ok(Transferring::Told(Transfer::Failed { code, .. })) if code == "cancelled"
        ));
        assert!(files.transfers.is_empty());

        // No view that has files: nothing is sent and the helper's way is dropped.
        let (events, heard) = std::sync::mpsc::channel();
        files.on_request(
            Request::Stage {
                handle: "0".repeat(32),
                offset: 0,
                events,
            },
            false,
            &mut writer,
        );
        assert!(heard.recv().is_err());
        assert!(files.transfers.is_empty());
    }

    #[test]
    fn the_terminals_folder_outlives_its_view_and_the_view_s_transfers_do_not() {
        use std::os::fd::AsRawFd;
        let (_read, write) = std::os::unix::net::UnixStream::pair().expect("a pair of sockets");
        let mut writer = Writer::new(write.as_raw_fd());
        let mut files = Files::default();
        let offer = StageOffer {
            dir: "/a".to_owned(),
            token: "0b5d7c1e9a3f4682d0e1c7a95b3f8d24".to_owned(),
        };
        files.offer(Some(offer.clone()));
        let (events, heard) = std::sync::mpsc::channel();
        files.on_request(
            Request::Stage {
                handle: "0".repeat(32),
                offset: 0,
                events,
            },
            true,
            &mut writer,
        );
        assert!(matches!(heard.try_recv(), Ok(Transferring::Id(_))));
        files.forget();
        // A helper that waits is told the view ended, by its way being dropped.
        assert!(heard.recv().is_err());
        let (reply, answer) = sync_channel(1);
        files.on_request(Request::StageOffer { reply }, true, &mut writer);
        assert!(matches!(answer.recv(), Ok(Ok(Some(kept))) if kept == offer));
        let (reply, answer) = sync_channel(1);
        files.on_request(Request::StageOffer { reply }, false, &mut writer);
        assert!(matches!(answer.recv(), Ok(Err(FileError::Unsupported))));
    }

    #[test]
    fn a_file_given_by_its_path_is_copied_with_the_rights_of_the_receiver_not_the_givers() {
        let dir = std::env::temp_dir().join(format!("pencere-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        let from = dir.join("from.bin");
        let bytes: Vec<u8> = (0..300_000u32).map(|n| (n % 251) as u8).collect();
        fs::write(&from, &bytes).expect("a file");
        fs::set_permissions(&from, fs::Permissions::from_mode(0o400)).expect("a mode");
        let to = dir.join("to.bin");
        let mut told = Vec::new();
        let copied = copy_given(&from, bytes.len() as u64, &to, &mut |n| {
            told.push(n);
            true
        });
        assert!(matches!(copied, Some(Ok(n)) if n == bytes.len() as u64));
        assert_eq!(fs::read(&to).expect("the copy"), bytes);
        assert_eq!(told.last(), Some(&(bytes.len() as u64)));
        // A part made anew has the mode of a new file; the copy must not carry the read-only one.
        let fresh = dir.join("fresh");
        fs::write(&fresh, b"").expect("a file");
        let wanted = fs::metadata(&fresh).expect("meta").permissions().mode();
        assert_eq!(
            fs::metadata(&to).expect("meta").permissions().mode(),
            wanted
        );
        assert!(!to.with_file_name(".to.bin.pencere-part").exists());
        // A clone, where the file system makes one, is the same bytes and is not written through.
        if let Some(part) = Part::cloned(&dir.join("cloned.bin"), &from, bytes.len() as u64) {
            assert_eq!(part.len(), bytes.len() as u64);
            part.keep(&dir.join("cloned.bin")).expect("kept");
            assert_eq!(fs::read(dir.join("cloned.bin")).expect("the clone"), bytes);
            fs::set_permissions(dir.join("cloned.bin"), fs::Permissions::from_mode(0o600))
                .expect("a mode");
            fs::write(dir.join("cloned.bin"), b"changed").expect("written");
            assert_eq!(fs::read(&from).expect("the file"), bytes, "written through");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_versions_that_have_files_are_lua_2_1_and_wasm_3_and_later() {
        assert!(has_files(Kind::Lua, "2.1"));
        assert!(has_files(Kind::Lua, "2.10"));
        assert!(has_files(Kind::Lua, "3.0"));
        assert!(!has_files(Kind::Lua, "2.0"));
        assert!(has_files(Kind::Wasm, "3.0.0"));
        assert!(!has_files(Kind::Wasm, "2.9.9"));
        assert!(!has_files(Kind::Web, "9.0"));
        assert!(!has_files(Kind::Lua, "x"));
    }
}
