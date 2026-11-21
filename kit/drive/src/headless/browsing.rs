//! The headless host's browsers: what it shows in place of an engine, so that a page that names one
//! can be run, told what the browser is and does, and looked at in a test.
//!
//! The host stands for one that shows browsers (a host that shows none is asked for:
//! [`Opening::browsers`]). It draws no pages: the room of a browser is where its engine would draw
//! them, and a picture of the page has a box there. What a page asks of a browser is kept for the
//! test to read ([`Headless::browser_asks`]), and what the browser would tell the page is told it by
//! the test ([`Headless::browser_state`]).

use pencere_view::{BrowserAsk, BrowserState, Id, Key, Mods, Shortcut};

use super::{Error, Headless, Opening, Peer};
use crate::Browser;

impl<P: Peer> Opening<P> {
    /// Whether the host shows browsers (the rooms of [`pencere_view::Element::browser`]): it does
    /// unless said. A host that shows none draws a placeholder in the room of each and tells the
    /// page, once for each, that it is unsupported ([`crate::Page::set_browsers`]).
    pub fn browsers(mut self, shown: bool) -> Self {
        self.browsers = shown;
        self
    }
}

impl<P: Peer> Headless<P> {
    /// The browsers on the page, as of its last frame.
    pub fn browsers(&self) -> &[Browser] {
        self.page.browsers()
    }

    /// Every ask the page made of its browsers that the host was given, oldest first: an address a
    /// browser does not go to is not among them (the page was told it was refused).
    pub fn browser_asks(&self) -> &[(Id, BrowserAsk)] {
        &self.seen.browser_asks
    }

    /// The keys the browser that had the keyboard took for itself, oldest first: those that are not
    /// the page's ([`pencere_view::BrowserOptions::keys`]).
    pub fn browser_keys(&self) -> &[(Id, Key, Mods)] {
        &self.seen.browser_keys
    }

    /// The browser named `name` is, or does, what `state` says, as its engine would tell the page.
    ///
    /// # Errors
    ///
    /// No browser of this name is on the page ([`Error::NoSuch`]), or as [`Headless::settle`].
    pub fn browser_state(&mut self, name: &str, state: BrowserState) -> Result<(), Error> {
        let id = self.browser_named(name)?;
        self.page.browser(id, state);
        self.settle()
    }

    /// A link of the browser named `name` opens a new window at `url`, and the page said it is the
    /// page that opens it.
    ///
    /// # Errors
    ///
    /// As [`Headless::browser_state`].
    pub fn browser_window(&mut self, name: &str, url: &str) -> Result<(), Error> {
        let id = self.browser_named(name)?;
        self.page.browser_window(id, url);
        self.settle()
    }

    fn browser_named(&self, name: &str) -> Result<Id, Error> {
        let id = Id::named(name);
        if self.page.browsers().iter().any(|browser| browser.id == id) {
            Ok(id)
        } else {
            Err(Error::NoSuch(name.to_owned()))
        }
    }

    /// A key typed while the browser `browser` has the keyboard: the page's where the page keeps it,
    /// else the browser's.
    pub(super) fn browser_key(&mut self, browser: Id, key: Key, mods: Mods) {
        let kept = self
            .page
            .browsers()
            .iter()
            .find(|held| held.id == browser)
            .is_some_and(|held| {
                held.options
                    .keys
                    .iter()
                    .any(|kept| *kept == Shortcut::new(key, mods))
            });
        if kept {
            self.page.input(pencere_view::Input::Key { key, mods });
        } else {
            self.seen.browser_keys.push((browser, key, mods));
        }
    }
}
