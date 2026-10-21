//! The payloads of the messages (specification Appendix A), as the host reads
//! and writes them.
//!
//! Members are written in the order the specification gives, the one place it
//! differs from the fixtures being `welcome`: with `supported` the fixtures (and
//! §5.2) write `supported` before `host`, so the two shapes of the answer are two
//! types here.

use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::json;

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

/// A feature a host may list in its `welcome`.
pub const FEATURE_BUNDLE: &str = "bundle";

/// The program's `hello`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hello {
    /// The versions the program speaks.
    pub versions: Vec<u32>,
}

#[derive(Serialize)]
struct Offer<'a> {
    version: u32,
    token: &'a str,
    host: &'a HostInfo,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    features: &'a [&'static str],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    restart: bool,
}

#[derive(Serialize)]
struct Unsupported<'a> {
    supported: &'a [u32],
    host: &'a HostInfo,
}

/// The host's `welcome` carrying a version and a token.
pub fn welcome_offer(
    version: u32,
    token: &str,
    host: &HostInfo,
    features: &[&'static str],
    restart: bool,
) -> String {
    wire(&Offer {
        version,
        token,
        host,
        features,
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
    #[derive(Serialize)]
    struct Need<'a> {
        missing: &'a [String],
    }
    wire(&Need { missing })
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
}

/// The host's `closed`.
pub fn closed(reason: Reason, code: Option<&str>) -> String {
    #[derive(Serialize)]
    struct Closed<'a> {
        reason: Reason,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<&'a str>,
    }
    wire(&Closed { reason, code })
}

fn wire<T: Serialize>(value: &T) -> String {
    // The types above hold strings and numbers only, which cannot fail to
    // serialise; an empty payload is not a message anyone could mistake for one.
    json::to_wire(value).unwrap_or_default()
}

/// The program's `bye`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bye {
    /// `exit`, `signal` or `error`, when it says.
    pub reason: Option<String>,
}

// ---- open ------------------------------------------------------------------

/// What reading the payload of an `open` for its token found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TokenProbe {
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
pub(crate) fn probe_token(payload: &str) -> TokenProbe {
    match serde_json::from_str::<TokenOnly>(payload) {
        Ok(TokenOnly(Some(token))) => TokenProbe::Token(token),
        Ok(TokenOnly(None)) => TokenProbe::NoToken,
        Err(_) if serde_json::from_str::<IgnoredAny>(payload).is_ok() => TokenProbe::NoToken,
        Err(_) => TokenProbe::NotJson,
    }
}

/// The rest of an `open`, read once its token has been accepted.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenBody {
    pub tool: ToolCall,
    #[serde(default)]
    pub bundle: Option<RawBundle>,
}

/// The tool an `open` asks to call.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ToolCall {
    /// The tool's name.
    pub name: String,
    /// The arguments to call it with.
    #[serde(default)]
    pub arguments: Option<serde_json::Map<String, Value>>,
}

/// A bundle manifest as it arrives, before it is checked.
#[derive(Debug, Deserialize)]
pub(crate) struct RawBundle {
    pub entry: String,
    pub files: Vec<RawFile>,
    #[serde(default)]
    pub meta: Option<Value>,
}

/// One file of a manifest as it arrives.
#[derive(Debug, Deserialize)]
pub(crate) struct RawFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub media_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> HostInfo {
        HostInfo {
            name: "pencere-test".into(),
            version: "0.0.0".into(),
        }
    }

    #[test]
    fn the_answers_are_written_as_the_fixtures_are() {
        assert_eq!(
            welcome_offer(1, "q7Vh2sB0xJm9kLw4TzN1cA", &host(), &["bundle"], false),
            r#"{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle"]}"#
        );
        assert_eq!(
            welcome_offer(1, "Zk3mP8rD5uYb1Xe6HqS0vg", &host(), &["bundle"], true),
            r#"{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle"],"restart":true}"#
        );
        assert_eq!(
            welcome_offer(1, "t", &host(), &[], false),
            r#"{"version":1,"token":"t","host":{"name":"pencere-test","version":"0.0.0"}}"#
        );
        assert_eq!(
            welcome_unsupported(&host()),
            r#"{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}"#
        );
        assert_eq!(need(&[]), r#"{"missing":[]}"#);
        assert_eq!(
            closed(Reason::Refused, Some("token")),
            r#"{"reason":"refused","code":"token"}"#
        );
        assert_eq!(closed(Reason::User, None), r#"{"reason":"user"}"#);
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
    fn the_token_is_read_before_the_rest_and_without_building_it() {
        assert_eq!(
            probe_token(r#"{"token":"abc","tool":{"name":"t"}}"#),
            TokenProbe::Token("abc".into())
        );
        assert_eq!(probe_token(r#"{"tool":{"name":"t"}}"#), TokenProbe::NoToken);
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
