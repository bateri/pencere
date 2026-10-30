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

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use pencere_wire::base64;
use pencere_wire::bundle::Digest;
use pencere_wire::frame::{FileParams, Message as Frame};

use crate::Kind;
use crate::control::{Request, ViewControl};
use crate::handlers::Work;
use crate::link::Writer;
use crate::payload;

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

/// What is known of a place to save in while its writes go out.
#[derive(Default)]
struct Saving {
    /// The terminal said the file failed, with its code: later writes are not sent.
    failed: Option<String>,
    /// The write with `end` is out, and waits for the `wrote`.
    last: Option<SyncSender<Result<(), FileError>>>,
}

/// The loop's side: what the view that runs has attached, and the answers the helpers wait for.
/// Everything is the view's and goes with it ([`Files::forget`]): a helper that waits is told the
/// view ended.
#[derive(Default)]
pub(crate) struct Files {
    attached: HashSet<Digest>,
    attaches: HashMap<u32, Attaching>,
    reads: HashMap<u32, SyncSender<Result<Vec<u8>, FileError>>>,
    /// The handle each write that is out was for.
    writes: HashMap<u32, String>,
    saves: HashMap<String, Saving>,
}

impl Files {
    /// The view is over: what it had attached is no longer, and every helper that waits is told so
    /// (its answer's way is dropped).
    pub(crate) fn forget(&mut self) {
        *self = Self::default();
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
                match writer.message("read", None, None, &payload::read(&handle, offset, length)) {
                    Ok(id) => {
                        self.reads.insert(id, reply);
                    }
                    Err(_) => {
                        let _ = reply.send(Err(FileError::Ended));
                    }
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
            // The others are the loop's own.
            _ => {}
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
    pub(crate) fn on_data(&mut self, message: &Frame) {
        let Some(reply) = message.re().and_then(|re| self.reads.remove(&re)) else {
            return;
        };
        let answer = match message.error_code() {
            Some(code) => Err(FileError::Refused(code.to_owned())),
            None => base64::decode(message.payload.as_bytes())
                .ok_or_else(|| FileError::Refused("io".to_owned())),
        };
        let _ = reply.send(answer);
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
struct Part {
    path: PathBuf,
    file: fs::File,
}

impl Part {
    fn create(to: &Path) -> io::Result<Self> {
        let name = to
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a place with no name"))?;
        let mut part_name = std::ffi::OsString::from(".");
        part_name.push(name);
        part_name.push(".pencere-part");
        let path = to.with_file_name(part_name);
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        parts().push(path.clone());
        Ok(Self { path, file })
    }

    /// Gives the part the name of its place.
    fn keep(self, to: &Path) -> io::Result<()> {
        self.file.sync_all()?;
        fs::rename(&self.path, to)?;
        parts().retain(|part| *part != self.path);
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        parts().retain(|part| *part != self.path);
    }
}

// ---- the helpers -----------------------------------------------------------------------

impl ViewControl {
    /// Hands the loop a request that is answered on `answer`; `Ended` when no loop runs.
    fn ask<T>(
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
        let mut part = Part::create(to)?;
        let mut out = std::collections::VecDeque::new();
        let mut asked: u64 = 0;
        let mut received: u64 = 0;
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
            part.file.write_all(&bytes)?;
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

/// Fills `piece` from `file` as far as it goes; how much it holds.
fn read_piece(file: &mut fs::File, piece: &mut [u8]) -> io::Result<usize> {
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
