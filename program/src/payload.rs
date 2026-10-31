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

/// The feature a terminal lists when it closes a view at the program's request (§10.8).
pub(crate) const FEATURE_DISMISS: &str = "dismiss";

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

/// The payload of an `attach` (§8.8): each file by its digest and its length.
pub(crate) fn attach(files: &[(String, u64)]) -> String {
    #[derive(Serialize)]
    struct File<'a> {
        sha256: &'a str,
        size: u64,
    }
    #[derive(Serialize)]
    struct Attach<'a> {
        files: Vec<File<'a>>,
    }
    wire(&Attach {
        files: files
            .iter()
            .map(|(sha256, size)| File {
                sha256,
                size: *size,
            })
            .collect(),
    })
}

/// The payload of a `detach` (§8.8).
pub(crate) fn detach(digests: &[String]) -> String {
    #[derive(Serialize)]
    struct Detach<'a> {
        files: &'a [String],
    }
    wire(&Detach { files: digests })
}

/// The payload of a `read` (§8.9).
pub(crate) fn read(handle: &str, offset: u64, length: u64) -> String {
    #[derive(Serialize)]
    struct Read<'a> {
        handle: &'a str,
        offset: u64,
        length: u64,
    }
    wire(&Read {
        handle,
        offset,
        length,
    })
}

/// The `dismiss`: the token the view was opened with, as the `bye` has it (§10.8).
pub(crate) fn dismiss(token: &str) -> String {
    #[derive(Serialize)]
    struct Dismiss<'a> {
        token: &'a str,
    }
    wire(&Dismiss { token })
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
    /// The kinds of the program's list that the terminal opens, and at no version the program
    /// speaks, each with the versions it does open (§5.2).
    pub(crate) opens: Vec<Listed>,
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

/// Reads a payload of the terminal as JSON, unless it nests as deep as no receiver of the protocol
/// takes (specification §4.5).
fn parsed(payload: &str) -> Option<Value> {
    if pencere_wire::json::too_deep(payload) {
        return None;
    }
    serde_json::from_str(payload).ok()
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
    let body: Value = parsed(payload)?;
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
            opens: listed(body.get("opens")),
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

/// The sentence for a program whose terminal opens kinds of its list, but not in the versions it
/// speaks (§5.2): the versions of those kinds that it is written for, and those the terminal
/// opens. `None` when the terminal named no such kind.
pub(crate) fn no_version_sentence(mine: &[(Kind, &[String])], opens: &[Listed]) -> Option<String> {
    let (mut wanted, mut held) = (Vec::new(), Vec::new());
    for (kind, versions) in mine {
        let Some(host) = opens.iter().find(|offer| offer.kind == kind.name()) else {
            continue;
        };
        wanted.push(format!("{} {}", kind.name(), versions.join(" or ")));
        held.push(format!("{} {}", kind.name(), host.versions.join(" or ")));
    }
    if wanted.is_empty() {
        return None;
    }
    Some(format!(
        "this program is written for {}; this terminal opens {}",
        wanted.join(", "),
        held.join(", ")
    ))
}

/// Reads a `need`: the digests the terminal lacks, in its order.
pub(crate) fn need(payload: &str) -> Option<Vec<String>> {
    let body: Value = parsed(payload)?;
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

/// A digest of an `attach` that the terminal does not attach, and why (`need.refused`, §8.8):
/// `limit`, `view`, `size` or `digest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) sha256: String,
    pub(crate) code: String,
}

/// Reads what a `need` refuses: nothing for the `need` of an `open`, and for one that answers an
/// `attach` whose files were all taken.
pub(crate) fn refused(payload: &str) -> Option<Vec<Refusal>> {
    let body: Value = parsed(payload)?;
    let body = body.as_object()?;
    body.get("missing")?.as_array()?;
    let Some(refused) = body.get("refused") else {
        return Some(Vec::new());
    };
    Some(
        refused
            .as_array()?
            .iter()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                let sha256 = entry.get("sha256")?.as_str()?;
                let code = entry.get("code")?.as_str()?;
                let digest = sha256.len() == 64
                    && sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
                digest.then(|| Refusal {
                    sha256: sha256.to_owned(),
                    code: code.to_owned(),
                })
            })
            .collect(),
    )
}

/// Reads a `wrote`: the end of a file the program wrote, or the first thing that went wrong with
/// it, by its code (§8.10).
pub(crate) fn wrote(payload: &str) -> Option<Option<String>> {
    let body: Value = parsed(payload)?;
    let body = body.as_object()?;
    Some(body.get("code").and_then(Value::as_str).map(str::to_owned))
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
    let body: Value = parsed(payload)?;
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

    #[test]
    fn the_dismiss_is_written_as_the_fixture_is() {
        if fixtures().is_none() {
            return;
        }
        assert_eq!(
            dismiss("q7Vh2sB0xJm9kLw4TzN1cA"),
            payload_of("to-host/dismiss.frame")
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
    fn the_versions_of_both_sides_are_said_in_words_for_the_kinds_the_terminal_named() {
        let mine = vec!["2.0".to_owned(), "3.0".to_owned()];
        let web = vec!["1.0".to_owned()];
        let program: Vec<(Kind, &[String])> = vec![(Kind::Wasm, &mine), (Kind::Web, &web)];
        let opens = [
            Listed {
                kind: "wasm".into(),
                versions: vec!["1.0".into(), "1.1".into()],
            },
            Listed {
                kind: "lua".into(),
                versions: vec!["1.0".into()],
            },
        ];
        // Only a kind that the program lists and the terminal named: `lua` is not the program's.
        assert_eq!(
            no_version_sentence(&program, &opens).as_deref(),
            Some(
                "this program is written for wasm 2.0 or 3.0; this terminal opens wasm 1.0 or 1.1"
            )
        );
        assert_eq!(no_version_sentence(&program, &[]), None);
    }

    #[test]
    fn a_welcome_is_read_as_the_fixtures_have_it() {
        let offer = welcome(
            r#"{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"t","version":"0"},"features":["dismiss"],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"web","versions":["1.0"]}]}"#,
        );
        let Some(Welcome::Offer(offer)) = offer else {
            panic!("an offer");
        };
        assert_eq!(offer.token, "q7Vh2sB0xJm9kLw4TzN1cA");
        assert!(offer.features.iter().any(|f| f == FEATURE_DISMISS));
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

    /// The messages of a fixture of what a host writes: `(kind, id, re, payload)`.
    fn read_from_host(name: &str) -> Vec<(String, u32, Option<u32>, String)> {
        let dir = fixtures().expect("the repository");
        let bytes = std::fs::read(dir.join(name)).expect("a fixture");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToProgram).feed(&bytes, &mut events);
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::Message(m) => Some((m.kind.clone(), m.id, m.re(), m.payload.clone())),
                Event::Error(_) => None,
            })
            .collect()
    }

    #[test]
    fn every_message_the_sessions_have_a_host_write_is_read_by_this_library() {
        let Some(dir) = fixtures() else {
            return;
        };
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).expect("a manifest"))
                .expect("JSON");
        let mut read = 0;
        for session in manifest["session"].as_array().expect("sessions") {
            for step in session["steps"].as_array().expect("steps") {
                let Some(file) = step["out"].as_str() else {
                    continue;
                };
                for (kind, _, _, payload) in read_from_host(file) {
                    let label = format!("{} {file} {kind}", session["name"]);
                    read += 1;
                    match kind.as_str() {
                        "welcome" => assert!(welcome(&payload).is_some(), "{label}"),
                        "need" => {
                            assert!(need(&payload).is_some(), "{label}");
                            assert!(refused(&payload).is_some(), "{label}");
                        }
                        "wrote" => assert!(wrote(&payload).is_some(), "{label}"),
                        // The answers to a `stage` and a `fetch` (§8.11) are JSON objects.
                        "staged" | "fetched" => {
                            assert!(pencere_wire::json::is_object(&payload), "{label}");
                        }
                        // The bytes of a file the person gave, in base64 (§8.9).
                        "data" => assert!(
                            pencere_wire::base64::decode(payload.as_bytes()).is_some(),
                            "{label}"
                        ),
                        "closed" => assert!(closed(&payload).is_some(), "{label}"),
                        // The words of a page, as the host passes them on.
                        "msg" => {
                            assert!(crate::handlers::read_message(&payload).is_some(), "{label}")
                        }
                        other => panic!("{label}: a host writes no {other} to a program"),
                    }
                }
            }
        }
        assert!(read > 80, "{read}");
    }

    #[test]
    fn what_a_view_says_is_read_as_the_message_fixtures_have_it() {
        let Some(dir) = fixtures() else {
            return;
        };
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).expect("a manifest"))
                .expect("JSON");
        let entries = manifest["message"].as_array().expect("messages");
        assert!(entries.len() >= 9);
        for entry in entries {
            let file = entry["file"].as_str().expect("a file");
            let text = std::fs::read_to_string(dir.join(file)).expect("a fixture");
            let read = crate::handlers::read_message(&text).map(|message| {
                let kind = match message {
                    pencere_wire::messages::Message::Call(_) => "call",
                    pencere_wire::messages::Message::Subscribe(_) => "subscribe",
                    pencere_wire::messages::Message::Cancel(_) => "cancel",
                    _ => "another",
                };
                (kind, message.id())
            });
            let expected = entry["reads"].as_object().map(|reads| {
                (
                    reads["kind"].as_str().expect("a kind"),
                    reads["id"].as_u64().expect("an id"),
                )
            });
            assert_eq!(read, expected, "{file}");
        }
    }

    #[test]
    fn what_a_restart_and_a_refusal_say_is_read_as_the_sessions_have_it() {
        if fixtures().is_none() {
            return;
        }
        // A restart is an offer with no hello it answers, a new token and `restart`.
        let restart = read_from_host("session/restart/06-out.frame");
        let (_, _, re, payload) = &restart[0];
        assert_eq!(*re, None);
        let Some(Welcome::Offer(offer)) = welcome(payload) else {
            panic!("an offer");
        };
        assert!(offer.restart);
        assert_eq!(offer.token, "Zk3mP8rD5uYb1Xe6HqS0vg");
        // A kind the host opens at no version the program speaks is named in `opens`.
        let (_, _, _, payload) = &read_from_host("to-program/welcome-opens.frame")[0];
        let Some(Welcome::Offer(offer)) = welcome(payload) else {
            panic!("an offer");
        };
        assert_eq!(offer.views.len(), 1);
        assert_eq!(offer.opens.len(), 1);
        assert_eq!(offer.opens[0].kind, "wasm");
        // A `closed` that answers something has `re`; one that ends a running view has none.
        let (_, _, re, payload) = &read_from_host("session/view-ends/06-out.frame")[0];
        assert_eq!(*re, None);
        assert_eq!(closed(payload).map(|c| c.reason), Some(ClosedReason::User));
        let (_, _, re, payload) = &read_from_host("session/screen-refused/06-out.frame")[0];
        assert_eq!(*re, Some(2));
        assert_eq!(
            closed(payload).and_then(|c| c.code).as_deref(),
            Some("screen")
        );
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
