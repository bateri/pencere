//! `App`: the view and the pictures of a program, laid out as `pencere new` lays them out, carried in
//! the program's own binary.
//!
//! A project keeps its page in `src/view/` (`page.lua`, a page of the kit, and the scripts it
//! `require`s; or `index.html` and its files, a web page) and its pictures in `src/assets/`. A build
//! script copies them into the binary ([`build::app`]), and [`app!`](crate::app) is them, with
//! [`App::views`] making the views a program offers of them by the rules a Lua app has: the page of
//! the kit if there is one, then the web page if there is one.
//!
//! Under `pencere dev` the program is told where the project's files are (`PENCERE_DEV_APP`), and
//! the app is read from there instead: [`Options::app`](crate::Options::app) then shows the view
//! again whenever one of its files changes, without ending the program.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use crate::{Bundle, BundleError, File, Kind, View, ViewControl};

/// The variable `pencere dev` sets to the project's `src/` for the program it runs.
pub const DEV_VARIABLE: &str = "PENCERE_DEV_APP";

/// The version of the page API a page of the kit is written against (`lua` 1.0), as a Lua app's.
const PAGE_VERSION: &str = "1.0";

/// The directories of an app, under `src/`, and the page of each kind of view.
const VIEW: &str = "view";
const ASSETS: &str = "assets";
const PAGE: &str = "view/page.lua";
const WEB_PAGE: &str = "view/index.html";

/// A file of the app: its path in the app and its bytes.
type AppFile = (String, Cow<'static, [u8]>);

/// The files of a program's view and pictures, by their path in the app (`view/page.lua`,
/// `assets/icon.svg`).
#[derive(Debug, Clone)]
pub struct App {
    source: Source,
}

#[derive(Debug, Clone)]
enum Source {
    /// Carried in the binary.
    Embedded(&'static [(&'static str, &'static [u8])]),
    /// Read from a project's `src/` when asked for.
    Dir(PathBuf),
}

impl App {
    /// The files a build script carried into the binary ([`build::app`]); what [`app!`](crate::app)
    /// expands to. Under `pencere dev` the project's own files are read instead.
    #[must_use]
    pub fn embedded(files: &'static [(&'static str, &'static [u8])]) -> Self {
        match std::env::var_os(DEV_VARIABLE) {
            Some(dir) if !dir.is_empty() => Self::dir(dir),
            _ => Self {
                source: Source::Embedded(files),
            },
        }
    }

    /// The files of a project's `src/` (with `view/` and `assets/` in it), read from the disk each
    /// time the views are made.
    #[must_use]
    pub fn dir(src: impl Into<PathBuf>) -> Self {
        Self {
            source: Source::Dir(src.into()),
        }
    }

    /// The directory the files are read from, when they are not carried in the binary.
    #[must_use]
    pub fn source_dir(&self) -> Option<&Path> {
        match &self.source {
            Source::Dir(dir) => Some(dir),
            Source::Embedded(_) => None,
        }
    }

    /// The files, by their path in the app.
    fn files(&self) -> Result<Vec<AppFile>, BundleError> {
        match &self.source {
            Source::Embedded(files) => Ok(files
                .iter()
                .map(|(path, data)| ((*path).to_owned(), Cow::Borrowed(*data)))
                .collect()),
            Source::Dir(dir) => {
                let mut files = Vec::new();
                for top in [VIEW, ASSETS] {
                    for path in walk(dir, top)
                        .map_err(|why| BundleError::new(format!("{}: {why}", dir.display())))?
                    {
                        let data = fs::read(dir.join(&path)).map_err(|why| {
                            BundleError::new(format!("{}: {why}", dir.join(&path).display()))
                        })?;
                        files.push((path, Cow::Owned(data)));
                    }
                }
                Ok(files)
            }
        }
    }

    /// The views the app has, in the order a program offers them: the page of the kit
    /// (`view/page.lua`, with the other scripts of `view/` it may `require` and the pictures of
    /// `assets/`), then the web page (`view/index.html` and the other files of `view/`). A terminal
    /// takes the first of them it can show.
    ///
    /// # Errors
    ///
    /// When the app has neither, when its files cannot be read, or when they are not a bundle.
    pub fn views(&self) -> Result<Vec<View>, BundleError> {
        let files = self.files()?;
        let has = |wanted: &str| files.iter().any(|(path, _)| path == wanted);
        let mut views = Vec::new();
        if has(PAGE) {
            let mut list = Vec::new();
            for (path, data) in &files {
                if let Some(script) = path.strip_prefix("view/") {
                    if is_page_script(script) {
                        list.push(File::new(script, data.clone()));
                    }
                } else if path.starts_with("assets/") {
                    list.push(File::new(path.clone(), data.clone()));
                }
            }
            let bundle = Bundle::new("page.lua", list)?;
            views.push(View::new(Kind::Lua, &[PAGE_VERSION], bundle));
        }
        if has(WEB_PAGE) {
            let list = files.iter().filter_map(|(path, data)| {
                path.strip_prefix("view/")
                    .map(|relative| File::new(relative, data.clone()))
            });
            views.push(View::web(Bundle::new("index.html", list)?));
        }
        if views.is_empty() {
            return Err(BundleError::new(format!(
                "the app has no view: {PAGE} is a page of the kit and {WEB_PAGE} a web page"
            )));
        }
        Ok(views)
    }

    /// When each file of the app last changed, to tell whether one did: nothing for an app carried
    /// in the binary, which does not change.
    pub(crate) fn stamps(&self) -> Vec<(String, SystemTime, u64)> {
        let Source::Dir(dir) = &self.source else {
            return Vec::new();
        };
        let mut stamps = Vec::new();
        for top in [VIEW, ASSETS] {
            for path in walk(dir, top).unwrap_or_default() {
                if let Ok(meta) = fs::metadata(dir.join(&path)) {
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    stamps.push((path, modified, meta.len()));
                }
            }
        }
        stamps
    }
}

/// How often an app read from the disk is looked at, and how long a change is let settle (a save is
/// often two or three writes).
const EVERY: Duration = Duration::from_millis(250);
const SETTLE: Duration = Duration::from_millis(120);

/// Watches an app read from the disk until it is dropped: when one of its files changes, the view
/// `control` holds is shown again with the app's new views.
pub(crate) struct Watch {
    stop: Arc<AtomicBool>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn watch(app: App, control: ViewControl) -> Watch {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    std::thread::spawn(move || {
        let mut before = app.stamps();
        while !stopped.load(Ordering::SeqCst) {
            std::thread::sleep(EVERY);
            let now = app.stamps();
            if now == before {
                continue;
            }
            std::thread::sleep(SETTLE);
            before = app.stamps();
            match app.views() {
                // A view that cannot be shown again (no view up yet, a terminal that cannot) is
                // shown as it is the next time the program runs.
                Ok(views) => {
                    if let Err(why) = control.reopen(Some(views)) {
                        eprintln!("pencere: the view could not be shown again: {why}");
                    }
                }
                Err(why) => eprintln!("pencere: the view could not be shown again: {why}"),
            }
        }
    });
    Watch { stop }
}

/// A script a page may `require`: Luau's two extensions.
fn is_page_script(path: &str) -> bool {
    path.ends_with(".lua") || path.ends_with(".luau")
}

/// The files under `root/top`, by their paths from `root` with `/` between the parts, in order. A
/// directory that is not there has none; what is hidden (a name that starts with a dot) is left out.
fn walk(root: &Path, top: &str) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    let mut pending = vec![top.to_owned()];
    while let Some(relative) = pending.pop() {
        let entries = match fs::read_dir(root.join(&relative)) {
            Ok(entries) => entries,
            Err(why) if why.kind() == io::ErrorKind::NotFound => continue,
            Err(why) => return Err(why),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{relative}: a name that is not UTF-8"),
                ));
            };
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{relative}/{name}");
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// What a build script calls to carry the app into the binary.
pub mod build {
    use std::fmt::Write as _;
    use std::io;
    use std::path::{Path, PathBuf};

    /// The name of the file the build script writes in `OUT_DIR`, which [`app!`](crate::app)
    /// includes.
    pub const OUT_FILE: &str = "pencere-app.rs";

    /// Carries the files of `src/view/` and `src/assets/` of the package being built into its
    /// binary, for [`app!`](crate::app). Call it from the package's `build.rs`:
    ///
    /// ```no_run
    /// // build.rs
    /// pencere::build::app().expect("the app's files");
    /// ```
    ///
    /// Cargo builds the package again when a file of either directory changes.
    ///
    /// # Errors
    ///
    /// When it is not run by Cargo as a build script, or when the files cannot be read or written.
    pub fn app() -> io::Result<()> {
        let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
            io::Error::other("pencere::build::app is called from a build script, run by Cargo")
        })?;
        let out = std::env::var_os("OUT_DIR").ok_or_else(|| {
            io::Error::other("pencere::build::app is called from a build script, run by Cargo")
        })?;
        app_of(
            &PathBuf::from(manifest).join("src"),
            &PathBuf::from(out).join(OUT_FILE),
        )
    }

    /// The same for the project whose `src/` is `src`, written to `out`.
    pub(crate) fn app_of(src: &Path, out: &Path) -> io::Result<()> {
        let mut code = String::from("&[\n");
        for top in [super::VIEW, super::ASSETS] {
            println!("cargo:rerun-if-changed={}", src.join(top).display());
            for path in super::walk(src, top)? {
                let absolute = src.join(&path);
                let absolute = absolute.to_str().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}: a path that is not UTF-8", absolute.display()),
                    )
                })?;
                let _ = writeln!(
                    code,
                    "    ({path:?}, include_bytes!({absolute:?}) as &[u8]),"
                );
            }
        }
        code.push(']');
        std::fs::write(out, code)
    }
}

/// The app's view and pictures, carried into the binary by the package's build script
/// ([`build::app`]), or under `pencere dev` read from the project.
///
/// ```ignore
/// let outcome = pencere::serve(pencere::Options::new().app(pencere::app!()).program("tool"))?;
/// ```
#[macro_export]
macro_rules! app {
    () => {
        $crate::App::embedded(include!(concat!(env!("OUT_DIR"), "/pencere-app.rs")))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pencere-app-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn a_page_of_the_kit_takes_its_scripts_and_the_pictures_and_a_web_page_comes_after_it() {
        let root = project(
            "both",
            &[
                ("view/page.lua", "return {}"),
                ("view/parts/ui.lua", "return {}"),
                ("view/index.html", "<p>"),
                ("view/app.js", ""),
                ("view/.hidden.lua", ""),
                ("assets/icon.svg", "<svg/>"),
            ],
        );
        let views = App::dir(&root).views().unwrap();
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].kind, Kind::Lua);
        assert_eq!(views[1].kind, Kind::Web);
        let paths = |view: &View| match &view.source {
            crate::view::Source::Fixed(bundle) => {
                let mut paths: Vec<String> = bundle
                    .files()
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect();
                paths.sort();
                paths
            }
            crate::view::Source::PerVersion(_) => unreachable!(),
        };
        let lua = paths(&views[0]);
        assert_eq!(lua, ["assets/icon.svg", "page.lua", "parts/ui.lua"]);
        let web = paths(&views[1]);
        assert_eq!(web, ["app.js", "index.html", "page.lua", "parts/ui.lua"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_app_with_no_page_says_where_one_goes() {
        let root = project("none", &[("assets/icon.svg", "<svg/>")]);
        let why = App::dir(&root).views().unwrap_err().to_string();
        assert!(why.contains("view/page.lua"), "{why}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_build_script_writes_what_the_binary_carries_and_a_change_is_seen_in_the_stamps() {
        let root = project(
            "build",
            &[("view/page.lua", "return {}"), ("assets/a b.png", "x")],
        );
        let out = root.join("out.rs");
        build::app_of(&root, &out).unwrap();
        let code = fs::read_to_string(&out).unwrap();
        assert!(
            code.contains("(\"view/page.lua\", include_bytes!("),
            "{code}"
        );
        assert!(
            code.contains("(\"assets/a b.png\", include_bytes!("),
            "{code}"
        );
        let app = App::dir(&root);
        let before = app.stamps();
        assert_eq!(before.len(), 2);
        fs::write(root.join("view/page.lua"), "return { render = nil }").unwrap();
        assert_ne!(app.stamps(), before);
        fs::remove_dir_all(root).unwrap();
    }
}
