//! Bundles (specification §8): the files of a view, listed with their SHA-256
//! digests, loaded once and kept by content.
//!
//! This module holds the rules: what a manifest may say, which digests a host
//! lacks, how a file is verified. Where files are kept is the terminal's, behind
//! [`Store`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use sha2::{Digest as _, Sha256};

use crate::base64;
use crate::message::{RawBundle, RawFile};
use crate::sandbox::UiMeta;

/// The most files a manifest may list (specification §8.2).
pub const MAX_FILES: usize = 1024;
/// The most bytes one file may have: 8 MiB.
pub const MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;
/// The most bytes the files of a manifest may have together: 64 MiB.
pub const MAX_TOTAL_SIZE: u64 = 64 * 1024 * 1024;
/// The most bytes a path may have.
pub const MAX_PATH: usize = 255;

/// The limits a host holds a manifest to. A host may enforce lower ones than the
/// protocol's; it cannot enforce higher ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleLimits {
    /// Files in a manifest.
    pub files: usize,
    /// Bytes in one file.
    pub file: u64,
    /// Bytes in all the files.
    pub total: u64,
}

impl BundleLimits {
    /// The protocol's limits.
    pub const PROTOCOL: Self = Self {
        files: MAX_FILES,
        file: MAX_FILE_SIZE,
        total: MAX_TOTAL_SIZE,
    };

    /// These limits, lowered to the protocol's where they are higher.
    pub fn clamped(self) -> Self {
        Self {
            files: self.files.min(MAX_FILES),
            file: self.file.min(MAX_FILE_SIZE),
            total: self.total.min(MAX_TOTAL_SIZE),
        }
    }
}

impl Default for BundleLimits {
    fn default() -> Self {
        Self::PROTOCOL
    }
}

/// A SHA-256 digest.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Reads 64 lowercase hex digits.
    pub fn parse(hex: &str) -> Option<Self> {
        if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            let pair = std::str::from_utf8(pair).ok()?;
            *byte = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// The 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({self})")
    }
}

/// A file's bytes together with the digest they have. It cannot be made with any
/// other digest, which is how a [`Store`] keeps a file under its own and nothing
/// else (specification §8.5).
#[derive(Clone, PartialEq, Eq)]
pub struct Verified {
    digest: Digest,
    bytes: Vec<u8>,
}

impl Verified {
    /// The file of these bytes, under their digest.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            digest: Digest::of(&bytes),
            bytes,
        }
    }

    /// The digest the bytes have.
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The digest and the bytes.
    pub fn into_parts(self) -> (Digest, Vec<u8>) {
        (self.digest, self.bytes)
    }
}

impl fmt::Debug for Verified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Verified({}, {} bytes)", self.digest, self.bytes.len())
    }
}

/// Where a host keeps the files it has verified, by digest. How much it keeps and
/// for how long is its own policy: it may drop any entry at any time, and a
/// program sends every file of its manifest at every open it makes.
pub trait Store {
    /// Whether the store holds the file with this digest.
    fn contains(&self, digest: &Digest) -> bool;
    /// Keeps a verified file.
    fn insert(&mut self, file: Verified);
    /// The file with this digest.
    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>>;
}

/// A store in memory that keeps everything it is given.
#[derive(Debug, Default)]
pub struct MemoryStore {
    files: HashMap<Digest, Arc<[u8]>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many files it holds.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Store for MemoryStore {
    fn contains(&self, digest: &Digest) -> bool {
        self.files.contains_key(digest)
    }

    fn insert(&mut self, file: Verified) {
        let (digest, bytes) = file.into_parts();
        self.files.insert(digest, bytes.into());
    }

    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>> {
        self.files.get(digest).cloned()
    }
}

/// Why a manifest was refused (the `code` of the refusal, specification §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The manifest is malformed.
    Manifest,
    /// A limit is exceeded.
    Limit,
}

/// One file of a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Where the view finds it: relative, `/`-separated.
    pub path: String,
    /// Its digest.
    pub digest: Digest,
    /// Its length, as the manifest says.
    pub size: u64,
    /// Its media type: `type/subtype`, lowercase.
    pub media_type: String,
}

/// A manifest that follows the grammar of §8.2 and is within the host's limits.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    entry: String,
    files: Vec<File>,
    meta: Option<UiMeta>,
}

fn is_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-'))
}

/// Whether `path` is one a manifest may list (specification §8.2).
pub fn is_path(path: &str) -> bool {
    (1..=MAX_PATH).contains(&path.len()) && path.split('/').all(is_segment)
}

fn is_media_type(text: &str) -> bool {
    fn name(part: &str) -> bool {
        let mut bytes = part.bytes();
        (1..=127).contains(&part.len())
            && bytes
                .next()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            && bytes.all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(
                        b,
                        b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                    )
            })
    }
    text.split_once('/')
        .is_some_and(|(kind, subtype)| name(kind) && name(subtype))
}

impl Manifest {
    pub(crate) fn check(raw: RawBundle, limits: BundleLimits) -> Result<Self, Refusal> {
        let meta = raw
            .meta
            .as_ref()
            .map(UiMeta::from_value)
            .transpose()
            .map_err(|_| Refusal::Manifest)?;
        let mut files = Vec::with_capacity(raw.files.len().min(MAX_FILES + 1));
        let mut paths = std::collections::HashSet::new();
        for RawFile {
            path,
            sha256,
            size,
            media_type,
        } in raw.files
        {
            let digest = Digest::parse(&sha256).ok_or(Refusal::Manifest)?;
            if !is_path(&path) || !is_media_type(&media_type) || !paths.insert(path.clone()) {
                return Err(Refusal::Manifest);
            }
            files.push(File {
                path,
                digest,
                size,
                media_type,
            });
        }
        if !paths.contains(&raw.entry) {
            return Err(Refusal::Manifest);
        }
        let limits = limits.clamped();
        let total = files
            .iter()
            .try_fold(0u64, |sum, f| sum.checked_add(f.size));
        if files.len() > limits.files
            || files.iter().any(|f| f.size > limits.file)
            || total.is_none_or(|total| total > limits.total)
        {
            return Err(Refusal::Limit);
        }
        Ok(Self {
            entry: raw.entry,
            files,
            meta,
        })
    }

    /// The path of the document.
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// The files, in the order the manifest lists them.
    pub fn files(&self) -> &[File] {
        &self.files
    }

    /// The `_meta.ui` of the view the manifest carries.
    pub fn meta(&self) -> Option<&UiMeta> {
        self.meta.as_ref()
    }

    /// The file at `path`. A host serves nothing that is not in the manifest, so
    /// this is how it answers a request from the view.
    pub fn file(&self, path: &str) -> Option<&File> {
        self.files.iter().find(|f| f.path == path)
    }

    /// The digests the store does not hold, each once, in the order of their
    /// first appearance (specification §8.3).
    pub fn missing(&self, store: &dyn Store) -> Vec<Digest> {
        let mut missing: Vec<Digest> = Vec::new();
        for file in &self.files {
            if !store.contains(&file.digest) && !missing.contains(&file.digest) {
                missing.push(file.digest);
            }
        }
        missing
    }

    /// Whether the store holds every file of the manifest.
    pub fn is_held(&self, store: &dyn Store) -> bool {
        self.files.iter().all(|f| store.contains(&f.digest))
    }

    /// The digests of the manifest with the largest size it gives each.
    fn sizes(&self) -> HashMap<Digest, u64> {
        let mut sizes: HashMap<Digest, u64> = HashMap::new();
        for file in &self.files {
            let size = sizes.entry(file.digest).or_default();
            *size = (*size).max(file.size);
        }
        sizes
    }
}

/// What came of a `file` message.
#[derive(Debug, PartialEq, Eq)]
pub enum Received {
    /// Nobody asked for it, or it has been verified already: nothing is kept.
    Ignored,
    /// It is what the digest says: keep it.
    Verified(Verified),
    /// It is not: the open ends.
    Failed,
}

/// The files a host is waiting for.
#[derive(Debug)]
pub struct Receiving {
    asked: Vec<(Digest, u64, bool)>,
}

impl Receiving {
    /// Waits for `missing`, whose largest sizes the manifest gives.
    pub fn new(manifest: &Manifest, missing: &[Digest]) -> Self {
        let sizes = manifest.sizes();
        Self {
            asked: missing
                .iter()
                .map(|d| (*d, sizes.get(d).copied().unwrap_or(0), false))
                .collect(),
        }
    }

    /// Takes the payload of a `file` message with digest `digest`.
    ///
    /// A file is within its size when its decoded length is no more than the
    /// size the manifest gives its digest (the largest one, when several files
    /// share it); the digest decides whether it is the file. A payload too long
    /// for that is refused without decoding it.
    pub fn accept(&mut self, digest: &Digest, payload: &str) -> Received {
        let Some((_, size, done)) = self.asked.iter_mut().find(|(d, ..)| d == digest) else {
            return Received::Ignored;
        };
        if *done {
            return Received::Ignored;
        }
        let longest = usize::try_from(*size)
            .ok()
            .and_then(|size| size.div_ceil(3).checked_mul(4));
        if longest.is_none_or(|longest| payload.len() > longest) {
            return Received::Failed;
        }
        let Some(bytes) = base64::decode(payload.as_bytes()) else {
            return Received::Failed;
        };
        if u64::try_from(bytes.len()).is_ok_and(|len| len > *size) {
            return Received::Failed;
        }
        let file = Verified::from_bytes(bytes);
        if file.digest() != digest {
            return Received::Failed;
        }
        *done = true;
        Received::Verified(file)
    }

    /// Whether every file asked for has been verified.
    pub fn is_complete(&self) -> bool {
        self.asked.iter().all(|(.., done)| *done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(value: serde_json::Value) -> RawBundle {
        serde_json::from_value(value).unwrap()
    }

    fn digest_of(text: &str) -> String {
        Digest::of(text.as_bytes()).to_string()
    }

    fn manifest_json(files: &[(&str, &str)]) -> serde_json::Value {
        json!({
            "entry": files[0].0,
            "files": files.iter().map(|(path, body)| json!({
                "path": path, "sha256": digest_of(body), "size": body.len(), "type": "text/plain",
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn a_digest_is_sixty_four_lowercase_hex_digits() {
        let hex = digest_of("abc");
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(Digest::parse(&hex).unwrap().to_string(), hex);
        assert!(Digest::parse(&hex.to_uppercase()).is_none());
        assert!(Digest::parse(&hex[1..]).is_none());
        assert!(Digest::parse(&format!("{hex}0")).is_none());
        assert!(Digest::parse(&"g".repeat(64)).is_none());
    }

    #[test]
    fn paths_are_a_closed_grammar() {
        for good in [
            "a",
            "index.html",
            "a/b/c.js",
            "x-y_z~1.min.js",
            "...",
            &"a".repeat(255),
        ] {
            assert!(is_path(good), "{good}");
        }
        for bad in [
            "",
            "/a",
            "a/",
            "a//b",
            ".",
            "..",
            "a/../b",
            "../etc/passwd",
            "a/./b",
            "a b",
            "a\\b",
            "caf\u{e9}",
            "a?b",
            "a#b",
            "a:b",
            "a%2e",
            &"a".repeat(256),
            "a\nb",
            "a\0b",
        ] {
            assert!(!is_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn media_types_are_lowercase_type_slash_subtype() {
        for good in [
            "text/html",
            "application/json",
            "image/svg+xml",
            "font/woff2",
        ] {
            assert!(is_media_type(good), "{good}");
        }
        for bad in [
            "",
            "text",
            "text/",
            "/html",
            "Text/Html",
            "text/html; charset=utf-8",
            "a/b/c",
            " text/html",
        ] {
            assert!(!is_media_type(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_manifest_is_checked_in_two_steps_manifest_then_limit() {
        let ok = Manifest::check(
            raw(manifest_json(&[("index.html", "<p>"), ("app.js", "1")])),
            BundleLimits::PROTOCOL,
        )
        .unwrap();
        assert_eq!(ok.entry(), "index.html");
        assert_eq!(ok.files().len(), 2);
        assert_eq!(ok.file("app.js").unwrap().size, 1);
        assert!(ok.file("other.js").is_none());

        let check = |value: serde_json::Value, limits: BundleLimits| {
            Manifest::check(raw(value), limits).map(|_| ())
        };
        // Not a manifest.
        let mut value = manifest_json(&[("index.html", "<p>")]);
        value["entry"] = json!("missing.html");
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        let value = manifest_json(&[("index.html", "a"), ("index.html", "b")]);
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        let value = manifest_json(&[("../x", "a")]);
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        let mut value = manifest_json(&[("a", "a")]);
        value["files"][0]["sha256"] = json!("ABC");
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        let mut value = manifest_json(&[("a", "a")]);
        value["files"][0]["type"] = json!("Text/Plain");
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        let mut value = manifest_json(&[("a", "a")]);
        value["meta"] = json!({"csp": 5});
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Manifest));
        // Within the grammar, over a limit: a malformed one is a manifest even when it is also too big.
        let mut value = manifest_json(&[("a", "a")]);
        value["files"][0]["size"] = json!(MAX_FILE_SIZE + 1);
        assert_eq!(check(value, BundleLimits::PROTOCOL), Err(Refusal::Limit));
        let mut value = manifest_json(&[("a", "a"), ("b", "b")]);
        value["files"][0]["size"] = json!(MAX_FILE_SIZE);
        value["files"][1]["size"] = json!(MAX_FILE_SIZE);
        assert!(check(value.clone(), BundleLimits::PROTOCOL).is_ok());
        let lower = BundleLimits {
            total: MAX_FILE_SIZE,
            ..BundleLimits::PROTOCOL
        };
        assert_eq!(check(value, lower), Err(Refusal::Limit));
        let files: Vec<serde_json::Value> = (0..=MAX_FILES)
            .map(|n| json!({"path": format!("f{n}"), "sha256": digest_of("x"), "size": 1, "type": "text/plain"}))
            .collect();
        let many = json!({"entry": "f0", "files": files});
        assert_eq!(
            check(many.clone(), BundleLimits::PROTOCOL),
            Err(Refusal::Limit)
        );
        let mut with_a_bad_path = many;
        with_a_bad_path["files"][5]["path"] = json!("../x");
        assert_eq!(
            check(with_a_bad_path, BundleLimits::PROTOCOL),
            Err(Refusal::Manifest)
        );
        // A host cannot allow more than the protocol does.
        let generous = BundleLimits {
            files: usize::MAX,
            file: u64::MAX,
            total: u64::MAX,
        };
        let mut value = manifest_json(&[("a", "a")]);
        value["files"][0]["size"] = json!(MAX_FILE_SIZE + 1);
        assert_eq!(check(value, generous), Err(Refusal::Limit));
        // Sizes that do not add up in 64 bits are over the limit, not a panic.
        let mut value = manifest_json(&[("a", "a"), ("b", "b")]);
        value["files"][0]["size"] = json!(u64::MAX);
        value["files"][1]["size"] = json!(u64::MAX);
        assert_eq!(check(value, generous), Err(Refusal::Limit));
    }

    #[test]
    fn what_is_missing_is_each_digest_once_in_the_order_of_the_manifest() {
        let manifest = Manifest::check(
            raw(manifest_json(&[
                ("a", "one"),
                ("b", "two"),
                ("c", "one"),
                ("d", "three"),
            ])),
            BundleLimits::PROTOCOL,
        )
        .unwrap();
        let mut store = MemoryStore::new();
        let all = manifest.missing(&store);
        assert_eq!(
            all.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec![digest_of("one"), digest_of("two"), digest_of("three")]
        );
        store.insert(Verified::from_bytes(b"two".to_vec()));
        assert_eq!(
            manifest.missing(&store),
            vec![Digest::of(b"one"), Digest::of(b"three")]
        );
        assert!(!manifest.is_held(&store));
        store.insert(Verified::from_bytes(b"one".to_vec()));
        store.insert(Verified::from_bytes(b"three".to_vec()));
        assert!(manifest.missing(&store).is_empty());
        assert!(manifest.is_held(&store));
        assert_eq!(store.get(&Digest::of(b"one")).as_deref(), Some(&b"one"[..]));
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn a_file_is_kept_only_if_it_was_asked_for_and_is_what_its_digest_says() {
        let manifest = Manifest::check(
            raw(manifest_json(&[("a", "hello"), ("b", "world")])),
            BundleLimits::PROTOCOL,
        )
        .unwrap();
        let hello = Digest::of(b"hello");
        let world = Digest::of(b"world");
        let mut receiving = Receiving::new(&manifest, &[hello]);
        // Not asked for.
        assert_eq!(
            receiving.accept(&world, &base64::encode(b"world")),
            Received::Ignored
        );
        assert!(!receiving.is_complete());
        // Asked for, but other bytes.
        assert_eq!(
            receiving.accept(&hello, &base64::encode(b"HELLO")),
            Received::Failed
        );
        // Not base64.
        assert_eq!(receiving.accept(&hello, "not base64!"), Received::Failed);
        // Too long for its size, refused before it is decoded.
        assert_eq!(
            receiving.accept(&hello, &base64::encode(&[0u8; 300])),
            Received::Failed
        );
        // Right.
        let Received::Verified(file) = receiving.accept(&hello, &base64::encode(b"hello")) else {
            panic!("not verified")
        };
        assert_eq!(file.digest(), &hello);
        assert_eq!(file.bytes(), b"hello");
        assert!(receiving.is_complete());
        // Again: ignored.
        assert_eq!(
            receiving.accept(&hello, &base64::encode(b"hello")),
            Received::Ignored
        );
    }

    #[test]
    fn a_file_shorter_than_its_size_is_within_it_and_the_digest_decides() {
        let mut value = manifest_json(&[("a", "hello")]);
        value["files"][0]["size"] = json!(100);
        let manifest = Manifest::check(raw(value), BundleLimits::PROTOCOL).unwrap();
        let mut receiving = Receiving::new(&manifest, &[Digest::of(b"hello")]);
        assert!(matches!(
            receiving.accept(&Digest::of(b"hello"), &base64::encode(b"hello")),
            Received::Verified(_)
        ));
    }

    #[test]
    fn an_empty_file_is_a_file() {
        let manifest =
            Manifest::check(raw(manifest_json(&[("a", "")])), BundleLimits::PROTOCOL).unwrap();
        let mut receiving = Receiving::new(&manifest, &[Digest::of(b"")]);
        assert!(matches!(
            receiving.accept(&Digest::of(b""), ""),
            Received::Verified(_)
        ));
        assert!(receiving.is_complete());
    }
}
