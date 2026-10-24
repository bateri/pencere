//! The payloads a program writes (`hello`, `open`, `bye`) and reads (`welcome`, `need`, `closed`),
//! as Appendix A of the specification gives them.
//!
//! What the terminal writes is read as untrusted: a member of the wrong kind is no member, and
//! what cannot be used is `None`, never a panic. Members are written in the order of the
//! specification, which is the order the fields are declared in, because the fixtures are bytes.

use serde::Serialize;
use serde_json::Value;

use crate::bundle::Bundle;
use crate::view::{Access, Icon, Kind, is_version};

/// The versions of the wire this library speaks (specification §5.4).
pub(crate) const VERSIONS: &[u32] = &[1];

/// The feature of a `welcome`: the terminal takes bundles (§8.1).
pub(crate) const FEATURE_BUNDLE: &str = "bundle";

/// Why a program ends (`bye.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ByeReason {
    /// The program is leaving after a panic or a failure of its own.
    Error,
    /// A signal asked the program to end.
    Signal,
}

impl ByeReason {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Signal => "signal",
        }
    }
}

// ---- what the program writes ---------------------------------------------------------

#[derive(Serialize)]
struct OfferedKind<'a> {
    kind: &'a str,
    versions: &'a [String],
}

#[derive(Serialize)]
struct Hello<'a> {
    versions: &'a [u32],
    views: Vec<OfferedKind<'a>>,
}

/// The `hello`: the versions of the wire the program speaks and the kinds of view it can show,
/// each with the versions of its interface, in the program's order (§5.1).
pub(crate) fn hello(offers: &[(Kind, &[String])]) -> String {
    let hello = Hello {
        versions: VERSIONS,
        views: offers
            .iter()
            .map(|(kind, versions)| OfferedKind {
                kind: kind.name(),
                versions,
            })
            .collect(),
    };
    wire(&hello)
}

#[derive(Serialize)]
struct ManifestFile<'a> {
    path: &'a str,
    sha256: String,
    size: usize,
    #[serde(rename = "type")]
    media_type: &'a str,
}

#[derive(Serialize)]
struct ManifestBody<'a> {
    entry: &'a str,
    files: Vec<ManifestFile<'a>>,
}

#[derive(Serialize)]
struct Open<'a> {
    token: &'a str,
    view: &'a str,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    program: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    icons: &'a [Icon],
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<&'a Access>,
    bundle: ManifestBody<'a>,
}

/// What an `open` says besides its bundle (§6.3).
pub(crate) struct OpenFields<'a> {
    pub(crate) token: &'a str,
    pub(crate) kind: Kind,
    pub(crate) version: &'a str,
    pub(crate) program: Option<&'a str>,
    pub(crate) title: Option<&'a str>,
    pub(crate) icons: &'a [Icon],
    pub(crate) access: Option<&'a Access>,
}

/// The `open` of a view with its bundle's manifest (§6.3).
pub(crate) fn open(fields: &OpenFields<'_>, bundle: &Bundle) -> String {
    let body = Open {
        token: fields.token,
        view: fields.kind.name(),
        version: fields.version,
        program: fields.program,
        title: fields.title,
        icons: fields.icons,
        access: fields.access,
        bundle: ManifestBody {
            entry: bundle.entry(),
            files: bundle
                .files()
                .iter()
                .map(|file| ManifestFile {
                    path: &file.path,
                    sha256: file.digest.to_string(),
                    size: file.size,
                    media_type: &file.media_type,
                })
                .collect(),
        },
    };
    wire(&body)
}

/// The `bye`: the token the view was opened with, and why the program ends (§10.3).
pub(crate) fn bye(token: &str, reason: ByeReason) -> String {
    #[derive(Serialize)]
    struct Bye<'a> {
        token: &'a str,
        reason: &'a str,
    }
    wire(&Bye {
        token,
        reason: reason.name(),
    })
}

/// A value of the types above as the wire writes JSON. They hold strings and numbers only, which
/// cannot fail to serialise; an empty payload is not a message anyone could mistake for one.
fn wire<T: Serialize>(value: &T) -> String {
    pencere_wire::json::to_wire(value).unwrap_or_default()
}

// ---- what the terminal writes ----------------------------------------------------------

/// A kind of view and the versions of its interface, as a `welcome` lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) kind: String,
    pub(crate) versions: Vec<String>,
}

/// What a terminal answers to `hello` with a version in common (§5.2), or offers again unasked
/// (§10.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) token: String,
    pub(crate) features: Vec<String>,
    /// What the terminal will open. For an answer to a `hello` the program's list cut down to what
    /// both speak, in the program's order; for an unsolicited `welcome`, every kind the terminal
    /// opens, in its own order. Empty when nothing is shared.
    pub(crate) views: Vec<Listed>,
    pub(crate) restart: bool,
}

/// What a terminal answers to `hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Welcome {
    Offer(Offer),
    /// A terminal that shares no version of the wire with the program.
    Unsupported,
}

/// A token: 16 bytes in base64url without padding (§6.1).
fn is_token(text: &str) -> bool {
    text.len() == 22
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The entries of a `views` list that are of the shape of §5.2: a string `kind` and a list of
/// versions, at least one of them a version of §5.5. A kind listed twice counts once, at its first
/// place (§5.1). Anything else in the list is left out.
fn listed(value: Option<&Value>) -> Vec<Listed> {
    let mut views: Vec<Listed> = Vec::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(kind) = item.get("kind").and_then(Value::as_str) else {
            continue;
        };
        if views.iter().any(|view| view.kind == kind) {
            continue;
        }
        let versions: Vec<String> = strings(item.get("versions"))
            .into_iter()
            .filter(|v| is_version(v))
            .collect();
        if !versions.is_empty() {
            views.push(Listed {
                kind: kind.to_owned(),
                versions,
            });
        }
    }
    views
}

/// Reads a `welcome`. A version the program did not offer is not one the terminal can have
/// chosen, so such a terminal is one that shares no version.
pub(crate) fn welcome(payload: &str) -> Option<Welcome> {
    let body: Value = serde_json::from_str(payload).ok()?;
    let body = body.as_object()?;
    if let Some(version) = body.get("version") {
        let version = version.as_u64().filter(|v| *v >= 1)?;
        let token = body.get("token")?.as_str().filter(|t| is_token(t))?;
        if !VERSIONS.iter().any(|v| u64::from(*v) == version) {
            return Some(Welcome::Unsupported);
        }
        return Some(Welcome::Offer(Offer {
            token: token.to_owned(),
            features: strings(body.get("features")),
            views: listed(body.get("views")),
            restart: body.get("restart") == Some(&Value::Bool(true)),
        }));
    }
    body.get("supported")
        .is_some_and(Value::is_array)
        .then_some(Welcome::Unsupported)
}

/// What the program opens, given its own list and what a `welcome` lists (§5.3): the first kind
/// of the program's list that the terminal lists and, for it, the first of the program's
/// versions of it that the terminal lists. The program's order decides: the answer to a `hello`
/// already keeps it (§5.2), and an unsolicited `welcome` lists the terminal's own order (§10.7).
/// The kind is the index in the program's list. `None` when nothing is shared.
pub(crate) fn choose(mine: &[(Kind, &[String])], theirs: &[Listed]) -> Option<(usize, String)> {
    for (at, (kind, versions)) in mine.iter().enumerate() {
        let Some(host) = theirs.iter().find(|offer| offer.kind == kind.name()) else {
            continue;
        };
        if let Some(version) = versions.iter().find(|v| host.versions.contains(v)) {
            return Some((at, version.clone()));
        }
    }
    None
}

/// Reads a `need`: the digests the terminal lacks, in its order.
pub(crate) fn need(payload: &str) -> Option<Vec<String>> {
    let body: Value = serde_json::from_str(payload).ok()?;
    let missing = body.as_object()?.get("missing")?.as_array()?;
    Some(
        missing
            .iter()
            .filter_map(Value::as_str)
            .filter(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .map(str::to_owned)
            .collect(),
    )
}

/// Why a terminal ended or refused a view (`closed.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosedReason {
    Refused,
    Error,
    User,
    View,
    Program,
}

/// What a terminal says when a view ended or was refused (§6.4, §10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Closed {
    pub(crate) reason: ClosedReason,
    pub(crate) code: Option<String>,
    pub(crate) message: Option<String>,
}

/// Reads a `closed`. A reason this version does not know still ends the view, and is read as
/// `error`: a terminal that has learned a new reason has no other way to say that the view is
/// gone.
pub(crate) fn closed(payload: &str) -> Option<Closed> {
    let body: Value = serde_json::from_str(payload).ok()?;
    let body = body.as_object()?;
    let reason = match body.get("reason")?.as_str()? {
        "refused" => ClosedReason::Refused,
        "user" => ClosedReason::User,
        "view" => ClosedReason::View,
        "program" => ClosedReason::Program,
        _ => ClosedReason::Error,
    };
    let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(Closed {
        reason,
        code: text("code"),
        message: text("message"),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::bundle::File;
    use crate::view::{IconTheme, Permission};
    use pencere_wire::frame::{Direction, Event, StreamDecoder};

    /// The directory of the draft's fixtures, when this is the repository (a published crate does
    /// not carry them).
    fn fixtures() -> Option<std::path::PathBuf> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        if !root.join("Cargo.toml").is_file() {
            return None;
        }
        let dir = root.join("spec/draft/fixtures");
        assert!(dir.is_dir(), "the fixtures moved: {}", dir.display());
        Some(dir)
    }

    /// The messages of a fixture of what a program writes: `(kind, id, payload)`.
    fn read(name: &str) -> Vec<(String, u32, String)> {
        let dir = fixtures().expect("the repository");
        let bytes = std::fs::read(dir.join(name)).expect("a fixture");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToHost).feed(&bytes, &mut events);
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::Message(m) => Some((m.kind, m.id, m.payload)),
                Event::Error(_) => None,
            })
            .collect()
    }

    fn payload_of(name: &str) -> String {
        read(name).into_iter().next().expect("a message").2
    }

    fn versions(list: &[&str]) -> Vec<String> {
        list.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn the_hello_is_written_as_the_fixtures_are() {
        if fixtures().is_none() {
            return;
        }
        let web = versions(&["1.0"]);
        assert_eq!(
            hello(&[(Kind::Web, &web)]),
            payload_of("to-host/hello.frame")
        );
        let (wasm, lua) = (versions(&["0.5", "0.4"]), versions(&["0.5"]));
        assert_eq!(
            hello(&[(Kind::Wasm, &wasm), (Kind::Lua, &lua), (Kind::Web, &web)]),
            payload_of("to-host/hello-views.frame")
        );
    }

    #[test]
    fn the_bye_is_written_as_the_fixture_is() {
        if fixtures().is_none() {
            return;
        }
        assert_eq!(
            bye("q7Vh2sB0xJm9kLw4TzN1cA", ByeReason::Signal),
            payload_of("to-host/bye.frame")
        );
    }

    /// The three files of the bundle in the examples of the specification, byte for byte.
    fn page() -> Bundle {
        Bundle::new(
            "index.html",
            [
                File::new(
                    "index.html",
                    &b"<!doctype html>\n<meta charset=\"utf-8\">\n<link rel=\"stylesheet\" href=\"style.css\">\n<div id=\"app\"></div>\n<script src=\"app.js\"></script>\n"[..],
                ),
                File::new(
                    "app.js",
                    &b"document.getElementById(\"app\").textContent = \"hello from pencere\";\n"[..],
                ),
                File::new(
                    "style.css",
                    &b"body { margin: 0; font: 14px var(--font-sans, sans-serif); }\n"[..],
                ),
            ],
        )
        .expect("a bundle")
    }

    /// A component that is no more than its header: the bundle of the fixtures' kit views.
    fn component() -> Bundle {
        Bundle::single("view.wasm", &b"\0asm\x01\0\0\0"[..]).expect("a bundle")
    }

    const TOKEN: &str = "q7Vh2sB0xJm9kLw4TzN1cA";

    #[test]
    fn the_open_of_a_page_is_written_as_the_fixture_is() {
        if fixtures().is_none() {
            return;
        }
        let access = Access {
            connect: vec!["https://api.example.com".into()],
            permissions: vec![Permission::ClipboardWrite],
            ..Access::default()
        };
        let fields = OpenFields {
            token: TOKEN,
            kind: Kind::Web,
            version: "1.0",
            program: Some("containers"),
            title: Some("Containers"),
            icons: &[],
            access: Some(&access),
        };
        assert_eq!(open(&fields, &page()), payload_of("to-host/open-web.frame"));
    }

    #[test]
    fn the_open_of_a_view_the_terminal_draws_is_written_as_the_fixtures_are() {
        if fixtures().is_none() {
            return;
        }
        let fields = |kind, icons| OpenFields {
            token: TOKEN,
            kind,
            version: "0.5",
            program: Some("demopen"),
            title: Some("Containers"),
            icons,
            access: None,
        };
        assert_eq!(
            open(&fields(Kind::Wasm, &[]), &component()),
            payload_of("to-host/open-wasm.frame")
        );

        // The script of the fixture is not among the files here: it is a file whose bytes this
        // test does not have. Another script is the same `open` in everything but the digest and
        // the size.
        let script = Bundle::single("view.lua", b"return { name = 'a view' }\n\n\n".to_vec())
            .expect("a bundle");
        let written: Value =
            serde_json::from_str(&open(&fields(Kind::Lua, &[]), &script)).expect("JSON");
        let fixture: Value =
            serde_json::from_str(&payload_of("to-host/open-lua.frame")).expect("JSON");
        for pointer in [
            "/token",
            "/view",
            "/version",
            "/program",
            "/title",
            "/bundle/entry",
            "/bundle/files/0/path",
            "/bundle/files/0/type",
        ] {
            assert_eq!(
                written.pointer(pointer),
                fixture.pointer(pointer),
                "{pointer}"
            );
        }

        let icon = Icon {
            src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==".into(),
            // Written here in another order than the wire's.
            theme: Some(IconTheme::Dark),
            sizes: vec!["1x1".into()],
            mime_type: Some("image/png".into()),
        };
        assert_eq!(
            open(&fields(Kind::Wasm, &[icon]), &component()),
            payload_of("to-host/open-icons.frame")
        );
    }

    #[test]
    fn a_welcome_is_read_as_the_fixtures_have_it() {
        let offer = welcome(
            r#"{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"t","version":"0"},"features":["bundle","dismiss"],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"web","versions":["1.0"]}]}"#,
        );
        let Some(Welcome::Offer(offer)) = offer else {
            panic!("an offer");
        };
        assert_eq!(offer.token, "q7Vh2sB0xJm9kLw4TzN1cA");
        assert!(offer.features.iter().any(|f| f == FEATURE_BUNDLE));
        assert_eq!(offer.views.len(), 2);
        assert!(!offer.restart);

        // A kind listed twice counts once; an entry that is not of the shape is left out.
        let Some(Welcome::Offer(offer)) = welcome(
            r#"{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","features":[],"views":[{"kind":"web","versions":["1.0"]},{"kind":"web","versions":["2.0"]},{"kind":"lua","versions":["x"]},"web",{"versions":["1.0"]}],"restart":true}"#,
        ) else {
            panic!("an offer");
        };
        assert_eq!(offer.views.len(), 1);
        assert!(offer.restart);

        assert_eq!(
            welcome(r#"{"supported":[2],"host":{"name":"t","version":"0"}}"#),
            Some(Welcome::Unsupported)
        );
        assert_eq!(
            welcome(r#"{"version":2,"token":"q7Vh2sB0xJm9kLw4TzN1cA"}"#),
            Some(Welcome::Unsupported)
        );
        // A token that is not 22 characters of base64url, a payload that is not an object.
        assert_eq!(welcome(r#"{"version":1,"token":"short"}"#), None);
        assert_eq!(welcome("[1]"), None);
        assert_eq!(welcome("not json"), None);
    }

    #[test]
    fn the_program_chooses_in_its_own_order() {
        let (wasm, lua, web) = (
            versions(&["1.0", "0.9"]),
            versions(&["1.0"]),
            versions(&["1.0"]),
        );
        let mine: Vec<(Kind, &[String])> =
            vec![(Kind::Wasm, &wasm), (Kind::Lua, &lua), (Kind::Web, &web)];
        let host = |list: &[(&str, &[&str])]| -> Vec<Listed> {
            list.iter()
                .map(|(kind, v)| Listed {
                    kind: (*kind).into(),
                    versions: versions(v),
                })
                .collect()
        };
        // The terminal's own order does not matter, the program's does.
        assert_eq!(
            choose(&mine, &host(&[("web", &["1.0"]), ("lua", &["1.0"])])),
            Some((1, "1.0".into()))
        );
        // The first version of the program's that the terminal speaks.
        assert_eq!(
            choose(&mine, &host(&[("wasm", &["0.9"])])),
            Some((0, "0.9".into()))
        );
        // A kind at no version in common is not a kind in common.
        assert_eq!(choose(&mine, &host(&[("wasm", &["2.0"])])), None);
        assert_eq!(choose(&mine, &[]), None);
    }

    #[test]
    fn a_need_and_a_closed_are_read_as_the_fixtures_have_them() {
        let digest = "a".repeat(64);
        assert_eq!(
            need(&format!(
                r#"{{"missing":["{digest}","short",7,"{}"]}}"#,
                "G".repeat(64)
            )),
            Some(vec![digest])
        );
        assert_eq!(need(r#"{"missing":[]}"#), Some(vec![]));
        assert_eq!(need(r#"{"other":[]}"#), None);

        let closed_user = closed(r#"{"reason":"user"}"#).expect("closed");
        assert_eq!(closed_user.reason, ClosedReason::User);
        let failed = closed(r#"{"reason":"error","code":"view","message":"script:12: boom"}"#)
            .expect("closed");
        assert_eq!(failed.reason, ClosedReason::Error);
        assert_eq!(failed.code.as_deref(), Some("view"));
        assert_eq!(failed.message.as_deref(), Some("script:12: boom"));
        // A reason this version does not know still ends the view.
        assert_eq!(
            closed(r#"{"reason":"later"}"#).map(|c| c.reason),
            Some(ClosedReason::Error)
        );
        assert_eq!(closed(r#"{"code":"x"}"#), None);
    }
}
