//! The engine's guards for the room of a browser: the host's browser is found in it, hidden or not,
//! drawn in its place under what the page puts inside the room, and the pointer in it is its own.

use crate::{
    AccessibleRole, BrowserOptions, BrowserWindows, Font, Id, Key, Laid, Measure, Metrics, Mods,
    Paint, Point, Rect, Role, Shortcut, Size, div, layout, text,
};

const BROWSER: Id = Id::new(1);
const OTHER: Id = Id::new(2);
const PRESSED: Id = Id::new(3);

/// Every character is `0.5 × size` wide.
struct Host;

impl Measure for Host {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

fn lay(root: crate::Element) -> Laid {
    layout(root, Size::new(400.0, 300.0), 1.0, &mut Host)
}

#[test]
fn a_browser_is_found_where_its_room_is_hidden_or_not() {
    let root = div()
        .child(
            div()
                .browser(
                    BROWSER,
                    BrowserOptions {
                        profile: Some("work".into()),
                        windows: BrowserWindows::Page,
                        keys: vec![Shortcut::new(Key::Character('l'), Mods::default())],
                    },
                )
                .size(100.0, 50.0),
        )
        .child(
            div()
                .hidden()
                .child(div().browser(OTHER, BrowserOptions::default())),
        )
        // A second of the same name is not another browser.
        .child(div().browser(BROWSER, BrowserOptions::default()));
    let laid = lay(root);
    let browsers = laid.browsers();
    assert_eq!(browsers.len(), 2);
    let (first, hidden) = (&browsers[0], &browsers[1]);
    assert_eq!(first.id, BROWSER);
    assert_eq!(first.rect, Rect::new(0.0, 0.0, 100.0, 50.0));
    assert!(!first.hidden);
    assert_eq!(first.options.profile.as_deref(), Some("work"));
    assert_eq!(first.options.windows, BrowserWindows::Page);
    assert_eq!(first.options.keys.len(), 1);
    assert_eq!(hidden.id, OTHER);
    assert!(
        hidden.hidden,
        "a hidden browser is still one: it keeps its page"
    );
    assert_eq!(hidden.options, BrowserOptions::default());
}

#[test]
fn the_browser_is_drawn_in_its_place_over_its_room_and_under_what_is_inside_it() {
    let root = div().child(
        div()
            .browser(BROWSER, BrowserOptions::default())
            .size(200.0, 100.0)
            .bg(Role::Raised(1))
            .child(text("Loading")),
    );
    let laid = lay(root);
    let list = laid.paint(&mut Host);
    let slot = list
        .items
        .iter()
        .position(|item| matches!(item, Paint::Browser(slot) if slot.id == BROWSER))
        .expect("the browser's place is in the list");
    let room = list
        .items
        .iter()
        .position(|item| {
            matches!(item, Paint::Quad(quad) if quad.fill.and_then(|f| f.role()) == Some(Role::Raised(1)))
        })
        .expect("the room's fill is drawn");
    let words = list
        .items
        .iter()
        .position(|item| matches!(item, Paint::Text(run) if run.text == "Loading"))
        .expect("what the page put in the room is drawn");
    assert!(room < slot, "the room's fill is under the browser");
    assert!(
        slot < words,
        "what the page put in the room is over the browser"
    );
    let Some(Paint::Browser(slot)) = list.items.get(slot) else {
        unreachable!();
    };
    assert_eq!(slot.rect, Rect::new(0.0, 0.0, 200.0, 100.0));
}

#[test]
fn a_browser_is_one_to_a_screen_reader_and_cannot_be_made_one_by_a_role() {
    let root = div()
        .child(
            div()
                .browser(BROWSER, BrowserOptions::default())
                .label("Docs")
                .size(100.0, 50.0),
        )
        // The role of a browser is not one a page gives to another box.
        .child(
            div()
                .id(OTHER)
                .role(AccessibleRole::Browser)
                .child(text("x")),
        );
    let said = lay(root).accessible();
    assert!(said.iter().any(|item| item.role == AccessibleRole::Browser
        && item.id == Some(BROWSER)
        && item.words == "Docs"));
    assert!(
        said.iter()
            .all(|item| !(item.role == AccessibleRole::Browser && item.id == Some(OTHER)))
    );
}

#[test]
fn the_pointer_in_a_browser_is_its_own_and_not_a_drag_of_a_box_around_it() {
    let root = div().child(
        div().id(PRESSED).carry(crate::Carry::text("words")).child(
            div()
                .browser(BROWSER, BrowserOptions::default())
                .size(100.0, 50.0),
        ),
    );
    let laid = lay(root);
    assert!(laid.carry_at(Point::new(10.0, 10.0)).is_none());
}
