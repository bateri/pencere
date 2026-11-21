//! A browser of the host's on the page: what the page says of one ([`BrowserOptions`]), what it asks
//! of it ([`BrowserAsk`]) and what the host tells it ([`BrowserState`]).
//!
//! The engine is the host's and so is the web page it shows: the page gets nothing of what is
//! loaded (no document, no console, no cookie), only where the browser is and what it is doing.

use crate::Shortcut;

/// The most characters of a profile's name.
pub const PROFILE_CHARS: usize = 32;

/// The most keys a browser may keep for the page ([`BrowserOptions::keys`]).
pub const MOST_KEYS: usize = 32;

/// The most bytes of an address a page asks a browser to go to.
pub const ADDRESS_BYTES: usize = 8192;

/// The most bytes of what a page asks a browser to find ([`BrowserAsk::Find`]): far more than a
/// person types into a search, and a bound on what a page hands the host's engine.
pub const PATTERN_BYTES: usize = 1024;

/// The least a page may zoom a browser to ([`BrowserAsk::Zoom`]): a quarter of the size.
pub const ZOOM_LEAST: f32 = 0.25;

/// The most a page may zoom a browser to ([`BrowserAsk::Zoom`]): five times the size.
pub const ZOOM_MOST: f32 = 5.0;

/// `zoom` held to what a browser may be zoomed to: from [`ZOOM_LEAST`] to [`ZOOM_MOST`], a number
/// that is not one taken for no zoom (1).
#[must_use]
pub fn zoom_allowed(zoom: f32) -> f32 {
    if zoom.is_nan() {
        1.0
    } else {
        zoom.clamp(ZOOM_LEAST, ZOOM_MOST)
    }
}

/// Whether `name` is a profile name: 1 to [`PROFILE_CHARS`] of `a` to `z`, `0` to `9` and `-`. The
/// host keeps a folder by it, so it is a word and never a path.
#[must_use]
pub fn profile_allowed(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= PROFILE_CHARS
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// What a browser does with a link that opens a new window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserWindows {
    /// It opens in the same browser.
    #[default]
    Here,
    /// The host opens none: the page is told ([`crate::Event::BrowserWindow`]) and decides.
    Page,
}

/// What a page says of a browser when it names its room ([`crate::Element::browser`]). Where it
/// goes first is not among them: that is an ask ([`BrowserAsk::Go`]), as every move of it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserOptions {
    /// The name of its profile: what the browser keeps of the sites it has been to (cookies and
    /// storage) is kept apart under this name and the program's. None: a profile that is gone when
    /// the browser is. It is read when the room is first made; another name under the same id is
    /// another browser ([`profile_allowed`]).
    pub profile: Option<String>,
    pub windows: BrowserWindows,
    /// The keys that go to the page, as [`crate::Event::Input`] keys, while the browser has the
    /// keyboard, and the browser never sees them. Every other key goes to the browser first, and to
    /// the page as one of these only where the browser, and the web page in it, has no use for it.
    pub keys: Vec<Shortcut>,
}

/// What a page asks of a browser, in the reply to what it was told.
#[derive(Clone, Debug, PartialEq)]
pub enum BrowserAsk {
    /// Go to an address ([`go_refusal`] says which a browser goes to).
    Go(String),
    /// Back to the page before, if there is one.
    Back,
    /// On to the page after, if there is one.
    Forward,
    /// Load the page again: the way out of a browser that crashed. Without `cache` the browser
    /// loads it again from where it is, and not from what it keeps of it.
    Reload { cache: bool },
    /// Stop loading.
    Stop,
    /// Look for `pattern` (plain text, not a regular expression) in the page the browser shows, mark
    /// what it finds and go to the first match at or after where the page is scrolled to; `case`
    /// matches the case written, else either. The browser tells what it finds in
    /// [`BrowserState::found`], until [`BrowserAsk::StopFinding`]. An empty `pattern` is a
    /// `StopFinding`, and one that is not a search's ([`find_refusal`]) is refused.
    Find { pattern: String, case: bool },
    /// Go to the next match of the search, and after the last to the first.
    FindNext,
    /// Go to the match before, and before the first to the last.
    FindPrevious,
    /// End the search: nothing is marked, and [`BrowserState::found`] is none.
    StopFinding,
    /// Show the page at this much of its size ([`zoom_allowed`] holds it from [`ZOOM_LEAST`] to
    /// [`ZOOM_MOST`]); the browser keeps it for the pages it goes to, and tells it in
    /// [`BrowserState::zoom`].
    Zoom(f32),
    /// Open the host's tools for looking into the page.
    Devtools,
    /// An ask this kit does not know (a command a later kit has, or none the page's table is):
    /// never given to a host, the page is told it was refused ([`BrowserRefusal::Ask`]).
    Unknown,
}

/// Why a browser was not asked to go where the page said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserRefusal {
    /// The address is of a kind a browser does not go to ([`go_refusal`]).
    Scheme,
    /// The address is not one a browser is given: too long, or with a space or a control character
    /// or a character that is not ASCII in it.
    Address,
    /// The ask is not one the kit this page runs on knows: a later kit's, or none at all. A page
    /// that asks it learns, and does not fail.
    Ask,
    /// What the page asked to find is not a search's ([`find_refusal`]): too long, or with a control
    /// character (a line break, a tab) in it.
    Pattern,
}

impl BrowserRefusal {
    /// The word the page is told it by.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Scheme => "scheme",
            Self::Address => "address",
            Self::Ask => "ask",
            Self::Pattern => "pattern",
        }
    }
}

/// Whether a browser may be asked to go to `url`, and if not, why: `http`, `https`, `about:blank`
/// and `data:` addresses are gone to, and no other (`file:`, `chrome:`, `devtools:`, `view-source:`,
/// `javascript:` and the rest are the host's own, or the person's machine's). An address is at most
/// [`ADDRESS_BYTES`] of printable ASCII without the space.
#[must_use]
pub fn go_refusal(url: &str) -> Option<BrowserRefusal> {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let scheme = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:")
        || url.eq_ignore_ascii_case("about:blank");
    if !scheme {
        return Some(BrowserRefusal::Scheme);
    }
    if url.len() > ADDRESS_BYTES || !url.bytes().all(|b| (0x21..0x7f).contains(&b)) {
        return Some(BrowserRefusal::Address);
    }
    None
}

/// Whether a browser may be asked to find `pattern`, and if not, why: at most [`PATTERN_BYTES`], with
/// no control character in it, as what a person types into a search is. A page that hands on what a
/// field held, pasted lines and all, is told and does not fail.
#[must_use]
pub fn find_refusal(pattern: &str) -> Option<BrowserRefusal> {
    (pattern.len() > PATTERN_BYTES || pattern.chars().any(char::is_control))
        .then_some(BrowserRefusal::Pattern)
}

/// A load that failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserError {
    /// The address that did not load.
    pub url: String,
    /// The number the host's engine gives to what went wrong: for telling failures apart, nothing
    /// else is promised of it.
    pub code: i32,
    /// What went wrong, in a sentence a person can read.
    pub message: String,
}

/// What a search of a browser finds ([`BrowserAsk::Find`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowserFound {
    /// How many matches there are in the page.
    pub count: u32,
    /// The match the browser is at, counted from 1; none where there are none.
    pub current: Option<u32>,
}

/// What the host tells the page of a browser: all of it, each time any of it changes.
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserState {
    /// The address the browser is at (empty before the first load).
    pub url: String,
    /// The title of the page it shows (empty where it has none).
    pub title: String,
    /// A page is loading.
    pub loading: bool,
    /// There is a page to go back to.
    pub back: bool,
    /// There is a page to go on to.
    pub forward: bool,
    /// What the browser is, as a whole: one of [`BrowserState::READY`], [`BrowserState::STARTING`],
    /// [`BrowserState::CRASHED`], [`BrowserState::NO_ROUTE`] and [`BrowserState::UNSUPPORTED`], or a
    /// word a later host says: a set that is open, in which a page takes a word it does not know
    /// for a browser that is not ready.
    pub status: String,
    /// A sentence for the person, in every status but `ready`.
    pub message: Option<String>,
    /// The last load failed, until the next one begins.
    pub error: Option<BrowserError>,
    /// What the search finds, while there is one ([`BrowserAsk::Find`]), else none.
    pub found: Option<BrowserFound>,
    /// How large the page is shown, against its size (1 is its size; [`BrowserAsk::Zoom`]).
    pub zoom: f32,
}

impl BrowserState {
    /// The browser shows pages.
    pub const READY: &'static str = "ready";
    /// The host is getting what it shows pages with ready; it takes a while.
    pub const STARTING: &'static str = "starting";
    /// What the browser shows pages with ended; a reload makes it again.
    pub const CRASHED: &'static str = "crashed";
    /// The host cannot reach the machine the program runs on from this view, or may not.
    pub const NO_ROUTE: &'static str = "no_route";
    /// The host shows no browsers.
    pub const UNSUPPORTED: &'static str = "unsupported";

    /// A state that says only what the browser is.
    #[must_use]
    pub fn with_status(status: &str, message: Option<&str>) -> Self {
        Self {
            status: status.to_owned(),
            message: message.map(str::to_owned),
            ..Self::default()
        }
    }
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            url: String::new(),
            title: String::new(),
            loading: false,
            back: false,
            forward: false,
            status: Self::READY.to_owned(),
            message: None,
            error: None,
            found: None,
            zoom: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_is_held_from_a_quarter_to_five_times_and_a_number_that_is_not_one_is_no_zoom() {
        assert!((zoom_allowed(1.5) - 1.5).abs() < f32::EPSILON);
        assert!((zoom_allowed(0.0) - ZOOM_LEAST).abs() < f32::EPSILON);
        assert!((zoom_allowed(-3.0) - ZOOM_LEAST).abs() < f32::EPSILON);
        assert!((zoom_allowed(99.0) - ZOOM_MOST).abs() < f32::EPSILON);
        assert!((zoom_allowed(f32::INFINITY) - ZOOM_MOST).abs() < f32::EPSILON);
        assert!((zoom_allowed(f32::NAN) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_profile_name_is_a_word_a_folder_can_be_named() {
        assert!(profile_allowed("work"));
        assert!(profile_allowed("a-b-1"));
        assert!(profile_allowed(&"x".repeat(PROFILE_CHARS)));
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a b",
            "A",
            "a_b",
            "a.b",
            "é",
            &"x".repeat(PROFILE_CHARS + 1),
        ] {
            assert!(!profile_allowed(name), "{name:?}");
        }
    }

    #[test]
    fn a_browser_goes_to_http_https_about_blank_and_data_and_to_no_other_kind_of_address() {
        for url in [
            "http://localhost:3000",
            "https://example.com/a?b=c#d",
            "HTTPS://EXAMPLE.COM",
            "about:blank",
            "About:Blank",
            "data:text/html,<p>hi</p>",
        ] {
            assert_eq!(go_refusal(url), None, "{url}");
        }
        for url in [
            "",
            "localhost:3000",
            "file:///etc/passwd",
            "chrome://settings",
            "devtools://devtools/bundled/inspector.html",
            "view-source:https://example.com",
            "javascript:alert(1)",
            "about:srcdoc",
            "about:blank#x",
            "ftp://example.com/",
            "//example.com",
            " https://example.com",
        ] {
            assert_eq!(go_refusal(url), Some(BrowserRefusal::Scheme), "{url:?}");
        }
    }

    #[test]
    fn an_address_of_a_kind_a_browser_goes_to_is_still_refused_where_it_is_not_one_it_is_given() {
        for url in [
            "https://exa mple.com",
            "https://example.com/\n",
            "https://example.com/ü",
            "http://example.com/\u{7f}",
        ] {
            assert_eq!(go_refusal(url), Some(BrowserRefusal::Address), "{url:?}");
        }
        let longest = format!("https://{}", "a".repeat(ADDRESS_BYTES - "https://".len()));
        assert_eq!(go_refusal(&longest), None);
        assert_eq!(
            go_refusal(&format!("{longest}a")),
            Some(BrowserRefusal::Address)
        );
    }

    #[test]
    fn a_pattern_is_what_a_search_holds_and_no_more() {
        let longest = "é".repeat(PATTERN_BYTES / 2);
        for pattern in ["", "café au lait", longest.as_str()] {
            assert_eq!(find_refusal(pattern), None, "{pattern:?}");
        }
        for pattern in [
            "two\nlines",
            "a\tb",
            "\u{7f}",
            "\u{85}",
            &format!("{longest}x"),
        ] {
            assert_eq!(
                find_refusal(pattern),
                Some(BrowserRefusal::Pattern),
                "{pattern:?}"
            );
        }
        assert_eq!(BrowserRefusal::Pattern.word(), "pattern");
    }
}
