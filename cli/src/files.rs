//! The files of an app: `app.lua`, the page, the modules `require` loads and the assets of the
//! view. They are in the directory the app is written in (`pencere dev`), or in the executable
//! `pencere build` made of it (a built app).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The directory of a view's pictures and fonts, in the bundle the view comes in.
pub const ASSETS: &str = "assets";

/// The most bytes the files of an app have together: the bundle of a view is held to 64 MiB, and
/// the scripts are on top of what the view has.
const MAX_TOTAL: u64 = 96 * 1024 * 1024;

/// Where the files are.
#[derive(Debug)]
enum Source {
    /// Read from the directory each time: the app being written, which changes under the runner.
    Dir(PathBuf),
    /// In the executable.
    Packed(BTreeMap<String, Vec<u8>>),
}

/// The files of an app, by their path from its directory, separated by `/`.
#[derive(Debug)]
pub struct Files {
    source: Source,
}

/// What reading a payload went wrong with.
#[derive(Debug)]
pub struct Corrupt;

impl fmt::Display for Corrupt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the app inside this executable is damaged")
    }
}

/// Whether `path` is one an app names: relative, with `/`, with no `.` or `..` part.
fn is_clean(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Whether the file at `path` (relative, clean) belongs to an app: the scripts, and every file in
/// a directory of the app (its pictures, the web page of a view). Nothing hidden, and none of the
/// directories a project keeps its tools' leavings in. A file beside `app.lua` that is not a script
/// is not the app's: that is where a built app, and the notes about it, are.
pub fn belongs(path: &str) -> bool {
    let mut parts = path.split('/').peekable();
    let mut top = true;
    while let Some(part) = parts.next() {
        if part.starts_with('.') || matches!(part, "node_modules" | "target") {
            return false;
        }
        if top && parts.peek().is_none() {
            return part.ends_with(".lua");
        }
        top = false;
    }
    true
}

impl Files {
    /// The files in a directory, read as they are asked for.
    pub fn in_dir(root: impl Into<PathBuf>) -> Self {
        Self {
            source: Source::Dir(root.into()),
        }
    }

    /// Files that are already read.
    pub fn packed(files: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            source: Source::Packed(files),
        }
    }

    /// The bytes of a file.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        if !is_clean(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{path:?} is not a path inside the app"),
            ));
        }
        match &self.source {
            Source::Dir(root) => fs::read(root.join(path)),
            Source::Packed(files) => files.get(path).cloned().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{path}: not in the app"))
            }),
        }
    }

    /// Whether the app has the file.
    pub fn has(&self, path: &str) -> bool {
        is_clean(path)
            && match &self.source {
                Source::Dir(root) => root.join(path).is_file(),
                Source::Packed(files) => files.contains_key(path),
            }
    }

    /// The paths of the files of an app, sorted. Of a directory the files that belong to an app
    /// (`belongs`).
    pub fn paths(&self) -> io::Result<Vec<String>> {
        match &self.source {
            Source::Packed(files) => Ok(files.keys().cloned().collect()),
            Source::Dir(root) => {
                let mut found = Vec::new();
                walk(root, "", &mut found)?;
                found.sort();
                Ok(found)
            }
        }
    }

    /// The paths under `directory`, sorted, with their full path from the app's directory.
    pub fn under(&self, directory: &str) -> io::Result<Vec<String>> {
        let prefix = format!("{}/", directory.trim_end_matches('/'));
        Ok(self
            .paths()?
            .into_iter()
            .filter(|path| path.starts_with(&prefix))
            .collect())
    }

    /// Every file of an app, read, for `pencere build`.
    pub fn collect(&self) -> io::Result<BTreeMap<String, Vec<u8>>> {
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for path in self.paths()? {
            let bytes = self.read(&path)?;
            total += bytes.len() as u64;
            if total > MAX_TOTAL {
                return Err(io::Error::other(format!(
                    "the files of the app are over {} MiB together",
                    MAX_TOTAL / (1024 * 1024)
                )));
            }
            files.insert(path, bytes);
        }
        Ok(files)
    }

    /// The files of a built app, from the bytes of its payload.
    pub fn unpack(payload: &[u8]) -> Result<(Meta, Self), Corrupt> {
        let mut at = 0usize;
        let take = |at: &mut usize, n: usize| -> Result<&[u8], Corrupt> {
            let end = at.checked_add(n).ok_or(Corrupt)?;
            let slice = payload.get(*at..end).ok_or(Corrupt)?;
            *at = end;
            Ok(slice)
        };
        let number = |bytes: &[u8]| -> Result<usize, Corrupt> {
            let array: [u8; 8] = bytes.try_into().map_err(|_| Corrupt)?;
            usize::try_from(u64::from_le_bytes(array)).map_err(|_| Corrupt)
        };
        if take(&mut at, MAGIC.len())? != MAGIC {
            return Err(Corrupt);
        }
        let name_len = number(take(&mut at, 8)?)?;
        let name = std::str::from_utf8(take(&mut at, name_len)?)
            .map_err(|_| Corrupt)?
            .to_owned();
        let count = number(take(&mut at, 8)?)?;
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let path_len = number(take(&mut at, 8)?)?;
            let path = std::str::from_utf8(take(&mut at, path_len)?)
                .map_err(|_| Corrupt)?
                .to_owned();
            let data_len = number(take(&mut at, 8)?)?;
            let data = take(&mut at, data_len)?.to_vec();
            if !is_clean(&path) {
                return Err(Corrupt);
            }
            files.insert(path, data);
        }
        Ok((
            Meta { name },
            Self {
                source: Source::Packed(files),
            },
        ))
    }
}

/// What a built app says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// What it is called: the program's name the terminal shows.
    pub name: String,
}

/// The first bytes of a payload: what it is, and the layout it has.
const MAGIC: &[u8] = b"pencere-app\x01";

/// The bytes of a payload: the name, then the files, each as its path and its data, with lengths
/// that are 64-bit little-endian numbers.
pub fn pack(meta: &Meta, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::from(MAGIC);
    let number = |n: usize| (n as u64).to_le_bytes();
    out.extend(number(meta.name.len()));
    out.extend(meta.name.as_bytes());
    out.extend(number(files.len()));
    for (path, data) in files {
        out.extend(number(path.len()));
        out.extend(path.as_bytes());
        out.extend(number(data.len()));
        out.extend(data);
    }
    out
}

fn walk(root: &Path, prefix: &str, found: &mut Vec<String>) -> io::Result<()> {
    let directory = if prefix.is_empty() {
        root.to_path_buf()
    } else {
        root.join(prefix)
    };
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        // A link to a directory is not followed: an app is what is in its directory. A link to a
        // file is: the bytes of what it points to are the file's, wherever that is.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            let leaf = path.rsplit('/').next().unwrap_or("");
            if leaf.starts_with('.') || matches!(leaf, "node_modules" | "target") {
                continue;
            }
            walk(root, &path, found)?;
        } else if (kind.is_file() || kind.is_symlink()) && belongs(&path) && entry.path().is_file()
        {
            found.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_comes_back_as_it_went() {
        let mut files = BTreeMap::new();
        files.insert("app.lua".to_owned(), b"print('hi')".to_vec());
        files.insert("assets/a.png".to_owned(), vec![0, 1, 2, 255]);
        let meta = Meta {
            name: "containers".into(),
        };
        let (read, files_back) = Files::unpack(&pack(&meta, &files)).expect("a payload");
        assert_eq!(read, meta);
        assert_eq!(files_back.read("app.lua").expect("a file"), b"print('hi')");
        assert_eq!(
            files_back.paths().expect("paths"),
            ["app.lua", "assets/a.png"]
        );
    }

    #[test]
    fn a_payload_cut_short_or_with_a_bad_path_is_refused() {
        let mut files = BTreeMap::new();
        files.insert("app.lua".to_owned(), b"x".to_vec());
        let bytes = pack(&Meta { name: "a".into() }, &files);
        for end in 0..bytes.len() {
            assert!(Files::unpack(&bytes[..end]).is_err(), "{end}");
        }
        let mut bad = BTreeMap::new();
        bad.insert("../outside.lua".to_owned(), b"x".to_vec());
        assert!(Files::unpack(&pack(&Meta { name: "a".into() }, &bad)).is_err());
    }

    #[test]
    fn what_belongs_to_an_app_is_scripts_and_what_is_in_its_directories() {
        for path in [
            "app.lua",
            "page.lua",
            "lib/util.lua",
            "assets/box.svg",
            "assets/a/b.png",
        ] {
            assert!(belongs(path), "{path}");
        }
        for path in [
            "README.md",
            "containers-lua",
            ".hidden.lua",
            "node_modules/x/y.lua",
            "assets/.DS_Store",
            "web/node_modules/x.js",
            "target/a.lua",
        ] {
            assert!(!belongs(path), "{path}");
        }
    }

    #[test]
    fn a_path_outside_the_app_cannot_be_read() {
        let files = Files::in_dir("/nonexistent");
        for path in ["../etc/passwd", "/etc/passwd", "a//b", "", "a/./b"] {
            let error = files.read(path).expect_err(path);
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{path}");
        }
    }
}
