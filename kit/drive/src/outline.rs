//! Where a page stands in the host's view, and what the host keeps of it besides its pixels: the
//! fields it puts over it, what a screen reader says of it, its tooltips and its commands.

use pencere_view::{Accessible, Command, FieldKind, Id, Laid, Point, Rect};

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

/// The size of the kit's text where a page does not say one (`pencere-view`'s default, 12.5
/// points): a host whose text is this size draws a page at its own size.
pub const ZOOM_TEXT: f32 = 12.5;

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

    /// A point of the host's as the page's.
    pub fn to_page(self, at: Point) -> Point {
        Point::new(at.x / self.zoom, (at.y - self.inset) / self.zoom)
    }
}

/// A text field the host puts over a page: its name (the id of the element that is its room), its
/// kind, where its room stands in the host's points, what it says while it is empty — the words
/// the page put in that room — and what it is to a screen reader.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub id: Id,
    pub kind: FieldKind,
    pub rect: Rect,
    pub hint: Option<String>,
    pub label: Option<String>,
}

/// What the host keeps of a page besides its pixels, in the host's points: the fields it puts
/// over it and what a screen reader says of it. A frame says when it changed
/// ([`crate::Painted::outline`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub fields: Vec<Field>,
    pub accessible: Vec<Accessible>,
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
    /// The outline of `laid`, placed in the host's view as `at` says.
    pub fn of(laid: &Laid, at: Placement) -> Self {
        Self {
            fields: laid
                .fields()
                .into_iter()
                .map(|field| Field {
                    id: field.id,
                    kind: field.kind,
                    rect: at.to_host(field.rect),
                    hint: field.hint,
                    label: field.label,
                })
                .collect(),
            accessible: laid
                .accessible()
                .into_iter()
                .map(|item| Accessible {
                    rect: at.to_host(item.rect),
                    ..item
                })
                .collect(),
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
}
