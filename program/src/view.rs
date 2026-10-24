//! What a program offers to show: the kinds of view, their versions and their files, and the
//! words and pictures the terminal puts around the view.

use std::fmt;
use std::sync::Arc;

use serde::Serialize;

use crate::bundle::{Bundle, BundleError};

/// What a view is made of, and so who draws it (specification §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// A web page in a web view of the terminal's, that reaches the program only through the six
    /// messages.
    Web,
    /// A WebAssembly component that the terminal runs in its own sandbox and draws itself.
    Wasm,
    /// A script in the Lua language that the terminal runs in the same sandbox and draws itself.
    Lua,
}

impl Kind {
    /// The name the wire gives the kind.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Wasm => "wasm",
            Self::Lua => "lua",
        }
    }

    /// The media type the entry of a bundle has when the view is of this kind (§6.3).
    #[must_use]
    pub fn entry_type(self) -> &'static str {
        match self {
            Self::Web => "text/html",
            Self::Wasm => pencere_wire::bundle::WASM_MEDIA_TYPE,
            Self::Lua => pencere_wire::bundle::LUA_MEDIA_TYPE,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The version of a `web` view's interface (the bridge of §9.4) that this library speaks.
pub const WEB_VERSION: &str = "1.0";

/// Whether `text` is a version of §5.5: `major.minor`, each without a leading zero.
pub(crate) fn is_version(text: &str) -> bool {
    let number = |part: &str| {
        (1..=9).contains(&part.len())
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    text.split_once('.')
        .is_some_and(|(major, minor)| number(major) && number(minor))
}

/// Makes the bundle of a version of a kind's interface.
pub(crate) type BundleOf = dyn Fn(&str) -> Result<Bundle, BundleError> + Send;

/// The files of a view: one bundle, or a function of the version that makes the bundle of that
/// version.
pub(crate) enum Source {
    Fixed(Arc<Bundle>),
    PerVersion(Box<BundleOf>),
}

/// A kind of view the program can show (specification §5.5).
pub struct View {
    pub(crate) kind: Kind,
    pub(crate) versions: Vec<String>,
    pub(crate) source: Source,
    pub(crate) access: Option<Access>,
}

impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("View")
            .field("kind", &self.kind)
            .field("versions", &self.versions)
            .field("access", &self.access)
            .finish_non_exhaustive()
    }
}

impl View {
    /// A view of `kind` written against the versions of its interface `versions`, in the order
    /// the program prefers them, with its files.
    #[must_use]
    pub fn new(kind: Kind, versions: &[&str], bundle: Bundle) -> Self {
        Self {
            kind,
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
            source: Source::Fixed(Arc::new(bundle)),
            access: None,
        }
    }

    /// A `web` view: a page, at the version of the bridge this library speaks.
    #[must_use]
    pub fn web(bundle: Bundle) -> Self {
        Self::new(Kind::Web, &[WEB_VERSION], bundle)
    }

    /// A view with a bundle for each version of its kind's interface: `files` is asked for the
    /// bundle of the version that the terminal and the program agree on.
    #[must_use]
    pub fn per_version(
        kind: Kind,
        versions: &[&str],
        files: impl Fn(&str) -> Result<Bundle, BundleError> + Send + 'static,
    ) -> Self {
        Self {
            kind,
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
            source: Source::PerVersion(Box::new(files)),
            access: None,
        }
    }

    /// What a `web` view asks to reach, which the person is asked about (§12.2). Not for a view
    /// the terminal draws itself.
    #[must_use]
    pub fn access(mut self, access: Access) -> Self {
        self.access = Some(access);
        self
    }
}

/// The browser capabilities a page may ask for (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// The camera.
    Camera,
    /// The microphone.
    Microphone,
    /// The location.
    Geolocation,
    /// Writing to the clipboard.
    ClipboardWrite,
}

/// What a page asks to reach (§6.3, §12.2). Origins are `scheme://host[:port]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Access {
    /// The origins the page may send network requests to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub connect: Vec<String>,
    /// The origins it may load scripts, styles, images, fonts and media from.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    /// The origins it may nest as frames.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<String>,
    /// The origins a `<base>` element may name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub base: Vec<String>,
    /// The browser capabilities it asks for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Permission>,
}

/// The background an icon was drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconTheme {
    /// For a light background.
    Light,
    /// For a dark background.
    Dark,
}

/// An icon for the program (§7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Icon {
    /// A `data:` URI with base64 data.
    pub src: String,
    /// Its media type, when the URI does not say.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// `WxH`, as HTML has them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<String>,
    /// The background it was drawn for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<IconTheme>,
}

impl Icon {
    /// An icon from a `data:` URI.
    #[must_use]
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            mime_type: None,
            sizes: Vec::new(),
            theme: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_major_dot_minor() {
        for good in ["1.0", "0.5", "10.20", "123456789.0"] {
            assert!(is_version(good), "{good}");
        }
        for bad in [
            "1",
            "1.",
            ".1",
            "01.0",
            "1.00",
            "1.0.0",
            "a.b",
            "",
            "1234567890.0",
            "-1.0",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
    }
}
