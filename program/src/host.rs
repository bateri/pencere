//! The terminal a program runs in, as its `welcome` names it (specification §5.2): a name and a
//! version, for words and for work that only that terminal does, never for what the terminal can
//! show (that is `features`, and the kit's `pencere.has`).
//!
//! The reading is the program's own and lenient: a `host` that is not an object with a string
//! `name` and a string `version` is no host, and the rest of the message stands.

use serde_json::Value;

/// The terminal that answered the program: the `host` of its last `welcome`.
///
/// `name` is the stable, lower-case name of the terminal's product, the same on every channel and
/// in every build of it; compare it with the names you know by equality. `version` is the
/// terminal's own text with no form and no order promised: compare it only for equality. A name
/// you do not know is a terminal like any other: restrict nothing because of it (specification
/// §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// The product's name, in lower case: `"bateri"`.
    pub name: String,
    /// The terminal's own version text.
    pub version: String,
}

/// The `host` of a `welcome`; `None` when it is not there or not of the shape.
pub(crate) fn read(value: Option<&Value>) -> Option<Host> {
    let host = value?.as_object()?;
    Some(Host {
        name: host.get("name")?.as_str()?.to_owned(),
        version: host.get("version")?.as_str()?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_json(text: &str) -> Option<Host> {
        let value: Value = serde_json::from_str(text).expect("json");
        read(Some(&value))
    }

    #[test]
    fn a_host_is_a_name_and_a_version_and_nothing_else_is_one() {
        assert_eq!(
            read_json(r#"{"name":"bateri","version":"0.9.1"}"#),
            Some(Host {
                name: "bateri".to_owned(),
                version: "0.9.1".to_owned(),
            })
        );
        // A member it does not know is skipped.
        assert!(read_json(r#"{"name":"bateri","version":"1","build":7}"#).is_some());
        for bad in [
            r#"{"name":"bateri"}"#,
            r#"{"version":"1"}"#,
            r#"{"name":1,"version":"1"}"#,
            r#"{"name":"bateri","version":null}"#,
            r#""bateri""#,
            "[]",
            "null",
        ] {
            assert_eq!(read_json(bad), None, "{bad}");
        }
        assert_eq!(read(None), None);
    }
}
