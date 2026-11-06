//! The files that release an app (`spec/draft/apps.md`): the `app.json` that names a release, the
//! signature beside it, and the choice of the newest release a host can open.
//!
//! Like the rest of the crate this does no I/O: a terminal downloads the two files (and, for an
//! older release, the `app.json` a `previous` entry names), hands their bytes over and acts on what
//! comes back. What it reads is as hostile as any stream, so [`MAX_MANIFEST_BYTES`] is checked
//! before anything is parsed and nothing is allocated for a length the file claims.
//!
//! Which keys to believe is not here. [`verify`] says the signature holds for the key the file
//! names; whether that key is one the person trusts is the terminal's to decide (specification
//! Section 8).

use std::collections::BTreeMap;
use std::fmt;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

use crate::base64;
use crate::json::Object;
use crate::message::ViewOffer;
use crate::session::{Offer, match_views};

/// The longest `app.json` a reader takes (specification Section 3).
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// The most entries `previous` has (specification Section 4).
pub const MAX_PREVIOUS: usize = 32;

/// The longest name of an app or of a target, in bytes.
const MAX_NAME: usize = 64;

/// The longest address, in bytes.
const MAX_ADDRESS: usize = 2048;

/// The longest `app.json.sig` a reader takes: the text of a signature and a line feed are 97 bytes.
const MAX_SIGNATURE_BYTES: usize = 128;

/// The text a key and a signature begin with.
const KEY_PREFIX: &str = "ed25519:";

/// Why a file is not accepted. The names ([`Error::reason`]) are the `outcome`s of the fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The file is longer than [`MAX_MANIFEST_BYTES`].
    TooLarge,
    /// The file is not a JSON object of the members the specification asks for.
    Shape,
    /// The `name` is not one the specification allows.
    Name,
    /// The `version` is not three numbers.
    Version,
    /// The `protocol` is empty or has a version that is not positive.
    Protocol,
    /// The `views` is empty.
    Views,
    /// The `key` is not an Ed25519 public key in the text of the specification.
    Key,
    /// A target is named with characters the specification does not allow, or `files` is empty.
    Target,
    /// A digest is not 64 lowercase hexadecimal digits.
    Digest,
    /// An address is not an `https:` or `file:` one, or is too long, or has a byte that is not
    /// printable ASCII.
    Address,
    /// `previous` has more than [`MAX_PREVIOUS`] entries.
    TooManyPrevious,
    /// The signature is not the key's over the bytes of the file, or is not written as it should
    /// be.
    Signature,
}

impl Error {
    /// A short name for the reason, in the words of the fixtures' `outcome`.
    pub fn reason(self) -> &'static str {
        match self {
            Error::TooLarge => "too-large",
            Error::Shape => "shape",
            Error::Name => "name",
            Error::Version => "version",
            Error::Protocol => "protocol",
            Error::Views => "views",
            Error::Key => "key",
            Error::Target => "target",
            Error::Digest => "digest",
            Error::Address => "address",
            Error::TooManyPrevious => "previous",
            Error::Signature => "signature",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::TooLarge => "app.json is longer than 65536 bytes",
            Error::Shape => "app.json is not an object with the members it needs",
            Error::Name => "the name is not lowercase letters, digits and - from 1 to 64 bytes",
            Error::Version => "the version is not three numbers",
            Error::Protocol => "the protocol versions are empty or not positive",
            Error::Views => "the views are empty",
            Error::Key => "the key is not an ed25519 public key",
            Error::Target => {
                "a target name is not lowercase letters, digits, _ and -, or there is no file"
            }
            Error::Digest => "a digest is not 64 lowercase hexadecimal digits",
            Error::Address => "an address is not an https: or file: one of printable ASCII",
            Error::TooManyPrevious => "previous has more than 32 entries",
            Error::Signature => "the signature does not hold",
        })
    }
}

impl std::error::Error for Error {}

/// A version of an app: three numbers, compared by number, the first number first
/// (specification Section 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The first number.
    pub major: u32,
    /// The second number.
    pub minor: u32,
    /// The third number.
    pub patch: u32,
}

impl Version {
    /// `major.minor.patch`: three numbers of decimal digits without a leading zero, each of at
    /// most nine digits, and nothing else.
    pub fn parse(text: &str) -> Option<Version> {
        let mut numbers = text.split('.');
        let major = number(numbers.next()?)?;
        let minor = number(numbers.next()?)?;
        let patch = number(numbers.next()?)?;
        if numbers.next().is_some() {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A number of the specification's ABNF: `"0" / ( %x31-39 *8DIGIT )`.
fn number(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    let ok = match bytes.split_first() {
        Some((b'0', rest)) => rest.is_empty(),
        Some((first, rest)) => {
            (b'1'..=b'9').contains(first) && rest.len() <= 8 && rest.iter().all(u8::is_ascii_digit)
        }
        None => false,
    };
    if ok { text.parse().ok() } else { None }
}

/// The file of a release for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The address of the executable file.
    pub url: String,
    /// Its SHA-256 digest, 64 lowercase hexadecimal digits.
    pub sha256: String,
}

/// An earlier release a reader may fall back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Previous {
    /// Its version.
    pub version: Version,
    /// The protocol versions it speaks.
    pub protocol: Vec<u32>,
    /// The kinds of view it shows, in the shape of the `views` of a `hello`.
    pub views: Vec<ViewOffer>,
    /// The address of its `app.json`.
    pub manifest: String,
}

/// A release, as its `app.json` says it (specification Section 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The name of the app.
    pub name: String,
    /// The version of the release.
    pub version: Version,
    /// The protocol versions the release speaks.
    pub protocol: Vec<u32>,
    /// The kinds of view it shows, in the shape of the `views` of a `hello`.
    pub views: Vec<ViewOffer>,
    /// The author's public key as text, `ed25519:` and base64: what the terminal compares with the
    /// keys it trusts.
    pub key: String,
    /// The file for each target.
    pub files: BTreeMap<String, File>,
    /// The earlier releases a reader may fall back to.
    pub previous: Vec<Previous>,
}

/// What a host takes of a release and its `previous`: see [`Manifest::select`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection<'a> {
    /// The release itself: it opens and no entry of `previous` that opens has a greater version.
    Current,
    /// An entry of `previous`: its `app.json` is the one to fetch.
    Previous(&'a Previous),
    /// Nothing opens. The requirement of the latest release, which is the release itself, is what
    /// would serve.
    Unsupported {
        /// The release's version.
        version: Version,
        /// The protocol versions it speaks.
        protocol: &'a [u32],
        /// The kinds of view it shows.
        views: &'a [ViewOffer],
    },
}

impl Manifest {
    /// The newest release among this one and `previous` that a host opens: the one of the greatest
    /// version number, whatever order they are listed in and whenever they were published
    /// (specification Section 7). `protocols` are the protocol versions the host speaks (for
    /// instance [`message::SUPPORTED_VERSIONS`](crate::message::SUPPORTED_VERSIONS)) and `views`
    /// what it offers, as its session's configuration lists them.
    pub fn select<'a>(&'a self, protocols: &[u32], views: &[Offer]) -> Selection<'a> {
        let mut best: Option<(Version, Option<&'a Previous>)> = None;
        if opens(protocols, views, &self.protocol, &self.views) {
            best = Some((self.version, None));
        }
        for earlier in &self.previous {
            let newer = best.is_none_or(|(version, _)| earlier.version > version);
            if newer && opens(protocols, views, &earlier.protocol, &earlier.views) {
                best = Some((earlier.version, Some(earlier)));
            }
        }
        match best {
            Some((_, None)) => Selection::Current,
            Some((_, Some(earlier))) => Selection::Previous(earlier),
            None => Selection::Unsupported {
                version: self.version,
                protocol: &self.protocol,
                views: &self.views,
            },
        }
    }
}

/// Whether a release with this requirement opens in a host: the host speaks one of its protocol
/// versions, and the handshake would list a kind of the release's `views` in its `welcome`
/// (specification Section 6).
fn opens(protocols: &[u32], host: &[Offer], protocol: &[u32], views: &[ViewOffer]) -> bool {
    protocol.iter().any(|version| protocols.contains(version))
        && !match_views(host, views).0.is_empty()
}

/// The address of the signature of the `app.json` at `address` (specification Section 3).
pub fn signature_address(address: &str) -> String {
    format!("{address}.sig")
}

/// Whether `text` is an address of the specification (Section 4.5): `https://` and something, or
/// `file:///`, of printable ASCII and at most 2048 bytes.
pub fn is_address(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() <= MAX_ADDRESS
        && bytes.iter().all(|b| (0x21..=0x7e).contains(b))
        && (text
            .strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty())
            || text.starts_with("file:///"))
}

/// Reads an `app.json` and checks the shape of what it says (specification Section 4), without
/// looking at a signature: what it returns is not to be acted on before [`verify`] has held.
pub fn parse(bytes: &[u8]) -> Result<Manifest, Error> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Shape)?;
    let Object(raw) = serde_json::from_str::<Object<Raw>>(text).map_err(|_| Error::Shape)?;
    if raw.previous.len() > MAX_PREVIOUS {
        return Err(Error::TooManyPrevious);
    }
    if !is_name(&raw.name) {
        return Err(Error::Name);
    }
    let version = Version::parse(&raw.version).ok_or(Error::Version)?;
    check_requirement(&raw.protocol, &raw.views)?;
    key_of(&raw.key)?;
    if raw.files.is_empty() {
        return Err(Error::Target);
    }
    let mut files = BTreeMap::new();
    for (target, Object(file)) in raw.files {
        if !is_target(&target) {
            return Err(Error::Target);
        }
        if !is_digest(&file.sha256) {
            return Err(Error::Digest);
        }
        if !is_address(&file.url) {
            return Err(Error::Address);
        }
        files.insert(
            target,
            File {
                url: file.url,
                sha256: file.sha256,
            },
        );
    }
    let mut previous = Vec::with_capacity(raw.previous.len());
    for Object(earlier) in raw.previous {
        let version = Version::parse(&earlier.version).ok_or(Error::Version)?;
        check_requirement(&earlier.protocol, &earlier.views)?;
        if !is_address(&earlier.manifest) {
            return Err(Error::Address);
        }
        previous.push(Previous {
            version,
            protocol: earlier.protocol,
            views: earlier.views,
            manifest: earlier.manifest,
        });
    }
    Ok(Manifest {
        name: raw.name,
        version,
        protocol: raw.protocol,
        views: raw.views,
        key: raw.key,
        files,
        previous,
    })
}

/// Reads an `app.json` and checks that `signature`, the bytes of its `app.json.sig`, is the
/// signature of the key the file names over the bytes of the file as they are (specification
/// Section 5). The shape is checked first, because the key is in the file.
///
/// That the signature holds says whoever holds that key wrote the file, and the key is the file's
/// own: whether it is a key to believe is for the caller to decide with [`Manifest::key`].
pub fn verify(bytes: &[u8], signature: &[u8]) -> Result<Manifest, Error> {
    let manifest = parse(bytes)?;
    let key = key_of(&manifest.key)?;
    let signature = signature_of(signature)?;
    key.verify_strict(bytes, &signature)
        .map_err(|_| Error::Signature)?;
    Ok(manifest)
}

/// What serde reads of `app.json`: the members as they are written, checked afterwards.
#[derive(Deserialize)]
struct Raw {
    name: String,
    version: String,
    protocol: Vec<u32>,
    views: Vec<ViewOffer>,
    key: String,
    files: BTreeMap<String, Object<RawFile>>,
    previous: Vec<Object<RawPrevious>>,
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    sha256: String,
}

#[derive(Deserialize)]
struct RawPrevious {
    version: String,
    protocol: Vec<u32>,
    views: Vec<ViewOffer>,
    manifest: String,
}

/// A requirement is a non-empty list of positive protocol versions and a non-empty list of views.
fn check_requirement(protocol: &[u32], views: &[ViewOffer]) -> Result<(), Error> {
    if protocol.is_empty() || protocol.contains(&0) {
        return Err(Error::Protocol);
    }
    if views.is_empty() {
        return Err(Error::Views);
    }
    Ok(())
}

fn is_name(text: &str) -> bool {
    let start = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let bytes = text.as_bytes();
    bytes.len() <= MAX_NAME
        && bytes.first().is_some_and(start)
        && bytes.iter().all(|b| start(b) || *b == b'-')
}

fn is_target(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_NAME
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

fn is_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The key a text names, if it is the text of one: its 32 bytes are a point of the curve.
fn key_of(text: &str) -> Result<VerifyingKey, Error> {
    let coded = text.strip_prefix(KEY_PREFIX).ok_or(Error::Key)?;
    let bytes = base64::decode(coded.as_bytes()).ok_or(Error::Key)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| Error::Key)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| Error::Key)
}

/// The signature in the bytes of an `app.json.sig`: one line of text, with a line feed after it
/// or none.
fn signature_of(file: &[u8]) -> Result<Signature, Error> {
    if file.len() > MAX_SIGNATURE_BYTES {
        return Err(Error::Signature);
    }
    let line = file.strip_suffix(b"\n").unwrap_or(file);
    let text = std::str::from_utf8(line).map_err(|_| Error::Signature)?;
    let coded = text.strip_prefix(KEY_PREFIX).ok_or(Error::Signature)?;
    let bytes = base64::decode(coded.as_bytes()).ok_or(Error::Signature)?;
    Signature::from_slice(&bytes).map_err(|_| Error::Signature)
}
