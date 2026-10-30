//! Files beside a view's bundle (specification §8.8 to §8.10): what a program attaches to the view
//! that runs, so that the view can draw it by its digest; what the person gives the view (a drop,
//! a paste, the Open panel), which the program reads by its handle; and the place the person
//! chooses in the Save panel, which the program writes to.
//!
//! The session rules on all of it and does none of the I/O, as it does no other: it keeps the
//! handles, counts what is attached and what is asked, and writes `need`, `data` and `wrote`. The
//! terminal reads and writes the person's files when the session asks it to
//! ([`Action::ReadFile`], [`Action::WriteFile`], [`Action::AbortWrite`]), shows the panels
//! ([`Action::Pick`], [`Action::Save`]) and hands back what came of it
//! ([`Session::file_read`], [`Session::file_written`], [`Session::panel_closed`]).
//!
//! A view's files are its own and die with it: a handle, an attached digest and a panel are
//! nothing once the view has ended, whatever ended it (§10.2). Only a view the terminal draws
//! itself, opened at a version of its interface that has them (`lua` 2.1, `wasm` 3.0), has them at
//! all (§5.5); every other view, a page among them, is answered as if it had none.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Action, Phase, Session, ViewKind};
use crate::base64;
use crate::bundle::{Digest, MAX_FILE_SIZE, MAX_FILES, MAX_TOTAL_SIZE, Received, Receiving, Store};
use crate::frame::{FileParams, Message, is_handle};
use crate::message::{self, Attach, Detach, Read, Refusal};
use crate::text;
use crate::token::Entropy;

/// The most bytes one `read` may ask for and one `write` may carry (specification §8.9): a larger
/// message would hold up what the view and the program say to each other behind it, since the
/// chunks of a message go out with nothing between them (§4.4).
pub const MAX_FILE_PIECE: u64 = 1024 * 1024;

/// The most `read`s of a program that may be out at once (specification §8.9); one more is
/// answered `busy`.
pub const MAX_OPEN_READS: usize = 2;

/// The most characters of the name a view proposes for a file it saves (§8.10).
const MAX_SAVE_NAME_CHARS: usize = 255;

/// The name an attached file has for the view that draws it: `sha256:` and its digest.
pub const ATTACHED_SCHEME: &str = "sha256:";

/// What went wrong with a file the terminal read or wrote, by the code the program is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileError {
    /// The file is not what it was when the person gave it (its size or the time it changed), or
    /// it cannot be read any more.
    Gone,
    /// The bytes asked for are not in the file.
    Range,
    /// The place the person chose to save in has no room.
    Space,
    /// Any other failure of reading or writing.
    Io,
}

impl FileError {
    /// The code the program is told (`e` of a `data`, `code` of a `wrote`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Gone => "gone",
            Self::Range => "range",
            Self::Space => "space",
            Self::Io => "io",
        }
    }
}

/// A file the person gave a view (a drop, a paste, a pick in the Open panel), as the terminal
/// grants it a handle (specification §8.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGrant {
    /// Its length in bytes, when the person gave it.
    pub size: u64,
}

/// What a handle is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    /// A file the person gave, which the program reads.
    Read { size: u64 },
    /// A place the person chose to save in, which the program writes to.
    Save,
}

/// Where a place to save stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Saving {
    /// Nothing has been written, or the writes so far went well; the next goes at `next`.
    Open { next: u64, written: bool },
    /// The last write came with `end`; its result is awaited or has come.
    Ended,
    /// A write failed and the program has been told; the rest are ignored.
    Failed,
}

struct Handle {
    usage: Use,
    saving: Saving,
}

/// An `attach` whose files are on their way.
struct Pending {
    /// The id of the `attach`, for the `re` of its second `need`.
    attach: u32,
    receiving: Receiving,
    /// The digests asked for, with the bytes reserved for each.
    asked: Vec<(Digest, u64)>,
    refused: Vec<Refusal>,
}

/// A view's files. Empty, and refusing everything, for a view that has none.
pub(super) struct Files {
    /// Whether the view was opened at a version that has them.
    capable: bool,
    attached: HashMap<Digest, Arc<[u8]>>,
    attached_bytes: u64,
    pending: Vec<Pending>,
    handles: HashMap<String, Handle>,
    /// The `read`s out, by id, with the length each asked for.
    reads: Vec<(u32, u64)>,
    /// The `write`s whose result the terminal has not given, by id, with their handle and
    /// whether each is the last of its file.
    writes: Vec<(u32, String, bool)>,
    /// The panel that is open, by its id.
    panel: Option<u32>,
}

impl Files {
    /// The files of a view of `kind` opened at `version` of its interface.
    pub(super) fn new(kind: ViewKind, version: &str) -> Self {
        Self {
            capable: has_files(kind, version),
            attached: HashMap::new(),
            attached_bytes: 0,
            pending: Vec::new(),
            handles: HashMap::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            panel: None,
        }
    }

    /// The bytes and count attached or on their way, which the limits of §8.8 hold.
    fn load(&self) -> (usize, u64) {
        let reserved: u64 = self
            .pending
            .iter()
            .flat_map(|p| p.asked.iter())
            .map(|(_, size)| *size)
            .sum();
        let count = self.attached.len() + self.pending.iter().map(|p| p.asked.len()).sum::<usize>();
        (count, self.attached_bytes.saturating_add(reserved))
    }

    /// The places to save that were left unfinished, which the terminal is to throw away.
    pub(super) fn unfinished_saves(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .handles
            .iter()
            .filter(|(_, h)| h.usage == Use::Save && h.saving != Saving::Ended)
            .filter(|(_, h)| matches!(h.saving, Saving::Open { written: true, .. }))
            .map(|(name, _)| name.clone())
            .collect();
        out.sort();
        out
    }
}

/// Whether a view of `kind` at `version` has files beside its bundle: a view the terminal draws
/// itself at `lua` 2.1 and later, or `wasm` 3.0 and later (specification §5.5).
pub fn has_files(kind: ViewKind, version: &str) -> bool {
    let Some((major, minor)) = version.split_once('.') else {
        return false;
    };
    let (Ok(major), Ok(minor)) = (major.parse::<u32>(), minor.parse::<u32>()) else {
        return false;
    };
    match kind {
        ViewKind::Lua => (major, minor) >= (2, 1),
        ViewKind::Wasm => major >= 3,
        ViewKind::Web => false,
    }
}

/// The digest an attached file's name stands for, if it is one: `sha256:` and 64 lowercase hex
/// digits.
pub fn attached_digest(name: &str) -> Option<Digest> {
    name.strip_prefix(ATTACHED_SCHEME).and_then(Digest::parse)
}

impl<S: Store, E: Entropy> Session<S, E> {
    fn files(&self) -> Option<&Files> {
        match &self.phase {
            Phase::Live(live) if live.files.capable => Some(&live.files),
            _ => None,
        }
    }

    fn files_mut(&mut self) -> Option<&mut Files> {
        match &mut self.phase {
            Phase::Live(live) if live.files.capable => Some(&mut live.files),
            _ => None,
        }
    }

    // ---- attaching (§8.8) -------------------------------------------------

    pub(super) fn on_attach(&mut self, message: &Message) {
        let Some(attach) = crate::json::from_object::<Attach>(&message.payload) else {
            self.ignored();
            return;
        };
        // Rows whose digest is not one are nobody's: there is nothing to refuse them by.
        let mut rows: Vec<(Digest, u64)> = Vec::new();
        for file in attach.files {
            match Digest::parse(&file.0.sha256) {
                Some(digest) if !rows.iter().any(|(d, _)| *d == digest) => {
                    rows.push((digest, file.0.size));
                }
                _ => self.diagnostics.ignored = self.diagnostics.ignored.saturating_add(1),
            }
        }
        let Some(files) = self.files() else {
            let refused: Vec<Refusal> = rows
                .iter()
                .map(|(digest, _)| Refusal {
                    sha256: digest.to_string(),
                    code: "view",
                })
                .collect();
            self.send(
                "need",
                Some(message.id),
                &message::need_refusing(&[], &refused),
            );
            return;
        };
        let (mut count, mut bytes) = files.load();
        let mut missing: Vec<(Digest, u64)> = Vec::new();
        let mut held: Vec<Digest> = Vec::new();
        let mut refused: Vec<Refusal> = Vec::new();
        for (digest, size) in rows {
            if files.attached.contains_key(&digest) {
                continue;
            }
            if size > MAX_FILE_SIZE
                || count >= MAX_FILES
                || bytes.saturating_add(size) > MAX_TOTAL_SIZE
            {
                refused.push(Refusal {
                    sha256: digest.to_string(),
                    code: "limit",
                });
                continue;
            }
            count += 1;
            bytes = bytes.saturating_add(size);
            if self.store.contains(&digest) {
                held.push(digest);
            } else {
                missing.push((digest, size));
            }
        }
        // What the store holds is attached at once; a store may drop an entry at any time
        // (§8.5), and one that is gone is asked for like any other.
        for digest in held {
            match self.store.get(&digest) {
                Some(file) => self.pin(digest, file),
                None => missing.push((digest, MAX_FILE_SIZE)),
            }
        }
        let names: Vec<String> = missing.iter().map(|(d, _)| d.to_string()).collect();
        if !missing.is_empty()
            && let Some(files) = self.files_mut()
        {
            files.pending.push(Pending {
                attach: message.id,
                receiving: Receiving::from_sizes(&missing),
                asked: missing,
                refused: Vec::new(),
            });
        }
        self.send(
            "need",
            Some(message.id),
            &message::need_refusing(&names, &refused),
        );
    }

    fn pin(&mut self, digest: Digest, file: Arc<[u8]>) {
        if let Some(files) = self.files_mut()
            && !files.attached.contains_key(&digest)
        {
            let len = u64::try_from(file.len()).unwrap_or(u64::MAX);
            files.attached_bytes = files.attached_bytes.saturating_add(len);
            files.attached.insert(digest, file);
        }
    }

    /// A `file` while the view runs: one of the files of an `attach` (§8.8). Returns whether it
    /// was one; a file nothing waits for is ignored by the caller.
    pub(super) fn attached_file(&mut self, digest: &Digest, payload: &str) -> bool {
        let Some(files) = self.files_mut() else {
            return false;
        };
        let Some(index) = files
            .pending
            .iter()
            .position(|p| p.receiving.awaits(digest))
        else {
            return false;
        };
        let Some(pending) = files.pending.get_mut(index) else {
            return false;
        };
        match pending.receiving.accept(digest, payload) {
            Received::Ignored => return false,
            Received::Failed(why) => {
                pending.receiving.give_up(digest);
                pending.asked.retain(|(d, _)| d != digest);
                pending.refused.push(Refusal {
                    sha256: digest.to_string(),
                    code: why.code(),
                });
            }
            Received::Verified(file) => {
                pending.asked.retain(|(d, _)| d != digest);
                let bytes: Arc<[u8]> = Arc::from(file.bytes());
                self.store.insert(file);
                self.pin(*digest, bytes);
                // Another attach may wait for the same file: it has come for both.
                if let Some(files) = self.files_mut() {
                    for other in &mut files.pending {
                        if other.receiving.awaits(digest) {
                            other.receiving.give_up(digest);
                            other.asked.retain(|(d, _)| d != digest);
                        }
                    }
                }
            }
        }
        self.attaches_complete();
        true
    }

    /// Answers each `attach` whose files have all come, with its second `need`.
    fn attaches_complete(&mut self) {
        let Some(files) = self.files_mut() else {
            return;
        };
        let (done, waiting): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut files.pending)
            .into_iter()
            .partition(|p| p.receiving.is_complete());
        files.pending = waiting;
        for pending in done {
            self.send(
                "need",
                Some(pending.attach),
                &message::need_refusing(&[], &pending.refused),
            );
        }
    }

    pub(super) fn on_detach(&mut self, message: &Message) {
        let Some(detach) = crate::json::from_object::<Detach>(&message.payload) else {
            self.ignored();
            return;
        };
        let Some(files) = self.files_mut() else {
            self.ignored();
            return;
        };
        let mut forgotten = Vec::new();
        for text in detach.files {
            let Some(digest) = Digest::parse(&text) else {
                continue;
            };
            if let Some(file) = files.attached.remove(&digest) {
                let len = u64::try_from(file.len()).unwrap_or(u64::MAX);
                files.attached_bytes = files.attached_bytes.saturating_sub(len);
                forgotten.push(format!("{ATTACHED_SCHEME}{digest}"));
            }
        }
        if !forgotten.is_empty() {
            self.actions.push(Action::ForgetFiles(forgotten));
        }
    }

    /// The bytes of the file a program attached to the view that runs, by the name the view draws
    /// it by (`sha256:` and its digest). Nothing else answers: a digest the program did not attach
    /// to this view is no file of the view's, whatever the store holds (§8.8).
    pub fn attached(&self, name: &str) -> Option<Arc<[u8]>> {
        let digest = attached_digest(name)?;
        self.files()?.attached.get(&digest).cloned()
    }

    // ---- what the person gives (§8.9) -------------------------------------

    /// The person gave the view that runs a file (a drop, a paste, a pick in the Open panel): the
    /// session makes it a handle, which the terminal hands the view with the file's name, size and
    /// type, and by which the program reads it. `None` when no view that has files runs.
    ///
    /// The handle is 128 bits of the terminal's randomness, written as 32 lowercase hex digits,
    /// and is nothing once the view has ended.
    pub fn grant_file(&mut self, grant: &FileGrant) -> Option<String> {
        self.files()?;
        let handle = hex(&self.entropy.token_bytes());
        let files = self.files_mut()?;
        files.handles.insert(
            handle.clone(),
            Handle {
                usage: Use::Read { size: grant.size },
                saving: Saving::Open {
                    next: 0,
                    written: false,
                },
            },
        );
        Some(handle)
    }

    /// The person chose a place to save in, in the Save panel the view asked for: the session
    /// makes it a handle, which the terminal hands the view with the name the person chose, and to
    /// which the program writes. `None` when no view that has files runs.
    pub fn grant_save(&mut self) -> Option<String> {
        self.files()?;
        let handle = hex(&self.entropy.token_bytes());
        let files = self.files_mut()?;
        files.handles.insert(
            handle.clone(),
            Handle {
                usage: Use::Save,
                saving: Saving::Open {
                    next: 0,
                    written: false,
                },
            },
        );
        Some(handle)
    }

    /// Whether `handle` is one of a file the person gave the view that runs, which the terminal
    /// may draw for the view by its name (`file:` and the handle).
    pub fn holds_file(&self, handle: &str) -> bool {
        self.files()
            .and_then(|files| files.handles.get(handle))
            .is_some_and(|h| matches!(h.usage, Use::Read { .. }))
    }

    pub(super) fn on_read(&mut self, message: &Message) {
        let Some(read) = crate::json::from_object::<Read>(&message.payload) else {
            self.ignored();
            return;
        };
        let refusal = match self.files() {
            None => Some("handle"),
            Some(files) => match files.handles.get(&read.handle) {
                Some(Handle {
                    usage: Use::Read { size },
                    ..
                }) if is_handle(&read.handle) => {
                    if read.length == 0 || read.length > MAX_FILE_PIECE || read.offset > *size {
                        Some("range")
                    } else if files.reads.len() >= MAX_OPEN_READS {
                        Some("busy")
                    } else {
                        None
                    }
                }
                _ => Some("handle"),
            },
        };
        if let Some(code) = refusal {
            self.data(message.id, Err(code));
            return;
        }
        if let Some(files) = self.files_mut() {
            files.reads.push((message.id, read.length));
        }
        self.actions.push(Action::ReadFile {
            read: message.id,
            handle: read.handle,
            offset: read.offset,
            length: read.length,
        });
    }

    /// What came of the [`Action::ReadFile`] whose `read` this is: the bytes, at most as many as
    /// it asked for (fewer when the file ends first), or what went wrong. A result for a read that
    /// is not out (its view has ended) is ignored.
    pub fn file_read(&mut self, read: u32, result: Result<Vec<u8>, FileError>) {
        let Some(files) = self.files_mut() else {
            self.ignored();
            return;
        };
        let Some(index) = files.reads.iter().position(|(id, _)| *id == read) else {
            self.ignored();
            return;
        };
        let (_, asked) = files.reads.remove(index);
        match result {
            Ok(mut bytes) => {
                bytes.truncate(usize::try_from(asked).unwrap_or(usize::MAX));
                self.data(read, Ok(&bytes));
            }
            Err(error) => self.data(read, Err(error.code())),
        }
    }

    /// Writes the `data` that answers the `read` `re`: the bytes in base64, or an empty payload
    /// and the code. It is what the view brings the program and is held, and dropped at a signal
    /// character, as the view's words are (§4.8).
    fn data(&mut self, re: u32, result: Result<&[u8], &str>) {
        match result {
            Ok(bytes) => {
                let payload = base64::encode(bytes);
                self.queue_with("data", Some(re), FileParams::NONE, &payload, true);
            }
            Err(code) => {
                let params = FileParams {
                    error: Some(code),
                    ..FileParams::NONE
                };
                self.queue_with("data", Some(re), params, "", true);
            }
        }
    }

    // ---- saving (§8.10) ----------------------------------------------------

    pub(super) fn on_write(&mut self, message: &Message) {
        let id = message.id;
        let Some(handle) = message.handle().map(str::to_owned) else {
            self.ignored();
            return;
        };
        let state = self
            .files()
            .and_then(|files| files.handles.get(&handle))
            .filter(|h| h.usage == Use::Save)
            .map(|h| h.saving);
        let next = match state {
            None => return self.wrote(id, Some("handle")),
            // The program has been told; it stops, and what it sent meanwhile is nothing.
            Some(Saving::Failed | Saving::Ended) => return self.ignored(),
            Some(Saving::Open { next, .. }) => next,
        };
        let bytes = if message.payload.len() > max_base64(MAX_FILE_PIECE) {
            Err("size")
        } else if message.offset() != Some(next) {
            Err("order")
        } else {
            base64::decode(message.payload.as_bytes()).ok_or("order")
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(code) => {
                self.save_failed(&handle);
                return self.wrote(id, Some(code));
            }
        };
        let end = message.end();
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if let Some(h) = self.files_mut().and_then(|f| f.handles.get_mut(&handle)) {
            h.saving = if end {
                Saving::Ended
            } else {
                Saving::Open {
                    next: next.saturating_add(len),
                    written: true,
                }
            };
        }
        if let Some(files) = self.files_mut() {
            files.writes.push((id, handle.clone(), end));
        }
        self.actions.push(Action::WriteFile {
            write: id,
            handle,
            offset: next,
            bytes,
            end,
        });
    }

    /// What came of the [`Action::WriteFile`] whose `write` this is. The program is told of the
    /// end of its file and of the first thing that went wrong, and of nothing else.
    pub fn file_written(&mut self, write: u32, result: Result<(), FileError>) {
        let Some(files) = self.files_mut() else {
            self.ignored();
            return;
        };
        let Some(index) = files.writes.iter().position(|(id, ..)| *id == write) else {
            self.ignored();
            return;
        };
        let (_, handle, end) = files.writes.remove(index);
        let failed = files
            .handles
            .get(&handle)
            .is_some_and(|h| h.saving == Saving::Failed);
        match result {
            Err(_) if failed => {}
            Err(error) => {
                self.save_failed(&handle);
                self.wrote(write, Some(error.code()));
            }
            Ok(()) if end && !failed => self.wrote(write, None),
            Ok(()) => {}
        }
    }

    fn save_failed(&mut self, handle: &str) {
        if let Some(h) = self.files_mut().and_then(|f| f.handles.get_mut(handle)) {
            h.saving = Saving::Failed;
        }
        self.actions.push(Action::AbortWrite {
            handle: handle.to_owned(),
        });
    }

    fn wrote(&mut self, re: u32, code: Option<&str>) {
        self.send("wrote", Some(re), &message::wrote(code));
    }

    // ---- the panels (§8.9, §8.10) -----------------------------------------

    /// A view the terminal draws itself asks for the Open panel, in what it answered something the
    /// person did (the kit holds the panels to that, `kit/SPEC.md`). One panel is open at a time:
    /// asked for while one is, while no view that has files runs, it is dropped and counted as
    /// ignored. The terminal shows it ([`Action::Pick`]) and calls
    /// [`panel_closed`](Self::panel_closed) when the person has answered.
    pub fn module_pick(&mut self, multiple: bool) {
        if let Some(panel) = self.open_panel() {
            self.actions.push(Action::Pick { panel, multiple });
        }
    }

    /// A view the terminal draws itself asks for the Save panel, with the name it proposes (cut to
    /// 255 characters and cleaned of what a terminal would act on, as a title is) and its media
    /// type. Held to the rule of [`module_pick`](Self::module_pick).
    pub fn module_save(&mut self, name: &str, media_type: Option<&str>) {
        let name = text::words(name, MAX_SAVE_NAME_CHARS).unwrap_or_default();
        let media_type = media_type.map(str::to_owned);
        if let Some(panel) = self.open_panel() {
            self.actions.push(Action::Save {
                panel,
                name,
                media_type,
            });
        }
    }

    fn open_panel(&mut self) -> Option<u32> {
        let free = self.files().is_some_and(|files| files.panel.is_none());
        if !free {
            self.ignored();
            return None;
        }
        self.panels = self.panels.wrapping_add(1).max(1);
        let panel = self.panels;
        if let Some(files) = self.files_mut() {
            files.panel = Some(panel);
        }
        Some(panel)
    }

    /// The panel [`Action::Pick`] or [`Action::Save`] showed is gone: the person answered or
    /// turned it away. Another may be asked for.
    pub fn panel_closed(&mut self, panel: u32) {
        if let Some(files) = self.files_mut()
            && files.panel == Some(panel)
        {
            files.panel = None;
        }
    }
}

/// The longest base64 text of `bytes` bytes.
fn max_base64(bytes: u64) -> usize {
    usize::try_from(bytes.div_ceil(3).saturating_mul(4)).unwrap_or(usize::MAX)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
