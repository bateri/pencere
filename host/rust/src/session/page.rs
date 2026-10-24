//! The bridge of a page (specification §9.4 to §9.7): what the host does with the messages a web
//! view sends it, and the `context` it tells the page.
//!
//! A page and its program talk in the six messages of §9.2, which the host carries and does not
//! read. It reads that a message is one complete JSON object, in both directions, and the name
//! of its member, to tell its own messages — `context`, `close` and `link` — from the six: in
//! the page's direction to handle them, in the program's to drop a message that is named like
//! one of them (§9.3). It never builds a tree of a message, whatever the message holds.

use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{Action, Cause, MAX_OPEN_LINKS, MAX_VIEW_MESSAGE, Phase, Session, ViewContext};
use crate::bundle::Store;
use crate::json::Object;
use crate::sandbox::{self, Grant};
use crate::token::Entropy;

/// The names of the host's own messages (specification §9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Own {
    Context,
    Close,
    Link,
}

/// The members of an object, as far as the host looks at them: the first name, and whether
/// there is a name besides it. The values are skipped without being built.
struct Members {
    first: Option<String>,
    several: bool,
}

impl<'de> Deserialize<'de> for Members {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Members;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
                let mut members = Members {
                    first: None,
                    several: false,
                };
                while let Some(name) = map.next_key::<String>()? {
                    map.next_value::<IgnoredAny>()?;
                    match &members.first {
                        None => members.first = Some(name),
                        // A name twice is one member to a page, which reads the last of them:
                        // it is one name, not two.
                        Some(first) if *first != name => members.several = true,
                        Some(_) => {}
                    }
                }
                Ok(members)
            }
        }
        deserializer.deserialize_map(V)
    }
}

impl Members {
    /// The host's own message these members are, if they are exactly one member of that name.
    fn own(&self) -> Option<Own> {
        if self.several {
            return None;
        }
        match self.first.as_deref()? {
            "context" => Some(Own::Context),
            "close" => Some(Own::Close),
            "link" => Some(Own::Link),
            _ => None,
        }
    }
}

/// What a message is, to the host: `None` when it is not an object the host can read.
///
/// A text that is an object to a web view but that this reader cannot read (nested deeper than
/// the reader goes, say) is `None` too: a message the host cannot tell the name of is not
/// passed on in the direction where passing it would let a program speak as the host.
fn read_members(text: &str) -> Option<Members> {
    serde_json::from_str(text).ok()
}

/// Whether a message of the program may go to a page: it is an object that is not named like
/// one of the host's own messages (specification §9.4).
pub(super) fn to_page(text: &str) -> bool {
    read_members(text).is_some_and(|members| members.own().is_none())
}

/// Whether a message is a JSON object the host can read, whatever its members are: what a view
/// the terminal draws itself may say in either direction. A text that only begins with `{` is
/// not one.
pub(super) fn is_object(text: &str) -> bool {
    read_members(text).is_some()
}

/// The first `context` message, or a later one, as JSON text: `{"context":{…}}` in the order of
/// specification §9.5. What the person allowed is `grant`; the member is left out when nothing
/// was.
pub(super) fn context_text(context: &ViewContext, grant: &Grant) -> String {
    #[derive(Serialize)]
    struct Colors {
        background: String,
        foreground: String,
        dim: String,
        accent: String,
        border: String,
        success: String,
        error: String,
        warning: String,
        info: String,
    }
    #[derive(Serialize)]
    struct Area {
        width: u32,
        height: u32,
    }
    #[derive(Serialize)]
    struct Context<'a> {
        mode: &'static str,
        colors: Colors,
        #[serde(skip_serializing_if = "Option::is_none")]
        font: Option<&'a str>,
        size: Area,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
        #[serde(rename = "timeZone", skip_serializing_if = "Option::is_none")]
        time_zone: Option<&'a str>,
        #[serde(rename = "reduceMotion")]
        reduce_motion: bool,
        #[serde(skip_serializing_if = "Grant::is_empty")]
        access: &'a Grant,
    }
    #[derive(Serialize)]
    struct Message<'a> {
        context: Context<'a>,
    }
    let theme = &context.theme;
    let roles = theme.roles();
    let font = theme.font_mono();
    let message = Message {
        context: Context {
            mode: theme.mode(),
            colors: Colors {
                background: roles.background.css(),
                foreground: roles.foreground.css(),
                dim: roles.dim.css(),
                accent: roles.accent.css(),
                border: roles.border.css(),
                success: roles.success.css(),
                error: roles.error.css(),
                warning: roles.warning.css(),
                info: roles.info.css(),
            },
            font: (!font.is_empty()).then_some(font),
            size: Area {
                width: context.size.width,
                height: context.size.height,
            },
            locale: context.locale.as_deref(),
            time_zone: context.time_zone.as_deref(),
            reduce_motion: context.reduce_motion,
            access: grant,
        },
    };
    // The types above hold strings, numbers and booleans only, which cannot fail to serialise.
    crate::json::to_wire(&message).unwrap_or_default()
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// A message from a page, as JSON text (specification §9.4): what its web view's message
    /// channel delivered.
    ///
    /// An object with exactly one member, named `context`, `close` or `link`, is the host's own
    /// message and is handled here: `context` is answered with the current one
    /// ([`Action::ToView`]), `close` ends the view (the program is told `closed` with reason
    /// `view`) and `link` asks the person ([`Action::OpenLink`]). Any other object is a message of
    /// the six, passed to the program as a `msg`, unread; the whitespace between its tokens is
    /// left out, and the order the page sent its messages in is the order they are written in,
    /// a `close` among them. What is not an object, and anything that arrives while no page is
    /// open, is dropped and counted as ignored. A message over [`MAX_VIEW_MESSAGE`] bytes, as the
    /// page sent it, ends the view as a failure (specification §9.3), since no one could answer
    /// it. A view the terminal draws itself is not a page: its messages are
    /// [`module_message`](Self::module_message)'s.
    pub fn view_message(&mut self, text: &str) {
        let page = matches!(&self.phase, Phase::Live(live) if live.kind == super::ViewKind::Web);
        if !page {
            self.ignored();
            return;
        }
        if text.len() > MAX_VIEW_MESSAGE {
            self.view_said_too_much();
            return;
        }
        let Some(members) = read_members(text) else {
            self.ignored();
            return;
        };
        match members.own() {
            Some(Own::Context) => {
                self.diagnostics.read = self.diagnostics.read.saturating_add(1);
                self.tell_context();
            }
            Some(Own::Close) => {
                self.diagnostics.read = self.diagnostics.read.saturating_add(1);
                self.close(Cause::View, None);
            }
            Some(Own::Link) => self.page_link(text),
            None => {
                self.send_json(text);
            }
        }
    }

    /// The page asks to open a link: the person is asked if the host opens links, the address is
    /// one the host may open and not too many questions are open already; otherwise it is
    /// dropped (specification §9.7). The page is never answered.
    fn page_link(&mut self, text: &str) {
        #[derive(Deserialize)]
        struct Link {
            link: Object<Address>,
        }
        #[derive(Deserialize)]
        struct Address {
            url: String,
        }
        let url = serde_json::from_str::<Object<Link>>(text)
            .ok()
            .map(|message| message.0.link.0.url);
        let Phase::Live(live) = &mut self.phase else {
            return;
        };
        match url {
            Some(url)
                if self.config.open_links
                    && sandbox::is_openable_link(&url)
                    && live.links.len() < MAX_OPEN_LINKS =>
            {
                self.links = self.links.wrapping_add(1);
                live.links.push(self.links);
                self.diagnostics.read = self.diagnostics.read.saturating_add(1);
                self.actions.push(Action::OpenLink {
                    link: self.links,
                    url,
                });
            }
            _ => self.ignored(),
        }
    }

    /// The person's answer to [`Action::OpenLink`]: the host opens the link in the person's
    /// browser if the person agreed, and does nothing otherwise. Either way the page is not
    /// told, because whether the person agreed is not the page's to learn (specification §9.7).
    /// The terminal opens the link itself; this ends the question, so an answer to one that is
    /// not open, because the view has closed since, changes nothing.
    pub fn link_decision(&mut self, link: u32, allow: bool) {
        // The terminal acts on `allow`; the session keeps only the count of open questions.
        let _ = allow;
        if let Phase::Live(live) = &mut self.phase {
            live.links.retain(|known| *known != link);
        }
    }

    /// The `context` the page is told now, as JSON text, if a page is open (specification
    /// §9.5): what [`ViewStart::context`](super::ViewStart::context) was at the start, brought up
    /// to date. A terminal that has not delivered the first one yet because the page has not
    /// loaded delivers this instead.
    pub fn page_context(&self) -> Option<String> {
        match &self.phase {
            Phase::Live(live) if live.kind == super::ViewKind::Web => {
                Some(context_text(&self.context, &live.grant))
            }
            _ => None,
        }
    }

    /// The page asked for the context again: it is told the current one.
    fn tell_context(&mut self) {
        let Phase::Live(live) = &mut self.phase else {
            return;
        };
        let text = context_text(&self.context, &live.grant);
        live.context = Some(text.clone());
        self.actions.push(Action::ToView(text));
    }

    /// Something the page is told changed (the size of the pane, the theme, the person's
    /// language, time zone or setting for motion): a page that runs gets the new `context` if
    /// that changed what it was told last. A view the terminal draws itself is told by the kit.
    pub(super) fn context_changed(&mut self) {
        let Phase::Live(live) = &mut self.phase else {
            return;
        };
        if live.kind != super::ViewKind::Web {
            return;
        }
        let text = context_text(&self.context, &live.grant);
        if live.context.as_deref() == Some(text.as_str()) {
            return;
        }
        live.context = Some(text.clone());
        self.actions.push(Action::ToView(text));
    }
}
