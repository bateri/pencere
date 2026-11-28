//! Where a page stands in the host's view, and what the host keeps of it besides its pixels: the
//! fields it puts over it, what a screen reader says of it, its tooltips and its commands.

use crate::browsers::Browser;
use pencere_view::{
    Accessible, AccessibleRole, Command, FieldKind, Font, Id, Laid, Point, Rect, State,
};

/// Where a page stands in the host's view: below the host's own `inset` points (a strip of its
/// own over the page, which the page never draws into), and `zoom` of the host's points to one of
/// the page's. A terminal draws a page as large as its own text is to the kit's size
/// ([`Placement::zoom_for`]), so that the page's words are the size of the terminal's.
///
/// Everything the drive is told and tells is in the host's points: the pointer, the files dropped,
/// the fields and the screen reader's boxes, the menu's place, an editor's caret.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub inset: f32,
    pub zoom: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            inset: 0.0,
            zoom: 1.0,
        }
    }
}

/// The size of the kit's text where a page does not say one (12.5 points, `pencere-view`'s): a
/// host whose text is this size draws a page at its own size (a text field of its own is set in
/// the text of its element, which is this size where nothing says another).
pub use pencere_view::ZOOM_TEXT;

/// The least and the most a page is drawn at, against its own size: a terminal set in a tiny or a
/// huge face keeps a page that can still be laid out in its view.
const ZOOM_RANGE: (f32, f32) = (0.5, 4.0);

impl Placement {
    /// `zoom` for a host whose text is `points` large ([`ZOOM_TEXT`]); 1 for one that is not a
    /// size.
    pub fn zoom_for(points: f64) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let zoom = (points as f32) / ZOOM_TEXT;
        if zoom.is_finite() && zoom > 0.0 {
            zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
        } else {
            1.0
        }
    }

    /// A box of the page in the host's points.
    pub fn to_host(self, rect: Rect) -> Rect {
        let z = self.zoom;
        Rect::new(rect.x * z, rect.y * z + self.inset, rect.w * z, rect.h * z)
    }

    /// A point of the page in the host's points.
    pub fn point_to_host(self, at: Point) -> Point {
        Point::new(at.x * self.zoom, at.y * self.zoom + self.inset)
    }

    /// A point of the host's as the page's.
    pub fn to_page(self, at: Point) -> Point {
        Point::new(at.x / self.zoom, (at.y - self.inset) / self.zoom)
    }
}

/// A text field the host puts over a page: its name (the id of the element that is its room), its
/// kind, where its room, its padding box and its text stand in the host's points, what it says
/// while it is empty — the words the page put in that room — and what it is to a screen reader.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub id: Id,
    pub kind: FieldKind,
    /// Its room: all of it is the field's for the pointer.
    pub rect: Rect,
    /// The room less its border ([`pencere_view::Field::padding_box`]): where a field of lines
    /// cuts its text and scrolls it, the padding scrolling with it.
    pub padding_box: Rect,
    /// Where its text stands: the padding box less the padding of the element
    /// ([`pencere_view::Field::text`]). Set in `font`, and in a field of lines broken at its width,
    /// from its top.
    pub text: Rect,
    /// The font its text is set in: the text of its element ([`pencere_view::Field::font`]), the
    /// size in the host's points like the boxes (the page's points times the zoom) and not in the
    /// layout's pixels.
    pub font: Font,
    pub hint: Option<String>,
    pub label: Option<String>,
    /// Something the page draws lies over it (a popover, a dialog): the host does not show it.
    pub covered: bool,
}

/// What the host keeps of a page besides its pixels, in the host's points: the fields it puts
/// over it and what a screen reader says of it. A frame says when it changed
/// ([`crate::Painted::outline`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub fields: Vec<Field>,
    /// The browsers of the page, as the page names them ([`crate::Browser`]).
    pub browsers: Vec<Browser>,
    pub accessible: Vec<Accessible>,
    /// How many items the page has behind a modal layer that is in front: they are not in
    /// `accessible` ([`pencere_view::Laid::accessible_behind`]).
    pub behind: usize,
    /// What the page says of a part of it while the pointer rests there, innermost last.
    pub tips: Vec<Tip>,
    /// What the page can be told to do now (`Laid::commands`), the nearest the keyboard first:
    /// what the host lists where a person looks for what can be done, and tells the page of
    /// ([`crate::Page::command`]).
    pub commands: Vec<Command>,
}

/// A tooltip of the page: where it is in the host's view, and what it says (the keys that do the
/// same in brackets, as the system's own tooltips write a shortcut).
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub rect: Rect,
    pub words: String,
}

impl Outline {
    /// What the markers of an editor's gutter say of themselves, each with its box in the page's
    /// points: a screen reader says them and the host shows them while the pointer rests on them.
    pub fn add_markers(&mut self, markers: Vec<(Rect, String)>, at: Placement) {
        for (rect, words) in markers {
            let rect = at.to_host(rect);
            self.accessible.push(Accessible {
                role: AccessibleRole::Image,
                id: None,
                rect,
                words: words.clone(),
                state: State::default(),
                pressable: false,
                parent: None,
                detail: Default::default(),
            });
            self.tips.push(Tip { rect, words });
        }
    }

    /// The outline of `laid`, placed in the host's view as `at` says.
    pub fn of(laid: &Laid, at: Placement) -> Self {
        let (accessible, behind) = laid.accessible_behind();
        Self {
            browsers: laid
                .browsers()
                .into_iter()
                .map(|room| Browser::of(room, at))
                .collect(),
            fields: laid
                .fields()
                .into_iter()
                .map(|field| Field {
                    id: field.id,
                    kind: field.kind,
                    rect: at.to_host(field.rect),
                    padding_box: at.to_host(field.padding_box),
                    text: at.to_host(field.text),
                    font: Font {
                        size: field.font.size / laid.scale() * at.zoom,
                        ..field.font
                    },
                    hint: field.hint,
                    label: field.label,
                    covered: field.covered,
                })
                .collect(),
            accessible: accessible
                .into_iter()
                .map(|item| Accessible {
                    rect: at.to_host(item.rect),
                    ..item
                })
                .collect(),
            behind,
            tips: laid
                .tips()
                .into_iter()
                .map(|(rect, tip)| Tip {
                    rect: at.to_host(rect),
                    words: match tip.keys {
                        Some(keys) if !keys.is_empty() => format!("{} ({keys})", tip.words),
                        _ => tip.words,
                    },
                })
                .collect(),
            commands: laid.commands(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_drawn_as_large_as_the_hosts_text_has_fewer_points_in_the_same_room() {
        assert_eq!(Placement::zoom_for(12.5), 1.0);
        assert_eq!(Placement::zoom_for(25.0), 2.0);
        assert_eq!(Placement::zoom_for(1.0), 0.5, "held to the least");
        assert_eq!(Placement::zoom_for(500.0), 4.0, "held to the most");
        assert_eq!(Placement::zoom_for(f64::NAN), 1.0);
        assert_eq!(Placement::zoom_for(-3.0), 1.0);
        let at = Placement {
            inset: 32.0,
            zoom: 2.0,
        };
        // A box of the page is twice as large in the host's points, below the strip.
        assert_eq!(
            at.to_host(Rect::new(10.0, 5.0, 20.0, 4.0)),
            Rect::new(20.0, 42.0, 40.0, 8.0)
        );
        // And a point of the host's is the page's, the strip's room taken off first.
        assert_eq!(at.to_page(Point::new(20.0, 42.0)), Point::new(10.0, 5.0));
    }

    #[test]
    fn a_field_has_its_three_boxes_in_the_hosts_points() {
        use pencere_view::{Font, Measure, Metrics, Role, Size, div, layout};
        // A host whose line in a field is 20.
        struct Ruler;
        impl Measure for Ruler {
            fn measure(&mut self, _: &Font, text: &str) -> Metrics {
                Metrics {
                    width: text.len() as f32 * 7.0,
                    height: 16.0,
                    ascent: 12.0,
                }
            }
            fn field_line(&mut self, _: &Font) -> Option<f32> {
                Some(20.0)
            }
        }
        let field = div()
            .field(Id::named("box"), FieldKind::Lines)
            .field_rows(3)
            .w(100.0)
            .pt(4.0)
            .pb(10.0)
            .pl(3.0)
            .pr(7.0)
            .border(1.0, Role::Accent);
        let laid = layout(div().child(field), Size::new(200.0, 200.0), 1.0, &mut Ruler);
        let at = Placement {
            inset: 32.0,
            zoom: 2.0,
        };
        let fields = Outline::of(&laid, at).fields;
        // The room is 100 across and 76 down on the page: twice as large, below the strip.
        assert_eq!(fields[0].rect, Rect::new(0.0, 32.0, 200.0, 152.0));
        assert_eq!(fields[0].padding_box, Rect::new(2.0, 34.0, 196.0, 148.0));
        assert_eq!(fields[0].text, Rect::new(8.0, 42.0, 176.0, 120.0));
    }

    #[test]
    fn the_font_of_a_field_is_the_text_of_its_element_in_the_hosts_points() {
        use pencere_view::{Family, Font, Measure, Metrics, Size, div, layout};
        struct Ruler;
        impl Measure for Ruler {
            fn measure(&mut self, font: &Font, text: &str) -> Metrics {
                Metrics {
                    width: text.len() as f32 * font.size * 0.5,
                    height: font.size * 1.25,
                    ascent: font.size,
                }
            }
        }
        let field = div()
            .field(Id::named("box"), FieldKind::Line)
            .w(100.0)
            .h(30.0);
        // 15 points in the system's face, bold, on a screen of two pixels to a point with the page
        // drawn half as large again: the layout's font is 15 × 2 × 1.5 pixels, the host's is 22.5 points
        // (the page's points times the zoom), its boxes' unit.
        let root = div()
            .text_size(15.0)
            .weight(700)
            .family(Family::System)
            .child(field);
        let laid = layout(root, Size::new(200.0, 200.0), 3.0, &mut Ruler);
        let at = Placement {
            inset: 0.0,
            zoom: 1.5,
        };
        let font = Outline::of(&laid, at).fields[0].font;
        assert_eq!(
            font,
            Font {
                family: Family::System,
                size: 22.5,
                weight: 700,
                italic: false,
            }
        );
        // And in a box that says nothing, the kit's own size times the zoom.
        let plain = div().child(
            div()
                .field(Id::named("box"), FieldKind::Line)
                .w(100.0)
                .h(30.0),
        );
        let laid = layout(plain, Size::new(200.0, 200.0), 3.0, &mut Ruler);
        assert_eq!(Outline::of(&laid, at).fields[0].font.size, 12.5 * 1.5);
    }
}
