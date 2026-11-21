//! A grid of coverage blurred: how a shadow is softened, a box's in a host's picture
//! (`pencere_drive::shot`) and a path's in a canvas ([`crate::draw_figure`]), so that a deviation is
//! the same softness in both (what a `blur` of points comes to as a deviation is each one's own).

/// The room a blur works in: what a blur of a grid needs besides the grid, a copy of its cells and a
/// sum to each column. Kept, it is not asked for again by the next blur of a grid no larger.
#[derive(Default)]
pub struct Blur {
    source: Vec<f32>,
    sums: Vec<f32>,
}

impl Blur {
    /// `grid` of `size` (width, height) blurred as a Gaussian of deviation `sigma` is, over three
    /// passes of a box blur, each across and down: the box as wide as three passes of it make a
    /// Gaussian of that deviation (the width of the classic "boxes for a Gaussian":
    /// `sqrt(12 sigma^2 / 3 + 1)`, odd). What is past an edge of the grid is nothing.
    pub fn coverage(&mut self, grid: &mut [f32], (w, h): (usize, usize), sigma: f32) {
        if w == 0 || h == 0 || grid.len() < w * h {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let wide = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt().floor() as usize) | 1;
        let radius = wide / 2;
        self.sums.clear();
        self.sums.resize(w, 0.0);
        for _ in 0..3 {
            across(grid, w, radius, &mut self.source);
            down(grid, (w, h), radius, (&mut self.source, &mut self.sums));
        }
    }
}

/// Each row of `grid`, `w` wide, taken to the mean of the cells `radius` either side of each.
/// `source` is room for a row's own cells while it is made.
fn across(grid: &mut [f32], w: usize, radius: usize, source: &mut Vec<f32>) {
    #[allow(clippy::cast_precision_loss)]
    let scale = 1.0 / (2 * radius + 1) as f32;
    for row in grid.chunks_exact_mut(w) {
        source.clear();
        source.extend_from_slice(row);
        let mut sum: f32 = source.iter().take(radius + 1).sum();
        for (at, out) in row.iter_mut().enumerate() {
            *out = sum * scale;
            if let Some(gone) = at.checked_sub(radius).and_then(|i| source.get(i)) {
                sum -= gone;
            }
            if let Some(came) = source.get(at + radius + 1) {
                sum += came;
            }
        }
    }
}

/// Each column of `grid`, `w` wide and `h` high, taken to the mean of the cells `radius` either
/// side of each: every column at once, row by row, each with its own running sum (so that each is
/// summed in the order it would be by itself). `source` is room for the grid's own cells and `sums`
/// for a sum to each column.
fn down(
    grid: &mut [f32],
    (w, h): (usize, usize),
    radius: usize,
    (source, sums): (&mut Vec<f32>, &mut [f32]),
) {
    #[allow(clippy::cast_precision_loss)]
    let scale = 1.0 / (2 * radius + 1) as f32;
    source.clear();
    source.extend_from_slice(grid.get(..w * h).unwrap_or(&[]));
    sums.fill(0.0);
    for row in source.chunks_exact(w).take(radius + 1) {
        for (sum, cell) in sums.iter_mut().zip(row) {
            *sum += cell;
        }
    }
    for (at, out) in grid.chunks_exact_mut(w).take(h).enumerate() {
        for (cell, sum) in out.iter_mut().zip(sums.iter()) {
            *cell = sum * scale;
        }
        let row_at = |row: usize| source.get(row * w..(row + 1) * w);
        if let Some(gone) = at.checked_sub(radius).and_then(row_at) {
            for (sum, cell) in sums.iter_mut().zip(gone) {
                *sum -= cell;
            }
        }
        if let Some(came) = row_at(at + radius + 1) {
            for (sum, cell) in sums.iter_mut().zip(came) {
                *sum += cell;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The blur as it was first written, a box shadow's in a host's picture, kept as it was: what
    /// `Blur::coverage` does is what three passes of it did, cell for cell.
    mod first {
        /// One pass of a box blur across and down a grid of `size` (width, height), the box as wide as three
        /// passes of it make a Gaussian of deviation `sigma` (the width of the classic "boxes for a
        /// Gaussian": `sqrt(12 sigma^2 / 3 + 1)`, odd).
        pub fn pass(grid: &mut [f32], (w, h): (usize, usize), sigma: f32) {
            let wide = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt().floor() as usize) | 1;
            let radius = wide / 2;
            let mut line = vec![0.0_f32; w.max(h)];
            for row in 0..h {
                for (column, slot) in line.iter_mut().take(w).enumerate() {
                    *slot = grid.get(row * w + column).copied().unwrap_or(0.0);
                }
                smooth(&mut line, w, radius);
                for (column, value) in line.iter().take(w).enumerate() {
                    if let Some(cell) = grid.get_mut(row * w + column) {
                        *cell = *value;
                    }
                }
            }
            for column in 0..w {
                for (row, slot) in line.iter_mut().take(h).enumerate() {
                    *slot = grid.get(row * w + column).copied().unwrap_or(0.0);
                }
                smooth(&mut line, h, radius);
                for (row, value) in line.iter().take(h).enumerate() {
                    if let Some(cell) = grid.get_mut(row * w + column) {
                        *cell = *value;
                    }
                }
            }
        }

        /// The first `n` of `line`, each the mean of the ones `radius` either side (what is past an end is
        /// nothing).
        fn smooth(line: &mut [f32], n: usize, radius: usize) {
            let source: Vec<f32> = line.iter().take(n).copied().collect();
            let scale = 1.0 / (2 * radius + 1) as f32;
            let mut sum: f32 = source.iter().take(radius + 1).sum();
            for at in 0..n {
                if let Some(out) = line.get_mut(at) {
                    *out = sum * scale;
                }
                if let Some(gone) = at.checked_sub(radius).and_then(|i| source.get(i)) {
                    sum -= gone;
                }
                if let Some(came) = source.get(at + radius + 1) {
                    sum += came;
                }
            }
        }
    }

    fn grid(w: usize, h: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;
        (0..w * h)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if state >> 28 < 5 {
                    f32::from((state >> 8) as u8) / 255.0
                } else {
                    0.0
                }
            })
            .collect()
    }

    #[test]
    fn the_blur_is_what_three_passes_of_the_first_one_made_of_a_grid_cell_for_cell() {
        for (w, h, sigma) in [
            (1, 1, 3.0),
            (7, 5, 0.6),
            (30, 20, 1.5),
            (64, 33, 6.0),
            (9, 70, 12.0),
            (114, 114, 16.0),
            (5, 5, 40.0),
        ] {
            let mut now = grid(w, h, 7 + w as u32);
            let mut then = now.clone();
            Blur::default().coverage(&mut now, (w, h), sigma);
            for _ in 0..3 {
                first::pass(&mut then, (w, h), sigma);
            }
            let same = now
                .iter()
                .zip(&then)
                .all(|(a, b)| a.to_bits() == b.to_bits());
            assert!(same, "{w} x {h} at {sigma}");
        }
    }

    #[test]
    fn a_blur_leaves_what_it_is_given_as_much_as_it_had_and_spreads_it() {
        let mut cells = vec![0.0_f32; 21 * 21];
        cells[10 * 21 + 10] = 1.0;
        Blur::default().coverage(&mut cells, (21, 21), 2.0);
        let total: f32 = cells.iter().sum();
        assert!((total - 1.0).abs() < 1e-3, "{total}");
        assert!(cells[10 * 21 + 10] < 0.5 && cells[10 * 21 + 13] > 0.0);
    }
}
