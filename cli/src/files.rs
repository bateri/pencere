//! The files of an app: `app.lua`, the modules `require` loads, the page and the pictures of the
//! view. They are in the directory the app is written in (`pencere dev`), or in the executable
//! `pencere build` made of it (a built app).
//!
//! An app has one shape once it is packed, whatever its directory looks like: the program's
//! scripts at the top (`app.lua`, `api/files.lua`), the page in `view/` and the pictures in
//! `assets/`. Every path an app writes is a path of that shape. A directory is made into it in one
//! of two forms (`Project`):
//!
//! - `src/program/`, `src/view/` and `src/assets/` become the top, `view/` and `assets/`. This is
//!   the layout `pencere new` makes; the page may come from a build directory instead (`--web`).
//! - the simple form: a directory that is the packed shape already, with `app.lua` in it.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The directory of a view's pictures and fonts, in the bundle the view comes in.
pub const ASSETS: &str = "assets";

/// The directory of the page, in the packed app.
pub const VIEW: &str = "view";

/// The script that starts the app, from the top of the packed app.
pub const PROGRAM: &str = "app.lua";

/// The program, the page and the pictures, in a directory laid out as `pencere new` lays it.
const PROGRAM_DIR: &str = "src/program";
const VIEW_DIR: &str = "src/view";
const ASSETS_DIR: &str = "src/assets";

/// The most bytes the files of an app have together: the bundle of a view is held to 64 MiB, and
/// the scripts are on top of what the view has.
const MAX_TOTAL: u64 = 96 * 1024 * 1024;

/// A directory the app is written in, and where its parts are in it.
#[derive(Debug)]
struct Dir {
    /// Where the top of the packed app is.
    top: PathBuf,
    /// Directories that are packed under a name of the top instead of being in it: `view`, `assets`.
    named: Vec<(&'static str, PathBuf)>,
}

impl Dir {
    /// The file a packed path is.
    fn disk(&self, path: &str) -> PathBuf {
        for (name, disk) in &self.named {
            if let Some(rest) = path.strip_prefix(name).and_then(|r| r.strip_prefix('/')) {
                return disk.join(rest);
            }
        }
        self.top.join(path)
    }

    /// The packed paths of the files that belong to an app (`belongs`).
    fn paths(&self) -> io::Result<Vec<String>> {
        let mut found = Vec::new();
        for (name, _) in &self.named {
            // The packed `view/` is the page's: a directory of the program by that name could not
            // be reached, and is not silently left out.
            if self.top.join(name).exists() {
                return Err(io::Error::other(format!(
                    "{}: `{name}/` of the packed app is not the program's: name this directory otherwise",
                    self.top.join(name).display()
                )));
            }
        }
        walk(&self.top, "", &mut found)?;
        for (name, disk) in &self.named {
            // An app with no pictures has no directory for them.
            if disk.is_dir() {
                walk(disk, name, &mut found)?;
            }
        }
        found.sort();
        Ok(found)
    }
}

/// Where the files are.
#[derive(Debug)]
enum Source {
    /// Read from the directory each time: the app being written, which changes under the runner.
    Dir(Dir),
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

/// Whether `path` is a script a page of the kit may `require`: Luau's two extensions for a module.
pub fn is_page_script(path: &str) -> bool {
    path.ends_with(".lua") || path.ends_with(".luau")
}

/// Whether the file at `path` (a packed path, clean) belongs to an app: the scripts, and every file
/// in a directory of the app (its pictures, the web page of a view). Nothing hidden, and none of
/// the directories a project keeps its tools' leavings in. A file beside `app.lua` that is not a
/// script is not the app's: that is where a built app, and the notes about it, are.
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
    /// The files of the simple form: a directory that is the packed shape already, read as they
    /// are asked for.
    pub fn in_dir(root: impl Into<PathBuf>) -> Self {
        Self {
            source: Source::Dir(Dir {
                top: root.into(),
                named: Vec::new(),
            }),
        }
    }

    /// The files of a directory laid out as `pencere new` lays it (`src/program`, `src/view`,
    /// `src/assets`), read as they are asked for. The page is in `web` instead of `src/view` when
    /// that is given: the directory a web page is built into.
    pub fn laid(root: &Path, web: Option<&Path>) -> Self {
        Self {
            source: Source::Dir(Dir {
                top: root.join(PROGRAM_DIR),
                named: vec![
                    (
                        VIEW,
                        web.map_or_else(|| root.join(VIEW_DIR), Path::to_path_buf),
                    ),
                    (ASSETS, root.join(ASSETS_DIR)),
                ],
            }),
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
            Source::Dir(dir) => fs::read(dir.disk(path)),
            Source::Packed(files) => files.get(path).cloned().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{path}: not in the app"))
            }),
        }
    }

    /// Where the file is on disk, for an app that is read from a directory.
    pub fn on_disk(&self, path: &str) -> Option<PathBuf> {
        match &self.source {
            Source::Dir(dir) if is_clean(path) => Some(dir.disk(path)),
            _ => None,
        }
    }

    /// Whether the app has the file.
    pub fn has(&self, path: &str) -> bool {
        is_clean(path)
            && match &self.source {
                Source::Dir(dir) => dir.disk(path).is_file(),
                Source::Packed(files) => files.contains_key(path),
            }
    }

    /// The paths of the files of an app, sorted. Of a directory the files that belong to an app
    /// (`belongs`).
    pub fn paths(&self) -> io::Result<Vec<String>> {
        match &self.source {
            Source::Packed(files) => Ok(files.keys().cloned().collect()),
            Source::Dir(dir) => dir.paths(),
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

/// An app's directory, and the form it is written in.
#[derive(Debug)]
pub struct Project {
    root: PathBuf,
    laid: bool,
    web: Option<PathBuf>,
}

impl Project {
    /// The app in `root`: laid out as `pencere new` lays it (`src/program/app.lua`), or the simple
    /// form (`app.lua`). `web` is the directory a web page is built into, in the laid out form.
    pub fn find(root: &Path, web: Option<&Path>) -> Result<Self, String> {
        let laid = root.join(PROGRAM_DIR).join(PROGRAM).is_file();
        let simple = root.join(PROGRAM).is_file();
        if laid && simple {
            return Err(format!(
                "{} has both {PROGRAM_DIR}/{PROGRAM} and {PROGRAM}: an app is written in one of the two forms",
                root.display()
            ));
        }
        if !laid && !simple {
            return Err(format!(
                "{} has no {PROGRAM_DIR}/{PROGRAM}: an app is a directory with one (`pencere new` makes one; \
                 a directory with a {PROGRAM} in it is the simple form)",
                root.display()
            ));
        }
        if let Some(web) = web {
            if !laid {
                return Err(format!(
                    "--web is for an app with the {PROGRAM_DIR}/ layout: in the simple form the page is in {VIEW}/ \
                     or web/ of the directory"
                ));
            }
            if !web.join("index.html").is_file() {
                return Err(format!(
                    "--web {}: no index.html in it; it is the directory the page is built into",
                    web.display()
                ));
            }
        }
        Ok(Self {
            root: root.to_path_buf(),
            laid,
            web: web.map(Path::to_path_buf),
        })
    }

    /// The files of the app, read from the directory as they are asked for.
    pub fn files(&self) -> Files {
        if self.laid {
            Files::laid(&self.root, self.web.as_deref())
        } else {
            Files::in_dir(&self.root)
        }
    }

    /// The name of the directory: what the app is called unless it is told.
    pub fn name(&self) -> Result<String, String> {
        let absolute = std::path::absolute(&self.root)
            .map_err(|why| format!("{}: {why}", self.root.display()))?;
        absolute
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{} has no name to give the app", self.root.display()))
    }

    /// Where the built app goes unless it is told: `dist/` for an app with the layout (nothing
    /// generated is among the sources), beside the directory's own files in the simple form.
    pub fn output(&self, file: &str) -> PathBuf {
        if self.laid {
            self.root.join("dist").join(file)
        } else {
            PathBuf::from(file)
        }
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

/// The files of `directory` that belong to an app, with their packed paths: `prefix` is where the
/// directory is in the packed app.
fn walk(directory: &Path, prefix: &str, found: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        // A link to a directory is not followed: an app is what is in its directory. A link to a
        // file is: the bytes of what it points to are the file's, wherever that is.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target") {
                continue;
            }
            walk(&entry.path(), &path, found)?;
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
    /// A directory of the test's own, made of the files listed (each of one line).
    fn project_of(name: &str, paths: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pencere-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for path in paths {
            let file = dir.join(path);
            fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
            fs::write(file, format!("{path}\n")).expect("a file");
        }
        dir
    }

    #[test]
    fn a_directory_with_the_layout_is_packed_as_the_shape_the_app_runs_in() {
        let dir = project_of(
            "laid",
            &[
                "src/program/app.lua",
                "src/program/api/files.lua",
                "src/program/notes.md",
                "src/program/.hidden.lua",
                "src/view/page.lua",
                "src/assets/box.svg",
                "src/assets/.DS_Store",
                "src/assets/deep/a.png",
                "dist/textpen",
                "README.md",
                "app.lua.bak",
            ],
        );
        let files = Project::find(&dir, None).expect("an app").files();
        assert_eq!(
            files.paths().expect("paths"),
            [
                "api/files.lua",
                "app.lua",
                "assets/box.svg",
                "assets/deep/a.png",
                "view/page.lua"
            ]
        );
        assert_eq!(
            files.read("app.lua").expect("a file"),
            b"src/program/app.lua\n"
        );
        assert_eq!(
            files.read("view/page.lua").expect("a file"),
            b"src/view/page.lua\n"
        );
        assert_eq!(
            files.read("assets/box.svg").expect("a file"),
            b"src/assets/box.svg\n"
        );
        assert!(files.has("api/files.lua") && !files.has("src/view/page.lua"));
        assert_eq!(files.under("assets").expect("under").len(), 2);
        // The files are read as they are asked for: the app is being written.
        fs::write(dir.join("src/view/page.lua"), "changed").expect("a file");
        assert_eq!(files.read("view/page.lua").expect("a file"), b"changed");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_built_elsewhere_takes_the_place_of_the_view_directory() {
        let dir = project_of(
            "web",
            &[
                "src/program/app.lua",
                "src/view/index.html",
                "src/view/main.tsx",
                "src/assets/icon.png",
                "dist/view/index.html",
                "dist/view/assets/app-1.js",
            ],
        );
        let built = dir.join("dist/view");
        let files = Project::find(&dir, Some(&built)).expect("an app").files();
        assert_eq!(
            files.paths().expect("paths"),
            [
                "app.lua",
                "assets/icon.png",
                "view/assets/app-1.js",
                "view/index.html"
            ]
        );
        assert_eq!(
            files.read("view/index.html").expect("a file"),
            b"dist/view/index.html\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_app_without_pictures_or_a_page_is_still_an_app() {
        let dir = project_of("bare", &["src/program/app.lua"]);
        let files = Project::find(&dir, None).expect("an app").files();
        assert_eq!(files.paths().expect("paths"), ["app.lua"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_of_the_program_cannot_take_the_name_of_the_page_or_the_pictures() {
        let dir = project_of("clash", &["src/program/app.lua", "src/program/view/x.lua"]);
        let why = Project::find(&dir, None)
            .expect("an app")
            .files()
            .paths()
            .expect_err("a clash")
            .to_string();
        assert!(why.contains("view/"), "{why}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_simple_form_is_a_directory_that_is_the_shape_already() {
        let dir = project_of(
            "simple",
            &[
                "app.lua",
                "lib/x.lua",
                "page.lua",
                "web/index.html",
                "notes.md",
            ],
        );
        let project = Project::find(&dir, None).expect("an app");
        assert_eq!(
            project.files().paths().expect("paths"),
            ["app.lua", "lib/x.lua", "page.lua", "web/index.html"]
        );
        assert_eq!(project.output("hello"), PathBuf::from("hello"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_not_an_app_is_told_in_words_that_say_where_the_program_goes() {
        let both = project_of("both", &["app.lua", "src/program/app.lua"]);
        let why = Project::find(&both, None).expect_err("two forms");
        assert!(why.contains("one of the two forms"), "{why}");
        let none = project_of("none", &["README.md"]);
        let why = Project::find(&none, None).expect_err("no app");
        assert!(why.contains("src/program/app.lua"), "{why}");
        // A page built elsewhere is for the layout, and has to be a page.
        let simple = project_of("simple-web", &["app.lua", "built/index.html"]);
        let why = Project::find(&simple, Some(&simple.join("built"))).expect_err("simple");
        assert!(why.contains("--web"), "{why}");
        let laid = project_of("laid-web", &["src/program/app.lua", "built/main.js"]);
        let why = Project::find(&laid, Some(&laid.join("built"))).expect_err("no entry");
        assert!(why.contains("index.html"), "{why}");
        for dir in [both, none, simple, laid] {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_built_app_goes_to_dist_when_the_directory_has_the_layout() {
        let dir = project_of("output", &["src/program/app.lua"]);
        let project = Project::find(&dir, None).expect("an app");
        assert_eq!(project.output("hello"), dir.join("dist/hello"));
        assert_eq!(
            project.name().expect("a name"),
            dir.file_name().expect("a name").to_string_lossy()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
