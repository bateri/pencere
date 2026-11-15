//! A selection with round corners (`selection.radius`): the shape each range makes over the rows it
//! covers is worked out by [`pencere_view::bands`], the one rule the host's terminal draws its own
//! selection by, and drawn as boxes with round corners and, at each turn inward, a small figure.

use pencere_view::bands::{Band, Corner, Part, rounded};
use pencere_view::edit::Range;
use pencere_view::{Color, Figure, Paint, Quad, Rect, path};

use super::{Row, View};

/// How far a quarter of a circle's cubic curve has its handles out, as a fraction of the radius.
const HANDLE: f32 = 0.552_284_8;

impl View {
    /// The selection of `ranges` in `ground`, each range one shape over the rows `rows` (every row
    /// in view and one beyond it, so that a row at the edge knows its neighbour), `y` the top of a
    /// row on the page, `radius` a fraction of a row's height. `fade` is the room's opacity
    /// applied to a colour.
    pub(super) fn paint_rounded_selection(
        &self,
        ranges: &[Range],
        rows: &[&Row],
        (origin_x, y): (f32, &dyn Fn(&Row) -> f32),
        (ground, clip): (Color, Rect),
        radius: f32,
        out: &mut Vec<Paint>,
    ) {
        for range in ranges.iter().filter(|range| !range.is_empty()) {
            let bands: Vec<Band> = rows
                .iter()
                .filter_map(|row| {
                    let rect =
                        self.span(row, range.start(), range.end(), true, origin_x, y(row))?;
                    // On whole pixels, so that the figures of the turns, which are drawn in whole
                    // pixels, meet the boxes' edges with no seam between them.
                    Some(Band::new(
                        rect.y.round(),
                        rect.bottom().round(),
                        rect.x.round(),
                        rect.right().round(),
                    ))
                })
                .collect();
            for part in rounded(&bands, (radius * self.row).round().max(1.0)) {
                out.push(match part {
                    Part::Body { rect, radii, .. } => Paint::Quad(Quad {
                        rect,
                        radii,
                        fill: Some(ground),
                        gradient: None,
                        border: None,
                        shadow: None,
                        clip,
                    }),
                    Part::Fillet { rect, corner, .. } => self.fillet(rect, corner, ground, clip),
                });
            }
        }
    }

    /// The inside of a turn: a square of `rect`, filled but for a quarter of a circle about the
    /// corner [`Part::Fillet`] says, drawn as a figure (the same raster in every host).
    fn fillet(&self, rect: Rect, corner: Corner, ground: Color, clip: Rect) -> Paint {
        let s = self.scale.max(0.01);
        let side = rect.w.min(rect.h).floor();
        let rect = Rect::new(rect.x, rect.y, side, side);
        let r = side / s;
        let (right, bottom) = match corner {
            Corner::TopLeft => (false, false),
            Corner::TopRight => (true, false),
            Corner::BottomRight => (true, true),
            Corner::BottomLeft => (false, true),
        };
        // Worked out for the top left corner, where the circle is about the square's bottom left,
        // and turned about the square's middle for the others.
        let at = |x: f32, y: f32| {
            (
                if right { r - x } else { x },
                if bottom { r - y } else { y },
            )
        };
        let (a, b, c) = (at(0.0, 0.0), at(r, 0.0), at(r, r));
        let (h1, h2) = (at(r, r - HANDLE * r), at(HANDLE * r, 0.0));
        let shape = path()
            .move_to(a.0, a.1)
            .line_to(b.0, b.1)
            .line_to(c.0, c.1)
            .cubic_to(h1.0, h1.1, h2.0, h2.1, a.0, a.1)
            .close()
            .fill(ground);
        Paint::Figure(Figure {
            rect,
            scale: s,
            paths: vec![shape],
            clip: clip.intersect(rect),
        })
    }
}
