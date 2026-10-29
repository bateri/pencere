//! The guards of the shape of a selection: the cases of a terminal's cells (a lone band rounds its
//! four corners, equal bands are flush, a step is filled once by the narrower band, bands that touch
//! at a point stay two, a missing line splits the shape) and the cases only a free measure has
//! (edges a hair apart are one, a radius no band or step can take is held).

use crate::Rect;
use crate::bands::{Band, Corner, LEVEL, Part, rounded};

const CW: f32 = 10.0;
const CH: f32 = 20.0;

/// The band of cells `first..=last` on `row` of a grid of 10 by 20.
fn run(row: u32, first: u32, last: u32) -> Band {
    Band::new(
        row as f32 * CH,
        row as f32 * CH + CH,
        first as f32 * CW,
        (last + 1) as f32 * CW,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Open,
    Flush,
    Step,
}

/// How each corner of band `band` came out, top left, top right, bottom right, bottom left.
fn kinds(parts: &[Part], band: usize) -> [Kind; 4] {
    let order = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomRight,
        Corner::BottomLeft,
    ];
    let radii = parts
        .iter()
        .find_map(|part| match part {
            Part::Body {
                band: of, radii, ..
            } if *of == band => Some(*radii),
            _ => None,
        })
        .expect("a body for the band");
    let mut out = [Kind::Flush; 4];
    for (at, corner) in order.into_iter().enumerate() {
        let filled = parts.iter().any(|part| {
            matches!(part, Part::Fillet { band: of, corner: c, .. } if *of == band && *c == corner)
        });
        out[at] = if radii[at] > 0.0 {
            Kind::Open
        } else if filled {
            Kind::Step
        } else {
            Kind::Flush
        };
    }
    out
}

fn fillets(parts: &[Part]) -> Vec<(usize, Corner, Rect)> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Fillet { band, rect, corner } => Some((*band, *corner, *rect)),
            Part::Body { .. } => None,
        })
        .collect()
}

#[test]
fn a_lone_band_rounds_all_four_corners() {
    let parts = rounded(&[run(2, 3, 9)], 4.0);
    assert_eq!(kinds(&parts, 0), [Kind::Open; 4]);
    assert_eq!(parts.len(), 1);
    assert_eq!(
        parts[0],
        Part::Body {
            band: 0,
            rect: Rect::new(30.0, 40.0, 70.0, 20.0),
            radii: [4.0; 4],
        }
    );
}

#[test]
fn two_equal_bands_are_flush_where_they_meet() {
    use Kind::{Flush, Open};
    let parts = rounded(&[run(3, 2, 9), run(4, 2, 9)], 4.0);
    assert_eq!(kinds(&parts, 0), [Open, Open, Flush, Flush]);
    assert_eq!(kinds(&parts, 1), [Flush, Flush, Open, Open]);
    assert!(fillets(&parts).is_empty());
}

#[test]
fn a_step_is_filled_once_by_the_narrower_band() {
    use Kind::{Flush, Open, Step};
    // The state of a selection in a flow of text: the first line from the 4th column to the end,
    // the next from its start. On the left the lower is longer, so the upper's bottom left is a
    // step; the right edges are one.
    let parts = rounded(&[run(3, 4, 9), run(4, 0, 9)], 4.0);
    assert_eq!(kinds(&parts, 0), [Open, Open, Flush, Step]);
    assert_eq!(kinds(&parts, 1), [Open, Flush, Open, Open]);
    // The fillet is outside the upper band, beside its left edge, in its own line.
    assert_eq!(
        fillets(&parts),
        vec![(0, Corner::BottomLeft, Rect::new(36.0, 76.0, 4.0, 4.0))]
    );

    // The reverse: the lower is the narrow one, to the right of the upper's start.
    let parts = rounded(&[run(3, 0, 9), run(4, 0, 5)], 4.0);
    assert_eq!(kinds(&parts, 0), [Open, Open, Open, Flush]);
    assert_eq!(kinds(&parts, 1), [Flush, Step, Open, Open]);
    assert_eq!(
        fillets(&parts),
        vec![(1, Corner::TopRight, Rect::new(60.0, 80.0, 4.0, 4.0))]
    );

    // A step on both sides is two fillets, one for each.
    let parts = rounded(&[run(3, 3, 6), run(4, 0, 9)], 4.0);
    assert_eq!(fillets(&parts).len(), 2);
}

#[test]
fn bands_that_touch_at_a_point_stay_two() {
    // The upper starts at the column the lower ends before: no column is in both.
    let parts = rounded(&[run(3, 5, 9), run(4, 0, 4)], 4.0);
    assert_eq!(kinds(&parts, 0), [Kind::Open; 4]);
    assert_eq!(kinds(&parts, 1), [Kind::Open; 4]);
    assert!(fillets(&parts).is_empty());
}

#[test]
fn a_line_with_no_band_splits_the_shape() {
    let parts = rounded(&[run(3, 0, 9), run(5, 0, 9)], 4.0);
    assert_eq!(kinds(&parts, 0), [Kind::Open; 4]);
    assert_eq!(kinds(&parts, 1), [Kind::Open; 4]);
}

#[test]
fn a_gap_between_bands_splits_the_shape_as_a_missing_line_does() {
    let mut below = run(4, 0, 9);
    below.top += 8.0;
    below.bottom += 8.0;
    let parts = rounded(&[run(3, 0, 9), below], 4.0);
    assert_eq!(kinds(&parts, 0), [Kind::Open; 4]);
    assert_eq!(kinds(&parts, 1), [Kind::Open; 4]);
}

#[test]
fn a_radius_of_nothing_gives_the_bands_square() {
    for radius in [0.0, -3.0, f32::NAN, f32::NEG_INFINITY] {
        let parts = rounded(&[run(3, 4, 9), run(4, 0, 9)], radius);
        assert_eq!(parts.len(), 2);
        for part in parts {
            let Part::Body { radii, .. } = part else {
                panic!("a fillet at radius {radius}");
            };
            assert_eq!(radii, [0.0; 4]);
        }
    }
}

#[test]
fn edges_a_hair_apart_are_one() {
    use Kind::{Flush, Open};
    // The same column of two lines of a face with no grid, a fraction of a pixel apart: no step.
    let upper = run(3, 2, 9);
    let mut lower = run(4, 2, 9);
    lower.left += LEVEL * 0.9;
    lower.right -= LEVEL * 0.9;
    let parts = rounded(&[upper, lower], 4.0);
    assert_eq!(kinds(&parts, 0), [Open, Open, Flush, Flush]);
    assert_eq!(kinds(&parts, 1), [Flush, Flush, Open, Open]);
    assert!(fillets(&parts).is_empty());
    // A little more is a step.
    let mut lower = run(4, 2, 9);
    lower.left -= LEVEL * 1.5;
    let parts = rounded(&[upper, lower], 4.0);
    assert_eq!(fillets(&parts).len(), 1);
}

#[test]
fn a_radius_is_held_to_what_the_shape_can_take() {
    // A band 6 wide and 20 tall takes a radius of 3 at most.
    let parts = rounded(&[Band::new(0.0, 20.0, 0.0, 6.0)], 50.0);
    assert_eq!(
        parts,
        vec![Part::Body {
            band: 0,
            rect: Rect::new(0.0, 0.0, 6.0, 20.0),
            radii: [3.0; 4],
        }]
    );
    // A fillet takes no more than its step: 2 wide here.
    let upper = Band::new(0.0, 20.0, 8.0, 40.0);
    let lower = Band::new(20.0, 40.0, 6.0, 40.0);
    let parts = rounded(&[upper, lower], 5.0);
    assert_eq!(
        fillets(&parts),
        vec![(0, Corner::BottomLeft, Rect::new(6.0, 18.0, 2.0, 2.0))]
    );
}

#[test]
fn a_band_that_is_no_box_is_left_out_and_is_no_neighbour() {
    let nothing = Band::new(20.0, 20.0, 0.0, 50.0);
    let bad = Band::new(f32::NAN, 40.0, 0.0, 50.0);
    let parts = rounded(&[run(0, 0, 4), nothing, bad, run(3, 0, 4)], 4.0);
    assert_eq!(parts.len(), 2);
    assert_eq!(kinds(&parts, 0), [Kind::Open; 4]);
    assert_eq!(kinds(&parts, 3), [Kind::Open; 4]);
}

#[test]
fn what_is_not_a_number_never_panics() {
    let values = [
        0.0,
        1.0,
        -1.0,
        0.4999,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
    ];
    for a in values {
        for b in values {
            let bands = [
                Band::new(a, b, a, b),
                Band::new(b, a, b, a),
                Band::new(a, b, b, a),
            ];
            for radius in values {
                let _ = rounded(&bands, radius);
            }
        }
    }
}

struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// An edge: anything a float can be, or a place on a grid, or a fraction of a pixel.
    fn edge(&mut self) -> f32 {
        let pick = self.next() % 5;
        let n = self.next();
        match pick {
            0 => f32::from_bits((n >> 32) as u32),
            1 => (n % 400) as f32 / 4.0,
            2 => (n % 40) as f32 * CH,
            3 => (n % 8) as f32 * 0.25,
            _ => -((n % 100) as f32),
        }
    }
}

#[test]
fn bands_of_any_edges_are_shaped_without_a_panic() {
    let mut dice = Dice(0x2545_f491_4f6c_dd1d);
    for _ in 0..2_000 {
        let count = dice.next() % 6;
        let bands: Vec<Band> = (0..count)
            .map(|_| Band::new(dice.edge(), dice.edge(), dice.edge(), dice.edge()))
            .collect();
        let radius = dice.edge();
        for part in rounded(&bands, radius) {
            let (Part::Body { rect, .. } | Part::Fillet { rect, .. }) = part;
            assert!(rect.w > 0.0 && rect.h > 0.0, "{part:?} of {bands:?}");
        }
    }
}
