//! Looking at a page: what it costs while nobody looks (nothing), what the model says of the page
//! as a script or a Rust module made it, and what of it is never said.

use std::time::Duration;

use pencere_sandbox::{CallKind, Fault, Frame, Runner};
use pencere_view::{
    Event, FieldKind, Font, Id, Laid, Measure, Metrics, Motion, Point, Reply, Role, Shift, Size,
    div, text,
};

use crate::headless::Headless;
use crate::{FrameAt, Page, Placement, Setup};

/// Text measured with no renderer: each character half its font's size across.
struct Even;

impl Measure for Even {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

const SAVE: Id = Id::new(7);
const SECRET: Id = Id::new(8);

/// A page of as many rows as it was told to grow to (by a message or any event) and a button.
struct Growing {
    rows: usize,
    secret: bool,
    /// The rows move in and out, so that frames go on being drawn after the work.
    moving: bool,
}

impl Growing {
    fn page(&self) -> pencere_view::Element {
        let moving = self.moving;
        let mut page = div().col().children((0..self.rows).map(|row| {
            let line = text(format!("row {row}")).id(Id::new(100 + row as u64));
            if moving {
                line.h(20.0).motion(
                    Motion::new(0.2)
                        .place()
                        .enter(Shift::fade(0.2).by(0.0, 6.0))
                        .exit(Shift::fade(0.1)),
                )
            } else {
                line
            }
        }));
        page = page.child(div().press(SAVE).label("Save").h(20.0).child(text("Save")));
        if self.secret {
            page = page.child(
                div()
                    .field(SECRET, FieldKind::Secret)
                    .label("Password")
                    .value("hunter2")
                    .h(20.0),
            );
        }
        page
    }
}

impl Runner for Growing {
    fn render(&mut self, _size: Size, _now: f64, _reduce: bool) -> Result<Frame, Fault> {
        Ok(Frame {
            root: self.page(),
            animating: false,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }

    fn handle(&mut self, _event: Event) -> Result<Reply, Fault> {
        self.rows += 1;
        Ok(Reply::redraw())
    }

    fn lay_out(&mut self, _laid: std::sync::Arc<Laid>) {}

    fn message(&mut self, _text: String) -> Result<Reply, Fault> {
        self.rows += 1;
        Ok(Reply::redraw())
    }

    fn take_sent(&mut self) -> Vec<String> {
        Vec::new()
    }
}

fn frame() -> FrameAt {
    FrameAt {
        size: (800, 600),
        scale: 1.0,
        now: 1.0,
        reduce_motion: false,
    }
}

fn growing(secret: bool, placement: Placement) -> Page {
    Page::spawn(
        move || {
            Ok(Growing {
                rows: 2,
                secret,
                moving: false,
            })
        },
        Setup {
            placement,
            measure: Box::new(Even),
            ..Setup::default()
        },
        || {},
    )
}

/// Frames and drains, as a host does, until the page has nothing more to say: the module is waited
/// for after each frame, so that a page it asked for is the next frame's.
fn settle(page: &mut Page) {
    for step in 0..20 {
        assert!(page.module().flush(Duration::from_secs(5)));
        let told = page.drain();
        page.frame(FrameAt {
            now: 1.0 + f64::from(step) * 0.016,
            ..frame()
        });
        assert!(page.module().flush(Duration::from_secs(5)));
        let after = page.drain();
        if !told.redraw && !after.redraw && !page.animating() {
            return;
        }
    }
    panic!("the page never came to rest");
}

#[test]
fn a_page_nobody_looks_at_reads_no_clock_and_builds_no_model() {
    let mut page = growing(false, Placement::default());
    settle(&mut page);
    // A message, an event and a frame of another size: the first layout, a layout again.
    page.message("grow");
    settle(&mut page);
    page.focus(None);
    settle(&mut page);
    page.frame(FrameAt {
        size: (640, 480),
        ..frame()
    });
    settle(&mut page);
    assert!(!page.inspecting());
    assert!(page.inspection().is_none());
    assert_eq!(page.clock_reads(), 0);
    // And what it costs is not kept for a look at it later.
    page.inspect(true);
    assert!(
        page.inspection()
            .is_some_and(|looked| looked.frames.is_empty())
    );
}

#[test]
fn a_page_looked_at_says_its_elements_and_what_a_change_of_it_cost() {
    let mut page = growing(false, Placement::default());
    settle(&mut page);
    page.inspect(true);
    assert!(page.inspecting());
    let before = {
        page.frame(frame());
        page.inspection().expect("looked at").elements
    };
    // The root, two rows, the button and its words.
    assert_eq!(before, 1 + 2 + 2);

    page.message("grow");
    settle(&mut page);
    let after = page.inspection().expect("looked at");
    assert_eq!(after.elements, before + 1, "one more row");
    let calls: Vec<CallKind> = after
        .frames
        .iter()
        .flat_map(|frame| frame.script.iter().map(|call| call.kind))
        .collect();
    assert!(calls.contains(&CallKind::Message), "{calls:?}");
    assert!(calls.contains(&CallKind::Render), "{calls:?}");
    assert!(after.frames.iter().any(|frame| frame.renders >= 1));
    assert!(after.frames.iter().any(|frame| frame.layouts >= 1));
    assert!(
        after.frames.iter().all(|frame| frame.elements >= before),
        "each frame says how many elements it drew"
    );
    let text = after.text();
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.starts_with("page "))
    );

    page.focus(None);
    settle(&mut page);
    let kinds: Vec<CallKind> = page
        .inspection()
        .expect("looked at")
        .frames
        .iter()
        .flat_map(|frame| frame.script.iter().map(|call| call.kind))
        .collect();
    assert!(kinds.contains(&CallKind::Event), "{kinds:?}");

    page.inspect(false);
    assert!(page.inspection().is_none());
}

#[test]
fn what_an_element_is_said_to_be_is_found_by_the_element() {
    let mut page = growing(false, Placement::default());
    page.inspect(true);
    settle(&mut page);
    let looked = page.inspection().expect("looked at");
    let button = looked
        .items
        .iter()
        .find(|item| item.element.id == Some(SAVE))
        .expect("the button");
    assert!(
        button
            .said
            .iter()
            .any(|said| said.role == "button" && said.words == "Save" && said.pressable)
    );
    let plain: Vec<_> = looked
        .items
        .iter()
        .filter(|item| item.element.id.is_none() && item.said.is_empty())
        .collect();
    assert!(
        !plain.is_empty(),
        "elements of no name and nothing said are in it too"
    );
}

#[test]
fn the_value_of_a_secret_field_is_in_neither_the_model_nor_its_text() {
    let mut page = growing(true, Placement::default());
    page.inspect(true);
    settle(&mut page);
    let looked = page.inspection().expect("looked at");
    let field = looked
        .items
        .iter()
        .find(|item| item.element.id == Some(SECRET))
        .expect("the field");
    assert_eq!(field.element.field, Some(FieldKind::Secret));
    assert!(
        field.said.iter().any(|said| said.words == "Password"),
        "its name is said: {:?}",
        field.said
    );
    for written in [looked.text(), format!("{looked:?}")] {
        assert!(!written.contains("hunter2"), "{written}");
        assert!(written.contains("Password"));
    }
}

#[test]
fn a_box_is_in_the_hosts_points_and_a_point_of_the_host_finds_it() {
    let mut page = growing(
        false,
        Placement {
            inset: 32.0,
            zoom: 2.0,
        },
    );
    page.inspect(true);
    settle(&mut page);
    let looked = page.inspection().expect("looked at");
    let row = looked
        .items
        .iter()
        .find(|item| item.element.path.as_deref() == Some(&[0][..]))
        .expect("the first row");
    assert!(
        row.element.rect.y >= 32.0,
        "below the host's strip: {:?}",
        row.element.rect
    );
    let mut found = 0;
    for item in &looked.items {
        let (Some(path), false) = (&item.element.path, item.element.rect.w <= 0.0) else {
            continue;
        };
        let rect = item.element.rect;
        let middle = Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let at = page.inspect_at(middle).expect("something shows");
        assert!(at.starts_with(path), "{path:?} at {middle:?} is {at:?}");
        found += 1;
    }
    assert!(found >= 5, "{found}");
}

#[test]
fn what_a_change_cost_is_still_said_after_the_frames_that_settle_it() {
    // A host that settles draws frames after the one that did the work, and more where the page
    // moves: the cost of the work is not that of the last frame.
    for moving in [false, true] {
        let mut page = Headless::spawn(move || {
            Ok(Growing {
                rows: 2,
                secret: false,
                moving,
            })
        })
        .open()
        .expect("opens");
        page.inspect(true);
        page.settle().expect("settles");
        let before = page.inspection().expect("looked at").elements;
        page.move_to(Point::new(5.0, 5.0)).expect("moves");
        page.settle().expect("settles");
        let after = page.inspection().expect("looked at");
        assert!(
            after.elements > before,
            "moving {moving}: {before} then {}",
            after.elements
        );
        let kinds: Vec<CallKind> = after
            .frames
            .iter()
            .flat_map(|frame| frame.script.iter().map(|call| call.kind))
            .collect();
        assert!(
            kinds.contains(&CallKind::Event),
            "moving {moving}: {kinds:?}"
        );
        assert!(
            kinds.contains(&CallKind::Render),
            "moving {moving}: {kinds:?}"
        );
        assert!(
            after.text().lines().any(|line| line.starts_with("cost ")),
            "moving {moving}"
        );
        assert!(page.inspect_at(Point::new(5.0, 5.0)).is_some());
        page.inspect(false);
        assert!(page.inspection().is_none());
    }
}

/// A page whose box fades to its colour when anything happens: what moves only paints, and the
/// page is drawn again from the layout it has at each frame of the fade.
struct Fading {
    hot: bool,
}

impl Runner for Fading {
    fn render(&mut self, _size: Size, _now: f64, _reduce: bool) -> Result<Frame, Fault> {
        let fill = if self.hot {
            Role::Accent.solid()
        } else {
            Role::Accent.alpha(0.0)
        };
        Ok(Frame {
            root: div().col().child(
                div()
                    .id(Id::new(9))
                    .w(50.0)
                    .h(20.0)
                    .bg(fill)
                    .motion(Motion::new(0.5).color()),
            ),
            animating: false,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }

    fn handle(&mut self, _event: Event) -> Result<Reply, Fault> {
        self.hot = true;
        Ok(Reply::redraw())
    }

    fn lay_out(&mut self, _laid: std::sync::Arc<Laid>) {}

    fn take_sent(&mut self) -> Vec<String> {
        Vec::new()
    }
}

#[test]
fn the_frames_of_a_fade_do_not_push_out_the_work_that_began_it() {
    let mut page = Headless::spawn(|| Ok(Fading { hot: false }))
        .open()
        .expect("opens");
    page.inspect(true);
    page.settle().expect("settles");
    page.move_to(Point::new(5.0, 5.0)).expect("moves");
    page.settle().expect("settles");
    // The box fades for half a second: thirty frames and more, each of which draws the layout it
    // has again, and none of which is work of the page's.
    page.advance(1.0).expect("advances");
    let looked = page.inspection().expect("looked at");
    let kinds: Vec<CallKind> = looked
        .frames
        .iter()
        .flat_map(|frame| frame.script.iter().map(|call| call.kind))
        .collect();
    assert!(kinds.contains(&CallKind::Event), "{kinds:?}");
    assert!(kinds.contains(&CallKind::Render), "{kinds:?}");
    let text = looked.text();
    assert!(
        text.lines()
            .any(|line| line.starts_with("cost ") && line.contains("event")),
        "{text}"
    );
}

#[test]
fn the_text_has_a_line_for_each_element_indented_by_its_depth_and_the_cost_on_lines_of_its_own() {
    let mut page = growing(false, Placement::default());
    page.inspect(true);
    settle(&mut page);
    let text = page.inspection().expect("looked at").text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"page 800x600 · 5 elements"));
    assert_eq!(lines.get(1), Some(&"box 0,0 800x600"));
    assert!(
        lines
            .get(2)
            .is_some_and(|line| line.starts_with("  text #")),
        "{text}"
    );
    assert!(
        lines
            .get(3)
            .is_some_and(|line| line.starts_with("    says text \"row 0\"")),
        "{text}"
    );
    // What was measured is on lines that begin with `cost`, and nowhere else.
    assert!(lines.iter().any(|line| line.starts_with("cost ")), "{text}");
    assert!(
        lines
            .iter()
            .filter(|line| !line.starts_with("cost "))
            .all(|line| !line.contains(" ms")),
        "{text}"
    );
}

#[test]
fn the_style_is_read_where_the_layout_is_only_due_again_and_not_of_a_page_it_is_not_of() {
    let mut page = growing(false, Placement::default());
    page.inspect(true);
    settle(&mut page);
    let styled = |page: &Page| {
        let looked = page.inspection().expect("looked at");
        looked
            .items
            .iter()
            .filter(|item| item.element.placing.is_some())
            .count()
    };
    let all = page.inspection().expect("looked at").items.len();
    assert_eq!(styled(&page), all);
    // The host's measure changed: the page is to be laid out again, and it is the same page.
    page.set_measure(Box::new(Even));
    assert_eq!(styled(&page), all);
    // The script made another page, which is not laid out yet: the style that is read is not of
    // the layout.
    page.message("grow");
    assert!(page.module().flush(Duration::from_secs(5)));
    page.drain();
    assert_eq!(styled(&page), 0);
    settle(&mut page);
    assert_eq!(styled(&page), all + 1);
}

#[test]
fn a_length_that_rounds_to_nothing_is_written_as_nothing() {
    use crate::inspect::number;
    assert_eq!(number(-0.001), "0");
    assert_eq!(number(-0.0), "0");
    assert_eq!(number(0.004), "0");
    assert_eq!(number(12.0), "12");
    assert_eq!(number(1.5), "1.5");
    assert_eq!(number(-2.25), "-2.25");
}
