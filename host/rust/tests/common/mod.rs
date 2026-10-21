//! Reading the fixtures of the specification: the one truth the Rust crate and
//! the TypeScript SDK are both tested from.

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use pencere::base64;
use pencere::bundle::{Digest, Store, Verified};
use pencere::message::HostInfo;
use pencere::session::{Action, Config, Session, Size};
use pencere::theme::{Rgb, Theme};
use serde_json::Value;

/// The directory of the draft's fixtures.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/draft/fixtures")
}

/// The bytes of a fixture file, by its path in `manifest.json`.
pub fn file(path: &str) -> Vec<u8> {
    let full = root().join(path);
    std::fs::read(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// `manifest.json`.
pub fn manifest() -> Value {
    serde_json::from_slice(&file("manifest.json")).unwrap()
}

/// The 16 bytes a base64url token stands for.
pub fn token_bytes(token: &str) -> [u8; 16] {
    let standard = format!("{}==", token.replace('-', "+").replace('_', "/"));
    let bytes = base64::decode(standard.as_bytes()).unwrap();
    bytes.try_into().unwrap()
}

/// A store that holds digests and no bytes: what a session fixture's `store`
/// says the host already has.
#[derive(Default)]
pub struct Held(pub HashSet<Digest>);

impl Store for Held {
    fn contains(&self, digest: &Digest) -> bool {
        self.0.contains(digest)
    }
    fn insert(&mut self, file: Verified) {
        self.0.insert(*file.digest());
    }
    fn get(&self, _: &Digest) -> Option<Arc<[u8]>> {
        None
    }
}

/// An entropy source that yields the tokens a fixture issues, in order.
pub fn issuing(tokens: Vec<[u8; 16]>) -> impl FnMut() -> [u8; 16] + use<> {
    let mut next = tokens.into_iter();
    move || next.next().unwrap_or([0xee; 16])
}

pub fn theme() -> Theme {
    Theme::new(Rgb(0, 0, 0), Rgb(0xee, 0xee, 0xee), "\"Menlo\", monospace")
}

pub fn config(host: &Value) -> Config {
    let host = HostInfo {
        name: host["name"].as_str().unwrap().to_owned(),
        version: host["version"].as_str().unwrap().to_owned(),
    };
    Config::new(
        host,
        theme(),
        Size {
            width: 800,
            height: 600,
        },
    )
}

/// The bytes of the `Write` actions in `actions`.
pub fn written(actions: &[Action]) -> Vec<u8> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Write(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect()
}

/// A session configured as a fixture's `config` says.
pub fn session_of(
    config: &Value,
    tokens: &Value,
) -> Session<Held, impl FnMut() -> [u8; 16] + use<>> {
    let held = config["store"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| Digest::parse(d.as_str().unwrap()).unwrap())
        .collect();
    let issued = tokens
        .as_array()
        .unwrap()
        .iter()
        .map(|t| token_bytes(t.as_str().unwrap()))
        .collect();
    Session::new(self::config(&config["host"]), Held(held), issuing(issued))
}

/// The bodies of the frames of a stream, as a terminal's parser would hand them
/// over.
pub fn bodies(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = bytes[at..].windows(2).position(|w| w == b"\x1b_") {
        let body_start = at + start + 2;
        let end = bytes[body_start..]
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .expect("a frame that ends");
        out.push(&bytes[body_start..body_start + end]);
        at = body_start + end + 2;
    }
    out
}
