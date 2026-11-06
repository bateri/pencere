//! `pencere key` and `pencere dist`: the key is made once and kept, and the release `dist` writes
//! is one that `pencere-host` reads the way a terminal will.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use pencere_host::apps::{self, Selection};
use pencere_host::session::{Offer, ViewKind};
use pencere_wire::base64;
use sha2::{Digest as _, Sha256};

const PENCERE: &str = env!("CARGO_BIN_EXE_pencere");

/// A new directory for one test, with nothing in it.
fn directory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pencere-dist-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir.canonicalize().expect("a path")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../spec/draft/fixtures")
        .canonicalize()
        .expect("the fixtures")
}

/// What `pencere` says and how it ends, run with a `HOME` of the test's own and no key named by
/// the environment.
fn pencere(home: &Path, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let mut run = Command::new(PENCERE);
    run.args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env_remove("PENCERE_SIGNING_KEY");
    for (name, value) in env {
        run.env(name, value);
    }
    let output = run.output().expect("pencere runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The key of the fixtures (`spec/draft/fixtures/apps/`) as `pencere key` keeps it: the seed
/// `first`, `first + 1`, ... is what the README of the fixtures says they are made of.
fn write_key(path: &Path, first: u8) {
    let seed: Vec<u8> = (first..first + 32).collect();
    fs::write(path, format!("ed25519-seed:{}\n", base64::encode(&seed))).expect("a key");
}

const TEST_KEY: &str = "ed25519:A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=";

/// A program that says hello with `hello` (the payload) when its input and its output are a
/// terminal, and records where it was started. It then waits to be ended.
fn program(dir: &Path, name: &str, hello: &str) -> PathBuf {
    let frame = dir.join(format!("{name}.frame"));
    fs::write(&frame, format!("\x1b_pencere;hello;id=1;{hello}\x1b\\")).expect("a frame");
    let record = dir.join(format!("{name}.record"));
    let path = dir.join(name);
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ -t 0 ] && [ -t 1 ]; then\n  printf '%s\\n' \"$HOME\" \"$PWD\" \"$EDITOR\" \"$VISUAL\" \"$XDG_CONFIG_HOME\" \"$XDG_STATE_HOME\" > '{}'\n  cat '{}'\nfi\nexec sleep 30\n",
            record.display(),
            frame.display()
        ),
    )
    .expect("a script");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
    path
}

const LUA_03: &str = r#"{"versions":[1],"views":[{"kind":"lua","versions":["0.3"]}]}"#;
const LUA_04: &str = r#"{"versions":[1],"views":[{"kind":"lua","versions":["0.4"]}]}"#;

/// A directory with the files of a release of `demopen`, for two targets.
fn files(dir: &Path, version: &str) -> PathBuf {
    let out = dir.join("release");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("a directory");
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"] {
        fs::write(
            out.join(format!("demopen-{version}-{target}")),
            format!("the program for {target}"),
        )
        .expect("a file");
    }
    // What is in the directory and is not a file of the release is left alone.
    fs::write(out.join("README"), "not a file of the release").expect("a file");
    fs::write(
        out.join("demopen-0.0.1-aarch64-apple-darwin"),
        "an older one",
    )
    .expect("a file");
    out
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The release `dist` wrote in `out`, read as a terminal reads it.
fn written(out: &Path) -> apps::Manifest {
    let text = fs::read(out.join("app.json")).expect("an app.json");
    let signature = fs::read(out.join("app.json.sig")).expect("a signature");
    apps::verify(&text, &signature).expect("a release that can be read")
}

#[test]
fn a_key_is_made_once_and_the_same_public_half_is_printed_after() {
    let home = directory("key");
    let (code, public, said) = pencere(&home, &home, &["key"], &[]);
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("keep a copy"), "{said}");
    let public = public.trim().to_owned();
    assert!(public.starts_with("ed25519:") && public.len() == "ed25519:".len() + 44);

    let file = home.join(".config/pencere/signing.key");
    assert_eq!(fs::metadata(&file).expect("the key").mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(file.parent().expect("a folder"))
            .expect("it")
            .mode()
            & 0o777,
        0o700
    );
    let secret = fs::read_to_string(&file).expect("text");
    assert!(secret.starts_with("ed25519-seed:") && secret.lines().count() == 1);

    // Again: the same public half, and the file is what it was.
    let (code, again, said) = pencere(&home, &home, &["key"], &[]);
    assert_eq!(code, 0, "{said}");
    assert_eq!(again.trim(), public);
    assert!(!said.contains("keep a copy"), "{said}");
    assert_eq!(fs::read_to_string(&file).expect("text"), secret);

    // Another key, another place; the environment names one too.
    let other = home.join("other.key");
    let (code, second, _) = pencere(
        &home,
        &home,
        &["key", "--key", other.to_str().expect("utf8")],
        &[],
    );
    assert_eq!(code, 0);
    assert_ne!(second.trim(), public);
    let named = home.join("named.key");
    let (code, third, _) = pencere(
        &home,
        &home,
        &["key"],
        &[("PENCERE_SIGNING_KEY", named.to_str().expect("utf8"))],
    );
    assert_eq!(code, 0);
    assert!(named.exists());
    assert_ne!(third.trim(), second.trim());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_file_that_is_not_a_key_is_not_written_over() {
    let home = directory("not-a-key");
    let file = home.join("signing.key");
    for text in ["", "hello\n", "ed25519-seed:AAAA\n", "ed25519-seed:!!!\n"] {
        fs::write(&file, text).expect("a file");
        let args = ["key", "--key", file.to_str().expect("utf8")];
        let (code, out, said) = pencere(&home, &home, &args, &[]);
        assert_eq!(code, 1, "{text:?}");
        assert!(out.is_empty());
        assert!(said.contains("is not a signing key"), "{said}");
        assert_eq!(fs::read_to_string(&file).expect("text"), text);
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn dist_without_a_key_stops_and_says_which_command_makes_one() {
    let home = directory("no-key");
    let release = files(&home, "0.6.0");
    let hello = program(&home, "demopen-hello", LUA_03);
    let args = [
        "dist",
        "0.6.0",
        "--files",
        release.to_str().expect("utf8"),
        "--describe-with",
        hello.to_str().expect("utf8"),
        "--base",
        "file:///somewhere/",
        "--previous",
        "none",
    ];
    let (code, _, said) = pencere(&home, &home, &args, &[]);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("`pencere key` makes one"), "{said}");
    assert!(!release.join("app.json").exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_release_made_from_files_names_them_and_what_the_program_says_it_needs() {
    let home = directory("files");
    write_key(&home.join("signing.key"), 0);
    let release = files(&home, "0.6.0");
    let hello = program(&home, "demopen-hello", LUA_03);
    let args = [
        "dist",
        "0.6.0",
        "--name",
        "demopen",
        "--files",
        release.to_str().expect("utf8"),
        "--describe-with",
        hello.to_str().expect("utf8"),
        "--base",
        "file:///somewhere/v0.6.0",
        "--previous",
        "none",
    ];
    let env = [(
        "PENCERE_SIGNING_KEY",
        home.join("signing.key").to_str().expect("utf8").to_owned(),
    )];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let (code, said, why) = pencere(&home, &home, &args, &env);
    assert_eq!(code, 0, "{why}");
    assert!(said.contains("2 files"), "{said}");

    let manifest = written(&release);
    assert_eq!(manifest.name, "demopen");
    assert_eq!(manifest.version.to_string(), "0.6.0");
    assert_eq!(manifest.key, TEST_KEY);
    assert_eq!(manifest.protocol, [1]);
    // What the program said, and nothing the author typed.
    assert_eq!(manifest.views.len(), 1);
    assert_eq!(manifest.views[0].kind, "lua");
    assert_eq!(manifest.views[0].versions, ["0.3"]);
    assert!(manifest.previous.is_empty());
    assert_eq!(
        manifest.files.len(),
        2,
        "the older file and the README are not in it"
    );
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"] {
        let entry = &manifest.files[target];
        assert_eq!(
            entry.url,
            format!("file:///somewhere/v0.6.0/demopen-0.6.0-{target}")
        );
        assert_eq!(
            entry.sha256,
            sha256(format!("the program for {target}").as_bytes())
        );
    }
    // A host that opens the lua view at 0.3 opens it.
    assert_eq!(
        manifest.select(&[1], &[Offer::new(ViewKind::Lua, &["0.3"])]),
        Selection::Current
    );
    // And the sums are the ones of the files, in the format the other tools read.
    let sums = fs::read_to_string(release.join("SHA256SUMS")).expect("sums");
    let mut expected = String::new();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"] {
        expected.push_str(&format!(
            "{}  demopen-0.6.0-{target}\n",
            sha256(format!("the program for {target}").as_bytes())
        ));
    }
    assert_eq!(sums, expected);

    // The program was asked in a place of its own, with the person's folders out of reach.
    let record = fs::read_to_string(home.join("demopen-hello.record")).expect("a record");
    let lines: Vec<&str> = record.lines().collect();
    assert_eq!(lines.len(), 6, "{record}");
    assert_ne!(Path::new(lines[0]), home.as_path());
    assert!(lines[0].contains("pencere-dist-hello-"), "{record}");
    assert!(lines[1].ends_with("/cwd"), "{record}");
    assert_eq!((lines[2], lines[3]), ("true", "true"));
    assert!(
        lines[4].starts_with(lines[0].rsplit_once('/').expect("a path").0),
        "{record}"
    );
    assert!(lines[5].contains("pencere-dist-hello-"), "{record}");
    let _ = fs::remove_dir_all(&home);
}

/// The args that release `version` of `demopen` from `release`, with the program that says hello.
fn release_args(release: &Path, hello: &Path, version: &str, previous: &str) -> Vec<String> {
    [
        "dist",
        version,
        "--name",
        "demopen",
        "--files",
        release.to_str().expect("utf8"),
        "--describe-with",
        hello.to_str().expect("utf8"),
        "--base",
        "file:///somewhere/",
        "--previous",
        previous,
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect()
}

fn run_dist(home: &Path, args: &[String], key: &Path) -> (i32, String, String) {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    pencere(
        home,
        home,
        &args,
        &[("PENCERE_SIGNING_KEY", key.to_str().expect("utf8"))],
    )
}

#[test]
fn the_list_of_earlier_releases_is_made_from_the_one_published_before() {
    let home = directory("previous");
    let key = home.join("signing.key");
    write_key(&key, 0);
    let valid = fixtures().join("apps/valid/app.json");
    let valid_address = format!("file://{}", valid.display());

    // A new requirement: the release before (0.5.1, lua 0.3) is listed, with what it listed.
    let release = files(&home, "0.6.0");
    let hello = program(&home, "new-requirement", LUA_04);
    let (code, _, why) = run_dist(
        &home,
        &release_args(&release, &hello, "0.6.0", valid.to_str().expect("utf8")),
        &key,
    );
    assert_eq!(code, 0, "{why}");
    let manifest = written(&release);
    let listed: Vec<String> = manifest
        .previous
        .iter()
        .map(|p| p.version.to_string())
        .collect();
    assert_eq!(listed, ["0.4.3", "0.5.1"]);
    assert_eq!(manifest.previous[1].manifest, valid_address);
    assert_eq!(manifest.previous[1].views[0].versions, ["0.3"]);
    // A host that opens lua 0.3 only is given 0.5.1, and one that opens 0.2, 0.4.3.
    for (lua, chosen) in [("0.3", "0.5.1"), ("0.2", "0.4.3")] {
        match manifest.select(&[1], &[Offer::new(ViewKind::Lua, &[lua])]) {
            Selection::Previous(earlier) => assert_eq!(earlier.version.to_string(), chosen),
            other => panic!("{other:?}"),
        }
    }

    // The same requirement as the release before: it is redundant, and what it listed stays.
    let release = files(&home, "0.6.0");
    let hello = program(&home, "same-requirement", LUA_03);
    let (code, _, why) = run_dist(
        &home,
        &release_args(&release, &hello, "0.6.0", valid.to_str().expect("utf8")),
        &key,
    );
    assert_eq!(code, 0, "{why}");
    let listed: Vec<String> = written(&release)
        .previous
        .iter()
        .map(|p| p.version.to_string())
        .collect();
    assert_eq!(listed, ["0.4.3"]);

    // The version that is released already.
    let release = files(&home, "0.5.1");
    let (code, _, said) = run_dist(
        &home,
        &release_args(&release, &hello, "0.5.1", valid.to_str().expect("utf8")),
        &key,
    );
    assert_eq!(code, 1);
    assert!(said.contains("already released"), "{said}");
    assert!(!release.join("app.json").exists());

    // Signed with another key than the release before: stopped, and nothing is written.
    let other = home.join("other.key");
    write_key(&other, 0x20);
    let release = files(&home, "0.6.0");
    let (code, _, said) = run_dist(
        &home,
        &release_args(&release, &hello, "0.6.0", valid.to_str().expect("utf8")),
        &other,
    );
    assert_eq!(code, 1);
    assert!(said.contains("another key"), "{said}");
    assert!(!release.join("app.json").exists());

    // An earlier app.json that is not what was signed: stopped.
    let tampered = fixtures().join("apps/tampered/app.json");
    let (code, _, said) = run_dist(
        &home,
        &release_args(&release, &hello, "0.6.0", tampered.to_str().expect("utf8")),
        &key,
    );
    assert_eq!(code, 1);
    assert!(said.contains("signature"), "{said}");

    // Nothing there is a first release, not an error.
    let (code, _, said) = run_dist(
        &home,
        &release_args(
            &release,
            &hello,
            "0.6.0",
            home.join("nothing/app.json").to_str().expect("utf8"),
        ),
        &key,
    );
    assert_eq!(code, 0, "{said}");
    assert!(written(&release).previous.is_empty());

    // An earlier app.json with no signature beside it is an error, not a first release.
    let unsigned = home.join("unsigned");
    fs::create_dir_all(&unsigned).expect("a directory");
    fs::copy(&valid, unsigned.join("app.json")).expect("a copy");
    let (code, _, said) = run_dist(
        &home,
        &release_args(
            &release,
            &hello,
            "0.6.0",
            unsigned.join("app.json").to_str().expect("utf8"),
        ),
        &key,
    );
    assert_eq!(code, 1);
    assert!(said.contains("no app.json.sig"), "{said}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_program_that_says_no_hello_is_not_released() {
    let home = directory("no-hello");
    let key = home.join("signing.key");
    write_key(&key, 0);
    let release = files(&home, "0.6.0");
    let silent = home.join("silent");
    fs::write(&silent, "#!/bin/sh\nexit 0\n").expect("a script");
    fs::set_permissions(&silent, fs::Permissions::from_mode(0o755)).expect("executable");
    let (code, _, said) = run_dist(
        &home,
        &release_args(&release, &silent, "0.6.0", "none"),
        &key,
    );
    assert_eq!(code, 1);
    assert!(said.contains("without saying hello"), "{said}");
    assert!(!release.join("app.json").exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_rust_program_is_released_at_the_version_of_its_cargo_toml() {
    let home = directory("cargo");
    let key = home.join("signing.key");
    write_key(&key, 0);
    let project = home.join("demopen");
    fs::create_dir_all(&project).expect("a directory");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"demopen\"\nversion = \"0.5.1\"\nedition = \"2024\"\n",
    )
    .expect("a Cargo.toml");
    let release = files(&home, "0.6.0");
    let hello = program(&home, "cargo-hello", LUA_03);

    // A version that says another than the Cargo.toml does.
    let mut args = release_args(&release, &hello, "0.6.0", "none");
    args.drain(2..4);
    args.push(project.to_str().expect("utf8").to_owned());
    let (code, _, said) = run_dist(&home, &args, &key);
    assert_eq!(code, 1);
    assert!(said.contains("Cargo.toml says version 0.5.1"), "{said}");
    assert!(!release.join("app.json").exists());

    // The same version: the app is called what Cargo.toml calls it.
    let release = files(&home, "0.5.1");
    let mut args = release_args(&release, &hello, "0.5.1", "none");
    args.drain(2..4);
    args.push(project.to_str().expect("utf8").to_owned());
    let (code, _, said) = run_dist(&home, &args, &key);
    assert_eq!(code, 0, "{said}");
    assert_eq!(written(&release).name, "demopen");

    // And without the files of the targets it is not built here.
    let mut args = vec![
        "dist".to_owned(),
        "0.5.1".to_owned(),
        project.to_str().expect("utf8").to_owned(),
    ];
    args.extend([
        "--base".to_owned(),
        "file:///x/".to_owned(),
        "--previous".to_owned(),
        "none".to_owned(),
    ]);
    let (code, _, said) = run_dist(&home, &args, &key);
    assert_eq!(code, 1);
    assert!(said.contains("--files"), "{said}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_version_is_three_numbers() {
    let home = directory("version");
    for version in ["0.6", "v0.6.0", "0.6.0-rc1", "01.2.3"] {
        let (code, _, said) = pencere(&home, &home, &["dist", version], &[]);
        assert_eq!(code, 64, "{version}: {said}");
        assert!(said.contains("is not a version"), "{said}");
    }
    let (code, _, said) = pencere(&home, &home, &["dist"], &[]);
    assert_eq!(code, 64, "{said}");
    let _ = fs::remove_dir_all(&home);
}

/// A Lua app is made for every target, from the runners of a release of the command (a directory
/// here), and the file of this machine says what it needs: it is a runner, with the app in it.
#[test]
fn a_lua_app_is_made_for_every_target_and_says_what_it_needs() {
    if Command::new("curl").arg("--version").output().is_err() {
        println!("SKIPPED: curl is not installed");
        return;
    }
    let home = directory("lua");
    let key = home.join("signing.key");
    write_key(&key, 0);
    let (code, _, said) = pencere(&home, &home, &["new", "demo", "--lang", "lua"], &[]);
    assert_eq!(code, 0, "{said}");

    // A release of the command: the runner of every target, which is this one for all.
    let version = env!("CARGO_PKG_VERSION");
    let releases = home.join("releases");
    let tagged = releases.join(format!("cli-v{version}"));
    fs::create_dir_all(&tagged).expect("a directory");
    let runner = fs::read(PENCERE).expect("the runner");
    let mut sums = String::new();
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
    ] {
        let file = format!("pencere-{version}-{target}");
        fs::write(tagged.join(&file), &runner).expect("a runner");
        sums.push_str(&format!("{}  {file}\n", sha256(&runner)));
    }
    fs::write(tagged.join("SHA256SUMS"), sums).expect("sums");

    let releases_url = format!("file://{}", releases.display());
    let cache = home.join("cache");
    let (code, said, why) = pencere(
        &home,
        &home,
        &[
            "dist",
            "0.1.0",
            "demo",
            "--base",
            "file:///somewhere/v0.1.0/",
            "--previous",
            "none",
        ],
        &[
            ("PENCERE_SIGNING_KEY", key.to_str().expect("utf8")),
            ("PENCERE_RELEASES", &releases_url),
            ("PENCERE_CACHE", cache.to_str().expect("utf8")),
        ],
    );
    assert_eq!(code, 0, "{why}");
    assert!(said.contains("4 files"), "{said}");
    let dist = home.join("demo/dist");
    let manifest = written(&dist);
    assert_eq!(manifest.name, "demo");
    assert_eq!(manifest.files.len(), 4);
    for target in manifest.files.keys() {
        assert!(
            dist.join(format!("demo-0.1.0-{target}")).is_file(),
            "{target}"
        );
    }
    // What the app says is the runner's: the lua view, in some version, and a protocol.
    assert_eq!(manifest.protocol, [1]);
    assert!(
        manifest.views.iter().any(|view| view.kind == "lua"),
        "{:?}",
        manifest.views
    );
    let _ = fs::remove_dir_all(&home);
}
