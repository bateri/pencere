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
fn a_second_room_of_the_same_name_draws_no_browser() {
    let root = div()
        .child(
            div()
                .browser(BROWSER, BrowserOptions::default())
                .size(100.0, 50.0),
        )
        .child(
            div()
                .browser(BROWSER, BrowserOptions::default())
                .size(80.0, 40.0),
        );
    let laid = lay(root);
    let slots: Vec<Rect> = laid
        .paint(&mut Host)
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Browser(slot) => Some(slot.rect),
            _ => None,
        })
        .collect();
    // The one the host's browser is in, as `Laid::browsers` has it: the first.
    assert_eq!(slots, [laid.browsers()[0].pixels]);
    assert_eq!(slots, [Rect::new(0.0, 0.0, 100.0, 50.0)]);
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

/// A browser is a replaced element: its room is the content box of its element, and the border and the
/// padding stand around it.
#[test]
fn the_room_of_a_browser_is_the_content_box_of_its_element() {
    let root = div().child(div().h(40.0)).child(
        div()
            .browser(BROWSER, BrowserOptions::default())
            .size(100.0, 60.0)
            .border(2.0, Role::Border)
            .p(4.0),
    );
    let laid = lay(root);
    let room = laid.browsers().remove(0);
    let content = Rect::new(6.0, 46.0, 88.0, 48.0);
    assert_eq!(room.pixels, content);
    assert_eq!(room.rect, content);
    assert_eq!(room.clip, content);
    let list = laid.paint(&mut Host);
    let slot = list
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Browser(slot) => Some(slot),
            _ => None,
        })
        .expect("the browser's slot");
    assert_eq!(slot.rect, Rect::new(0.0, 40.0, 100.0, 60.0));
    assert_eq!(slot.padding_box, Rect::new(2.0, 42.0, 96.0, 56.0));
    assert_eq!(slot.content, content);
    assert_eq!(slot.clip, content);
    // Without a border or a padding it is the box, as it was.
    let root = div().child(
        div()
            .browser(BROWSER, BrowserOptions::default())
            .size(100.0, 60.0),
    );
    let room = lay(root).browsers().remove(0);
    assert_eq!(room.pixels, Rect::new(0.0, 0.0, 100.0, 60.0));
}
