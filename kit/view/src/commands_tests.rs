//! The engine's guards for what a page says of itself besides how it looks: the commands it can be
//! told to do and their keys, what can be dragged and where it goes, what each element is to a
//! screen reader, and the fills and figures it draws. The host's measuring is a fixed-width
//! stand-in.

use crate::{
    AccessibleRole, Command, Font, Id, Key, Laid, Measure, Metrics, Mods, Paint, Point, Rect, Role,
    Shortcut, Size, Ui, canvas, div, layout, path, text,
};

struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

fn lay(root: crate::Element, w: f32, h: f32) -> Laid {
    layout(root, Size::new(w, h), 1.0, &mut Fixed)
}

fn keys(words: &str) -> Shortcut {
    Shortcut::parse(words).expect("keys")
}

const PAGE: Id = Id::new(1);
const LIST: Id = Id::new(2);
const FIELD: Id = Id::new(3);
const QUIT: Id = Id::new(10);
const FIND: Id = Id::new(11);
const DELETE: Id = Id::new(12);
const FILTER_FIND: Id = Id::new(13);
const RENAME: Id = Id::new(14);

/// A page that quits and finds, a list that deletes and renames (renaming cannot be done now), and
/// a field inside the list whose own find wins over the page's while the keyboard is on it.
fn page(focus: Option<Id>) -> Laid {
    let root = div()
        .id(PAGE)
        .command(Command::new(QUIT, "Quit").keys(keys("cmd+q")))
        .command(Command::new(FIND, "Find").keys(keys("cmd+f")))
        .child(
            div()
                .id(LIST)
                .focused(focus == Some(LIST))
                .command(Command::new(DELETE, "Delete").keys(keys("backspace")))
                .command(
                    Command::new(RENAME, "Rename")
                        .keys(keys("enter"))
                        .enabled(false),
                )
                .child(
                    div()
                        .id(FIELD)
                        .h(10.0)
                        .focused(focus == Some(FIELD))
                        .command(Command::new(FILTER_FIND, "Find in list").keys(keys("cmd+f"))),
                ),
        );
    lay(root, 100.0, 100.0)
}

fn ids(commands: &[Command]) -> Vec<Id> {
    commands.iter().map(|command| command.id).collect()
}

#[test]
fn the_commands_are_those_of_the_element_the_keyboard_is_on_and_of_the_boxes_around_it() {
    assert_eq!(ids(&page(None).commands()), vec![QUIT, FIND]);
    assert_eq!(
        ids(&page(Some(LIST)).commands()),
        vec![DELETE, RENAME, QUIT, FIND]
    );
    let in_field = page(Some(FIELD)).commands();
    assert_eq!(
        ids(&in_field),
        vec![FILTER_FIND, DELETE, RENAME, QUIT, FIND]
    );
    // The page's find is listed, but its keys are the field's while the field has the keyboard.
    assert_eq!(in_field.last().and_then(|command| command.keys), None);
}

#[test]
fn a_key_does_the_nearest_command_that_has_it_and_can_be_done() {
    let cmd = Mods {
        command: true,
        ..Mods::default()
    };
    assert_eq!(page(None).command_for(Key::Character('f'), cmd), Some(FIND));
    assert_eq!(
        page(Some(FIELD)).command_for(Key::Character('F'), cmd),
        Some(FILTER_FIND)
    );
    assert_eq!(
        page(Some(LIST)).command_for(Key::Backspace, Mods::default()),
        Some(DELETE)
    );
    // Renaming cannot be done now: enter is a key for the page.
    assert_eq!(
        page(Some(LIST)).command_for(Key::Enter, Mods::default()),
        None
    );
    // Deleting is the list's: with the keyboard on nothing, backspace is a key.
    assert_eq!(
        page(None).command_for(Key::Backspace, Mods::default()),
        None
    );
    assert_eq!(
        page(None).command_for(Key::Character('q'), Mods::default()),
        None
    );
}

const CARD: Id = Id::new(20);
const DONE: Id = Id::new(21);
const TODO: Id = Id::new(22);

/// A card in the to-do column and an empty done column beside it.
fn board() -> Laid {
    let root = div()
        .row()
        .child(
            div()
                .drop_target(TODO)
                .size(50.0, 100.0)
                .child(div().draggable(CARD).size(40.0, 20.0)),
        )
        .child(div().drop_target(DONE).size(50.0, 100.0));
    lay(root, 100.0, 100.0)
}

#[test]
fn what_is_pressed_and_dragged_far_enough_is_dragged_and_put_down_on_the_target_it_ends_over() {
    let laid = board();
    let mut ui = Ui::default();
    assert_eq!(laid.drag_source(Point::new(5.0, 5.0)), Some(CARD));
    ui.point(&laid, Some(Point::new(5.0, 5.0)));
    assert_eq!(ui.press(&laid, Point::new(5.0, 5.0)), Some(CARD));
    // A hand that shakes is not a drag.
    assert!(!ui.point(&laid, Some(Point::new(6.0, 6.0))));
    assert_eq!(ui.dragging(), None);
    assert!(ui.point(&laid, Some(Point::new(60.0, 10.0))));
    let drag = ui.dragging().expect("dragged");
    assert_eq!(drag.source, CARD);
    assert_eq!(drag.target, Some(DONE));
    assert_eq!(drag.at, Point::new(60.0, 10.0));
    // Let go: no click, and the drop is told once.
    assert_eq!(ui.release(&laid, Point::new(70.0, 50.0)), None);
    let dropped = ui.take_drop().expect("dropped");
    assert_eq!((dropped.source, dropped.target), (CARD, Some(DONE)));
    assert_eq!(ui.take_drop(), None);
    assert_eq!(ui.dragging(), None);
}

#[test]
fn a_press_that_does_not_go_far_is_a_click_and_a_drag_given_up_drops_nothing() {
    let laid = board();
    let mut ui = Ui::default();
    ui.press(&laid, Point::new(5.0, 5.0));
    ui.point(&laid, Some(Point::new(7.0, 6.0)));
    assert_eq!(ui.release(&laid, Point::new(7.0, 6.0)), Some(CARD));
    assert_eq!(ui.take_drop(), None);

    ui.press(&laid, Point::new(5.0, 5.0));
    ui.point(&laid, Some(Point::new(60.0, 10.0)));
    assert!(ui.cancel_drag());
    assert_eq!(ui.release(&laid, Point::new(60.0, 10.0)), None);
    assert_eq!(ui.take_drop(), None);
    // The card is not a target of itself: over it, the column it is in is.
    ui.press(&laid, Point::new(5.0, 5.0));
    ui.point(&laid, Some(Point::new(20.0, 60.0)));
    assert_eq!(ui.dragging().and_then(|drag| drag.target), Some(TODO));
    ui.point(&laid, Some(Point::new(30.0, 10.0)));
    assert_eq!(ui.dragging().and_then(|drag| drag.target), Some(TODO));
}

const SWITCH: Id = Id::new(30);
const SAVE: Id = Id::new(31);

#[test]
fn an_element_is_what_it_says_to_a_screen_reader_with_its_state() {
    let root = div()
        .child(
            div()
                .press(SWITCH)
                .role(AccessibleRole::Switch)
                .checked(true)
                .label("Group")
                .child(text("Group")),
        )
        .child(
            div()
                .role(AccessibleRole::Heading(1))
                .child(text("Containers")),
        )
        .child(div().press(SAVE).disabled(true).h(10.0).child(text("Save")))
        .child(
            div()
                .role(AccessibleRole::Progress)
                .value("40%")
                .label("Pulling")
                .h(4.0),
        );
    let laid = lay(root, 200.0, 200.0);
    let items = laid.accessible();
    let read: Vec<(AccessibleRole, &str)> = items
        .iter()
        .map(|item| (item.role, item.words.as_str()))
        .collect();
    assert_eq!(
        read,
        vec![
            (AccessibleRole::Switch, "Group"),
            (AccessibleRole::Heading(1), "Containers"),
            (AccessibleRole::Button, "Save"),
            (AccessibleRole::Progress, "Pulling"),
        ]
    );
    assert_eq!(items[0].state.checked, Some(true));
    assert!(items[2].state.disabled);
    assert_eq!(items[3].state.value.as_deref(), Some("40%"));
    // What cannot be used now does not answer the pointer, nor take the keyboard.
    let save = laid.bounds(SAVE).expect("on the page");
    assert_eq!(laid.hit(Point::new(save.x + 1.0, save.y + 1.0)), None);
}

#[test]
fn a_fill_that_runs_from_one_colour_to_another_is_drawn_and_read_as_its_middle() {
    let root = div()
        .bg_gradient(90.0, Role::Accent, Role::Background)
        .child(text("hi"));
    let list = lay(root, 100.0, 20.0).paint(&mut Fixed);
    let Some(Paint::Quad(quad)) = list.items.first() else {
        panic!("a box first");
    };
    let gradient = quad.gradient.expect("the gradient");
    assert_eq!(gradient.angle, 90.0);
    assert_eq!(gradient.from.role(), Some(Role::Accent));
    assert_eq!(quad.fill, None);
    let Some(Paint::Text(run)) = list.items.get(1) else {
        panic!("then the words");
    };
    assert_eq!(run.grounds, vec![gradient.middle()]);
}

#[test]
fn a_canvas_hands_its_paths_to_the_host_in_its_box_as_faint_as_it_is() {
    let ring = path()
        .arc(10.0, 10.0, 8.0, 0.0, 270.0)
        .stroke(2.0, Role::Accent);
    let root = div()
        .p(5.0)
        .child(canvas([ring.clone()]).size(20.0, 20.0).opacity(0.5));
    let list = layout(root, Size::new(200.0, 200.0), 2.0, &mut Fixed).paint(&mut Fixed);
    let Some(Paint::Figure(figure)) = list.items.first() else {
        panic!("a figure");
    };
    assert_eq!(figure.rect, Rect::new(10.0, 10.0, 40.0, 40.0));
    assert_eq!(figure.scale, 2.0);
    assert_eq!(figure.paths[0].segments, ring.segments);
    let stroke = figure.paths[0].stroke.expect("a line");
    assert_eq!((stroke.width, stroke.color.alpha), (2.0, 0.5));
    // Three quarters of a turn: a move and three quarter curves, ending at nine o'clock.
    assert_eq!(ring.segments.len(), 4);
    let crate::Segment::Cubic { to, .. } = ring.segments[3] else {
        panic!("a curve");
    };
    assert!(
        (to.x - 2.0).abs() < 1e-4 && (to.y - 10.0).abs() < 1e-4,
        "{to:?}"
    );
}

#[test]
fn a_box_is_held_to_its_greatest_height_and_scrolls_across() {
    let root = div().child(
        div()
            .id(LIST)
            .max_h(30.0)
            .scroll()
            .scrolled(15.0, 0.0)
            .row()
            .child(div().size(300.0, 50.0).no_shrink()),
    );
    let laid = lay(root, 100.0, 100.0);
    let list = laid.bounds(LIST).expect("on the page");
    assert_eq!(list.h, 30.0);
    let extent = laid.scroll_extent(LIST).expect("it scrolls");
    assert_eq!(extent.max_offset_x(), 200.0);
    assert_eq!(laid.rects().nth(2).map(|r| r.x), Some(-15.0));
}
