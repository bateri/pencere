//! `pencere test` finds the runner of tests and becomes it. The runner here is a script that says
//! what it was given, so that nothing of the kit is needed to see that the command hands over
//! everything: the arguments, the status and the place.

// A test fails by panicking.
#![allow(clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PENCERE: &str = env!("CARGO_BIN_EXE_pencere");

fn directory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pencere-test-cmd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir.canonicalize().expect("a path")
}

fn script(path: &Path, text: &str) {
    std::fs::write(path, text).expect("a script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("executable");
}

/// A copy of the command in a directory with nothing beside it: the one built here is next to
/// the runner of tests the workspace builds, which this is not about.
fn alone(dir: &Path) -> PathBuf {
    let copy = dir.join("pencere");
    // A new file, not the old one written over: macOS kills a signed program whose file changed
    // under its cached signature.
    let _ = std::fs::remove_file(&copy);
    std::fs::copy(PENCERE, &copy).expect("a copy of the command");
    copy
}

fn run(command: &Path, args: &[&str], env: &[(&str, &Path)]) -> (i32, String, String) {
    let mut run = Command::new(command);
    run.args(args)
        .env_remove("PENCERE_TEST")
        .env_remove("PENCERE_RELEASES")
        .stdin(Stdio::null());
    for (name, value) in env {
        run.env(name, value);
    }
    let output = run.output().expect("the command");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_runner_named_is_run_with_the_arguments_in_the_place_and_its_status_is_the_commands() {
    let dir = directory("named");
    let runner = dir.join("a-runner");
    script(&runner, "#!/bin/sh\necho \"cwd=$(pwd) args=$*\"\nexit 7\n");
    let (code, out, _) = run(
        &alone(&dir),
        &["test", "test/a_test.lua:3", "--grep", "two words"],
        &[("PENCERE_TEST", &runner)],
    );
    assert_eq!(code, 7);
    assert!(
        out.contains("args=test/a_test.lua:3 --grep two words"),
        "{out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_runner_that_is_not_there_is_said_and_so_is_how_to_make_one() {
    let dir = directory("missing");
    let nothing = dir.join("nothing-here");
    let command = alone(&dir);
    let (code, _, err) = run(&command, &["test"], &[("PENCERE_TEST", &nothing)]);
    assert_eq!(code, 1);
    assert!(
        err.contains("PENCERE_TEST names") && err.contains("not a file"),
        "{err}"
    );

    // Beside the command, nowhere named, and a release that is not there: it is told how to be had.
    let cache = dir.join("cache");
    let url = Path::new("file:///nonexistent-pencere-release");
    let (code, _, err) = run(
        &command,
        &["test"],
        &[("PENCERE_RELEASES", url), ("PENCERE_CACHE", &cache)],
    );
    assert_eq!(code, 1);
    assert!(
        err.contains("cargo build --release -p pencere-test"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_runner_beside_the_command_is_the_one_run() {
    let dir = directory("beside");
    let command = alone(&dir);
    script(
        &dir.join("pencere-test"),
        "#!/bin/sh\necho \"beside: $*\"\n",
    );
    let (code, out, err) = run(&command, &["test", "--jobs", "2"], &[]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "beside: --jobs 2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_runner_of_the_release_is_fetched_once_checked_and_kept() {
    let dir = directory("release");
    let version = env!("CARGO_PKG_VERSION");
    let target = env!("PENCERE_TARGET");
    let file = format!("pencere-test-{version}-{target}");
    let release = dir.join(format!("releases/cli-v{version}"));
    std::fs::create_dir_all(&release).expect("a directory");
    let text = "#!/bin/sh\necho \"from the release: $*\"\n";
    std::fs::write(release.join(&file), text).expect("a runner");
    // The sum is what `shasum` says of the file, as the release's SHA256SUMS lists it.
    let sum = Command::new("shasum")
        .args(["-a", "256"])
        .arg(release.join(&file))
        .output()
        .expect("shasum runs");
    let sum = String::from_utf8_lossy(&sum.stdout);
    let sum = sum.split_whitespace().next().expect("a sum").to_owned();
    std::fs::write(release.join("SHA256SUMS"), format!("{sum}  {file}\n")).expect("sums");
    let cache = dir.join("cache");
    let base = PathBuf::from(format!("file://{}", dir.join("releases").display()));
    let command = alone(&dir);
    let (code, out, err) = run(
        &command,
        &["test", "--grep", "x"],
        &[("PENCERE_RELEASES", &base), ("PENCERE_CACHE", &cache)],
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "from the release: --grep x");
    assert!(cache.join(&file).is_file(), "kept");
    // Kept: the release is not asked again.
    std::fs::remove_dir_all(dir.join("releases")).expect("the release is gone");
    let (code, out, _) = run(
        &command,
        &["test"],
        &[("PENCERE_RELEASES", &base), ("PENCERE_CACHE", &cache)],
    );
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "from the release:");

    // A file that is not the one the release lists is refused and not kept.
    let cache = dir.join("other-cache");
    std::fs::create_dir_all(&release).expect("a directory");
    std::fs::write(release.join(&file), "#!/bin/sh\necho changed\n").expect("a runner");
    std::fs::write(release.join("SHA256SUMS"), format!("{sum}  {file}\n")).expect("sums");
    let (code, _, err) = run(
        &command,
        &["test"],
        &[("PENCERE_RELEASES", &base), ("PENCERE_CACHE", &cache)],
    );
    assert_eq!(code, 1);
    assert!(
        err.contains("is not the file the release's SHA256SUMS lists"),
        "{err}"
    );
    assert!(!cache.join(&file).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
