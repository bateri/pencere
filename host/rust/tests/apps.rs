//! `app.json` and its signature (`spec/draft/apps.md`): every `apps` case of the fixtures' manifest
//! read as a terminal reads it, and the rules the fixtures are too big or too small to hold.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use pencere_host::apps::{
    Error, MAX_MANIFEST_BYTES, MAX_PREVIOUS, Manifest, Previous, Selection, Version, is_address,
    parse, signature_address, verify,
};
use pencere_host::message::SUPPORTED_VERSIONS;
use pencere_host::session::{Offer, ViewKind};
use serde_json::{Value, json};

fn text(path: &str) -> String {
    String::from_utf8(common::file(path)).unwrap()
}

fn cases() -> Vec<Value> {
    common::manifest()["apps"]["cases"]
        .as_array()
        .unwrap()
        .clone()
}

fn case(directory: &str) -> (Vec<u8>, Vec<u8>) {
    (
        common::file(&format!("apps/{directory}/app.json")),
        common::file(&format!("apps/{directory}/app.json.sig")),
    )
}

/// What a host that speaks `host["protocol"]` and offers `host["views"]` is configured with.
fn host(host: &Value) -> (Vec<u32>, Vec<Offer>) {
    let protocol = host["protocol"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u32::try_from(v.as_u64().unwrap()).unwrap())
        .collect();
    let views = host["views"]
        .as_array()
        .unwrap()
        .iter()
        .map(|offer| {
            let kind = match offer["kind"].as_str().unwrap() {
                "lua" => ViewKind::Lua,
                "web" => ViewKind::Web,
                other => panic!("{other} is not a kind"),
            };
            let versions: Vec<&str> = offer["versions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            Offer::new(kind, &versions)
        })
        .collect();
    (protocol, views)
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// The valid fixture's `app.json` as a JSON value, to be changed and written again.
fn valid() -> Value {
    serde_json::from_slice(&common::file("apps/valid/app.json")).unwrap()
}

fn parsed(value: &Value) -> Result<Manifest, Error> {
    parse(&serde_json::to_vec(value).unwrap())
}

#[test]
fn every_case_comes_out_as_the_manifest_says() {
    let key = text("apps/key.pub");
    let mut seen = 0;
    for entry in cases() {
        let file = entry["file"].as_str().unwrap();
        let signature = entry["signature"].as_str().unwrap();
        let outcome = entry["outcome"].as_str().unwrap();
        let result = verify(&common::file(file), &common::file(signature));
        if outcome == "ok" {
            let manifest = result.unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(manifest.name, entry["name"].as_str().unwrap(), "{file}");
            assert_eq!(
                manifest.version.to_string(),
                entry["version"].as_str().unwrap(),
                "{file}"
            );
            assert_eq!(manifest.key, key, "{file}");
            for select in entry["selects"].as_array().unwrap() {
                let (protocols, views) = host(&select["host"]);
                let chosen = match manifest.select(&protocols, &views) {
                    Selection::Current => Some(manifest.version),
                    Selection::Previous(earlier) => Some(earlier.version),
                    Selection::Unsupported {
                        version,
                        protocol,
                        views,
                    } => {
                        // What would serve is the latest release's own requirement.
                        assert_eq!(version, manifest.version, "{file}");
                        assert_eq!(protocol, manifest.protocol, "{file}");
                        assert_eq!(views, manifest.views, "{file}");
                        None
                    }
                };
                let expected = select["chooses"].as_str().map(version);
                assert_eq!(chosen, expected, "{file} for {}", select["host"]);
            }
        } else {
            assert_eq!(result.unwrap_err().reason(), outcome, "{file}");
        }
        seen += 1;
    }
    assert_eq!(seen, 9);
}

#[test]
fn the_key_in_the_file_is_the_key_of_the_fixtures() {
    let (bytes, signature) = case("valid");
    let manifest = verify(&bytes, &signature).unwrap();
    assert_eq!(manifest.key, text("apps/key.pub"));
    assert_ne!(manifest.key, text("apps/other-key.pub"));
}

#[test]
fn the_signature_is_over_the_bytes_that_were_published() {
    // The same JSON written another way is not what was signed.
    let (bytes, signature) = case("valid");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let compact = serde_json::to_vec(&value).unwrap();
    assert_ne!(compact, bytes);
    assert!(parse(&compact).is_ok());
    assert_eq!(verify(&compact, &signature), Err(Error::Signature));
    // And with a byte more at the end.
    let mut longer = bytes.clone();
    longer.push(b'\n');
    assert_eq!(verify(&longer, &signature), Err(Error::Signature));
}

#[test]
fn a_signature_is_one_line_with_or_without_the_line_feed() {
    let (bytes, signature) = case("valid");
    assert!(!signature.ends_with(b"\n"));
    let mut with_feed = signature.clone();
    with_feed.push(b'\n');
    assert!(verify(&bytes, &with_feed).is_ok());

    let coded = std::str::from_utf8(&signature).unwrap();
    let bad: Vec<(&str, Vec<u8>)> = vec![
        (
            "two line feeds",
            [signature.clone(), b"\n\n".to_vec()].concat(),
        ),
        ("a space", [signature.clone(), b" ".to_vec()].concat()),
        ("no prefix", coded.strip_prefix("ed25519:").unwrap().into()),
        ("another prefix", coded.replace("ed25519:", "rsa:").into()),
        ("cut short", signature[..signature.len() - 4].to_vec()),
        ("an unpadded end", signature[..signature.len() - 2].to_vec()),
        ("nothing", Vec::new()),
        ("not base64", b"ed25519:!!!!".to_vec()),
        ("far too long", vec![b'A'; 100_000]),
        ("not UTF-8", vec![0xff, 0xfe]),
    ];
    for (what, signature) in bad {
        assert_eq!(verify(&bytes, &signature), Err(Error::Signature), "{what}");
    }
}

#[test]
fn the_limits_are_the_specifications() {
    let (bytes, _) = case("valid");
    // Exactly the limit is read, one byte over is not even parsed.
    let padded = |length: usize| {
        let mut out = bytes.clone();
        out.resize(length, b' ');
        out
    };
    assert!(parse(&padded(MAX_MANIFEST_BYTES)).is_ok());
    assert_eq!(parse(&padded(MAX_MANIFEST_BYTES + 1)), Err(Error::TooLarge));
    // A file that is nothing but a long string is refused for its size, not read.
    assert_eq!(
        parse(&vec![b'"'; 10 * MAX_MANIFEST_BYTES]),
        Err(Error::TooLarge)
    );

    let earlier = |count: usize| {
        let mut value = valid();
        let entry = value["previous"][0].clone();
        value["previous"] = Value::Array(vec![entry; count]);
        value
    };
    assert_eq!(
        parsed(&earlier(MAX_PREVIOUS)).unwrap().previous.len(),
        MAX_PREVIOUS
    );
    assert_eq!(
        parsed(&earlier(MAX_PREVIOUS + 1)),
        Err(Error::TooManyPrevious)
    );
}

#[test]
fn what_is_not_an_object_of_the_members_is_a_shape_error() {
    let value = valid();
    let mut without_name = value.clone();
    without_name.as_object_mut().unwrap().remove("name");
    let mut name_a_number = value.clone();
    name_a_number["name"] = json!(7);
    let mut protocol_negative = value.clone();
    protocol_negative["protocol"] = json!([-1]);
    let mut view_without_versions = value.clone();
    view_without_versions["views"] = json!([{ "kind": "lua" }]);
    let mut file_without_digest = value.clone();
    file_without_digest["files"]["aarch64-apple-darwin"]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    // An array of the members in order is not the object that has them.
    let array = json!([
        "demopen",
        "0.5.1",
        [1],
        [{ "kind": "lua", "versions": ["0.3"] }],
        value["key"],
        value["files"],
        []
    ]);
    for (what, value) in [
        ("no name", without_name),
        ("a name that is a number", name_a_number),
        ("a negative protocol", protocol_negative),
        ("a view without versions", view_without_versions),
        ("a file without a digest", file_without_digest),
        ("an array", array),
        ("a string", json!("demopen")),
        ("an empty array", json!([])),
    ] {
        assert_eq!(parsed(&value), Err(Error::Shape), "{what}");
    }
    for (what, bytes) in [
        ("nothing", b"".to_vec()),
        ("text", b"not json".to_vec()),
        ("not UTF-8", vec![0xff, 0xfe, b'{', b'}']),
        (
            "a byte order mark",
            [
                b"\xef\xbb\xbf".to_vec(),
                common::file("apps/valid/app.json"),
            ]
            .concat(),
        ),
    ] {
        assert_eq!(parse(&bytes), Err(Error::Shape), "{what}");
    }
    // A member of a later version of the document is no error.
    let mut more = valid();
    more["notes"] = json!({ "anything": [1, 2, 3] });
    assert!(parsed(&more).is_ok());
}

#[test]
fn names() {
    let set = |name: &str| {
        let mut value = valid();
        value["name"] = json!(name);
        parsed(&value).map(|m| m.name)
    };
    let sixty_four = "a".repeat(64);
    for name in ["a", "9lives", "a-b", "demo-pen-2", sixty_four.as_str()] {
        assert_eq!(set(name), Ok(name.to_owned()), "{name}");
    }
    let sixty_five = "a".repeat(65);
    for name in [
        "",
        "-a",
        "Ab",
        "aB",
        "a_b",
        "a b",
        "a.b",
        "é",
        "a/b",
        "..",
        sixty_five.as_str(),
    ] {
        assert_eq!(set(name), Err(Error::Name), "{name:?}");
    }
}

#[test]
fn versions() {
    let set = |text: &str| {
        let mut value = valid();
        value["version"] = json!(text);
        parsed(&value).map(|m| m.version)
    };
    for (text, expected) in [
        ("0.0.0", (0, 0, 0)),
        ("0.5.1", (0, 5, 1)),
        ("10.20.30", (10, 20, 30)),
        ("999999999.0.0", (999_999_999, 0, 0)),
    ] {
        let version = Version {
            major: expected.0,
            minor: expected.1,
            patch: expected.2,
        };
        assert_eq!(set(text), Ok(version), "{text}");
        assert_eq!(version.to_string(), text);
    }
    for text in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-rc1",
        "1.2.3+4",
        "v1.2.3",
        " 1.2.3",
        "1.2.3 ",
        "1.2.-3",
        "1..3",
        "1.2.1000000000",
        "a.b.c",
        "١.٢.٣",
    ] {
        assert_eq!(set(text), Err(Error::Version), "{text:?}");
    }
}

#[test]
fn versions_are_compared_by_number() {
    assert!(version("0.10.0") > version("0.9.0"));
    assert!(version("0.5.1") > version("0.4.4"));
    assert!(version("1.0.0") > version("0.99.99"));
    assert!(version("0.0.10") > version("0.0.9"));
    assert_eq!(version("2.0.0"), version("2.0.0"));
}

#[test]
fn addresses() {
    let long_ok = format!("https://x/{}", "a".repeat(2048 - "https://x/".len()));
    let long = format!("https://x/{}", "a".repeat(2048 - "https://x/".len() + 1));
    for address in [
        "https://x",
        "https://github.com/a/b/releases/latest/download/app.json",
        "file:///tmp/app.json",
        long_ok.as_str(),
    ] {
        assert!(is_address(address), "{address}");
    }
    for address in [
        "",
        "http://x/y",
        "HTTPS://x/y",
        "https://",
        "https:",
        "https:/x",
        "//x/y",
        "/relative/path",
        "app.json",
        "file:/x",
        "file://x/y",
        "file:",
        "ftp://x",
        "javascript:alert(1)",
        "https://a b",
        "https://a\tb",
        "https://a\nb",
        "https://é",
        "https://x/\u{7f}",
        long.as_str(),
    ] {
        assert!(!is_address(address), "{address:?}");
    }
    // The addresses of a file and of the manifest of a previous release are held to the same rule.
    for place in ["file", "previous"] {
        let mut value = valid();
        for bad in ["http://x/y", "relative/app.json", "https://a b"] {
            if place == "file" {
                value["files"]["aarch64-apple-darwin"]["url"] = json!(bad);
            } else {
                value["previous"][0]["manifest"] = json!(bad);
            }
            assert_eq!(parsed(&value), Err(Error::Address), "{place} {bad}");
        }
    }
    assert_eq!(
        signature_address("https://x/releases/latest/download/app.json"),
        "https://x/releases/latest/download/app.json.sig"
    );
}

#[test]
fn digests_targets_keys_and_requirements() {
    let digest = |text: &str| {
        let mut value = valid();
        value["files"]["aarch64-apple-darwin"]["sha256"] = json!(text);
        parsed(&value).map(|_| ())
    };
    assert_eq!(digest(&"0".repeat(64)), Ok(()));
    for bad in [
        "0".repeat(63),
        "0".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        String::new(),
    ] {
        assert_eq!(digest(&bad), Err(Error::Digest), "{bad}");
    }

    let target = |name: &str| {
        let mut value = valid();
        let file = value["files"]["aarch64-apple-darwin"].take();
        value["files"].as_object_mut().unwrap().clear();
        value["files"][name] = file;
        parsed(&value).map(|_| ())
    };
    assert_eq!(target("x86_64-unknown-linux-musl"), Ok(()));
    assert_eq!(target(&"a".repeat(64)), Ok(()));
    for bad in ["", "Linux", "a b", "a.b", "é", &"a".repeat(65)] {
        assert_eq!(target(bad), Err(Error::Target), "{bad:?}");
    }
    let mut no_files = valid();
    no_files["files"] = json!({});
    assert_eq!(parsed(&no_files), Err(Error::Target));

    let key = |text: &str| {
        let mut value = valid();
        value["key"] = json!(text);
        parsed(&value).map(|_| ())
    };
    let good = text("apps/key.pub");
    assert_eq!(key(&good), Ok(()));
    let coded = good.strip_prefix("ed25519:").unwrap();
    for bad in [
        String::new(),
        coded.to_owned(),
        format!("rsa:{coded}"),
        format!("ed25519:{}", &coded[..coded.len() - 4]),
        format!("ed25519:{coded}AAAA"),
        format!("ed25519: {coded}"),
        format!("{good} "),
        // Eight bits of a different last digit that base64 would not write for the bytes.
        format!("ed25519:{}B=", &coded[..coded.len() - 2]),
        // A 33-byte key.
        format!("ed25519:{}", pencere_host::base64::encode(&[7; 33])),
    ] {
        assert_eq!(key(&bad), Err(Error::Key), "{bad}");
    }

    for (what, protocol, views, expected) in [
        (
            "no protocol",
            json!([]),
            json!([{ "kind": "lua", "versions": ["0.3"] }]),
            Error::Protocol,
        ),
        (
            "protocol 0",
            json!([1, 0]),
            json!([{ "kind": "lua", "versions": ["0.3"] }]),
            Error::Protocol,
        ),
        ("no views", json!([1]), json!([]), Error::Views),
    ] {
        let mut value = valid();
        value["protocol"] = protocol.clone();
        value["views"] = views.clone();
        assert_eq!(parsed(&value), Err(expected), "{what}");
        // A previous release's requirement is held to the same.
        let mut value = valid();
        value["previous"][0]["protocol"] = protocol;
        value["previous"][0]["views"] = views;
        assert_eq!(parsed(&value), Err(expected), "previous: {what}");
    }

    // Names, addresses and versions of a `previous` entry are checked too.
    let mut value = valid();
    value["previous"][0]["version"] = json!("0.4");
    assert_eq!(parsed(&value), Err(Error::Version));
}

#[test]
fn a_previous_release_is_read_as_written() {
    let (bytes, signature) = case("valid");
    let manifest = verify(&bytes, &signature).unwrap();
    assert_eq!(
        manifest.previous,
        vec![Previous {
            version: version("0.4.3"),
            protocol: vec![1],
            views: vec![pencere_host::message::ViewOffer {
                kind: "lua".to_owned(),
                versions: vec!["0.2".to_owned()],
            }],
            manifest: "https://github.com/example/demopen/releases/download/v0.4.3/app.json"
                .to_owned(),
        }]
    );
    assert_eq!(manifest.files.len(), 2);
    assert_eq!(
        manifest.files["aarch64-apple-darwin"].sha256.len(),
        64,
        "a digest"
    );
}

/// A release that lists, in no order, the releases a host may fall back to.
fn releases(list: &[(&str, &[u32], &str)]) -> Manifest {
    let mut value = valid();
    value["version"] = json!("0.7.0");
    value["protocol"] = json!([1]);
    value["views"] = json!([{ "kind": "lua", "versions": ["0.9"] }]);
    value["previous"] = Value::Array(
        list.iter()
            .map(|(version, protocol, lua)| {
                json!({
                    "version": version,
                    "protocol": protocol,
                    "views": [{ "kind": "lua", "versions": [lua] }],
                    "manifest": format!("https://example.com/v{version}/app.json"),
                })
            })
            .collect(),
    );
    parsed(&value).unwrap()
}

fn chosen(manifest: &Manifest, protocols: &[u32], lua: &[&str]) -> Option<String> {
    match manifest.select(protocols, &[Offer::new(ViewKind::Lua, lua)]) {
        Selection::Current => Some(manifest.version.to_string()),
        Selection::Previous(earlier) => Some(earlier.version.to_string()),
        Selection::Unsupported { .. } => None,
    }
}

#[test]
fn the_greatest_openable_version_wins_whatever_the_order() {
    // 0.10.0 is above 0.9.0 by number, and the list is neither in order of version nor of date.
    let manifest = releases(&[
        ("0.5.0", &[1], "0.3"),
        ("0.10.0", &[1], "0.4"),
        ("0.9.0", &[1], "0.4"),
        ("0.2.0", &[1], "0.1"),
    ]);
    assert_eq!(chosen(&manifest, &[1], &["0.9"]).as_deref(), Some("0.7.0"));
    // A host that opens 0.4 and 0.3 is given the highest number of the ones it opens.
    assert_eq!(
        chosen(&manifest, &[1], &["0.3", "0.4"]).as_deref(),
        Some("0.10.0")
    );
    assert_eq!(chosen(&manifest, &[1], &["0.4"]).as_deref(), Some("0.10.0"));
    assert_eq!(chosen(&manifest, &[1], &["0.3"]).as_deref(), Some("0.5.0"));
    assert_eq!(chosen(&manifest, &[1], &["0.1"]).as_deref(), Some("0.2.0"));
    assert_eq!(chosen(&manifest, &[1], &["0.8"]), None);
    // The host's own order of its versions decides nothing: 0.10.0 is above 0.7.0.
    for lua in [["0.9", "0.4"], ["0.4", "0.9"]] {
        assert_eq!(chosen(&manifest, &[1], &lua).as_deref(), Some("0.10.0"));
    }
}

#[test]
fn the_release_itself_wins_a_tie() {
    let manifest = releases(&[("0.7.0", &[1], "0.9")]);
    assert!(matches!(
        manifest.select(&[1], &[Offer::new(ViewKind::Lua, &["0.9"])]),
        Selection::Current
    ));
}

#[test]
fn a_release_opens_only_with_a_protocol_version_in_common() {
    let manifest = releases(&[("0.6.0", &[2], "0.9"), ("0.5.0", &[1, 2], "0.9")]);
    // The current release speaks 1.
    assert_eq!(chosen(&manifest, &[1], &["0.9"]).as_deref(), Some("0.7.0"));
    assert_eq!(chosen(&manifest, &[2], &["0.9"]).as_deref(), Some("0.6.0"));
    assert_eq!(chosen(&manifest, &[3], &["0.9"]), None);
    // The protocol versions of the host are those of the crate: 1.
    assert_eq!(SUPPORTED_VERSIONS, &[1]);
}

#[test]
fn a_kind_the_host_does_not_open_does_not_count() {
    let manifest = releases(&[("0.5.0", &[1], "0.9")]);
    let web = [Offer::new(ViewKind::Web, &["0.9"])];
    assert!(matches!(
        manifest.select(&[1], &web),
        Selection::Unsupported { .. }
    ));
    assert!(matches!(
        manifest.select(&[1], &[]),
        Selection::Unsupported { .. }
    ));
    // A release whose views name a kind that is not in the table opens nowhere on its account.
    let mut value = valid();
    value["views"] = json!([{ "kind": "wasm", "versions": ["0.3"] }]);
    value["previous"] = json!([]);
    let manifest = parsed(&value).unwrap();
    assert!(matches!(
        manifest.select(&[1], &[Offer::new(ViewKind::Lua, &["0.3"])]),
        Selection::Unsupported { .. }
    ));
    // But another kind it lists, at a version the host speaks, is enough.
    value["views"] = json!([
        { "kind": "wasm", "versions": ["0.3"] },
        { "kind": "lua", "versions": ["0.3"] }
    ]);
    let manifest = parsed(&value).unwrap();
    assert!(matches!(
        manifest.select(&[1], &[Offer::new(ViewKind::Lua, &["0.3"])]),
        Selection::Current
    ));
}
