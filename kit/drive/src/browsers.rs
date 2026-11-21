//! The browsers a page names: what the drive holds of each so that a page that names one is told
//! where it stands whatever its host is, and what stands in the room of one a host cannot show.
//!
//! A browser's engine is the host's; the drive keeps the rules that are the same for every host.
//! A host that shows browsers says so ([`crate::Page::set_browsers`]) and tells the page what each
//! is and does ([`crate::Page::browser`]). A host that does not is not asked to do anything: the
//! drive tells the page, once for each browser it names, that this host shows none
//! (`"unsupported"`) and draws a placeholder in the room, so that a page never waits for a state
//! that no host will give. An address the page asks a browser to go to is held to the rule of
//! [`pencere_view::go_refusal`]: one that is not is not given to the host, and the page is told
//! why ([`Event::BrowserRefused`]), as it is told of an editor's ask that was refused; so is a
//! pattern to find that is not a search's ([`pencere_view::find_refusal`]) and an ask the kit does
//! not know ([`BrowserAsk::Unknown`]). An empty pattern ends the search, and a zoom is put right to
//! what a browser may be zoomed to ([`pencere_view::zoom_allowed`]).

use std::collections::HashMap;

use pencere_view::{
    BrowserAsk, BrowserOptions, BrowserRefusal, BrowserRoom, BrowserSlot, BrowserState, Color,
    Event, Family, Font, Id, Measure, Paint, Point, Quad, Rect, Role, Run, find_refusal,
    go_refusal, zoom_allowed,
};

use crate::outline::Placement;

/// What a host that shows no browser says in the room of one, and to the page.
pub const UNSUPPORTED: &str = "This terminal cannot show a browser.";

/// A browser the host puts on the page: its room in the host's points, with the options the page
/// named it by ([`pencere_view::Element::browser`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Browser {
    /// Its name: the id of the element that is its room.
    pub id: Id,
    pub options: BrowserOptions,
    /// Where it stands, in the host's points.
    pub rect: Rect,
    /// It, or a box that holds it, is hidden: it keeps its page and draws nothing.
    pub hidden: bool,
    /// What it is to a screen reader, where the page said.
    pub label: Option<String>,
}

impl Browser {
    /// The browser of `room`, placed in the host's view as `at` says.
    pub(crate) fn of(room: BrowserRoom, at: Placement) -> Self {
        Self {
            id: room.id,
            options: room.options,
            rect: at.to_host(room.rect),
            hidden: room.hidden,
            label: room.label,
        }
    }
}

/// What the drive holds of the browsers of a page.
#[derive(Default)]
pub(crate) struct Browsers {
    /// The host shows browsers.
    pub shown: bool,
    /// The browsers the page names, by the profile each was made with. A browser that is not on the
    /// page any more is forgotten, and a browser of another profile under the name of one is another
    /// browser.
    known: HashMap<Id, Option<String>>,
}

impl Browsers {
    /// The browsers the last frame named: one seen for the first time, or with another profile
    /// than it had, is a new browser, and where the host shows none it is told it is unsupported.
    /// What is to be told the page, in order.
    pub fn meet(&mut self, rooms: &[BrowserRoom]) -> Vec<Event> {
        self.known
            .retain(|id, _| rooms.iter().any(|room| room.id == *id));
        let mut told = Vec::new();
        for room in rooms {
            if self
                .known
                .get(&room.id)
                .is_some_and(|profile| *profile == room.options.profile)
            {
                continue;
            }
            // A host that shows none has the page told so; one that shows them tells what each is.
            if !self.shown {
                told.push(Event::Browser {
                    browser: room.id,
                    state: Box::new(BrowserState::with_status(
                        BrowserState::UNSUPPORTED,
                        Some(UNSUPPORTED),
                    )),
                });
            }
            self.known.insert(room.id, room.options.profile.clone());
        }
        told
    }

    /// The host says what the browser `id` is and does: what is told the page, none for a browser
    /// that is not on the page (one it took away, whose engine said a last word as it went).
    pub fn said(&self, id: Id, state: BrowserState) -> Option<Event> {
        self.known.contains_key(&id).then(|| Event::Browser {
            browser: id,
            state: Box::new(state),
        })
    }

    /// What the page asks of its browsers, in `asks`, held to the rules every host has: what the
    /// host may be asked is left in it, and for each address a browser does not go to, each pattern
    /// that is not a search's and each ask the kit does not know the page is told why; a find of
    /// nothing is the end of the search, and a zoom is put right to what a browser may be zoomed
    /// to. A host that shows no browser is asked nothing.
    pub fn check(&self, asks: &mut Vec<(Id, BrowserAsk)>) -> Vec<Event> {
        let mut told = Vec::new();
        asks.retain_mut(|(id, ask)| {
            if matches!(ask, BrowserAsk::Find { pattern, .. } if pattern.is_empty()) {
                *ask = BrowserAsk::StopFinding;
            }
            let refusal = match ask {
                BrowserAsk::Go(url) => go_refusal(url),
                BrowserAsk::Find { pattern, .. } => find_refusal(pattern),
                BrowserAsk::Unknown => Some(BrowserRefusal::Ask),
                _ => None,
            };
            if let Some(reason) = refusal {
                told.push(Event::BrowserRefused {
                    browser: *id,
                    reason,
                });
                return false;
            }
            if let BrowserAsk::Zoom(zoom) = ask {
                *zoom = zoom_allowed(*zoom);
            }
            self.shown
        });
        told
    }
}

/// The places of browsers in `items`, put right for a host that shows none: where `shown` is false
/// what stands in each is the placeholder ([`placeholder`]). `size` is the page's text in pixels.
pub(crate) fn settle(items: &mut Vec<Paint>, shown: bool, size: f32, measure: &mut dyn Measure) {
    if shown || !items.iter().any(|item| matches!(item, Paint::Browser(_))) {
        return;
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        match item {
            Paint::Browser(slot) => out.extend(placeholder(&slot, size, measure)),
            other => out.push(other),
        }
    }
    *items = out;
}

/// A box drawn around the room of a browser, quiet, where a picture of the page shows where the room
/// is: the engine draws in it, and a picture has none.
pub fn frame(slot: &BrowserSlot) -> Paint {
    Paint::Quad(Quad {
        rect: slot.rect,
        radii: [0.0; 4],
        fill: None,
        gradient: None,
        border: Some(pencere_view::Border {
            width: 1.0,
            color: Color::from(Role::Quiet),
        }),
        shadow: None,
        clip: slot.clip,
    })
}

/// What stands in the room of a browser the host cannot show: that it cannot, in the middle of the
/// room and quiet, in a frame ([`frame`]). `size` is the text's, pixels.
pub fn placeholder(slot: &BrowserSlot, size: f32, measure: &mut dyn Measure) -> Vec<Paint> {
    let font = Font {
        family: Family::Terminal,
        size,
        weight: 400,
        italic: false,
    };
    let metrics = measure.measure(&font, UNSUPPORTED);
    vec![
        frame(slot),
        Paint::Text(Run {
            origin: Point::new(
                slot.rect.x + (slot.rect.w - metrics.width) / 2.0,
                slot.rect.y + (slot.rect.h - metrics.height) / 2.0,
            ),
            text: UNSUPPORTED.to_owned(),
            font,
            color: Color::from(Role::Quiet),
            opacity: slot.opacity,
            grounds: Vec::new(),
            clip: slot.clip,
            underline: false,
            strike: false,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::{BrowserRefusal, Metrics};

    struct Mono;

    impl Measure for Mono {
        fn measure(&mut self, font: &Font, text: &str) -> Metrics {
            Metrics {
                width: text.chars().count() as f32 * font.size * 0.5,
                height: font.size * 1.25,
            }
        }
    }

    fn room(id: u64, profile: Option<&str>) -> BrowserRoom {
        BrowserRoom {
            id: Id::new(id),
            options: BrowserOptions {
                profile: profile.map(str::to_owned),
                ..BrowserOptions::default()
            },
            rect: Rect::new(0.0, 0.0, 400.0, 200.0),
            pixels: Rect::new(0.0, 0.0, 400.0, 200.0),
            clip: Rect::new(0.0, 0.0, 400.0, 200.0),
            hidden: false,
            label: None,
        }
    }

    fn state_of(event: &Event) -> (Id, &BrowserState) {
        match event {
            Event::Browser { browser, state } => (*browser, state),
            other => panic!("not a browser's: {other:?}"),
        }
    }

    #[test]
    fn a_host_that_shows_no_browser_has_each_one_told_it_is_unsupported_once() {
        let mut browsers = Browsers::default();
        let rooms = [room(1, None), room(2, None)];
        let told = browsers.meet(&rooms);
        assert_eq!(told.len(), 2);
        let (_, state) = state_of(&told[0]);
        assert_eq!(state.status, "unsupported");
        assert_eq!(state.message.as_deref(), Some(UNSUPPORTED));
        assert!(browsers.meet(&rooms).is_empty(), "once");
        // One that left the page and came back is a new browser.
        assert!(browsers.meet(&[room(2, None)]).is_empty());
        assert_eq!(browsers.meet(&rooms).len(), 1);
    }

    #[test]
    fn what_the_host_says_of_a_browser_the_page_does_not_name_is_not_told() {
        let mut browsers = Browsers {
            shown: true,
            ..Browsers::default()
        };
        browsers.meet(&[room(1, None)]);
        assert!(browsers.said(Id::new(1), BrowserState::default()).is_some());
        assert_eq!(browsers.said(Id::new(2), BrowserState::default()), None);
        // One that left the page is forgotten.
        browsers.meet(&[]);
        assert_eq!(browsers.said(Id::new(1), BrowserState::default()), None);
    }

    #[test]
    fn another_profile_under_the_same_name_is_another_browser() {
        let mut browsers = Browsers::default();
        assert_eq!(browsers.meet(&[room(1, Some("a"))]).len(), 1);
        assert!(browsers.meet(&[room(1, Some("a"))]).is_empty());
        assert_eq!(browsers.meet(&[room(1, Some("b"))]).len(), 1);
        assert_eq!(browsers.meet(&[room(1, None)]).len(), 1);
    }

    #[test]
    fn a_host_that_shows_browsers_is_told_nothing_of_one_it_makes() {
        let mut browsers = Browsers {
            shown: true,
            ..Browsers::default()
        };
        assert!(
            browsers
                .meet(&[room(1, None), room(2, Some("work"))])
                .is_empty()
        );
    }

    #[test]
    fn an_address_a_browser_does_not_go_to_is_told_the_page_and_not_given_the_host() {
        let browsers = Browsers {
            shown: true,
            ..Browsers::default()
        };
        let mut asks = vec![
            (Id::new(1), BrowserAsk::Go("javascript:alert(1)".into())),
            (Id::new(1), BrowserAsk::Go("https://example.com/".into())),
            (Id::new(2), BrowserAsk::Go("https://exa mple.com/".into())),
            (Id::new(1), BrowserAsk::Reload { cache: true }),
        ];
        let told = browsers.check(&mut asks);
        assert_eq!(
            asks,
            vec![
                (Id::new(1), BrowserAsk::Go("https://example.com/".into())),
                (Id::new(1), BrowserAsk::Reload { cache: true })
            ]
        );
        assert_eq!(
            told,
            vec![
                Event::BrowserRefused {
                    browser: Id::new(1),
                    reason: BrowserRefusal::Scheme
                },
                Event::BrowserRefused {
                    browser: Id::new(2),
                    reason: BrowserRefusal::Address
                },
            ]
        );
    }

    #[test]
    fn an_ask_the_kit_does_not_know_is_told_the_page_and_a_zoom_is_put_right_before_the_host_has_it()
     {
        let browsers = Browsers {
            shown: true,
            ..Browsers::default()
        };
        let mut asks = vec![
            (Id::new(1), BrowserAsk::Unknown),
            (Id::new(1), BrowserAsk::Zoom(40.0)),
            (Id::new(1), BrowserAsk::Zoom(0.0)),
            (Id::new(1), BrowserAsk::Zoom(1.5)),
            (Id::new(2), BrowserAsk::Devtools),
        ];
        let told = browsers.check(&mut asks);
        assert_eq!(
            asks,
            vec![
                (Id::new(1), BrowserAsk::Zoom(pencere_view::ZOOM_MOST)),
                (Id::new(1), BrowserAsk::Zoom(pencere_view::ZOOM_LEAST)),
                (Id::new(1), BrowserAsk::Zoom(1.5)),
                (Id::new(2), BrowserAsk::Devtools),
            ]
        );
        assert_eq!(
            told,
            vec![Event::BrowserRefused {
                browser: Id::new(1),
                reason: BrowserRefusal::Ask
            }]
        );
        // The page of a host that shows none is told as well, and asked nothing.
        let mut asks = vec![
            (Id::new(1), BrowserAsk::Unknown),
            (Id::new(1), BrowserAsk::Stop),
        ];
        assert_eq!(Browsers::default().check(&mut asks).len(), 1);
        assert!(asks.is_empty());
    }

    #[test]
    fn an_empty_find_ends_the_search_and_a_pattern_no_browser_is_given_is_told_the_page() {
        let browsers = Browsers {
            shown: true,
            ..Browsers::default()
        };
        let find = |pattern: &str| BrowserAsk::Find {
            pattern: pattern.into(),
            case: true,
        };
        let mut asks = vec![
            (Id::new(1), find("")),
            (Id::new(1), find("café au lait")),
            (Id::new(1), find("two\nlines")),
            (
                Id::new(2),
                find(&"x".repeat(pencere_view::PATTERN_BYTES + 1)),
            ),
            (Id::new(2), find(&"x".repeat(pencere_view::PATTERN_BYTES))),
        ];
        let told = browsers.check(&mut asks);
        assert_eq!(
            asks,
            vec![
                (Id::new(1), BrowserAsk::StopFinding),
                (Id::new(1), find("café au lait")),
                (Id::new(2), find(&"x".repeat(pencere_view::PATTERN_BYTES))),
            ]
        );
        assert_eq!(
            told,
            vec![
                Event::BrowserRefused {
                    browser: Id::new(1),
                    reason: BrowserRefusal::Pattern
                },
                Event::BrowserRefused {
                    browser: Id::new(2),
                    reason: BrowserRefusal::Pattern
                },
            ]
        );
    }

    #[test]
    fn a_host_that_shows_no_browser_is_asked_nothing() {
        let browsers = Browsers::default();
        let mut asks = vec![(Id::new(1), BrowserAsk::Back)];
        assert!(browsers.check(&mut asks).is_empty());
        assert!(asks.is_empty());
    }

    #[test]
    fn the_room_of_a_browser_that_is_not_shown_says_so() {
        let slot = BrowserSlot {
            id: Id::new(1),
            rect: Rect::new(10.0, 20.0, 400.0, 200.0),
            clip: Rect::new(10.0, 20.0, 400.0, 200.0),
            opacity: 1.0,
        };
        let mut items = vec![Paint::Browser(slot.clone())];
        settle(&mut items, false, 10.0, &mut Mono);
        assert!(items.iter().all(|item| !matches!(item, Paint::Browser(_))));
        let words: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                Paint::Text(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(words, vec![UNSUPPORTED]);
        // Where the host shows browsers the place is the host's.
        let mut items = vec![Paint::Browser(slot)];
        settle(&mut items, true, 10.0, &mut Mono);
        assert!(matches!(items[0], Paint::Browser(_)));
    }
}
