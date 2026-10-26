//! `pencere build`: the app in a directory, in one executable.
//!
//! The executable is a copy of the runner (this program, or the one of another target) with the
//! app's files in it. For this machine's target the runner is `pencere` itself. For another, it
//! is the same version's runner from the release, downloaded once and kept; no toolchain of that
//! target is needed, since nothing is compiled: a section is written into a file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest as _, Sha256};

use crate::files::{self, Meta, Project};
use crate::payload;

/// The targets a runner is made for.
pub const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
];

/// The target this runner was built for.
const OWN: &str = env!("PENCERE_TARGET");

/// The version of this runner: the release it came from names the runners of the other targets.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the releases are, unless `PENCERE_RELEASES` says otherwise (a mirror, or a directory
/// for a test).
const RELEASES: &str = "https://github.com/bateri/pencere/releases/download";

/// The name of the executable of a target, in a release.
pub fn release_file(target: &str) -> String {
    format!("pencere-{VERSION}-{target}")
}

/// The target if it is one a runner is made for: the four, and this program's own whatever it is
/// (a program built for another libc is still its own runner).
fn canonical(target: &str) -> Option<&'static str> {
    if target == OWN {
        return Some(OWN);
    }
    TARGETS.iter().find(|known| **known == target).copied()
}

/// The directory where downloaded runners are kept.
fn cache() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("PENCERE_CACHE") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(dir).join("pencere"));
    }
    let home = std::env::var_os("HOME")
        .ok_or("HOME is not set, so there is no cache to keep a runner in")?;
    let home = PathBuf::from(home);
    Ok(if cfg!(target_os = "macos") {
        home.join("Library/Caches/pencere")
    } else {
        home.join(".cache/pencere")
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The text at a URL (a file: URL too), with `curl`: the runner has no network client in it, and
/// a download is a thing the person asked for with `--target`.
pub fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "--retry", "2", "--output"])
        .arg(to)
        .arg(url)
        .status()
        .map_err(|why| {
            format!(
                "curl cannot be run ({why}): it fetches the runner of another target; --runner FILE names one you have"
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{url} could not be fetched"))
    }
}

/// The runner of `target`: from the cache, or fetched from the release and checked against its
/// `SHA256SUMS`.
fn download(target: &str) -> Result<Vec<u8>, String> {
    let base = std::env::var("PENCERE_RELEASES").unwrap_or_else(|_| RELEASES.to_owned());
    download_from(&base, &cache()?, target)
}

fn download_from(base: &str, directory: &Path, target: &str) -> Result<Vec<u8>, String> {
    let file = release_file(target);
    let kept = directory.join(&file);
    if let Ok(bytes) = fs::read(&kept) {
        return Ok(bytes);
    }
    let base = format!("{}/cli-v{VERSION}", base.trim_end_matches('/'));
    fs::create_dir_all(directory).map_err(|why| format!("{}: {why}", directory.display()))?;
    let partial = directory.join(format!("{file}.partial"));
    let sums = directory.join(format!("SHA256SUMS-{VERSION}.partial"));
    let result = (|| {
        eprintln!("pencere: fetching the runner for {target} ({file})");
        fetch(&format!("{base}/SHA256SUMS"), &sums)?;
        fetch(&format!("{base}/{file}"), &partial)?;
        let listed = fs::read_to_string(&sums).map_err(|why| why.to_string())?;
        let wanted = listed
            .lines()
            .filter_map(|line| line.split_once(char::is_whitespace))
            .find(|(_, name)| name.trim().trim_start_matches('*') == file)
            .map(|(sum, _)| sum.to_owned())
            .ok_or_else(|| format!("SHA256SUMS of the release does not list {file}"))?;
        let bytes = fs::read(&partial).map_err(|why| why.to_string())?;
        if sha256_hex(&bytes) != wanted {
            return Err(format!(
                "{file} is not the file the release's SHA256SUMS lists (sha256 {} instead of {wanted})",
                sha256_hex(&bytes)
            ));
        }
        fs::rename(&partial, &kept).map_err(|why| why.to_string())?;
        Ok(bytes)
    })();
    let _ = fs::remove_file(&partial);
    let _ = fs::remove_file(&sums);
    result
}

/// What `build` is asked: the app in `dir`, and what it is built as.
pub struct Request<'a> {
    /// The directory of the app.
    pub dir: &'a Path,
    /// The directory a web page is built into, to pack the page from there instead of `src/view`.
    pub web: Option<&'a Path>,
    /// What the app is called: the executable's name and the name in the runner's own messages
    /// (that of the directory when it is `None`).
    pub name: Option<&'a str>,
    /// The target to make it for (this machine's when it is `None`).
    pub target: Option<&'a str>,
    /// The program that is the runner (the runner of the target when it is `None`).
    pub runner: Option<&'a Path>,
    /// The executable to write (`dist/<name>` for an app with the layout, `<name>` in the simple
    /// form, `-<target>` after the name for another target, when it is `None`).
    pub out: Option<&'a Path>,
}

/// Makes the executable of the app in `request.dir`.
pub fn build(request: &Request) -> Result<(PathBuf, usize, u64), String> {
    let target = match request.target {
        None => OWN,
        Some(name) => canonical(name)
            .ok_or_else(|| format!("{name:?} is not a target; they are {}", TARGETS.join(", ")))?,
    };
    let project = Project::find(request.dir, request.web)?;
    let name = match request.name {
        Some(name) => name.to_owned(),
        None => project.name()?,
    };
    let source = project.files();
    let collected = source.collect().map_err(|why| why.to_string())?;
    // What would be refused when the app starts is told now: a script that does not compile, a
    // page the terminal would not take.
    crate::runtime::check(&collected)?;
    let out = match request.out {
        Some(out) => out.to_path_buf(),
        None if target == OWN => project.output(&name),
        None => project.output(&format!("{name}-{target}")),
    };
    // `pencere build myapp`, run beside the directory `myapp`, would make the app's own directory
    // the file.
    if out.is_dir() {
        return Err(format!(
            "{} is a directory: name the executable with -o",
            out.display()
        ));
    }
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
    }
    let bytes = match request.runner {
        Some(path) => fs::read(path).map_err(|why| format!("{}: {why}", path.display()))?,
        None if target == OWN => std::env::current_exe()
            .and_then(fs::read)
            .map_err(|why| format!("this program cannot be read to make a copy of it: {why}"))?,
        None => download(target)?,
    };
    let packed = files::pack(&Meta { name }, &collected);
    let count = collected.len();
    let size = packed.len() as u64;
    // A runner that is itself an app would carry two.
    payload::embed(bytes, packed, &out)
        .map_err(|why: io::Error| format!("{}: {why}", out.display()))?;
    Ok((out, count, size))
}

/// Makes the program of the Cargo package in `request.dir` with Cargo, as `cargo build --release`
/// (`--target` passed on), and copies it where an app's executable goes: `dist/<name>`, or
/// `dist/<name>-<target>` for another target. The page and the pictures are in it already, carried
/// by its build script; Cargo compiles, and so needs the toolchain of the target.
pub fn build_cargo(request: &Request) -> Result<PathBuf, String> {
    if request.web.is_some() || request.runner.is_some() {
        return Err(
            "--web and --runner are for a Lua app; a Rust program is built by Cargo, with its view in src/view/"
                .to_owned(),
        );
    }
    let root = request
        .dir
        .canonicalize()
        .map_err(|why| format!("{}: {why}", request.dir.display()))?;
    let name = match request.name {
        Some(name) => name.to_owned(),
        None => root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("the directory has no name to call the program by: --name names it")?
            .to_owned(),
    };
    let mut extra = vec!["--release"];
    if let Some(target) = request.target {
        extra.extend(["--target", target]);
    }
    let program = crate::dev_cargo::build(&root, &name, &extra).map_err(|why| match request.target {
        Some(target) if !target_installed(target) => format!(
            "{why}: the toolchain has no {target} (`rustup target add {target}`; a target of another \
             system needs its linker too, which `cargo zigbuild` brings)"
        ),
        _ => why,
    })?;
    let out = match (request.out, request.target) {
        (Some(out), _) => out.to_path_buf(),
        (None, None) => root.join("dist").join(&name),
        (None, Some(target)) => root.join("dist").join(format!("{name}-{target}")),
    };
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
    }
    fs::copy(&program, &out).map_err(|why| format!("{}: {why}", out.display()))?;
    Ok(out)
}

/// Whether rustup says the toolchain has `target` (taken to be so where there is no rustup to ask).
fn target_installed(target: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_or(true, |output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim() == target)
        })
}

pub fn command(args: &[String]) -> ExitCode {
    let mut dir = None;
    let mut web = None;
    let mut name = None;
    let mut target = None;
    let mut runner = None;
    let mut out = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match arg.as_str() {
            "--web" => match value("a directory") {
                Ok(v) => web = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            "--name" => match value("a name") {
                Ok(v) => name = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--target" => match value("a target") {
                Ok(v) => target = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--runner" => match value("a file") {
                Ok(v) => runner = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            "-o" | "--output" => match value("a file") {
                Ok(v) => out = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            other if other.starts_with('-') => {
                return crate::usage(format!("{other:?} is not an option of `build`"));
            }
            other if dir.is_none() => dir = Some(PathBuf::from(other)),
            other => return crate::usage(format!("{other:?}: one directory is enough")),
        }
    }
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let request = Request {
        dir: &dir,
        web: web.as_deref(),
        name: name.as_deref(),
        target: target.as_deref(),
        runner: runner.as_deref(),
        out: out.as_deref(),
    };
    if crate::dev_cargo::is_project(&dir) {
        return match build_cargo(&request) {
            Ok(out) => {
                let size = fs::metadata(&out).map_or(0, |meta| meta.len());
                println!(
                    "pencere: built {} ({:.1} MiB)",
                    out.display(),
                    size as f64 / (1024.0 * 1024.0)
                );
                ExitCode::SUCCESS
            }
            Err(why) => {
                eprintln!("pencere: {why}");
                ExitCode::from(1)
            }
        };
    }
    match build(&request) {
        Ok((out, count, _)) => {
            let size = fs::metadata(&out).map_or(0, |meta| meta.len());
            println!(
                "pencere: built {} ({} files in it, {:.1} MiB)",
                out.display(),
                count,
                size as f64 / (1024.0 * 1024.0)
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_one_of_the_four() {
        assert_eq!(
            canonical("x86_64-unknown-linux-musl"),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(canonical("x86_64-pc-windows-msvc"), None);
        assert_eq!(canonical(OWN), Some(OWN), "this program is its own runner");
    }

    /// A release in a directory of the test's own, which `curl` reads as a `file:` URL.
    fn release_in(name: &str, runner: &[u8], listed_sum: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("pencere-release-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let release = root.join(format!("releases/cli-v{VERSION}"));
        fs::create_dir_all(&release).expect("a release directory");
        let file = release_file("x86_64-unknown-linux-musl");
        fs::write(release.join(&file), runner).expect("a runner");
        fs::write(
            release.join("SHA256SUMS"),
            format!("{listed_sum}  {file}\n"),
        )
        .expect("sums");
        (root.join("releases"), root.join("cache"))
    }

    /// Whether `curl` is here: the downloads of a runner are made with it.
    fn curl_is_here() -> bool {
        let here = Command::new("curl").arg("--version").output().is_ok();
        if !here {
            println!("SKIPPED: curl is not installed");
        }
        here
    }

    #[test]
    fn the_runner_of_another_target_is_fetched_checked_and_kept() {
        if !curl_is_here() {
            return;
        }
        let runner = b"\x7fELF a runner";
        let (releases, cache) = release_in("good", runner, &sha256_hex(runner));
        let base = format!("file://{}", releases.display());
        let target = "x86_64-unknown-linux-musl";
        assert_eq!(
            download_from(&base, &cache, target).expect("fetched"),
            runner
        );
        // Kept: the release can be gone and the next build does not need it.
        fs::remove_dir_all(&releases).expect("removed");
        assert_eq!(
            download_from(&base, &cache, target).expect("from the cache"),
            runner
        );
        let _ = fs::remove_dir_all(cache.parent().expect("the root"));
    }

    #[test]
    fn a_runner_that_is_not_the_one_the_release_lists_is_refused_and_not_kept() {
        if !curl_is_here() {
            return;
        }
        let (releases, cache) =
            release_in("bad", b"\x7fELF a runner", &sha256_hex(b"something else"));
        let base = format!("file://{}", releases.display());
        let why = download_from(&base, &cache, "x86_64-unknown-linux-musl").expect_err("refused");
        assert!(why.contains("SHA256SUMS"), "{why}");
        assert!(
            !cache
                .join(release_file("x86_64-unknown-linux-musl"))
                .exists()
        );
        let why =
            download_from(&base, &cache, "aarch64-apple-darwin").expect_err("not in the release");
        assert!(why.contains("could not be fetched"), "{why}");
        let _ = fs::remove_dir_all(cache.parent().expect("the root"));
    }

    #[test]
    fn the_release_names_a_runner_by_version_and_target() {
        assert_eq!(
            release_file("aarch64-apple-darwin"),
            format!("pencere-{VERSION}-aarch64-apple-darwin")
        );
    }
}
