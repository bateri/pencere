//! `pencere upgrade`: the command replaced by the newest release of it (or the one asked for), as
//! `cli/install.sh` would install it: the release's file for this machine, checked against the
//! release's `SHA256SUMS`, put in the place of the running file in one rename.
//!
//! A command that Cargo installed is Cargo's to replace (it keeps a record of what it installed),
//! and is not touched: the person is told the line that does it. An app built by `pencere build`
//! is a copy of the command with an app in it and runs its app; it never comes here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::build_cmd::{fetch, sha256_hex};

/// The repository the releases are in.
const REPO: &str = "bateri/pencere";

/// The tags of this command's releases are `cli-v<version>`.
const TAG_PREFIX: &str = "cli-v";

/// This command's version and target.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const OWN: &str = env!("PENCERE_TARGET");

/// Where the listing of the releases and their files are: GitHub, or the directory
/// `PENCERE_RELEASES` names (a mirror, or a test's), which holds `releases.json`, the listing as the
/// API answers it, and a directory `<tag>/` for each release.
struct Source {
    listing: String,
    files: String,
}

impl Source {
    fn here() -> Self {
        match std::env::var("PENCERE_RELEASES") {
            Ok(base) if !base.is_empty() => {
                let base = base.trim_end_matches('/').to_owned();
                Self {
                    listing: format!("{base}/releases.json"),
                    files: base,
                }
            }
            _ => Self {
                listing: format!("https://api.github.com/repos/{REPO}/releases?per_page=100"),
                files: format!("https://github.com/{REPO}/releases/download"),
            },
        }
    }
}

/// A version, `x.y.z`, as three numbers to compare.
fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let numbers = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(numbers)
}

/// The target of the release's file for this machine: the command's own, except that on Linux the
/// releases are static (musl), whatever libc this one was built for.
fn release_target(own: &str) -> String {
    match own.split_once("-unknown-linux-") {
        Some((arch, _)) => format!("{arch}-unknown-linux-musl"),
        None => own.to_owned(),
    }
}

/// The name of the release's file for `target`.
fn file_of(version: &str, target: &str) -> String {
    format!("pencere-{version}-{target}")
}

/// The newest version of the command in the listing that has a file for `target`, as
/// `cli/install.sh` chooses it: drafts, pre-releases, the other lines' tags and a release without
/// the file (`cli-v0.1.0` was another package's) do not count.
fn newest(listing: &serde_json::Value, target: &str) -> Option<String> {
    listing
        .as_array()?
        .iter()
        .filter(|release| {
            !release
                .get("draft")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                && !release
                    .get("prerelease")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .filter_map(|release| {
            let tag = release.get("tag_name")?.as_str()?;
            let version = tag.strip_prefix(TAG_PREFIX)?;
            let numbers = parse(version)?;
            let wanted = file_of(version, target);
            let has = release.get("assets")?.as_array()?.iter().any(|asset| {
                asset.get("name").and_then(serde_json::Value::as_str) == Some(&wanted)
            });
            has.then(|| (numbers, version.to_owned()))
        })
        .max_by_key(|(numbers, _)| *numbers)
        .map(|(_, version)| version)
}

/// Whether the command at `exe` is one Cargo installed: in Cargo's `bin` directory.
fn installed_by_cargo(exe: &Path) -> bool {
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .map(|home| home.join("bin"));
    cargo_bin.is_some_and(|bin| exe.parent() == Some(bin.as_path()))
}

/// What `upgrade` did.
#[derive(Debug, PartialEq, Eq)]
enum Done {
    /// The command is the version asked for already.
    Current(String),
    /// A newer one is there (`--check`).
    Newer(String),
    /// The file was replaced with this version.
    Replaced(String),
}

/// Upgrades the command at `exe` from `source` to `version` (the newest when `None`), or with
/// `check` only says whether there is a newer one.
fn upgrade(
    exe: &Path,
    source: &Source,
    version: Option<&str>,
    check: bool,
) -> Result<Done, String> {
    let scratch = std::env::temp_dir().join(format!("pencere-upgrade-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|why| format!("{}: {why}", scratch.display()))?;
    let result = (|| {
        let version = match version {
            Some(version) => {
                let version = version.trim_start_matches('v').to_owned();
                parse(&version).ok_or_else(|| format!("{version:?} is not a version (x.y.z)"))?;
                version
            }
            None => {
                let listing_file = scratch.join("releases.json");
                fetch(&source.listing, &listing_file).map_err(|why| {
                    format!("{why}: the releases cannot be listed (is there a network, and has GitHub not answered too many times?)")
                })?;
                let text = fs::read_to_string(&listing_file).map_err(|why| why.to_string())?;
                let listing: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|why| format!("the listing of the releases is not JSON: {why}"))?;
                newest(&listing, &release_target(OWN)).ok_or_else(|| {
                    format!(
                        "{REPO} has no release of pencere for {} yet",
                        release_target(OWN)
                    )
                })?
            }
        };
        if version == VERSION {
            return Ok(Done::Current(version));
        }
        if check {
            let newer =
                matches!((parse(&version), parse(VERSION)), (Some(new), Some(old)) if new > old);
            return Ok(if newer {
                Done::Newer(version)
            } else {
                Done::Current(VERSION.to_owned())
            });
        }
        if installed_by_cargo(exe) {
            return Err(format!(
                "{} was installed by Cargo, which keeps a record of it: \
                 `cargo install --git https://github.com/{REPO} pencere-cli --locked --force` replaces it",
                exe.display()
            ));
        }
        let tag = format!("{TAG_PREFIX}{version}");
        let target = release_target(OWN);
        let file = file_of(&version, &target);
        let downloaded = scratch.join(&file);
        let sums = scratch.join("SHA256SUMS");
        fetch(&format!("{}/{tag}/{file}", source.files), &downloaded)
            .map_err(|why| format!("{why}: has the release {tag} a file for {target}?"))?;
        fetch(&format!("{}/{tag}/SHA256SUMS", source.files), &sums)?;
        let listed = fs::read_to_string(&sums).map_err(|why| why.to_string())?;
        let wanted: Vec<&str> = listed
            .lines()
            .filter_map(|line| line.split_once(char::is_whitespace))
            .filter(|(_, name)| name.trim().trim_start_matches('*') == file)
            .map(|(sum, _)| sum)
            .collect();
        let [wanted] = wanted.as_slice() else {
            return Err(format!(
                "SHA256SUMS of {tag} does not name {file} exactly once; nothing was changed"
            ));
        };
        let bytes = fs::read(&downloaded).map_err(|why| why.to_string())?;
        if sha256_hex(&bytes) != *wanted {
            return Err(format!(
                "{file} is not the file SHA256SUMS of {tag} has the sum of; nothing was changed"
            ));
        }
        replace(exe, &bytes)?;
        Ok(Done::Replaced(version))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Puts `bytes` in the place of the file `exe`: written beside it and renamed onto it, so that the
/// file is never half written, and a program running the old one keeps it (macOS kills an
/// executable whose file is written over under its cached signature).
fn replace(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = exe.parent().ok_or("the command's file has no directory")?;
    let staged = dir.join(format!(".pencere.{}", std::process::id()));
    let written = fs::write(&staged, bytes)
        .and_then(|()| fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)))
        .and_then(|()| fs::rename(&staged, exe));
    if let Err(why) = written {
        let _ = fs::remove_file(&staged);
        return Err(match why.kind() {
            std::io::ErrorKind::PermissionDenied => format!(
                "{} cannot be written by you: install the new one where you can \
                 (curl -fsSL https://raw.githubusercontent.com/{REPO}/main/cli/install.sh | sh), or with the rights to write there",
                dir.display()
            ),
            _ => format!("{}: {why}", exe.display()),
        });
    }
    Ok(())
}

pub fn command(args: &[String]) -> ExitCode {
    let mut check = false;
    let mut version = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--version" => match args.next() {
                Some(value) => version = Some(value.clone()),
                None => return crate::usage("--version needs a version (x.y.z)"),
            },
            other => return crate::usage(format!("{other:?} is not an option of `upgrade`")),
        }
    }
    let exe = match std::env::current_exe().and_then(|exe| exe.canonicalize()) {
        Ok(exe) => exe,
        Err(why) => {
            eprintln!("pencere: the command's own file cannot be found: {why}");
            return ExitCode::from(1);
        }
    };
    match upgrade(&exe, &Source::here(), version.as_deref(), check) {
        Ok(Done::Current(version)) => {
            println!("pencere {version} is the newest");
            ExitCode::SUCCESS
        }
        Ok(Done::Newer(version)) => {
            println!("pencere {version} is out (this is {VERSION}): `pencere upgrade` installs it");
            ExitCode::SUCCESS
        }
        Ok(Done::Replaced(version)) => {
            println!("pencere {VERSION} -> {version}: {}", exe.display());
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
    use serde_json::json;

    fn release(tag: &str, files: &[&str], draft: bool) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": false,
            "assets": files.iter().map(|name| json!({ "name": name })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn the_newest_is_the_commands_own_with_a_file_for_the_machine() {
        let target = "aarch64-apple-darwin";
        let listing = json!([
            release("crates-v0.9.0", &[], false),
            release(
                "cli-v0.10.0",
                &["pencere-0.10.0-aarch64-apple-darwin"],
                true
            ),
            release(
                "cli-v0.3.0",
                &["pencere-0.3.0-x86_64-unknown-linux-musl"],
                false
            ),
            release(
                "cli-v0.2.10",
                &["pencere-0.2.10-aarch64-apple-darwin"],
                false
            ),
            release("cli-v0.2.9", &["pencere-0.2.9-aarch64-apple-darwin"], false),
            release("cli-v0.1.0", &["pencere-cli.tgz"], false),
        ]);
        // Not the draft, not the one without this machine's file, and 0.2.10 after 0.2.9.
        assert_eq!(newest(&listing, target), Some("0.2.10".to_owned()));
        assert_eq!(newest(&json!([]), target), None);
    }

    #[test]
    fn a_linux_machine_takes_the_static_release_whatever_this_was_built_for() {
        assert_eq!(
            release_target("x86_64-unknown-linux-gnu"),
            "x86_64-unknown-linux-musl"
        );
        assert_eq!(
            release_target("aarch64-unknown-linux-musl"),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            release_target("aarch64-apple-darwin"),
            "aarch64-apple-darwin"
        );
    }

    #[test]
    fn a_version_is_three_numbers() {
        assert_eq!(parse("0.2.10"), Some((0, 2, 10)));
        assert_eq!(parse("0.2"), None);
        assert_eq!(parse("0.2.x"), None);
        assert_eq!(parse("1.2.3.4"), None);
    }

    /// Whether `curl` is here: the release is read with it.
    fn curl_is_here() -> bool {
        let here = std::process::Command::new("curl")
            .arg("--version")
            .output()
            .is_ok();
        if !here {
            println!("SKIPPED: curl is not installed");
        }
        here
    }

    /// A release of `version` in a directory of the test's own, with this machine's file.
    fn release_dir(root: &Path, version: &str, bytes: &[u8], listed_sum: &str) -> Source {
        let tag = format!("{TAG_PREFIX}{version}");
        let file = file_of(version, &release_target(OWN));
        fs::create_dir_all(root.join(&tag)).unwrap();
        fs::write(root.join(&tag).join(&file), bytes).unwrap();
        fs::write(
            root.join(&tag).join("SHA256SUMS"),
            format!("{listed_sum}  {file}\n"),
        )
        .unwrap();
        fs::write(
            root.join("releases.json"),
            json!([release(&tag, &[&file], false)]).to_string(),
        )
        .unwrap();
        let base = format!("file://{}", root.display());
        Source {
            listing: format!("{base}/releases.json"),
            files: base,
        }
    }

    #[test]
    fn the_command_is_replaced_by_the_newest_release_checked_against_its_sums() {
        if !curl_is_here() {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("pencere-upgrade-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("pencere");
        fs::write(&exe, b"the old one").unwrap();
        let new = b"\x7fELF the new one";
        let source = release_dir(&root.join("releases"), "99.0.0", new, &sha256_hex(new));
        assert_eq!(
            upgrade(&exe, &source, None, true).unwrap(),
            Done::Newer("99.0.0".to_owned())
        );
        assert_eq!(
            fs::read(&exe).unwrap(),
            b"the old one",
            "a check changes nothing"
        );
        assert_eq!(
            upgrade(&exe, &source, None, false).unwrap(),
            Done::Replaced("99.0.0".to_owned())
        );
        assert_eq!(fs::read(&exe).unwrap(), new);
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            fs::metadata(&exe).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_dir(&bin).unwrap().count(),
            1,
            "nothing is left beside it"
        );
        // A file the sums do not list is refused, and the command stays as it was.
        let bad = release_dir(
            &root.join("bad"),
            "99.0.1",
            b"other",
            &sha256_hex(b"something else"),
        );
        let why = upgrade(&exe, &bad, None, false).unwrap_err();
        assert!(why.contains("SHA256SUMS"), "{why}");
        assert_eq!(fs::read(&exe).unwrap(), new);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_command_cargo_installed_is_left_to_cargo() {
        let home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
        if let Some(home) = home {
            assert!(installed_by_cargo(&home.join("bin/pencere")));
        }
        assert!(!installed_by_cargo(Path::new("/opt/elsewhere/bin/pencere")));
    }
}
