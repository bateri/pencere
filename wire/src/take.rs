//! Something the host hands a program that is running for it to take (specification §10.9): an
//! address, a file, a folder. The program says in its `open` what it takes ([`Kind`]); the host
//! then writes a `take` ([`Take`]) for each thing the person gives it.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::json;
use crate::link;

/// The most bytes the path of a file or a folder may have in a `take`: the length the host's
/// folder on the program's machine may have, and what a path of the systems the host runs on
/// comes to.
pub const MAX_PATH: usize = 4096;

/// A kind of thing a program takes: a member of `takes` of its `open`, and of `opens` of the
/// `app.json` of a program that is started with one (the same words, specification apps §4.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// An `http` or `https` address.
    Url,
    /// The path of a file.
    File,
    /// The path of a folder.
    Folder,
}

impl Kind {
    /// The name the wire gives it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::File => "file",
            Self::Folder => "folder",
        }
    }

    /// The kind of that name; `None` for a name this version does not know.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "url" => Some(Self::Url),
            "file" => Some(Self::File),
            "folder" => Some(Self::Folder),
            _ => None,
        }
    }

    /// The kinds a `takes` names, in the order of their first mention. A value that is not a list,
    /// a member that is not a name of a kind and a name written twice are left out: a program
    /// written for a later version can name more than this one knows.
    #[must_use]
    pub fn of_list(value: &Value) -> Vec<Self> {
        let mut kinds = Vec::new();
        if let Value::Array(items) = value {
            for item in items {
                if let Some(kind) = item.as_str().and_then(Self::from_name)
                    && !kinds.contains(&kind)
                {
                    kinds.push(kind);
                }
            }
        }
        kinds
    }
}

/// One thing a host hands the program (a `take`), checked: [`Take::parse`] and [`Take::payload`]
/// give back only what is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Take {
    /// An address the person opened: `http` or `https`, as a link of a view is (§9.7).
    Url(String),
    /// A file, by its absolute path, and the line (from 1) and the column (from 1, and only with a
    /// line) the person means.
    File {
        /// The path, as the program's machine writes it.
        path: String,
        /// The line to show.
        line: Option<u32>,
        /// The column to show.
        column: Option<u32>,
    },
    /// A folder, by its absolute path.
    Folder(String),
}

/// The `take` as the wire writes it, in the order of Appendix A.
#[derive(Serialize)]
struct Written<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    folder: Option<&'a str>,
}

/// A path as a `take` carries it: something, not too long, and no control character in it.
fn path_ok(path: &str) -> bool {
    !path.is_empty() && path.len() <= MAX_PATH && !path.chars().any(char::is_control)
}

/// A position as a `take` carries it: a whole number from 1.
fn position(value: &Value) -> Option<u32> {
    let number = value.as_u64()?;
    u32::try_from(number).ok().filter(|n| *n >= 1)
}

impl Take {
    /// The kind of thing it is.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Url(_) => Kind::Url,
            Self::File { .. } => Kind::File,
            Self::Folder(_) => Kind::Folder,
        }
    }

    /// Whether it is something the wire carries: an address a view may ask to open
    /// ([`link::is_openable`]), a path that is not empty, within [`MAX_PATH`] and without a
    /// control character, and a column only where there is a line.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Url(url) => link::is_openable(url),
            Self::File { path, line, column } => {
                path_ok(path)
                    && line.is_none_or(|n| n >= 1)
                    && column.is_none_or(|n| n >= 1)
                    && (column.is_none() || line.is_some())
            }
            Self::Folder(path) => path_ok(path),
        }
    }

    /// The payload of the `take` that carries it (§10.9), as the wire writes it; `None` for what
    /// [`Take::is_valid`] refuses.
    #[must_use]
    pub fn payload(&self) -> Option<String> {
        if !self.is_valid() {
            return None;
        }
        let written = match self {
            Self::Url(url) => Written {
                url: Some(url),
                file: None,
                line: None,
                column: None,
                folder: None,
            },
            Self::File { path, line, column } => Written {
                url: None,
                file: Some(path),
                line: *line,
                column: *column,
                folder: None,
            },
            Self::Folder(path) => Written {
                url: None,
                file: None,
                line: None,
                column: None,
                folder: Some(path),
            },
        };
        json::to_wire(&written).ok()
    }

    /// What a `take` carries, read from its payload: exactly one of `url`, `file` and `folder`, a
    /// string that [`Take::is_valid`] takes; `line` and `column` only with `file`. A member of
    /// another name is ignored; anything else that is not as the specification says makes it
    /// `None`, and the program ignores the message.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        if json::too_deep(payload) || !json::is_object(payload) {
            return None;
        }
        let Value::Object(members) = serde_json::from_str::<Value>(payload).ok()? else {
            return None;
        };
        // `None` is a member that is there and is not what it should be; `Some(None)` is one that
        // is not there.
        let string = |name: &str| match members.get(name) {
            None => Some(None),
            Some(Value::String(text)) => Some(Some(text.clone())),
            Some(_) => None,
        };
        let number = |name: &str| match members.get(name) {
            None => Some(None),
            Some(value) => position(value).map(Some),
        };
        let (url, file, folder) = (string("url")?, string("file")?, string("folder")?);
        let (line, column) = (number("line")?, number("column")?);
        let take = match (url, file, folder) {
            (Some(url), None, None) if line.is_none() && column.is_none() => Self::Url(url),
            (None, Some(path), None) => Self::File { path, line, column },
            (None, None, Some(path)) if line.is_none() && column.is_none() => Self::Folder(path),
            _ => return None,
        };
        take.is_valid().then_some(take)
    }

    /// The value a program pushes to the page that listens for the `take` (§10.9): the object of
    /// the payload.
    #[must_use]
    pub fn value(&self) -> Value {
        let mut members = Map::new();
        match self {
            Self::Url(url) => {
                members.insert("url".into(), Value::from(url.as_str()));
            }
            Self::File { path, line, column } => {
                members.insert("file".into(), Value::from(path.as_str()));
                if let Some(line) = line {
                    members.insert("line".into(), Value::from(*line));
                }
                if let Some(column) = column {
                    members.insert("column".into(), Value::from(*column));
                }
            }
            Self::Folder(path) => {
                members.insert("folder".into(), Value::from(path.as_str()));
            }
        }
        Value::Object(members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, line: Option<u32>, column: Option<u32>) -> Take {
        Take::File {
            path: path.into(),
            line,
            column,
        }
    }

    #[test]
    fn a_take_is_written_as_the_fixtures_write_it() {
        assert_eq!(
            Take::Url("http://localhost:3000/".into())
                .payload()
                .as_deref(),
            Some(r#"{"url":"http://localhost:3000/"}"#)
        );
        assert_eq!(
            file("/home/alice/notes.txt", Some(12), Some(3))
                .payload()
                .as_deref(),
            Some(r#"{"file":"/home/alice/notes.txt","line":12,"column":3}"#)
        );
        assert_eq!(
            file("/home/alice/caf\u{e9}.txt", None, None)
                .payload()
                .as_deref(),
            Some(r#"{"file":"/home/alice/caf\u00e9.txt"}"#)
        );
        assert_eq!(
            Take::Folder("/home/alice/src".into()).payload().as_deref(),
            Some(r#"{"folder":"/home/alice/src"}"#)
        );
    }

    #[test]
    fn what_is_written_is_read_back() {
        for take in [
            Take::Url("https://example.com/a?b=c#d".into()),
            file("/a", None, None),
            file("/a b/c\u{1f600}", Some(1), None),
            file("/a", Some(40), Some(7)),
            Take::Folder("/srv".into()),
        ] {
            let payload = take.payload().expect("valid");
            assert_eq!(Take::parse(&payload), Some(take.clone()), "{payload}");
            assert_eq!(
                take.value()["file"].as_str().is_some(),
                take.kind() == Kind::File
            );
        }
    }

    #[test]
    fn what_is_not_one_thing_is_no_take() {
        for payload in [
            "{}",
            "[]",
            "null",
            r#"{"url":"https://a.example","file":"/a"}"#,
            r#"{"url":7}"#,
            r#"{"url":"ftp://a.example"}"#,
            r#"{"url":"https:/a.example"}"#,
            r#"{"url":"https://a.example/\u00e9"}"#,
            r#"{"url":"https://a.example","line":1}"#,
            r#"{"folder":"/a","line":1}"#,
            r#"{"file":"/a","column":3}"#,
            r#"{"file":"/a","line":0}"#,
            r#"{"file":"/a","line":"3"}"#,
            r#"{"file":"/a","line":1.5}"#,
            r#"{"file":"/a","line":4294967296}"#,
            r#"{"file":""}"#,
            r#"{"file":"/a\nb"}"#,
            r#"{"folder":"/a\u007f"}"#,
        ] {
            assert_eq!(Take::parse(payload), None, "{payload}");
        }
        let long = format!(r#"{{"file":"/{}"}}"#, "a".repeat(MAX_PATH));
        assert_eq!(Take::parse(&long), None);
        assert!(Take::Url("http://a.example/ b".into()).payload().is_none());
        assert!(file("/a", None, Some(2)).payload().is_none());
        assert!(file("/a", Some(0), None).payload().is_none());
    }

    #[test]
    fn the_scheme_of_an_address_is_read_in_either_letter_case_and_the_longest_fits() {
        for url in ["HTTPS://EXAMPLE.COM", "hTtP://example.com"] {
            let payload = format!(r#"{{"url":"{url}"}}"#);
            assert_eq!(Take::parse(&payload), Some(Take::Url(url.into())), "{url}");
        }
        let longest = format!("http://{}", "a".repeat(link::MAX_LINK - 7));
        assert!(Take::Url(longest.clone()).is_valid());
        assert!(!Take::Url(format!("{longest}a")).is_valid());
    }

    #[test]
    fn a_member_of_another_name_is_ignored() {
        assert_eq!(
            Take::parse(r#"{"url":"https://a.example","later":[1,2]}"#),
            Some(Take::Url("https://a.example".into()))
        );
    }

    #[test]
    fn the_kinds_a_program_names_are_the_ones_this_version_knows_once() {
        let kinds = |text: &str| Kind::of_list(&serde_json::from_str(text).unwrap());
        assert_eq!(
            kinds(r#"["file","later","url","file",3]"#),
            [Kind::File, Kind::Url]
        );
        assert_eq!(kinds(r#""url""#), []);
        assert_eq!(kinds("{}"), []);
        assert_eq!(kinds("[]"), []);
    }
}
