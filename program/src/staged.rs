//! The fast way of a file's bytes (specification §8.11): a pipe in the folder the terminal keeps on
//! the program's machine, instead of `data` and `write` messages through the tty.
//!
//! What the program asks and what it is told stay messages (`stage`, `fetch`, `unstage`, `staged`,
//! `fetched`); only the bytes cross through the pipe. The program uses the folder only after it has
//! proved the folder is the terminal's ([`prove`]), and whenever the way does not work (the terminal
//! is busy, a pipe stays silent, the bytes stop) it takes the transfer back and goes on with the
//! messages of §8.9 and §8.10, from where the file stands: [`Went::Rest`].

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::control::{Request, ViewControl};
use crate::files::{FileError, Part, Transferring, read_piece};
use crate::handlers::Work;
use crate::payload::{self, StageOffer, Transfer};
use crate::pipes;

/// The file in the folder that holds the `token` of the `welcome` (§8.11).
const MARKER: &str = "marker";

/// How long the terminal is waited for to name the pipe.
const NAMED_WITHIN: Duration = Duration::from_secs(10);

/// How long a pipe may stay silent, or have no reader, before the program takes the transfer back
/// and goes on with messages (§8.11).
const FIRST_BYTE: Duration = Duration::from_secs(3);

/// How long bytes that were coming may stop before the program takes the transfer back.
const STALL: Duration = Duration::from_secs(8);

/// How long the terminal is waited for to say that a file it was sent is saved, once the last byte
/// is in the pipe: what is left to cross is what the pipe and the terminal's way hold.
const SAVED_WITHIN: Duration = Duration::from_secs(30);

/// How long the terminal is waited for to say why a pipe it was writing to has closed.
const WHY_WITHIN: Duration = Duration::from_secs(3);

/// How many times a transfer goes on after the way to the program's machine was lost (`route`).
const ROUTES: u32 = 3;

/// How many bytes are read or written at a time.
const PIECE: usize = 128 << 10;

/// Progress is told at least this often.
const PROGRESS_EVERY: u64 = 1 << 20;

/// How long to wait before looking again at a pipe nobody writes to, or that nobody reads.
const LOOK_AGAIN: Duration = Duration::from_millis(2);

/// What the program knows of the folder a terminal offers.
pub(crate) enum Proof {
    /// The terminal offered none, or no view that has files runs.
    Not,
    /// The terminal offered one and it is not the terminal's, as far as the program can tell:
    /// another machine, another account.
    Unproven,
    /// The folder is the terminal's.
    Proven(Folder),
}

/// How far a fast transfer went.
pub(crate) enum Went {
    /// The file is through, and this long.
    Whole(u64),
    /// The fast way is over and the file is not: the rest goes in messages, from this offset.
    Rest { from: u64 },
}

/// A folder that is the terminal's, proved.
pub(crate) struct Folder {
    dir: PathBuf,
}

/// Proves that the folder a `welcome` offers is the terminal's (§8.11): an absolute path, a
/// directory of the program's own user that the group and others cannot use, with a `marker` that
/// is a regular file, opened without following a link, holding exactly the `token`.
pub(crate) fn prove(offer: &StageOffer) -> Option<Folder> {
    let dir = Path::new(&offer.dir);
    if !dir.is_absolute() {
        return None;
    }
    let meta = fs::metadata(dir).ok()?;
    if !meta.is_dir() || meta.uid() != pipes::euid() || meta.mode() & 0o077 != 0 {
        return None;
    }
    // Not blocking, so that a `marker` that is a pipe is not waited on: it is not a regular file.
    let marker = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join(MARKER))
        .ok()?;
    if !marker.metadata().ok()?.is_file() {
        return None;
    }
    let mut held = Vec::new();
    marker
        .take(offer.token.len() as u64 + 1)
        .read_to_end(&mut held)
        .ok()?;
    (held == offer.token.as_bytes()).then(|| Folder {
        dir: dir.to_owned(),
    })
}

impl Folder {
    /// Opens the pipe the terminal named, for reading or for writing, without blocking and without
    /// following a link; it is the terminal's pipe only if it is a FIFO of the program's own user.
    fn pipe(&self, name: &str, for_write: bool) -> io::Result<fs::File> {
        if !payload::is_pipe_name(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a name the terminal gives a pipe",
            ));
        }
        let pipe = fs::OpenOptions::new()
            .read(!for_write)
            .write(for_write)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(self.dir.join(name))?;
        let meta = pipe.metadata()?;
        if !meta.file_type().is_fifo() || meta.uid() != pipes::euid() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "not a pipe of the program's user",
            ));
        }
        Ok(pipe)
    }
}

impl ViewControl {
    /// What the program makes of the folder the terminal offers, if it does.
    pub(crate) fn stage_proof(&self) -> Proof {
        match self.ask(|reply| Request::StageOffer { reply }).recv() {
            Ok(Ok(Some(offer))) => prove(&offer).map_or(Proof::Unproven, Proof::Proven),
            _ => Proof::Not,
        }
    }
}

// ---- a transfer that is out --------------------------------------------------------------

/// A `stage` or a `fetch` the loop has sent, and what the terminal says of it. A transfer the
/// terminal has not ended when this is dropped is taken back with an `unstage`, on every way out.
struct Out<'a> {
    control: &'a ViewControl,
    id: u32,
    inbox: Receiver<Transferring>,
    /// The terminal has given its last answer.
    over: bool,
}

/// What the loop has for a transfer, if anything.
enum Heard {
    Word(Transfer),
    Nothing,
    Gone,
}

impl<'a> Out<'a> {
    /// Hands the loop the `stage` or `fetch` that `request` makes, with the way to tell this
    /// helper of its answers, and waits to learn the id it was sent with.
    fn begin(
        control: &'a ViewControl,
        request: impl FnOnce(Sender<Transferring>) -> Request,
    ) -> Result<Self, FileError> {
        let (events, inbox) = channel();
        let outbox = control.outbox().ok_or(FileError::Unsupported)?;
        if !outbox.send(Work::Control(request(events))) {
            return Err(FileError::Ended);
        }
        match inbox.recv() {
            Ok(Transferring::Id(id)) => Ok(Self {
                control,
                id,
                inbox,
                over: false,
            }),
            // The loop dropped the request: no view that has files, or the view ended.
            _ => Err(FileError::Ended),
        }
    }

    fn listened(&mut self, got: Heard) -> Result<Option<Transfer>, FileError> {
        match got {
            Heard::Word(told) => {
                if !matches!(told, Transfer::Pipe(_)) {
                    self.over = true;
                }
                Ok(Some(told))
            }
            Heard::Nothing => Ok(None),
            // The loop let go of the transfer: after its last answer, as it does, or because the
            // view ended.
            Heard::Gone if self.over => Ok(None),
            Heard::Gone => Err(FileError::Ended),
        }
    }

    /// What the terminal has said since, without waiting.
    fn heard(&mut self) -> Result<Option<Transfer>, FileError> {
        let got = match self.inbox.try_recv() {
            Ok(Transferring::Told(told)) => Heard::Word(told),
            Ok(Transferring::Id(_)) | Err(TryRecvError::Empty) => Heard::Nothing,
            Err(TryRecvError::Disconnected) => Heard::Gone,
        };
        self.listened(got)
    }

    /// What the terminal says within `within`, a pipe's name aside.
    fn wait(&mut self, within: Duration) -> Result<Option<Transfer>, FileError> {
        let until = Instant::now() + within;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let got = match self.inbox.recv_timeout(left) {
                Ok(Transferring::Told(Transfer::Pipe(_)) | Transferring::Id(_)) => {
                    if left.is_zero() {
                        return Ok(None);
                    }
                    continue;
                }
                Ok(Transferring::Told(told)) => Heard::Word(told),
                Err(RecvTimeoutError::Timeout) => Heard::Nothing,
                Err(RecvTimeoutError::Disconnected) => Heard::Gone,
            };
            return self.listened(got);
        }
    }

    /// The terminal's naming of the pipe, or how the transfer ended instead; `None` when the
    /// terminal did not answer in time.
    fn pipe(&mut self) -> Result<Option<Transfer>, FileError> {
        let until = Instant::now() + NAMED_WITHIN;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let got = match self.inbox.recv_timeout(left) {
                Ok(Transferring::Id(_)) => {
                    if left.is_zero() {
                        return Ok(None);
                    }
                    continue;
                }
                Ok(Transferring::Told(told)) => Heard::Word(told),
                Err(RecvTimeoutError::Timeout) => Heard::Nothing,
                Err(RecvTimeoutError::Disconnected) => Heard::Gone,
            };
            return self.listened(got);
        }
    }
}

impl Drop for Out<'_> {
    fn drop(&mut self) {
        if !self.over
            && let Some(outbox) = self.control.outbox()
        {
            outbox.send_without_waiting(Work::Control(Request::Unstage { id: self.id }));
        }
    }
}

/// Tells `progress` how far the file is when that is not what it was told last; a `false` from it
/// gives the transfer up.
fn tell(
    progress: &mut impl FnMut(u64) -> bool,
    now: u64,
    told: &mut Option<u64>,
    every: u64,
) -> Result<(), FileError> {
    let since = told.map_or(now + every, |told| now.saturating_sub(told));
    if since >= every && *told != Some(now) {
        *told = Some(now);
        if !progress(now) {
            return Err(FileError::Ended);
        }
    }
    Ok(())
}

// ---- taking a file ---------------------------------------------------------------------------

/// How one `stage` came out.
enum Staged {
    Whole,
    Rest,
    Route,
}

/// Receives the file of `handle` into `part`, through a pipe, and goes on after a `route` from
/// where `part` ends. `progress` is told how many bytes are in and gives the receiving up by
/// returning `false`.
pub(crate) fn receive(
    control: &ViewControl,
    folder: &Folder,
    handle: &str,
    part: &mut Part,
    progress: &mut impl FnMut(u64) -> bool,
) -> Result<Went, FileError> {
    let mut routes = 0;
    let mut told = None;
    loop {
        match stage(control, folder, handle, part, progress, &mut told)? {
            Staged::Whole => return Ok(Went::Whole(part.len())),
            Staged::Rest => return Ok(Went::Rest { from: part.len() }),
            Staged::Route => {
                routes += 1;
                if routes > ROUTES {
                    return Ok(Went::Rest { from: part.len() });
                }
            }
        }
    }
}

fn stage(
    control: &ViewControl,
    folder: &Folder,
    handle: &str,
    part: &mut Part,
    progress: &mut impl FnMut(u64) -> bool,
    told: &mut Option<u64>,
) -> Result<Staged, FileError> {
    let offset = part.len();
    let mut out = Out::begin(control, |events| Request::Stage {
        handle: handle.to_owned(),
        offset,
        events,
    })?;
    let name = match out.pipe()? {
        Some(Transfer::Pipe(name)) => name,
        Some(Transfer::Failed { code, .. }) => return staging_failed(&code),
        // No answer, or one out of order: the messages will do.
        _ => return Ok(Staged::Rest),
    };
    let Ok(mut pipe) = folder.pipe(&name, false) else {
        return Ok(Staged::Rest);
    };
    let mut buf = vec![0; PIECE];
    let opened = Instant::now();
    let mut last = opened;
    let mut end: Option<u64> = None;
    loop {
        while let Some(word) = out.heard()? {
            match word {
                Transfer::Done(at) => end = Some(at),
                Transfer::Failed { code, .. } => {
                    // What the pipe still holds comes before the end of the part.
                    drain(&mut pipe, part, &mut buf)?;
                    return staging_failed(&code);
                }
                Transfer::Pipe(_) => {}
            }
        }
        if let Some(end) = end {
            match part.len().cmp(&end) {
                std::cmp::Ordering::Equal => {
                    tell(progress, end, told, 0)?;
                    return Ok(Staged::Whole);
                }
                // More came than the file has: whatever it is, it is not the file.
                std::cmp::Ordering::Greater => {
                    part.restart()?;
                    *told = None;
                    return Ok(Staged::Rest);
                }
                std::cmp::Ordering::Less => {}
            }
        }
        let silent = part.len() == offset && end.is_none() && opened.elapsed() > FIRST_BYTE;
        if silent || last.elapsed() > STALL {
            return Ok(Staged::Rest);
        }
        match pipe.read(&mut buf) {
            Ok(0) => sleep(LOOK_AGAIN),
            Ok(n) => {
                part.push(buf.get(..n).unwrap_or_default())?;
                last = Instant::now();
                tell(progress, part.len(), told, PROGRESS_EVERY)?;
            }
            Err(why) if why.kind() == io::ErrorKind::WouldBlock => {
                pipes::ready(pipe.as_raw_fd(), false, 50);
            }
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Ok(Staged::Rest),
        }
    }
}

/// Reads what is in the pipe now into the part.
fn drain(pipe: &mut fs::File, part: &mut Part, buf: &mut [u8]) -> Result<(), FileError> {
    loop {
        match pipe.read(buf) {
            Ok(0) => return Ok(()),
            Ok(n) => part.push(buf.get(..n).unwrap_or_default())?,
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Ok(()),
        }
    }
}

/// What a `staged` with a code makes of the receiving.
fn staging_failed(code: &str) -> Result<Staged, FileError> {
    match code {
        "route" => Ok(Staged::Route),
        // The messages would say the same.
        "gone" | "size" | "handle" => Err(FileError::Refused(code.to_owned())),
        "cancelled" => Err(FileError::Ended),
        // `busy`, `io`, and what a later terminal says: the messages may still carry the file.
        _ => Ok(Staged::Rest),
    }
}

// ---- sending a file --------------------------------------------------------------------------

/// How one `fetch` came out.
enum Fetched {
    Whole,
    /// The fast way is over; the messages go on from this offset.
    Rest(u64),
    /// The way was lost; the terminal holds this many bytes.
    Route(u64),
}

/// Sends `file` to the place of `handle` through a pipe, and goes on after a `route` from where the
/// terminal says its file ends. `progress` is told how many bytes are out.
pub(crate) fn send(
    control: &ViewControl,
    folder: &Folder,
    handle: &str,
    file: &mut fs::File,
    progress: &mut impl FnMut(u64),
) -> Result<Went, FileError> {
    let size = file.metadata()?.len();
    let mut offset = 0;
    let mut routes = 0;
    loop {
        match fetch(control, folder, handle, file, size, offset, progress)? {
            Fetched::Whole => return Ok(Went::Whole(size)),
            Fetched::Rest(from) => return Ok(Went::Rest { from }),
            Fetched::Route(at) => {
                routes += 1;
                if routes > ROUTES {
                    return Ok(Went::Rest { from: at });
                }
                offset = at;
            }
        }
    }
}

fn fetch(
    control: &ViewControl,
    folder: &Folder,
    handle: &str,
    file: &mut fs::File,
    size: u64,
    offset: u64,
    progress: &mut impl FnMut(u64),
) -> Result<Fetched, FileError> {
    file.seek(SeekFrom::Start(offset))?;
    let mut out = Out::begin(control, |events| Request::Fetch {
        handle: handle.to_owned(),
        size,
        offset,
        events,
    })?;
    let name = match out.pipe()? {
        Some(Transfer::Pipe(name)) => name,
        Some(Transfer::Failed { code, at }) => return fetching_failed(&code, at, size, offset),
        // The `unstage` that follows leaves the place as it was before the `fetch`.
        _ => return Ok(Fetched::Rest(0)),
    };
    let began = Instant::now();
    let mut pipe = loop {
        match folder.pipe(&name, true) {
            Ok(pipe) => break pipe,
            // No one reads it yet.
            Err(why)
                if matches!(why.raw_os_error(), Some(libc::ENXIO | libc::ENOENT))
                    && began.elapsed() < FIRST_BYTE =>
            {
                match out.heard()? {
                    Some(Transfer::Failed { code, at }) => {
                        return fetching_failed(&code, at, size, offset);
                    }
                    // Nothing was left to send, and the file is saved already.
                    Some(Transfer::Done(saved)) if saved == size => {
                        progress(size);
                        return Ok(Fetched::Whole);
                    }
                    Some(Transfer::Done(_)) => return Err(disagree()),
                    _ => {}
                }
                sleep(LOOK_AGAIN * 10);
            }
            Err(_) => return Ok(Fetched::Rest(0)),
        }
    };
    let mut buf = vec![0; PIECE];
    let (mut at, mut have, mut used) = (offset, 0, 0);
    let mut moved = Instant::now();
    let mut told = offset;
    while at < size {
        match out.heard()? {
            Some(Transfer::Failed { code, at: held }) => {
                return fetching_failed(&code, held, size, offset);
            }
            // Saved before the last byte was written: the terminal and the program do not agree
            // on the file.
            Some(Transfer::Done(_)) => return Err(disagree()),
            _ => {}
        }
        if used == have {
            let want = usize::try_from(size - at).map_or(PIECE, |left| left.min(PIECE));
            have = read_piece(file, buf.get_mut(..want).unwrap_or_default())?;
            used = 0;
            if have == 0 {
                // The file is shorter than it was: the terminal says it is not as long as `size`.
                break;
            }
        }
        match pipe.write(buf.get(used..have).unwrap_or_default()) {
            Ok(n) => {
                used += n;
                at += n as u64;
                moved = Instant::now();
                if at - told >= PROGRESS_EVERY {
                    told = at;
                    progress(at);
                }
            }
            Err(why) if why.kind() == io::ErrorKind::WouldBlock => {
                if !pipes::ready(pipe.as_raw_fd(), true, 50) && moved.elapsed() > STALL {
                    return Ok(Fetched::Rest(0));
                }
            }
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            // The reader is gone: the terminal says why, or the program gives the way up.
            Err(_) => {
                return match out.wait(WHY_WITHIN)? {
                    Some(Transfer::Failed { code, at: held }) => {
                        fetching_failed(&code, held, size, offset)
                    }
                    _ => Ok(Fetched::Rest(0)),
                };
            }
        }
    }
    drop(pipe);
    match out.wait(SAVED_WITHIN)? {
        Some(Transfer::Done(saved)) if saved == size => {
            progress(size);
            Ok(Fetched::Whole)
        }
        Some(Transfer::Done(_)) => Err(disagree()),
        Some(Transfer::Failed { code, at: held }) => fetching_failed(&code, held, size, offset),
        // Never saved, and no word of why: the `unstage` that follows leaves the place as it was,
        // and the messages carry the file.
        _ => Ok(Fetched::Rest(0)),
    }
}

fn disagree() -> FileError {
    FileError::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "the terminal saved a file of another length",
    ))
}

/// What a `fetched` with a code makes of the sending.
fn fetching_failed(
    code: &str,
    held: Option<u64>,
    size: u64,
    offset: u64,
) -> Result<Fetched, FileError> {
    match code {
        "route" => match held {
            Some(at) if at <= size => Ok(Fetched::Route(at)),
            _ => Err(FileError::Refused("route".to_owned())),
        },
        // Nothing of this `fetch` was begun: the place stands where it did.
        "busy" => Ok(Fetched::Rest(offset)),
        "cancelled" => Err(FileError::Ended),
        other => Err(FileError::Refused(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    const TOKEN: &str = "0b5d7c1e9a3f4682d0e1c7a95b3f8d24";

    fn offer(dir: &Path) -> StageOffer {
        StageOffer {
            dir: dir.to_string_lossy().into_owned(),
            token: TOKEN.to_owned(),
        }
    }

    /// A folder as the terminal makes it: a directory of mode 0700 with a `marker`.
    fn folder_in(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("a directory");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("a mode");
        fs::write(dir.join(MARKER), TOKEN).expect("a marker");
        dir
    }

    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("pencere-staged-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a directory");
        root
    }

    #[test]
    fn a_folder_is_the_terminals_only_when_every_part_of_the_proof_holds() {
        let root = scratch("prove");
        let dir = folder_in(&root, "good");
        assert!(prove(&offer(&dir)).is_some());

        // Not an absolute path.
        assert!(
            prove(&StageOffer {
                dir: "good".to_owned(),
                token: TOKEN.to_owned(),
            })
            .is_none()
        );
        // Not there.
        assert!(prove(&offer(&root.join("missing"))).is_none());
        // Another token in the marker: a folder of another host at the same path.
        let other = folder_in(&root, "other");
        fs::write(other.join(MARKER), "ffffffffffffffffffffffffffffffff").expect("a marker");
        assert!(prove(&offer(&other)).is_none());
        // A marker that holds more than the token, or less.
        fs::write(other.join(MARKER), format!("{TOKEN}\n")).expect("a marker");
        assert!(prove(&offer(&other)).is_none());
        fs::write(other.join(MARKER), &TOKEN[..31]).expect("a marker");
        assert!(prove(&offer(&other)).is_none());
        // No marker, a marker that is a link, a marker that is a directory.
        let bare = folder_in(&root, "bare");
        fs::remove_file(bare.join(MARKER)).expect("no marker");
        assert!(prove(&offer(&bare)).is_none());
        std::os::unix::fs::symlink(dir.join(MARKER), bare.join(MARKER)).expect("a link");
        assert!(prove(&offer(&bare)).is_none(), "a link is followed");
        let hollow = folder_in(&root, "hollow");
        fs::remove_file(hollow.join(MARKER)).expect("no marker");
        fs::create_dir(hollow.join(MARKER)).expect("a directory");
        assert!(prove(&offer(&hollow)).is_none());
        // The group or others can use the folder.
        for mode in [0o710, 0o701, 0o750, 0o755, 0o770, 0o707] {
            fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).expect("a mode");
            assert!(prove(&offer(&dir)).is_none(), "mode {mode:o}");
        }
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("a mode");
        // The folder is a file.
        assert!(prove(&offer(&dir.join(MARKER))).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pipe_is_opened_only_when_it_is_a_fifo_the_terminal_could_have_named() {
        let root = scratch("pipe");
        let dir = folder_in(&root, "folder");
        let folder = prove(&offer(&dir)).expect("a proved folder");
        let name = "1".repeat(32);
        let status = Command::new("mkfifo")
            .arg(dir.join(&name))
            .status()
            .expect("mkfifo");
        assert!(status.success());
        assert!(folder.pipe(&name, false).is_ok());
        // A name that is not 32 lowercase hex digits is not joined to the folder.
        for bad in [
            "",
            "..",
            "../good",
            &"A".repeat(32),
            &"1".repeat(31),
            "marker",
        ] {
            assert!(folder.pipe(bad, false).is_err(), "{bad:?}");
        }
        // A regular file with a good name is not a pipe.
        let plain = "2".repeat(32);
        fs::write(dir.join(&plain), b"x").expect("a file");
        assert!(folder.pipe(&plain, false).is_err());
        // A link is not followed, even to a pipe.
        let linked = "3".repeat(32);
        std::os::unix::fs::symlink(dir.join(&name), dir.join(&linked)).expect("a link");
        assert!(folder.pipe(&linked, false).is_err());
        // A pipe nobody reads cannot be opened for writing, and a missing one not at all.
        assert_eq!(
            folder
                .pipe(&name, true)
                .err()
                .and_then(|e| e.raw_os_error()),
            Some(libc::ENXIO)
        );
        let reader = folder.pipe(&name, false).expect("a reader");
        assert!(folder.pipe(&name, true).is_ok());
        drop(reader);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn what_a_code_makes_of_a_transfer_is_to_go_on_by_messages_unless_they_would_say_the_same() {
        assert!(matches!(staging_failed("route"), Ok(Staged::Route)));
        assert!(matches!(staging_failed("busy"), Ok(Staged::Rest)));
        assert!(matches!(staging_failed("io"), Ok(Staged::Rest)));
        assert!(matches!(staging_failed("new"), Ok(Staged::Rest)));
        for code in ["gone", "size", "handle"] {
            assert!(matches!(staging_failed(code), Err(FileError::Refused(c)) if c == code));
        }
        assert!(matches!(staging_failed("cancelled"), Err(FileError::Ended)));

        assert!(matches!(
            fetching_failed("route", Some(4), 11, 0),
            Ok(Fetched::Route(4))
        ));
        // No `at`, or one past the file: the terminal and the program do not agree.
        assert!(fetching_failed("route", None, 11, 0).is_err());
        assert!(fetching_failed("route", Some(12), 11, 0).is_err());
        assert!(matches!(
            fetching_failed("busy", None, 11, 5),
            Ok(Fetched::Rest(5))
        ));
        for code in ["handle", "size", "space", "io", "gone"] {
            assert!(matches!(
                fetching_failed(code, None, 11, 0),
                Err(FileError::Refused(c)) if c == code
            ));
        }
        assert!(matches!(
            fetching_failed("cancelled", None, 11, 0),
            Err(FileError::Ended)
        ));
    }

    #[test]
    fn progress_is_told_at_the_steps_given_and_at_the_end() {
        let mut calls = Vec::new();
        let mut told = None;
        let mut progress = |n| {
            calls.push(n);
            true
        };
        tell(&mut progress, 10, &mut told, 100).expect("told");
        tell(&mut progress, 50, &mut told, 100).expect("told");
        tell(&mut progress, 120, &mut told, 100).expect("told");
        tell(&mut progress, 130, &mut told, 100).expect("told");
        // The end is told whatever the step.
        tell(&mut progress, 130, &mut told, 0).expect("told");
        tell(&mut progress, 131, &mut told, 0).expect("told");
        assert_eq!(calls, [10, 120, 130, 131]);
        let mut told = None;
        assert!(matches!(
            tell(&mut |_| false, 5, &mut told, 0),
            Err(FileError::Ended)
        ));
    }
}
