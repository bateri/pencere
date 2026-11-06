//! `pencere key`: the key an author signs the releases of an app with.
//!
//! The secret half is a file of one line, `ed25519-seed:` and the base64 of the 32 bytes the key is
//! made from, in `~/.config/pencere/signing.key` unless `--key` or `PENCERE_SIGNING_KEY` name
//! another. It is not put in an app's directory, where it would be committed. The public half is
//! what the command prints, and what `app.json` names in `key`.

use std::fs;
use std::io::{ErrorKind, Read, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use pencere_wire::base64;

/// What the file's line begins with.
const SEED_PREFIX: &str = "ed25519-seed:";

/// Where the signing key is: the file `--key` names, or else the one `PENCERE_SIGNING_KEY` names,
/// or else `~/.config/pencere/signing.key`.
pub fn key_path(flag: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = flag {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PENCERE_SIGNING_KEY").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or("HOME is not set, so there is no place for the signing key: --key FILE names one")?;
    Ok(PathBuf::from(home).join(".config/pencere/signing.key"))
}

/// The key in `path`.
pub fn load(path: &Path) -> Result<SigningKey, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(why) if why.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "there is no signing key at {}: `pencere key` makes one",
                path.display()
            ));
        }
        Err(why) => return Err(format!("{}: {why}", path.display())),
    };
    let not_a_key = || {
        format!(
            "{} is not a signing key: it should be one line, {SEED_PREFIX} and 32 bytes in base64",
            path.display()
        )
    };
    let coded = text
        .trim()
        .strip_prefix(SEED_PREFIX)
        .ok_or_else(not_a_key)?;
    let seed: [u8; 32] = base64::decode(coded.as_bytes())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(not_a_key)?;
    Ok(SigningKey::from_bytes(&seed))
}

/// The public half as `app.json` writes it: `ed25519:` and base64.
pub fn public_text(key: &SigningKey) -> String {
    format!("ed25519:{}", base64::encode(key.verifying_key().as_bytes()))
}

/// Makes a key and writes it to `path`, which must not exist: the file is made for this alone, so
/// that a key already there cannot be written over, and readable by its owner only.
fn generate(path: &Path) -> Result<SigningKey, String> {
    let mut seed = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut seed))
        .map_err(|why| format!("the system's random bytes could not be read: {why}"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|why| format!("{}: {why}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|why| format!("{}: {why}", path.display()))?;
    writeln!(file, "{SEED_PREFIX}{}", base64::encode(&seed))
        .and_then(|()| file.sync_all())
        .map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn command(args: &[String]) -> ExitCode {
    let mut flag = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--key" => match args.next() {
                Some(path) => flag = Some(PathBuf::from(path)),
                None => return crate::usage("--key needs a file"),
            },
            other => return crate::usage(format!("{other:?} is not an option of `key`")),
        }
    }
    let result = key_path(flag.as_deref()).and_then(|path| {
        if path.exists() {
            let key = load(&path)?;
            eprintln!("pencere: the signing key is {}", path.display());
            return Ok(key);
        }
        let key = generate(&path)?;
        eprintln!("pencere: made a signing key at {}", path.display());
        eprintln!(
            "pencere: keep a copy of it where it is safe. Whoever has the file can release as you, \
             and when it is lost no release can follow yours without the people who follow you \
             being asked again."
        );
        Ok(key)
    });
    match result {
        Ok(key) => {
            println!("{}", public_text(&key));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::from(1)
        }
    }
}
