//! The payloads of the messages (specification Appendix A), as the host reads
//! and writes them.
//!
//! Members are written in the order the specification gives, the one place it
//! differs from the fixtures being `welcome`: with `supported` the fixtures (and
//! §5.2) write `supported` before `host`, so the two shapes of the answer are two
//! types here.

use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::json::{self, Object};

/// The protocol versions this crate implements: this version, and the one
/// before it when there is one (specification §5.4).
pub const SUPPORTED_VERSIONS: &[u32] = &[1];

/// The name and version of a host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostInfo {
    /// The host's name.
    pub name: String,
    /// The host's version.
    pub version: String,
}

/// A feature a host may list in its `welcome`: it closes a view when the program
/// sends `dismiss` (specification §10.8).
pub const FEATURE_DISMISS: &str = "dismiss";

/// A feature a host may list in its `welcome`: it keeps a folder on the program's machine through
/// which the bytes of a file can travel, and says where in `stage` (specification §8.11).
pub const FEATURE_STAGE: &str = "stage";

/// The folder a host keeps on the program's machine, as the `welcome` offers it
/// (`stage`, specification §5.2 and §8.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StageDir {
    dir: String,
    token: String,
}

impl StageDir {
    /// The folder at `dir`, whose `marker` file holds `token`. `None` when `dir` is not an
    /// absolute path (it starts with `/`, has no NUL and is under 4096 bytes) or `token` is not
    /// 32 lowercase hex digits: a program checks both, so a host does not offer what it would
    /// refuse.
    #[must_use]
    pub fn new(dir: &str, token: &str) -> Option<Self> {
        let ok = dir.starts_with('/') && dir.len() < 4096 && !dir.contains('\0');
        (ok && crate::frame::is_handle(token)).then(|| Self {
            dir: dir.to_owned(),
            token: token.to_owned(),
        })
    }

    /// The folder's path.
    #[must_use]
    pub fn dir(&self) -> &str {
        &self.dir
    }

    /// What the folder's `marker` file holds.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// What a `welcome` says the host supports beyond the kinds of view (`features`, and the `stage` that
/// goes with the feature of that name; specification §5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct Features<'a> {
    /// The names of the optional parts the host supports.
    pub names: &'a [&'static str],
    /// Where the host's folder is, when [`FEATURE_STAGE`] is among the names.
    pub stage: Option<&'a StageDir>,
}

/// A kind of view and the versions of its interface, in the order of whoever wrote the list: an
/// entry of `views` in a `hello` and in a `welcome` (specification §5.1, §5.2, Appendix A).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ViewOffer {
    /// The name of the kind (specification §5.5).
    pub kind: String,
    /// The versions of the kind's interface (`major.minor`).
    pub versions: Vec<String>,
}

impl<'de> Deserialize<'de> for ViewOffer {
    /// An object with a string `kind` and a `versions` that is a list of strings; anything else,
    /// a list standing for the object included, is not an entry.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            kind: String,
            versions: Vec<String>,
        }
        let Object(Fields { kind, versions }) = Object::<Fields>::deserialize(deserializer)?;
        Ok(ViewOffer { kind, versions })
    }
}

/// The program's `hello`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hello {
    /// The protocol versions the program speaks.
    pub versions: Vec<u32>,
    /// The kinds of view the program can show, each with the versions of its interface the
    /// program speaks, in the order it prefers them (specification §5.1). Names the host does
    /// not know are kept: the host leaves them out of its answer.
    pub views: Vec<ViewOffer>,
}

#[derive(Serialize)]
struct Offer<'a> {
    version: u32,
    token: &'a str,
    host: &'a HostInfo,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    features: &'a [&'static str],
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'a StageDir>,
    views: &'a [ViewOffer],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    opens: &'a [ViewOffer],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    restart: bool,
}

#[derive(Serialize)]
struct Unsupported<'a> {
    supported: &'a [u32],
    host: &'a HostInfo,
}

/// The host's `welcome` carrying a version and a token. `views` are the kinds of view it will
/// open, each with the versions of it that the program and the host both speak, in the order it
/// was given (specification §5.2); it can be empty. `opens` are the kinds the program listed that
/// the host opens at no version the program speaks, each with the versions the host does open
/// (left out of the message when there are none).
pub fn welcome_offer(
    version: u32,
    token: &str,
    host: &HostInfo,
    features: &Features<'_>,
    views: &[ViewOffer],
    opens: &[ViewOffer],
    restart: bool,
) -> String {
    wire(&Offer {
        version,
        token,
        host,
        features: features.names,
        stage: features.stage,
        views,
        opens,
        restart,
    })
}

/// The host's `welcome` for a program that shares no version with it.
pub fn welcome_unsupported(host: &HostInfo) -> String {
    wire(&Unsupported {
        supported: SUPPORTED_VERSIONS,
        host,
    })
}

/// The host's `need`: the digests it does not hold.
pub fn need(missing: &[String]) -> String {
    need_refusing(missing, &[])
}

/// A digest of an `attach` that the host will not attach, and why (`Need.refused`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refusal {
    /// The digest, 64 lowercase hex digits.
    pub sha256: String,
    /// Why: `limit`, `view`, `size` or `digest` (specification §8.8).
    pub code: &'static str,
}

/// The host's `need` with the digests it refuses: the answer to an `attach` (specification
/// §8.8). An empty `refused` is left out, which makes it the `need` of an `open`.
pub fn need_refusing(missing: &[String], refused: &[Refusal]) -> String {
    #[derive(Serialize)]
    struct Need<'a> {
        missing: &'a [String],
        #[serde(skip_serializing_if = "<[Refusal]>::is_empty")]
        refused: &'a [Refusal],
    }
    wire(&Need { missing, refused })
}

/// What the host says of a `stage` or a `fetch`: the `staged` or `fetched` payloads of specification
/// §8.11. Each of the four answers has one member, and `at` goes with a `route` of a `fetch`.
#[derive(Serialize)]
struct Transfer<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pipe: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sent: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    saved: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<u64>,
}

const NO_TRANSFER: Transfer<'static> = Transfer {
    pipe: None,
    sent: None,
    saved: None,
    code: None,
    at: None,
};

/// The first answer to a `stage` or a `fetch`: the pipe the program opens (specification §8.11).
pub fn transfer_pipe(pipe: &str) -> String {
    wire(&Transfer {
        pipe: Some(pipe),
        ..NO_TRANSFER
    })
}

/// The last answer to a `stage`: the file went through the pipe, and ended at `sent`.
pub fn transfer_sent(sent: u64) -> String {
    wire(&Transfer {
        sent: Some(sent),
        ..NO_TRANSFER
    })
}

/// The last answer to a `fetch`: the file is saved, and `saved` bytes long.
pub fn transfer_saved(saved: u64) -> String {
    wire(&Transfer {
        saved: Some(saved),
        ..NO_TRANSFER
    })
}

/// The last answer to a `stage` or a `fetch` that failed, by `code`; `at` is the length the host
/// holds of a file, in the `route` of a `fetch`.
pub fn transfer_failed(code: &str, at: Option<u64>) -> String {
    wire(&Transfer {
        code: Some(code),
        at,
        ..NO_TRANSFER
    })
}

/// The host's `wrote`: the end of what a program wrote to a file the person chose to save, or
/// the first thing that went wrong with it, by `code` (specification §8.10).
pub fn wrote(code: Option<&str>) -> String {
    #[derive(Serialize)]
    struct Wrote<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<&'a str>,
    }
    wire(&Wrote { code })
}

/// Why the host ended or refused a view (`Closed.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    /// The host refused the `open`.
    Refused,
    /// The open or the view failed.
    Error,
    /// The person closed the view.
    User,
    /// The view asked to close.
    View,
    /// The program asked for the view to close and keeps running (`dismiss`).
    Program,
}

/// The host's `closed`. `message` is the words of a view that failed on its own (specification
/// §9.8); the session has cleaned them.
pub fn closed(reason: Reason, code: Option<&str>, message: Option<&str>) -> String {
    #[derive(Serialize)]
    struct Closed<'a> {
        reason: Reason,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<&'a str>,
    }
    wire(&Closed {
        reason,
        code,
        message,
    })
}

fn wire<T: Serialize>(value: &T) -> String {
    // The types above hold strings and numbers only, which cannot fail to
    // serialise; an empty payload is not a message anyone could mistake for one.
    json::to_wire(value).unwrap_or_default()
}

/// The program's `bye`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bye {
    /// The token the view was opened with (specification §10.3). Absent, or not a
    /// string, it cannot match anything: the `bye` is ignored.
    #[serde(default, deserialize_with = "string_or_none")]
    pub token: Option<String>,
    /// `exit`, `signal` or `error`, when it says.
    pub reason: Option<String>,
}

/// The program's `dismiss`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dismiss {
    /// The token the view was opened with (specification §10.8). Absent, or not a
    /// string, it cannot match anything: the `dismiss` is ignored.
    #[serde(default, deserialize_with = "string_or_none")]
    pub token: Option<String>,
}

// ---- the person's files ----------------------------------------------------

/// One file of an `attach`: its digest and its size, as the program says them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AttachFile {
    /// Its digest, as the text the program gives.
    pub sha256: String,
    /// Its length in bytes.
    pub size: u64,
}

/// The program's `attach`: files it adds to the view that runs (specification §8.8).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Attach {
    /// The files, in the program's order.
    pub files: Vec<Object<AttachFile>>,
}

/// The program's `detach`: digests of files it attached, which it lets go of.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Detach {
    /// The digests.
    pub files: Vec<String>,
}

/// The program's `stage`: the file of a handle, from `offset`, through the host's folder
/// (specification §8.11).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Stage {
    /// The handle of a file the person gave.
    pub handle: String,
    /// Where in the file the bytes start.
    pub offset: u64,
}

/// The program's `fetch`: the file the program sends through the host's folder, to be saved in the
/// place of a handle (specification §8.11).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Fetch {
    /// The handle of a place the person chose to save in.
    pub handle: String,
    /// The length of the whole file.
    pub size: u64,
    /// Where in the file the bytes sent start.
    pub offset: u64,
}

/// The program's `unstage`: it takes back a `stage` or a `fetch` (specification §8.11).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Unstage {
    /// The id of the `stage` or `fetch` message.
    pub id: u32,
}

/// The program's `read`: bytes of a file the person gave (specification §8.9).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Read {
    /// The handle the person's gesture made, as the program passes it on.
    pub handle: String,
    /// Where the bytes start.
    pub offset: u64,
    /// How many bytes, at most.
    pub length: u64,
}

/// A member that is a string, or absent, or anything else, which reads as absent.
fn string_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(MaybeString::deserialize(deserializer)?.0)
}

// ---- open ------------------------------------------------------------------

/// What reading the payload of an `open` for its token found.
#[derive(Debug, PartialEq, Eq)]
pub enum TokenProbe {
    /// The payload is not JSON: the message is ignored.
    NotJson,
    /// The payload is JSON that has no string `token`.
    NoToken,
    /// The `token` the payload carries.
    Token(String),
}

/// A value that is a string or anything else; the anything else is skipped
/// without being built.
struct MaybeString(Option<String>);

impl<'de> Deserialize<'de> for MaybeString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = MaybeString;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_str<E>(self, v: &str) -> Result<MaybeString, E> {
                Ok(MaybeString(Some(v.to_owned())))
            }
            fn visit_bool<E>(self, _: bool) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_i64<E>(self, _: i64) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_u64<E>(self, _: u64) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_f64<E>(self, _: f64) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_unit<E>(self) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_none<E>(self) -> Result<MaybeString, E> {
                Ok(MaybeString(None))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<MaybeString, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(MaybeString(None))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MaybeString, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(MaybeString(None))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// An object read for its `token` and nothing else.
struct TokenOnly(Option<String>);

impl<'de> Deserialize<'de> for TokenOnly {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = TokenOnly;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<TokenOnly, A::Error> {
                let mut token = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "token" {
                        token = map.next_value::<MaybeString>()?.0;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(TokenOnly(token))
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// Reads the token out of an `open` payload before anything else in it is read
/// (specification §6.4): a payload whose other members are huge or wrong costs
/// nothing here, and one that is not JSON is told apart from one with no token.
pub fn probe_token(payload: &str) -> TokenProbe {
    match serde_json::from_str::<TokenOnly>(payload) {
        Ok(TokenOnly(Some(token))) => TokenProbe::Token(token),
        Ok(TokenOnly(None)) => TokenProbe::NoToken,
        Err(_) if serde_json::from_str::<IgnoredAny>(payload).is_ok() => TokenProbe::NoToken,
        Err(_) => TokenProbe::NotJson,
    }
}

/// The rest of an `open`, read once its token has been accepted.
#[derive(Debug, Deserialize)]
pub struct OpenBody {
    /// The kind of view the program opens, by name (specification §6.3). Absent, or not a
    /// string, it is a malformed payload.
    #[serde(default, deserialize_with = "string_or_none")]
    pub view: Option<String>,
    /// The version of that kind's interface the view is written against. Absent, or not a
    /// string, it is a malformed payload.
    #[serde(default, deserialize_with = "string_or_none")]
    pub version: Option<String>,
    /// What the program calls itself and its view (specification §7.3).
    #[serde(default, deserialize_with = "string_or_none")]
    pub program: Option<String>,
    /// The title the program gives its view.
    #[serde(default, deserialize_with = "string_or_none")]
    pub title: Option<String>,
    /// The icons of the program, as the program wrote them: a host reads them (the host crate's
    /// `icon` module does) and whatever is wrong with them is no reason to refuse the view.
    #[serde(default)]
    pub icons: Option<Box<RawValue>>,
    /// What a page asks to reach, as the program wrote it: read only for a `web` view
    /// (specification §6.3).
    #[serde(default)]
    pub access: Option<Box<RawValue>>,
    /// The manifest of the view's files, before it is checked.
    #[serde(default)]
    pub bundle: Option<RawBundle>,
}

/// A bundle manifest as it arrives, before it is checked.
#[derive(Debug, Deserialize)]
pub struct RawBundle {
    /// The path of the document.
    pub entry: String,
    /// The files, as the manifest lists them.
    pub files: Vec<RawFile>,
}

/// One file of a manifest as it arrives.
#[derive(Debug, Deserialize)]
pub struct RawFile {
    /// Where the view finds it.
    pub path: String,
    /// Its digest, as the text the manifest gives.
    pub sha256: String,
    /// Its length, as the manifest says.
    pub size: u64,
    /// Its media type.
    #[serde(rename = "type")]
    pub media_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn host() -> HostInfo {
        HostInfo {
            name: "pencere-test".into(),
            version: "0.0.0".into(),
        }
    }

    fn names<'a>(names: &'a [&'static str]) -> Features<'a> {
        Features { names, stage: None }
    }

    fn offer(kind: &str, versions: &[&str]) -> ViewOffer {
        ViewOffer {
            kind: kind.into(),
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
        }
    }

    #[test]
    fn the_answers_are_written_as_the_fixtures_are() {
        assert_eq!(
            welcome_offer(
                1,
                "q7Vh2sB0xJm9kLw4TzN1cA",
                &host(),
                &names(&["dismiss"]),
                &[offer("web", &["1.0"])],
                &[],
                false
            ),
            r#"{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}"#
        );
        assert_eq!(
            welcome_offer(
                1,
                "Zk3mP8rD5uYb1Xe6HqS0vg",
                &host(),
                &names(&["dismiss"]),
                &[offer("wasm", &["0.5", "0.4"]), offer("web", &["1.0"])],
                &[],
                true
            ),
            r#"{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"web","versions":["1.0"]}],"restart":true}"#
        );
        assert_eq!(
            welcome_offer(1, "t", &host(), &Features::default(), &[], &[], false),
            r#"{"version":1,"token":"t","host":{"name":"pencere-test","version":"0.0.0"},"views":[]}"#
        );
        // The kinds the host opens at no version the program speaks, after `views`.
        assert_eq!(
            welcome_offer(
                1,
                "t",
                &host(),
                &Features::default(),
                &[offer("web", &["1.0"])],
                &[offer("wasm", &["0.5", "0.4"])],
                false
            ),
            r#"{"version":1,"token":"t","host":{"name":"pencere-test","version":"0.0.0"},"views":[{"kind":"web","versions":["1.0"]}],"opens":[{"kind":"wasm","versions":["0.5","0.4"]}]}"#
        );
        assert_eq!(
            welcome_unsupported(&host()),
            r#"{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}"#
        );
        assert_eq!(need(&[]), r#"{"missing":[]}"#);
        assert_eq!(
            closed(Reason::Refused, Some("token"), None),
            r#"{"reason":"refused","code":"token"}"#
        );
        assert_eq!(closed(Reason::User, None, None), r#"{"reason":"user"}"#);
        assert_eq!(
            closed(Reason::Error, Some("view"), Some("script:2: boom")),
            r#"{"reason":"error","code":"view","message":"script:2: boom"}"#
        );
        assert_eq!(
            closed(Reason::Program, None, None),
            r#"{"reason":"program"}"#
        );
    }

    #[test]
    fn a_welcome_names_the_folder_after_the_features() {
        let dir = StageDir::new("/home/u/.pencere/a1", "0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(
            welcome_offer(
                1,
                "t",
                &host(),
                &Features {
                    names: &["dismiss", "stage"],
                    stage: Some(&dir)
                },
                &[],
                &[],
                false
            ),
            r#"{"version":1,"token":"t","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss","stage"],"stage":{"dir":"/home/u/.pencere/a1","token":"0123456789abcdef0123456789abcdef"},"views":[]}"#
        );
    }

    #[test]
    fn a_folder_is_offered_only_as_an_absolute_path_with_a_token_of_32_hex_digits() {
        let token = "0123456789abcdef0123456789abcdef";
        assert!(StageDir::new("/a", token).is_some());
        assert!(StageDir::new("a/b", token).is_none());
        assert!(StageDir::new("", token).is_none());
        assert!(StageDir::new("/a\0b", token).is_none());
        assert!(StageDir::new(&format!("/{}", "a".repeat(4096)), token).is_none());
        assert!(StageDir::new("/a", "0123456789ABCDEF0123456789ABCDEF").is_none());
        assert!(StageDir::new("/a", "0123").is_none());
    }

    #[test]
    fn the_answers_to_a_stage_and_a_fetch_have_one_member_and_a_route_has_two() {
        assert_eq!(
            transfer_pipe("0123456789abcdef0123456789abcdef"),
            r#"{"pipe":"0123456789abcdef0123456789abcdef"}"#
        );
        assert_eq!(transfer_sent(11), r#"{"sent":11}"#);
        assert_eq!(transfer_saved(11), r#"{"saved":11}"#);
        assert_eq!(transfer_failed("busy", None), r#"{"code":"busy"}"#);
        assert_eq!(
            transfer_failed("route", Some(4)),
            r#"{"code":"route","at":4}"#
        );
    }

    #[test]
    fn a_host_name_with_a_line_feed_or_a_letter_from_elsewhere_stays_one_line_of_ascii() {
        let host = HostInfo {
            name: "line\nfeed \u{e9} \u{7f}".into(),
            version: "1".into(),
        };
        let text = welcome_unsupported(&host);
        assert!(json::is_frame_text(&text), "{text}");
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["host"]["name"], "line\nfeed \u{e9} \u{7f}");
    }

    #[test]
    fn a_hello_lists_kinds_with_their_versions_and_nothing_else_is_a_list_of_them() {
        let hello = |text: &str| json::from_object::<Hello>(text);
        let read = hello(
            r#"{"versions":[1],"views":[{"kind":"wasm","versions":["0.5","0.4"],"x":1},{"kind":"web","versions":[]}]}"#,
        )
        .unwrap();
        assert_eq!(read.versions, vec![1]);
        assert_eq!(
            read.views,
            vec![offer("wasm", &["0.5", "0.4"]), offer("web", &[])]
        );
        for bad in [
            r#"{"versions":[1]}"#,
            r#"{"views":[{"kind":"web","versions":["1.0"]}]}"#,
            r#"{"versions":[1],"views":["web"]}"#,
            r#"{"versions":[1],"views":{"kind":"web","versions":["1.0"]}}"#,
            r#"{"versions":[1],"views":[{"kind":"web"}]}"#,
            r#"{"versions":[1],"views":[{"versions":["1.0"]}]}"#,
            r#"{"versions":[1],"views":[{"kind":7,"versions":["1.0"]}]}"#,
            r#"{"versions":[1],"views":[{"kind":"web","versions":"1.0"}]}"#,
            r#"{"versions":[1],"views":[{"kind":"web","versions":[1.0]}]}"#,
            r#"{"versions":[1],"views":[["web",["1.0"]]]}"#,
        ] {
            assert_eq!(hello(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_token_is_read_before_the_rest_and_without_building_it() {
        assert_eq!(
            probe_token(r#"{"token":"abc","bundle":{"entry":"a"}}"#),
            TokenProbe::Token("abc".into())
        );
        assert_eq!(probe_token(r#"{"view":{"kind":"t"}}"#), TokenProbe::NoToken);
        assert_eq!(probe_token(r#"{"token":7}"#), TokenProbe::NoToken);
        assert_eq!(
            probe_token(r#"{"token":["a"],"x":{"y":[1,2,{}]}}"#),
            TokenProbe::NoToken
        );
        assert_eq!(probe_token(r#"{"token":null}"#), TokenProbe::NoToken);
        assert_eq!(probe_token("[\"abc\"]"), TokenProbe::NoToken);
        assert_eq!(probe_token("7"), TokenProbe::NoToken);
        assert_eq!(probe_token("{"), TokenProbe::NotJson);
        assert_eq!(probe_token(""), TokenProbe::NotJson);
        assert_eq!(probe_token(r#"{"token":"a"} x"#), TokenProbe::NotJson);
    }
}
