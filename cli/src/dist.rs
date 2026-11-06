//! `pencere dist <version>`: the release step of an app. It makes (or takes) the executable file of
//! each target, and writes beside them `SHA256SUMS`, the `app.json` that names the release
//! (`spec/draft/apps.md`) and its signature. It uploads nothing: that is the author's.
//!
//! What the release says it needs, `protocol` and `views`, is not written by the author: the
//! program says it, in the `hello` it sends as it starts. The file of this machine's target is
//! started on a pty of its own, in an empty folder with a home of its own, the first `hello` is
//! read and the program is ended.

use std::fs;
use std::io;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signer as _, SigningKey};
use pencere_host::apps::{self, Manifest, Previous, Version};
use pencere_wire::base64;
use pencere_wire::frame::{Direction, Event, StreamDecoder};
use pencere_wire::message::{Hello, ViewOffer};

use crate::build_cmd::{OWN, Request, TARGETS, build, sha256_hex};
use crate::key;

/// How long a program has to say hello.
const HELLO_WITHIN: Duration = Duration::from_secs(5);

/// What the command is told.
struct Options {
    version: String,
    dir: PathBuf,
    name: Option<String>,
    files: Option<PathBuf>,
    key: Option<PathBuf>,
    base: Option<String>,
    tag: Option<String>,
    previous: Option<String>,
    describe_args: Option<String>,
    describe_with: Option<PathBuf>,
}

pub fn command(args: &[String]) -> ExitCode {
    let mut version = None;
    let mut dir = None;
    let mut name = None;
    let mut files = None;
    let mut key = None;
    let mut base = None;
    let mut tag = None;
    let mut previous = None;
    let mut describe_args = None;
    let mut describe_with = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match arg.as_str() {
            "--name" => match value("a name") {
                Ok(v) => name = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--files" => match value("a directory") {
                Ok(v) => files = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            "--key" => match value("a file") {
                Ok(v) => key = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            "--base" => match value("an address") {
                Ok(v) => base = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--tag" => match value("a tag") {
                Ok(v) => tag = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--previous" => match value("an address, a file or none") {
                Ok(v) => previous = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--describe-args" => match value("the arguments to start the program with") {
                Ok(v) => describe_args = Some(v),
                Err(why) => return crate::usage(why),
            },
            "--describe-with" => match value("a file") {
                Ok(v) => describe_with = Some(PathBuf::from(v)),
                Err(why) => return crate::usage(why),
            },
            other if other.starts_with('-') => {
                return crate::usage(format!("{other:?} is not an option of `dist`"));
            }
            other if version.is_none() => version = Some(other.to_owned()),
            other if dir.is_none() => dir = Some(PathBuf::from(other)),
            other => return crate::usage(format!("{other:?}: one version and one directory")),
        }
    }
    let Some(version) = version else {
        return crate::usage("dist needs the version being released, MAJOR.MINOR.PATCH");
    };
    if Version::parse(&version).is_none() {
        return crate::usage(format!(
            "{version:?} is not a version: three numbers, MAJOR.MINOR.PATCH"
        ));
    }
    let options = Options {
        version,
        dir: dir.unwrap_or_else(|| PathBuf::from(".")),
        name,
        files,
        key,
        base,
        tag,
        previous,
        describe_args,
        describe_with,
    };
    match run(&options) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::from(1)
        }
    }
}

/// One file of the release: its target, its name in the directory and its digest.
struct Built {
    target: String,
    file: String,
    sha256: String,
}

fn run(options: &Options) -> Result<String, String> {
    let version = Version::parse(&options.version).ok_or("the version is not three numbers")?;
    let root = options
        .dir
        .canonicalize()
        .map_err(|why| format!("{}: {why}", options.dir.display()))?;
    // The key first: an author without one should not wait for four targets to be built.
    let key_file = key::key_path(options.key.as_deref())?;
    let signing = key::load(&key_file)?;
    let public = key::public_text(&signing);

    let cargo = crate::dev_cargo::is_project(&root);
    let package = if cargo {
        Some(cargo_package(&root)?)
    } else {
        None
    };
    if let Some(package) = &package {
        match &package.version {
            Some(declared) if *declared == options.version => {}
            Some(declared) => {
                return Err(format!(
                    "Cargo.toml says version {declared} and the command was given {}: a release \
                     is one version",
                    options.version
                ));
            }
            None => {
                return Err(
                    "Cargo.toml has no version of its own in [package] for the command to \
                     compare with"
                        .to_owned(),
                );
            }
        }
    }
    let name = match (&options.name, &package) {
        (Some(name), _) => name.clone(),
        (None, Some(package)) => package
            .name
            .clone()
            .ok_or("Cargo.toml has no name in [package]: --name names the app")?,
        (None, None) => crate::files::Project::find(&root, None)?.name()?,
    };
    if !apps::is_name(&name) {
        return Err(format!(
            "{name:?} is not a name for an app (lowercase letters, digits and - from 1 to 64 \
             bytes): --name names one"
        ));
    }

    let (directory, built) = match &options.files {
        Some(dir) => {
            let dir = dir
                .canonicalize()
                .map_err(|why| format!("{}: {why}", dir.display()))?;
            let built = collect(&dir, &name, &options.version)?;
            (dir, built)
        }
        None if cargo => {
            return Err(
                "a Rust program is built on the machine of each of its targets: build them, put \
                 <name>-<version>-<target> for each in a directory and name it with --files"
                    .to_owned(),
            );
        }
        None => {
            let directory = root.join("dist");
            let mut built = Vec::new();
            for target in TARGETS {
                let file = format!("{name}-{}-{target}", options.version);
                let out = directory.join(&file);
                eprintln!("pencere: making {file}");
                build(&Request {
                    dir: &root,
                    web: None,
                    name: Some(&name),
                    target: Some(target),
                    runner: None,
                    out: Some(&out),
                })?;
                built.push(describe_file(target, &file, &out)?);
            }
            (directory, built)
        }
    };
    let mut sums = String::new();
    for entry in &built {
        sums.push_str(&format!("{}  {}\n", entry.sha256, entry.file));
    }
    write(&directory.join("SHA256SUMS"), sums.as_bytes())?;

    let program = match &options.describe_with {
        Some(file) => file
            .canonicalize()
            .map_err(|why| format!("{}: {why}", file.display()))?,
        None => {
            let own = built
                .iter()
                .find(|entry| entry.target == OWN)
                .ok_or_else(|| {
                    format!(
                        "there is no file for {OWN}, the target of this machine, to ask what the \
                         program needs: --describe-with FILE names another program that can be \
                         started here"
                    )
                })?;
            directory.join(&own.file)
        }
    };
    let program_args: Vec<String> = options
        .describe_args
        .as_deref()
        .map(|text| text.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    let hello = hello_of(&program, &program_args, HELLO_WITHIN)?;

    let origin = github_repo(&root);
    let base = match (&options.base, &origin) {
        (Some(base), _) => {
            if base.ends_with('/') {
                base.clone()
            } else {
                format!("{base}/")
            }
        }
        (None, Some((owner, repo))) => {
            let tag = options
                .tag
                .clone()
                .unwrap_or_else(|| format!("v{}", options.version));
            format!("https://github.com/{owner}/{repo}/releases/download/{tag}/")
        }
        (None, None) => {
            return Err(
                "there is no GitHub origin to take the address of the files from: --base names \
                 the address they will be at"
                    .to_owned(),
            );
        }
    };

    let previous = previous_of(options, origin.as_ref(), &name, &public, version, &hello)?;

    let text = app_json(
        &name,
        &options.version,
        &hello,
        &public,
        &base,
        &built,
        &previous,
    );
    let signature = format!("{}\n", signature_text(&signing, text.as_bytes()));
    write(&directory.join("app.json"), text.as_bytes())?;
    write(&directory.join("app.json.sig"), signature.as_bytes())?;
    // What was written is read back the way every terminal will read it.
    let written = apps::verify(text.as_bytes(), signature.as_bytes())
        .map_err(|why| format!("the app.json that was made is not one that can be read: {why}"))?;
    if written.key != public || written.version != version {
        return Err("the app.json that was made does not say what it was made from".to_owned());
    }
    Ok(format!(
        "pencere: {name} {} is in {}: {} files, SHA256SUMS, app.json and app.json.sig, signed by \
         {public}; {} earlier {} listed. Upload them all with the release.",
        options.version,
        directory.display(),
        built.len(),
        previous.len(),
        if previous.len() == 1 {
            "release"
        } else {
            "releases"
        },
    ))
}

/// The digest of the file at `path`, as an entry of the release.
fn describe_file(target: &str, file: &str, path: &Path) -> Result<Built, String> {
    let bytes = fs::read(path).map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(Built {
        target: target.to_owned(),
        file: file.to_owned(),
        sha256: sha256_hex(&bytes),
    })
}

/// The files `<name>-<version>-<target>` in `dir`, one per target, by target.
fn collect(dir: &Path, name: &str, version: &str) -> Result<Vec<Built>, String> {
    let prefix = format!("{name}-{version}-");
    let mut found = Vec::new();
    let entries = fs::read_dir(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|why| format!("{}: {why}", dir.display()))?;
        let path = entry.path();
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(target) = file.strip_prefix(&prefix) else {
            continue;
        };
        if !path.is_file() || !apps::is_target(target) {
            continue;
        }
        found.push(describe_file(target, &file, &path)?);
    }
    found.sort_by(|a, b| a.target.cmp(&b.target));
    if found.is_empty() {
        return Err(format!(
            "{} has no file named {prefix}<target>",
            dir.display()
        ));
    }
    Ok(found)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|why| format!("{}: {why}", path.display()))
}

/// What `[package]` of a `Cargo.toml` says of the program.
struct Package {
    name: Option<String>,
    version: Option<String>,
}

/// The name and the version in `[package]` of `Cargo.toml`, read line by line: what is written
/// as `key = "text"` there. A version that is `version.workspace = true` is not one of them.
fn cargo_package(root: &Path) -> Result<Package, String> {
    let path = root.join("Cargo.toml");
    let text = fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))?;
    let mut package = Package {
        name: None,
        version: None,
    };
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[package]";
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|rest| rest.split_once('"'))
            .map(|(text, _)| text.to_owned())
        else {
            continue;
        };
        match key.trim() {
            "name" => package.name = Some(value),
            "version" => package.version = Some(value),
            _ => {}
        }
    }
    Ok(package)
}

/// `owner` and `repo` of a GitHub `origin` remote of the repository `dir` is in.
fn github_repo(dir: &Path) -> Option<(String, String)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?;
    let url = url.trim();
    let path = [
        "git@github.com:",
        "ssh://git@github.com/",
        "https://github.com/",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_owned(), repo.to_owned()))
}

/// The signature text of `bytes`, as `app.json.sig` holds it.
fn signature_text(key: &SigningKey, bytes: &[u8]) -> String {
    format!("ed25519:{}", base64::encode(&key.sign(bytes).to_bytes()))
}

fn json(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

fn protocol_json(versions: &[u32]) -> String {
    serde_json::to_string(versions).unwrap_or_default()
}

fn views_json(views: &[ViewOffer]) -> String {
    serde_json::to_string(views).unwrap_or_default()
}

/// The text of the `app.json`, the members in the order the specification lists them.
fn app_json(
    name: &str,
    version: &str,
    hello: &Hello,
    key: &str,
    base: &str,
    built: &[Built],
    previous: &[Previous],
) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"name\": {},\n", json(name)));
    out.push_str(&format!("  \"version\": {},\n", json(version)));
    out.push_str(&format!(
        "  \"protocol\": {},\n",
        protocol_json(&hello.versions)
    ));
    out.push_str(&format!("  \"views\": {},\n", views_json(&hello.views)));
    out.push_str(&format!("  \"key\": {},\n", json(key)));
    out.push_str("  \"files\": {\n");
    for (at, entry) in built.iter().enumerate() {
        out.push_str(&format!("    {}: {{\n", json(&entry.target)));
        out.push_str(&format!(
            "      \"url\": {},\n",
            json(&format!("{base}{}", entry.file))
        ));
        out.push_str(&format!("      \"sha256\": {}\n", json(&entry.sha256)));
        out.push_str(if at + 1 == built.len() {
            "    }\n"
        } else {
            "    },\n"
        });
    }
    out.push_str("  },\n  \"previous\": [");
    for (at, entry) in previous.iter().enumerate() {
        out.push_str(if at == 0 { "\n    {\n" } else { ",\n    {\n" });
        out.push_str(&format!(
            "      \"version\": {},\n",
            json(&entry.version.to_string())
        ));
        out.push_str(&format!(
            "      \"protocol\": {},\n",
            protocol_json(&entry.protocol)
        ));
        out.push_str(&format!("      \"views\": {},\n", views_json(&entry.views)));
        out.push_str(&format!(
            "      \"manifest\": {}\n    }}",
            json(&entry.manifest)
        ));
    }
    out.push_str(if previous.is_empty() {
        "]\n}"
    } else {
        "\n  ]\n}"
    });
    out
}

/// The earlier releases the new one lists: those of the release published before, and that
/// release itself, each requirement once and at its newest, the one the new release has dropped.
fn previous_of(
    options: &Options,
    origin: Option<&(String, String)>,
    name: &str,
    public: &str,
    version: Version,
    hello: &Hello,
) -> Result<Vec<Previous>, String> {
    let address = match (options.previous.as_deref(), origin) {
        (Some("none"), _) => return Ok(Vec::new()),
        (Some(given), _) if given.starts_with("https:") || given.starts_with("file:") => {
            given.to_owned()
        }
        (Some(path), _) => {
            let absolute = Path::new(path)
                .canonicalize()
                .or_else(|_| std::path::absolute(path))
                .map_err(|why| format!("{path}: {why}"))?;
            format!("file://{}", absolute.display())
        }
        (None, Some((owner, repo))) => {
            format!("https://github.com/{owner}/{repo}/releases/latest/download/app.json")
        }
        (None, None) => {
            return Err(
                "there is no GitHub origin to look for the earlier release at: --previous names \
                 its app.json (an address or a file), or none for a first release"
                    .to_owned(),
            );
        }
    };
    let Some(bytes) = fetch_optional(&address)? else {
        eprintln!(
            "pencere: there is no app.json at {address}: this is taken for the first release"
        );
        return Ok(Vec::new());
    };
    let signature = fetch_optional(&apps::signature_address(&address))?.ok_or_else(|| {
        format!("there is an app.json at {address} and no app.json.sig beside it")
    })?;
    let earlier = apps::verify(&bytes, &signature)
        .map_err(|why| format!("the earlier app.json at {address} cannot be taken: {why}"))?;
    if earlier.name != name {
        return Err(format!(
            "the earlier release at {address} is of the app {:?}, not {name:?}",
            earlier.name
        ));
    }
    if earlier.key != public {
        return Err(format!(
            "the earlier release at {address} is signed with another key ({}) than this one \
             ({public}); a release cannot be followed by one of another key, and changing the \
             key is not something this command does",
            earlier.key
        ));
    }
    if earlier.version == version {
        return Err(format!(
            "the version {version} is already released: {address} has it"
        ));
    }
    let manifest = if address.contains("/releases/latest/download/") {
        // The address of the latest release moves with the next one: the earlier release is
        // kept at the place its own files are.
        earlier
            .files
            .values()
            .find_map(|file| {
                file.url
                    .rsplit_once('/')
                    .map(|(dir, _)| format!("{dir}/app.json"))
            })
            .ok_or("the earlier release lists no file to tell where it is kept from")?
    } else {
        address.clone()
    };
    Ok(merge(&earlier, manifest, &hello.versions, &hello.views))
}

/// The list of a new release that needs `protocol` and `views`, made from the earlier release
/// and what it listed (specification Section 4.6).
fn merge(
    earlier: &Manifest,
    manifest: String,
    protocol: &[u32],
    views: &[ViewOffer],
) -> Vec<Previous> {
    let mut all = earlier.previous.clone();
    all.push(Previous {
        version: earlier.version,
        protocol: earlier.protocol.clone(),
        views: earlier.views.clone(),
        manifest,
    });
    all.retain(|entry| !(entry.protocol == protocol && entry.views == views));
    let mut kept: Vec<Previous> = Vec::new();
    for entry in all {
        match kept
            .iter_mut()
            .find(|held| held.protocol == entry.protocol && held.views == entry.views)
        {
            Some(held) if entry.version > held.version => *held = entry,
            Some(_) => {}
            None => kept.push(entry),
        }
    }
    kept.sort_by_key(|entry| entry.version);
    let excess = kept.len().saturating_sub(apps::MAX_PREVIOUS);
    kept.drain(..excess);
    kept
}

/// The bytes at `address`, or `None` when there is nothing there: a 404, or a file that does not
/// exist. Any other failure is an error.
fn fetch_optional(address: &str) -> Result<Option<Vec<u8>>, String> {
    let scratch = scratch_dir("fetch")?;
    let to = scratch.join("file");
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "2",
            "--max-time",
            "60",
            "--max-filesize",
        ])
        .arg((apps::MAX_MANIFEST_BYTES + 1).to_string())
        .args(["--write-out", "%{http_code}", "--output"])
        .arg(&to)
        .arg(address)
        .output();
    let result = match output {
        Err(why) => Err(format!(
            "curl cannot be run ({why}): it fetches the earlier release of the app; --previous \
             none says there is none"
        )),
        Ok(output) => {
            let code = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            match output.status.code() {
                // `file:` has no status line: curl's own for a file that is not there.
                Some(37) => Ok(None),
                Some(0) if code == "404" => Ok(None),
                Some(0)
                    if code.starts_with('2') || (code == "000" && address.starts_with("file:")) =>
                {
                    fs::read(&to)
                        .map(Some)
                        .map_err(|why| format!("{}: {why}", to.display()))
                }
                _ => Err(format!(
                    "{address} could not be fetched ({}): {}",
                    if code.is_empty() { "no answer" } else { &code },
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
            }
        }
    };
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// A new directory of this command's own in the temporary directory.
fn scratch_dir(what: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "pencere-dist-{what}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    Ok(dir)
}

fn check(rc: libc::c_int) -> io::Result<libc::c_int> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

/// A pty of 80 columns and 24 rows: the master, and the slave end.
#[allow(unsafe_code)]
fn open_pty() -> io::Result<(OwnedFd, OwnedFd)> {
    let (mut master, mut slave) = (-1, -1);
    let mut winsize = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: both descriptor out-pointers are valid, the name and the termios are null, and the
    // winsize outlives the call (macOS declares it `*mut` and Linux `*const`: a raw pointer fits
    // both).
    check(unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut winsize,
        )
    })?;
    // SAFETY: openpty returned two fresh descriptors that nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        // SAFETY: the descriptor is open and ours.
        check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    }
    Ok((master, slave))
}

/// Starts `program` on a pty, in a folder and with a home of its own, reads its first `hello` and
/// ends it. The program is told nothing of where it is: a person's `HOME`, `XDG_*` folders and
/// editor are not for it to read, and what it writes there is gone with the folder.
fn hello_of(program: &Path, args: &[String], within: Duration) -> Result<Hello, String> {
    let scratch = scratch_dir("hello")?;
    let result = ask(program, args, &scratch, within);
    let _ = fs::remove_dir_all(&scratch);
    result
}

#[allow(unsafe_code)]
fn ask(program: &Path, args: &[String], scratch: &Path, within: Duration) -> Result<Hello, String> {
    let folder = scratch.join("cwd");
    fs::create_dir_all(&folder).map_err(|why| format!("{}: {why}", folder.display()))?;
    let (master, slave) = open_pty().map_err(|why| format!("no pty could be opened: {why}"))?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&folder)
        .env("HOME", scratch.join("home"))
        .env("XDG_CONFIG_HOME", scratch.join("config"))
        .env("XDG_STATE_HOME", scratch.join("state"))
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("XDG_CACHE_HOME", scratch.join("cache"))
        .env("EDITOR", "true")
        .env("VISUAL", "true")
        .stdin(Stdio::from(
            slave.try_clone().map_err(|why| why.to_string())?,
        ))
        .stdout(Stdio::from(
            slave.try_clone().map_err(|why| why.to_string())?,
        ))
        .stderr(Stdio::from(slave));
    // SAFETY: the closure runs in the forked child before exec and makes two system calls, both
    // async-signal-safe. Stdin is already the slave.
    unsafe {
        command.pre_exec(|| {
            check(libc::setsid())?;
            check(libc::ioctl(0, libc::TIOCSCTTY as _, 0))?;
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|why| format!("{} could not be started: {why}", program.display()))?;
    // The command holds the slave: while it did, no end of file could come.
    drop(command);
    let result = read_hello(&master, within);
    end(&mut child);
    result.map_err(|why| {
        format!(
            "{} was started to ask it what it needs: {why}",
            program.display()
        )
    })
}

/// Reads what the program writes until the first `hello`.
#[allow(unsafe_code)]
fn read_hello(master: &OwnedFd, within: Duration) -> Result<Hello, String> {
    let until = Instant::now() + within;
    let mut decoder = StreamDecoder::new(Direction::ToHost);
    let mut buffer = [0u8; 4096];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(format!(
                "it did not say hello in {} seconds (a program that needs arguments to start is \
                 given them with --describe-args)",
                within.as_secs()
            ));
        }
        let mut poll = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = libc::c_int::try_from(left.as_millis()).unwrap_or(libc::c_int::MAX);
        // SAFETY: one valid pollfd, and the count says so.
        let ready = unsafe { libc::poll(&mut poll, 1, millis) };
        if ready < 0 {
            let why = io::Error::last_os_error();
            if why.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(why.to_string());
        }
        if ready == 0 {
            continue;
        }
        // SAFETY: the buffer is valid for its whole length.
        let read =
            unsafe { libc::read(master.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        let count = match usize::try_from(read) {
            Ok(0) => return Err(ended()),
            Ok(count) => count,
            Err(_) => {
                let why = io::Error::last_os_error();
                match why.kind() {
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => continue,
                    // What a pty master reports once the program has gone.
                    _ if why.raw_os_error() == Some(libc::EIO) => return Err(ended()),
                    _ => return Err(why.to_string()),
                }
            }
        };
        let mut events = Vec::new();
        decoder.feed(buffer.get(..count).unwrap_or_default(), &mut events);
        for event in events {
            if let Event::Message(message) = event
                && message.kind == "hello"
            {
                return pencere_wire::json::from_object::<Hello>(&message.payload)
                    .ok_or_else(|| "its hello is not one that can be read".to_owned());
            }
        }
    }
}

fn ended() -> String {
    "it ended without saying hello (a program says hello when its input and its output are a \
     terminal, as they are here; one that needs arguments to start is given them with \
     --describe-args)"
        .to_owned()
}

/// Ends the program and what it started: it is the leader of a session of its own.
#[allow(unsafe_code)]
fn end(child: &mut Child) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: a signal to the group of the process this one started and has not waited for.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: &str, version: &str) -> ViewOffer {
        ViewOffer {
            kind: kind.to_owned(),
            versions: vec![version.to_owned()],
        }
    }

    fn release(version: &str, lua: &str) -> Previous {
        Previous {
            version: Version::parse(version).expect("a version"),
            protocol: vec![1],
            views: vec![view("lua", lua)],
            manifest: format!("https://example.com/v{version}/app.json"),
        }
    }

    fn earlier(version: &str, lua: &str, listed: Vec<Previous>) -> Manifest {
        let at = release(version, lua);
        Manifest {
            name: "demo".to_owned(),
            version: at.version,
            protocol: at.protocol,
            views: at.views,
            key: String::new(),
            files: std::collections::BTreeMap::new(),
            previous: listed,
        }
    }

    fn versions(list: &[Previous]) -> Vec<String> {
        list.iter().map(|entry| entry.version.to_string()).collect()
    }

    #[test]
    fn the_earlier_release_and_its_list_come_before_the_new_one_by_version() {
        let before = earlier(
            "0.5.0",
            "0.3",
            vec![release("0.2.0", "0.1"), release("0.4.0", "0.2")],
        );
        let merged = merge(
            &before,
            "https://example.com/v0.5.0/app.json".into(),
            &[1],
            &[view("lua", "0.4")],
        );
        assert_eq!(versions(&merged), ["0.2.0", "0.4.0", "0.5.0"]);
    }

    #[test]
    fn an_earlier_release_with_the_same_requirement_is_dropped() {
        let before = earlier("0.5.0", "0.3", vec![release("0.4.0", "0.2")]);
        let merged = merge(
            &before,
            "https://example.com/v0.5.0/app.json".into(),
            &[1],
            &[view("lua", "0.3")],
        );
        assert_eq!(versions(&merged), ["0.4.0"]);
    }

    #[test]
    fn of_two_with_one_requirement_the_newer_is_kept() {
        let before = earlier(
            "0.5.0",
            "0.3",
            vec![release("0.3.0", "0.2"), release("0.4.0", "0.2")],
        );
        let merged = merge(
            &before,
            "https://example.com/v0.5.0/app.json".into(),
            &[1],
            &[view("lua", "0.4")],
        );
        assert_eq!(versions(&merged), ["0.4.0", "0.5.0"]);
    }

    #[test]
    fn a_list_that_would_pass_the_limit_loses_its_lowest_versions() {
        let listed: Vec<Previous> = (0..apps::MAX_PREVIOUS)
            .map(|at| release(&format!("0.{at}.0"), &format!("0.{at}")))
            .collect();
        let before = earlier("0.40.0", "0.40", listed);
        let merged = merge(
            &before,
            "https://example.com/v0.40.0/app.json".into(),
            &[1],
            &[view("lua", "0.41")],
        );
        assert_eq!(merged.len(), apps::MAX_PREVIOUS);
        assert_eq!(
            merged
                .first()
                .map(|entry| entry.version.to_string())
                .as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            merged
                .last()
                .map(|entry| entry.version.to_string())
                .as_deref(),
            Some("0.40.0")
        );
    }

    #[test]
    fn a_github_remote_gives_the_owner_and_the_repository() {
        let dir = std::env::temp_dir().join(format!("pencere-remote-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .expect("git")
        };
        git(&["init", "--quiet"]);
        for (remote, expected) in [
            (
                "git@github.com:bateri/textpen.git",
                Some(("bateri", "textpen")),
            ),
            (
                "https://github.com/bateri/textpen",
                Some(("bateri", "textpen")),
            ),
            (
                "https://github.com/bateri/textpen.git/",
                Some(("bateri", "textpen")),
            ),
            (
                "ssh://git@github.com/bateri/textpen.git",
                Some(("bateri", "textpen")),
            ),
            ("https://gitlab.com/bateri/textpen.git", None),
            ("git@github.com:textpen.git", None),
        ] {
            git(&["remote", "remove", "origin"]);
            git(&["remote", "add", "origin", remote]);
            let found = github_repo(&dir);
            assert_eq!(
                found.as_ref().map(|(o, r)| (o.as_str(), r.as_str())),
                expected,
                "{remote}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cargo_toml_is_read_by_its_package_section() {
        let dir = std::env::temp_dir().join(format!("pencere-cargo-toml-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nname = \"not-this\"\n\n[package]\nname = \"demo\"\nversion = \"0.5.1\" # a comment\nedition = \"2024\"\n\n[dependencies]\nversion = \"9.9.9\"\n",
        )
        .expect("a Cargo.toml");
        let package = cargo_package(&dir).expect("read");
        assert_eq!(package.name.as_deref(), Some("demo"));
        assert_eq!(package.version.as_deref(), Some("0.5.1"));
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion.workspace = true\n",
        )
        .expect("a Cargo.toml");
        let package = cargo_package(&dir).expect("read");
        assert_eq!(package.version, None);
        let _ = fs::remove_dir_all(&dir);
    }
}
