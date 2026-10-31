//! The files of the person's machine, as the stand-in plays them (specification §8.9, §8.10): the
//! files the person gives a view — dropped on it, pasted, chosen in the Open panel — and the places
//! the person chooses in the Save panel. The session holds the rules and grants the handles; what it
//! asks of the files (`ReadFile`, `WriteFile`, `AbortWrite`) is done here, on real files, where the
//! command line says.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use pencere_fake_host::log;
use pencere_fake_host::pane::Entropy;
use pencere_fake_host::store::Keep;
use pencere_host::session::{FileError, FileGrant, Session};
use serde_json::Value;

/// A file the person gave, as it was when it was given.
struct Given {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

/// A place the person chose to save in, and the part being written beside it.
struct Place {
    target: PathBuf,
    part: PathBuf,
    file: Option<File>,
}

/// What the person has given the view that runs and where it chose to save: the handles of that
/// view, which are nothing once it has ended.
#[derive(Default)]
pub struct Desk {
    given: HashMap<String, Given>,
    places: HashMap<String, Place>,
    /// The handles, in the order they were granted: `$1` is the first in the page's calls.
    handles: Vec<String>,
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || "?".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

impl Desk {
    /// A view starts: the handles of the one before are nothing.
    pub fn clear(&mut self) {
        for place in self.places.values() {
            let _ = fs::remove_file(&place.part);
        }
        *self = Self::default();
    }

    /// The person gives the view the file at `path` (`how`: drops, pastes, picks).
    pub fn give(&mut self, session: &mut Session<Keep, Entropy>, how: &str, path: &Path) {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(why) => {
                log::line("person", format!("cannot give {}: {why}", path.display()));
                return;
            }
        };
        let size = metadata.len();
        let Some(handle) = session.grant_file(&FileGrant { size }) else {
            log::line(
                "person",
                format!("{how} {}: the view takes no files", name_of(path)),
            );
            return;
        };
        self.handles.push(handle.clone());
        log::line(
            "person",
            format!(
                "{how} {} ({size} bytes): ${}",
                name_of(path),
                self.handles.len()
            ),
        );
        self.given.insert(
            handle,
            Given {
                path: path.to_owned(),
                size,
                modified: metadata.modified().ok(),
            },
        );
    }

    /// The person chooses `dir`/`name` in the Save panel.
    pub fn choose(&mut self, session: &mut Session<Keep, Entropy>, dir: &Path, name: &str) {
        let name = Path::new(name)
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        let Some(handle) = session.grant_save() else {
            log::line("person", "chooses a place: the view takes no files");
            return;
        };
        self.handles.push(handle.clone());
        log::line(
            "person",
            format!("saves as {name}: ${}", self.handles.len()),
        );
        self.places.insert(
            handle,
            Place {
                target: dir.join(&name),
                part: dir.join(format!(".{name}.pencere-part")),
                file: None,
            },
        );
    }

    /// Puts the handles granted in place of `"$1"`, `"$2"`, … in what the page sends.
    pub fn fill(&self, value: &mut Value) {
        match value {
            Value::String(text) => {
                let handle = text
                    .strip_prefix('$')
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| self.handles.get(n.checked_sub(1)?));
                if let Some(handle) = handle {
                    handle.clone_into(text);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.fill(item)),
            Value::Object(members) => members.values_mut().for_each(|item| self.fill(item)),
            _ => {}
        }
    }

    /// The path and the size of a file the person gave, if it is still as it was when it was given.
    pub fn source(&self, handle: &str) -> Result<(PathBuf, u64), FileError> {
        let given = self.given.get(handle).ok_or(FileError::Gone)?;
        let now = fs::metadata(&given.path).map_err(|_| FileError::Gone)?;
        if now.len() != given.size || now.modified().ok() != given.modified {
            return Err(FileError::Gone);
        }
        Ok((given.path.clone(), given.size))
    }

    /// The name the person gave a place to save in, and the part beside it.
    pub fn place_of(&self, handle: &str) -> Option<(PathBuf, PathBuf)> {
        let place = self.places.get(handle)?;
        Some((place.target.clone(), place.part.clone()))
    }

    /// The session asks for a piece of a file the person gave.
    pub fn read(&self, handle: &str, offset: u64, length: usize) -> Result<Vec<u8>, FileError> {
        let (path, _) = self.source(handle)?;
        let mut file = File::open(&path).map_err(|_| FileError::Gone)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| FileError::Io)?;
        let mut bytes = Vec::with_capacity(length);
        file.take(length as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| FileError::Io)?;
        Ok(bytes)
    }

    /// The session asks for bytes to be written to a place the person chose.
    pub fn write(
        &mut self,
        handle: &str,
        offset: u64,
        bytes: &[u8],
        end: bool,
    ) -> Result<(), FileError> {
        let place = self.places.get_mut(handle).ok_or(FileError::Io)?;
        let written = (|| -> io::Result<()> {
            if place.file.is_none() && offset > 0 {
                // A part that a `fetch` kept when the way was lost, which the writes go on with.
                let part = fs::OpenOptions::new()
                    .append(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&place.part)?;
                if part.metadata()?.len() != offset {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "not where it ends",
                    ));
                }
                place.file = Some(part);
            }
            if place.file.is_none() {
                // Made anew, and a link put where it goes is not written through.
                let _ = fs::remove_file(&place.part);
                place.file = Some(
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(&place.part)?,
                );
            }
            if let Some(file) = place.file.as_mut() {
                file.write_all(bytes)?;
            }
            if end && let Some(file) = place.file.take() {
                file.sync_all()?;
                fs::rename(&place.part, &place.target)?;
                log::line(
                    "host",
                    format!(
                        "saved {} ({} bytes)",
                        name_of(&place.target),
                        fs::metadata(&place.target)?.len()
                    ),
                );
            }
            Ok(())
        })();
        written.map_err(|why| {
            log::line(
                "host",
                format!("cannot write {}: {why}", name_of(&place.target)),
            );
            FileError::Io
        })
    }

    /// The session throws away what was written of a place.
    pub fn abort(&mut self, handle: &str) {
        if let Some(place) = self.places.get_mut(handle) {
            place.file = None;
            let _ = fs::remove_file(&place.part);
            log::line(
                "host",
                format!("throws away what was written of {}", name_of(&place.target)),
            );
        }
    }
}
