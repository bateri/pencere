//! The engine's guards for the room of an editor: the host's editor is found in it, hidden or not,
//! drawn in its place, scrolled as a box is, and what is anchored to its caret stands beside the
//! caret. The host's measuring is a fixed-width stand-in, and its editor says where its caret is.

use crate::edit::EditorOptions;
use crate::{
    AccessibleRole, EditorMetrics, Font, Id, Laid, Measure, Metrics, Paint, Point, Rect, Role,
    ScrollTo, Scrolled, Scrolls, Side, Size, div, layout, text,
};

const EDITOR: Id = Id::new(1);
const OTHER: Id = Id::new(2);
const POPUP: Id = Id::new(3);

/// Every character is `0.5 × size` wide; the editor's caret is where `caret` says, and its
/// content as large as `content`.
struct Host {
    caret: Option<Rect>,
    content: Size,
}

impl Measure for Host {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
            ascent: font.size,
        }
    }

    fn editor(&mut self, id: Id) -> Option<EditorMetrics> {
        (id == EDITOR).then_some(EditorMetrics {
            caret: self.caret,
            content: self.content,
            row: 0.0,
        })
    }
}

fn host(caret: Option<Rect>) -> Host {
    Host {
        caret,
        content: Size::new(300.0, 1000.0),
    }
}

/// An editor 200 points high, 40 down the page, with a popup anchored below its caret.
fn page(popup: bool) -> crate::Element {
    div()
        .child(div().h(40.0))
        .child(
            div()
                .editor(EDITOR, EditorOptions::default())
                .h(200.0)
                .bg(Role::Raised(1))
                .child(text("Write SQL…"))
                .child_if(popup.then(|| {
                    div()
                        .id(POPUP)
                        .size(80.0, 50.0)
                        .anchor(EDITOR.caret(), Side::Below)
                        .child(text("SELECT"))
                })),
        )
        .child(div().h(10.0))
}

fn lay(root: crate::Element, host: &mut Host) -> Laid {
    layout(root, Size::new(400.0, 300.0), 1.0, host)
}

#[test]
fn an_editor_is_found_where_its_room_is_hidden_or_not() {
    let root = div()
        .child(
            div()
                .editor(EDITOR, EditorOptions::default())
                .size(100.0, 50.0)
                .text_size(14.0)
                .child(text("Write SQL…")),
        )
        .child(div().hidden().child(div().editor(
            OTHER,
            EditorOptions {
                hint: Some("Nothing yet".into()),
                ..EditorOptions::default()
            },
        )))
        // A second of the same name is not another editor.
        .child(div().editor(EDITOR, EditorOptions::default()));
    let laid = lay(root, &mut host(None));
    let editors = laid.editors();
    assert_eq!(editors.len(), 2);
    let (first, hidden) = (&editors[0], &editors[1]);
    assert_eq!(first.id, EDITOR);
    assert_eq!(first.rect, Rect::new(0.0, 0.0, 100.0, 50.0));
    assert!(!first.hidden);
    assert_eq!(first.font.size, 14.0);
    assert_eq!(first.hint.as_deref(), Some("Write SQL…"));
    assert_eq!(hidden.id, OTHER);
    assert!(
        hidden.hidden,
        "a hidden editor is still one: it keeps its document"
    );
    assert_eq!(hidden.hint.as_deref(), Some("Nothing yet"));
}

#[test]
fn the_editor_is_drawn_in_its_place_over_its_room_and_its_hint_is_not() {
    let mut host = host(None);
    let laid = lay(page(false), &mut host);
    let list = laid.paint(&mut host);
    let slot = list
        .items
        .iter()
        .position(|item| matches!(item, Paint::Editor(slot) if slot.id == EDITOR))
        .expect("the editor's place is in the list");
    let room = list
        .items
        .iter()
        .position(|item| {
            matches!(item, Paint::Quad(quad) if quad.fill.and_then(|f| f.role()) == Some(Role::Raised(1)))
        })
        .expect("the room's fill is drawn");
    assert!(room < slot, "the room's fill is under the editor");
    let Some(Paint::Editor(slot)) = list.items.get(slot) else {
        unreachable!();
    };
    assert_eq!(slot.rect, Rect::new(0.0, 40.0, 400.0, 200.0));
    assert_eq!(
        slot.grounds.first().and_then(|g| g.role()),
        Some(Role::Raised(1))
    );
    assert!(
        list.items
            .iter()
            .all(|item| !matches!(item, Paint::Text(_))),
        "the hint is the editor's to draw"
    );
    // An overlay written inside the room is not in it: its words are the page's.
    let mut host = self::host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let laid = lay(page(true), &mut host);
    let list = laid.paint(&mut host);
    let words: Vec<&str> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(words, vec!["SELECT"]);
    assert_eq!(laid.editors()[0].hint.as_deref(), Some("Write SQL…"));
    let said = laid.accessible();
    assert!(said.iter().any(|item| item.role == AccessibleRole::Editor
        && item.id == Some(EDITOR)
        && item.words == "Write SQL…"));
}

#[test]
fn an_overlay_stands_below_the_caret_and_above_it_where_there_is_no_room() {
    // The caret 20 points down the content, 30 across: on the page at (30, 60).
    let mut caret = host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let laid = lay(page(true), &mut caret);
    assert_eq!(
        laid.bounds(EDITOR.caret()),
        Some(Rect::new(30.0, 60.0, 2.0, 16.0))
    );
    assert_eq!(laid.bounds(POPUP), Some(Rect::new(30.0, 76.0, 80.0, 50.0)));
    assert!(laid.anchored_to(EDITOR.caret()));
    // Near the bottom of the page, it goes above the caret.
    let mut low = host(Some(Rect::new(30.0, 210.0, 2.0, 16.0)));
    let laid = lay(page(true), &mut low);
    assert_eq!(laid.bounds(POPUP), Some(Rect::new(30.0, 200.0, 80.0, 50.0)));
}

#[test]
fn an_overlay_on_the_caret_follows_it_when_the_page_is_laid_out_again() {
    let mut here = host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let before = lay(page(true), &mut here).bounds(POPUP);
    let mut moved = host(Some(Rect::new(90.0, 36.0, 2.0, 16.0)));
    let after = lay(page(true), &mut moved).bounds(POPUP);
    assert_eq!(before, Some(Rect::new(30.0, 76.0, 80.0, 50.0)));
    assert_eq!(after, Some(Rect::new(90.0, 92.0, 80.0, 50.0)));
}

#[test]
fn an_editor_scrolls_as_a_box_does_over_what_its_editor_laid_out() {
    let mut host = host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let mut scrolls = Scrolls::new();
    let mut laid = lay(page(true), &mut host);
    scrolls.place(&mut laid, 0.0, false);
    let extent = laid.scroll_extent(EDITOR).expect("the editor scrolls");
    assert_eq!(extent.content, Size::new(300.0, 1000.0));
    assert_eq!(extent.view, Size::new(400.0, 200.0));
    assert!(!laid.wants_layout());
    // The editor asks for its caret to be kept in view: the page is not laid out for it, the
    // caret's box moves with the room, and the overlay anchored to it asks to be placed again.
    scrolls.ask(ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(10.0),
        animate: false,
        zoom: None,
        at: None,
    });
    scrolls.place(&mut laid, 0.1, false);
    assert_eq!(
        laid.scroll_extent(EDITOR).map(|e| e.offset),
        Some(Point::new(0.0, 10.0))
    );
    assert_eq!(
        laid.bounds(EDITOR.caret()),
        Some(Rect::new(30.0, 50.0, 2.0, 16.0))
    );
    assert_eq!(
        scrolls.take_moved(),
        vec![Scrolled::at(EDITOR, Point::new(0.0, 10.0))]
    );
    assert!(laid.wants_layout(), "the popup stands where the caret was");
    // Laid out again where it is scrolled, the popup is beside the caret.
    let mut root = page(true);
    scrolls.apply(&mut root);
    let mut again = lay(root, &mut host);
    scrolls.place(&mut again, 0.2, false);
    assert!(!again.wants_layout());
    assert_eq!(again.bounds(POPUP), Some(Rect::new(30.0, 66.0, 80.0, 50.0)));
}

/// An editor in a box with a border and a padding stands in the box's padding box, where it draws and
/// cuts, and counts its places from the corner of it; what the box has of padding is its to put before
/// its text, and scrolls with the text. The border is what the view has more than the content.
#[test]
fn an_editor_stands_in_the_padding_box_of_its_element_with_the_padding_before_its_text() {
    let mut host = host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let mut scrolls = Scrolls::new();
    let root = div().child(div().h(40.0)).child(
        div()
            .editor(EDITOR, EditorOptions::default())
            .w(300.0)
            .h(200.0)
            .border(2.0, Role::Border)
            .p(4.0),
    );
    let mut laid = lay(root, &mut host);
    scrolls.place(&mut laid, 0.0, false);
    // The border box is 300 by 200, 40 down: the padding box is 2 in from it, the content box 4 more.
    let padding_box = Rect::new(2.0, 42.0, 296.0, 196.0);
    let room = laid.editors().remove(0);
    assert_eq!(room.pixels, padding_box);
    assert_eq!(room.rect, padding_box, "points are pixels at this scale");
    assert_eq!(room.inset, [4.0; 4]);
    assert_eq!(room.clip, padding_box);
    let list = laid.paint(&mut host);
    let slot = list
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Editor(slot) => Some(slot),
            _ => None,
        })
        .expect("the editor's slot");
    assert_eq!(slot.rect, Rect::new(0.0, 40.0, 300.0, 200.0));
    assert_eq!(slot.padding_box, padding_box);
    assert_eq!(slot.content, Rect::new(6.0, 46.0, 288.0, 188.0));
    assert_eq!(slot.clip, padding_box, "cut at the padding box");
    // The content is the editor's from the corner of the padding box and the view is the border box:
    // the editor reaches its end when what it laid out less the padding box is scrolled.
    let extent = laid.scroll_extent(EDITOR).expect("the editor scrolls");
    assert_eq!(extent.content, Size::new(304.0, 1004.0));
    assert_eq!(extent.view, Size::new(300.0, 200.0));
    assert_eq!(extent.max_offset(), 1000.0 - 196.0);
    // Its caret is where the editor says, from the corner of the padding box, moved as it is scrolled.
    assert_eq!(
        laid.bounds(EDITOR.caret()),
        Some(Rect::new(32.0, 62.0, 2.0, 16.0))
    );
    scrolls.ask(ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(10.0),
        animate: false,
        zoom: None,
        at: None,
    });
    scrolls.place(&mut laid, 0.1, false);
    assert_eq!(
        laid.bounds(EDITOR.caret()),
        Some(Rect::new(32.0, 52.0, 2.0, 16.0))
    );
}

#[test]
fn an_editor_scrolled_with_nothing_on_its_caret_asks_for_nothing() {
    let mut host = host(Some(Rect::new(30.0, 20.0, 2.0, 16.0)));
    let mut scrolls = Scrolls::new();
    let mut laid = lay(page(false), &mut host);
    scrolls.place(&mut laid, 0.0, false);
    scrolls.ask(ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(100.0),
        animate: false,
        zoom: None,
        at: None,
    });
    scrolls.place(&mut laid, 0.1, false);
    assert_eq!(laid.scroll_extent(EDITOR).map(|e| e.offset.y), Some(100.0));
    assert!(!laid.wants_layout());
}

#[test]
fn a_caret_folded_away_has_no_box() {
    let laid = lay(page(true), &mut host(None));
    assert_eq!(laid.bounds(EDITOR.caret()), None);
    // The popup stands where its inset puts it.
    assert_eq!(laid.bounds(POPUP).map(|r| (r.x, r.y)), Some((0.0, 0.0)));
}

#[test]
fn an_element_that_is_no_editor_has_no_room_for_an_editors_options() {
    // A page is thousands of elements and a few editors at most, and what an editor is set up
    // with is several times the rest of an element: held in every element, it was most of the
    // memory of every page built and of every layout.
    assert!(
        std::mem::size_of::<crate::element::Meta>() < std::mem::size_of::<EditorOptions>(),
        "an element's meta is {} bytes and an editor's options {}",
        std::mem::size_of::<crate::element::Meta>(),
        std::mem::size_of::<EditorOptions>()
    );
    // The room of an editor still says how it is set up.
    let options = EditorOptions::default();
    let room = div().editor(EDITOR, options.clone());
    assert_eq!(room.meta.editor.as_deref(), Some(&options));
}
