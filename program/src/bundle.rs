//! The files of a view: a bundle, listed with the SHA-256 of each (specification §8).
//!
//! A program builds a bundle once, from a directory or from files it holds in memory (a
//! component or a script it carries in its own binary is one file). The manifest goes into the
//! `open`; the bytes stay here until the terminal says, in `need`, which of them it lacks.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use pencere_wire::bundle::{
    BundleLimits, Digest, MAX_FILE_SIZE, MAX_FILES, MAX_PATH, MAX_TOTAL_SIZE, Manifest, is_path,
};
use pencere_wire::message::{RawBundle, RawFile};

/// Why a set of files is not a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleError(String);

impl BundleError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The entry of a bundle is not the kind of file the kind of view needs (§6.3).
    pub(crate) fn mismatch(entry: &str, kind: &str, found: &str, wanted: &str) -> Self {
        Self::new(format!(
            "the entry {entry:?} of a {kind} view has the type {found}; it has to be {wanted}"
        ))
    }
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BundleError {}

/// A file to put in a bundle.
#[derive(Debug, Clone)]
pub struct File {
    path: String,
    data: Cow<'static, [u8]>,
    media_type: Option<String>,
}

impl File {
    /// A file at `path`: relative, `/`-separated, 1 to 255 bytes, every segment of `A-Z a-z 0-9 . _ ~ -`
    /// and not `.` or `..`. `data` can be bytes the program carries (`include_bytes!`) or bytes it
    /// has read.
    pub fn new(path: impl Into<String>, data: impl Into<Cow<'static, [u8]>>) -> Self {
        Self {
            path: path.into(),
            data: data.into(),
            media_type: None,
        }
    }

    /// Its media type, `type/subtype` in lowercase without parameters, when the extension of the
    /// path does not say it (or says it wrong).
    #[must_use]
    pub fn media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }
}

/// One file of a bundle that has been checked.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) path: String,
    pub(crate) digest: Digest,
    pub(crate) size: usize,
    pub(crate) media_type: String,
    data: Arc<[u8]>,
}

/// A bundle: its manifest and the bytes of its files.
#[derive(Debug, Clone)]
pub struct Bundle {
    entry: String,
    files: Vec<Entry>,
    by_digest: HashMap<Digest, usize>,
}

/// The media type of a path by its extension; `application/octet-stream` when it is not known.
fn media_type_of(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = match name.rfind('.') {
        Some(at) if at > 0 => name.get(at..).unwrap_or("").to_ascii_lowercase(),
        _ => String::new(),
    };
    match extension.as_str() {
        ".html" | ".htm" => "text/html",
        ".js" | ".mjs" => "text/javascript",
        ".css" => "text/css",
        ".json" | ".map" => "application/json",
        ".txt" => "text/plain",
        ".md" => "text/markdown",
        ".csv" => "text/csv",
        ".svg" => "image/svg+xml",
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".avif" => "image/avif",
        ".ico" => "image/x-icon",
        ".woff" => "font/woff",
        ".woff2" => "font/woff2",
        ".ttf" => "font/ttf",
        ".otf" => "font/otf",
        ".wasm" => "application/wasm",
        ".lua" | ".luau" => "text/x-lua",
        ".mp3" => "audio/mpeg",
        ".ogg" => "audio/ogg",
        ".wav" => "audio/wav",
        ".mp4" => "video/mp4",
        ".webm" => "video/webm",
        ".xml" => "application/xml",
        ".pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

impl Bundle {
    /// A bundle of files held in memory, listed in the order given: the digests the terminal
    /// asks for come in that order, and a program that wants the document first puts it first.
    /// `entry` is the path of the document of a page, or of the component or the script of a
    /// view the terminal draws itself.
    ///
    /// # Errors
    ///
    /// Says why the files are not a bundle: a path, a media type, a limit or the entry is not
    /// what specification §8.2 allows.
    pub fn new(entry: &str, files: impl IntoIterator<Item = File>) -> Result<Self, BundleError> {
        let mut listed: Vec<Entry> = Vec::new();
        let mut by_digest = HashMap::new();
        let mut paths = HashSet::new();
        let mut total: u64 = 0;
        for file in files {
            if listed.len() >= MAX_FILES {
                return Err(BundleError::new(format!(
                    "a bundle holds at most {MAX_FILES} files"
                )));
            }
            if !is_path(&file.path) {
                return Err(BundleError::new(format!(
                    "{:?} is not a path a bundle may list: 1 to {MAX_PATH} bytes, segments of A-Z a-z 0-9 . _ ~ - and not . or ..",
                    file.path
                )));
            }
            if !paths.insert(file.path.clone()) {
                return Err(BundleError::new(format!("{:?} is listed twice", file.path)));
            }
            let size = file.data.len() as u64;
            if size > MAX_FILE_SIZE {
                return Err(BundleError::new(format!(
                    "{:?} is {size} bytes; a file of a bundle holds at most {MAX_FILE_SIZE}",
                    file.path
                )));
            }
            total += size;
            if total > MAX_TOTAL_SIZE {
                return Err(BundleError::new(format!(
                    "a bundle holds at most {MAX_TOTAL_SIZE} bytes in all"
                )));
            }
            let media_type = file
                .media_type
                .unwrap_or_else(|| media_type_of(&file.path).to_owned());
            let digest = Digest::of(&file.data);
            by_digest.entry(digest).or_insert(listed.len());
            listed.push(Entry {
                path: file.path,
                digest,
                size: file.data.len(),
                media_type,
                data: Arc::from(file.data.into_owned()),
            });
        }
        if !paths.contains(entry) {
            return Err(BundleError::new(format!(
                "the entry {entry:?} is not one of the files"
            )));
        }
        let bundle = Self {
            entry: entry.to_owned(),
            files: listed,
            by_digest,
        };
        // The grammar of a media type is the wire's: a manifest that a terminal would call
        // malformed is not sent.
        bundle.manifest().map_err(|()| {
            BundleError::new("a media type is not `type/subtype` in lowercase, without parameters")
        })?;
        Ok(bundle)
    }

    /// A bundle of one file, the whole of a component or a script that the program carries: the
    /// file is the entry, and its media type comes from the extension of `name` (`.wasm`, `.lua`).
    ///
    /// # Errors
    ///
    /// As [`Bundle::new`].
    pub fn single(name: &str, data: impl Into<Cow<'static, [u8]>>) -> Result<Self, BundleError> {
        Self::new(name, [File::new(name, data)])
    }

    /// A bundle of the files under `directory`, recursively, listed in byte order of their
    /// paths. Names that begin with a dot are left out; a link to a directory is not followed.
    ///
    /// # Errors
    ///
    /// As [`Bundle::new`], and when the directory cannot be read.
    pub fn from_dir(directory: impl AsRef<Path>, entry: &str) -> Result<Self, BundleError> {
        let mut found: Vec<(String, Vec<u8>)> = Vec::new();
        walk(directory.as_ref(), "", &mut found)?;
        found.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        Self::new(
            entry,
            found.into_iter().map(|(path, data)| File::new(path, data)),
        )
    }

    /// The path of the entry.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// The media type of the entry.
    pub(crate) fn entry_type(&self) -> Option<&str> {
        self.files
            .iter()
            .find(|file| file.path == self.entry)
            .map(|file| file.media_type.as_str())
    }

    /// The files, in the order they are listed.
    pub(crate) fn files(&self) -> &[Entry] {
        &self.files
    }

    /// The bytes of the file whose digest is `digest`, if the bundle has one.
    pub(crate) fn read(&self, digest: &Digest) -> Option<&[u8]> {
        let at = self.by_digest.get(digest)?;
        self.files.get(*at).map(|file| &*file.data)
    }

    /// The manifest as the terminal will check it: the bundle held to the limits of the
    /// protocol by the code that holds a terminal to them.
    fn manifest(&self) -> Result<Manifest, ()> {
        let raw = RawBundle {
            entry: self.entry.clone(),
            files: self
                .files
                .iter()
                .map(|file| RawFile {
                    path: file.path.clone(),
                    sha256: file.digest.to_string(),
                    size: file.size as u64,
                    media_type: file.media_type.clone(),
                })
                .collect(),
        };
        Manifest::check(raw, BundleLimits::PROTOCOL).map_err(|_| ())
    }
}

fn walk(
    root: &Path,
    relative: &str,
    found: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), BundleError> {
    let directory = if relative.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let read = fs::read_dir(&directory)
        .map_err(|why| BundleError::new(format!("{}: {why}", directory.display())))?;
    for item in read {
        let item =
            item.map_err(|why| BundleError::new(format!("{}: {why}", directory.display())))?;
        let name = item.file_name().to_string_lossy().into_owned();
        // A name that begins with a dot is not part of a view: `.DS_Store`, `.git`.
        if name.starts_with('.') {
            continue;
        }
        let path = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        let full = root.join(&path);
        let Ok(kind) = item.file_type() else {
            continue;
        };
        // A link to a file is that file; a link to a directory is skipped, so that a loop
        // cannot trap the walk.
        let (is_dir, is_file) = if kind.is_symlink() {
            (false, fs::metadata(&full).is_ok_and(|meta| meta.is_file()))
        } else {
            (kind.is_dir(), kind.is_file())
        };
        if is_dir {
            walk(root, &path, found)?;
        } else if is_file {
            let data = fs::read(&full)
                .map_err(|why| BundleError::new(format!("{}: {why}", full.display())))?;
            found.push((path, data));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_media_type_comes_from_the_extension() {
        assert_eq!(media_type_of("index.html"), "text/html");
        assert_eq!(media_type_of("a/b/app.JS"), "text/javascript");
        assert_eq!(media_type_of("view.wasm"), "application/wasm");
        assert_eq!(media_type_of("view.lua"), "text/x-lua");
        assert_eq!(media_type_of("parts/init.luau"), "text/x-lua");
        assert_eq!(media_type_of("README"), "application/octet-stream");
        assert_eq!(media_type_of(".hidden"), "application/octet-stream");
    }

    #[test]
    fn files_that_are_not_a_bundle_are_told_why() {
        let one = || File::new("index.html", b"<p>".to_vec());
        assert!(Bundle::new("index.html", [one()]).is_ok());
        for (entry, files, why) in [
            ("index.html", vec![], "not one of the files"),
            ("index.html", vec![one(), one()], "listed twice"),
            (
                "index.html",
                vec![File::new("../x", b"x".to_vec()), one()],
                "not a path",
            ),
            (
                "index.html",
                vec![File::new("a b", b"x".to_vec()), one()],
                "not a path",
            ),
            (
                "index.html",
                vec![one().media_type("Text/HTML")],
                "media type",
            ),
        ] {
            let error = Bundle::new(entry, files).expect_err(why);
            assert!(error.to_string().contains(why), "{error} ({why})");
        }
    }

    #[test]
    fn a_file_over_the_limit_is_refused() {
        let big = vec![0u8; usize::try_from(MAX_FILE_SIZE).unwrap_or(0) + 1];
        let error = Bundle::single("view.wasm", big).expect_err("over the limit");
        assert!(error.to_string().contains("at most"), "{error}");
    }

    #[test]
    fn the_same_bytes_listed_twice_are_sent_once() {
        let bundle = Bundle::new(
            "a.txt",
            [
                File::new("a.txt", b"same".to_vec()),
                File::new("b.txt", b"same".to_vec()),
            ],
        )
        .expect("a bundle");
        let digest = Digest::of(b"same");
        assert_eq!(bundle.read(&digest), Some(&b"same"[..]));
        assert_eq!(bundle.files().len(), 2);
        assert_eq!(bundle.entry_type(), Some("text/plain"));
    }

    #[test]
    fn a_directory_is_listed_in_byte_order_without_its_hidden_files() {
        let dir = std::env::temp_dir().join(format!("pencere-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).expect("a directory");
        fs::write(dir.join("index.html"), b"<p>").expect("a file");
        fs::write(dir.join("b.css"), b"b").expect("a file");
        fs::write(dir.join(".DS_Store"), b"x").expect("a file");
        fs::write(dir.join("sub/a.js"), b"a").expect("a file");
        let bundle = Bundle::from_dir(&dir, "index.html").expect("a bundle");
        let paths: Vec<&str> = bundle.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["b.css", "index.html", "sub/a.js"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
