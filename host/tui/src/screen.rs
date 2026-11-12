//! The frames of a live terminal: only the cells that changed are written.

use std::collections::HashMap;
use std::fmt::Write;

use crate::grid::{Cell, Grid};
use crate::output::{self, Depth, Name, Placement, Sgr};
use crate::paint::{Frame, Link};

/// How many unchanged cells between two changed ones are written over rather than skipped: moving
/// the cursor costs about that many bytes.
const GAP: usize = 4;

/// What the terminal shows, so that the next frame is written as the difference.
#[derive(Debug)]
pub struct Screen {
    depth: Depth,
    shown: Option<Grid>,
    placed: Vec<Put>,
    /// The pixels the terminal keeps, and the numbers it keeps them under.
    stored: Store,
    /// The links the terminal was told of: a row that has one is written whole when it changes, so
    /// that every cell of it is written under the link and none is left under one it has not.
    linked: Vec<Link>,
    /// Where the cursor was left, once it is known (`Some(None)`: hidden).
    cursor: Option<Option<(u16, u16)>>,
}

impl Screen {
    /// A screen whose contents are not known: the first frame draws all of it.
    #[must_use]
    pub fn new(depth: Depth) -> Self {
        Self {
            depth,
            shown: None,
            placed: Vec::new(),
            stored: Store::new(),
            linked: Vec::new(),
            cursor: None,
        }
    }

    /// Whether the terminal was sent pictures it may still keep.
    #[must_use]
    pub fn holds_pictures(&self) -> bool {
        self.stored.used
    }

    /// What the terminal shows is not what was written (it was resized, or another program had it):
    /// the next frame draws all of it.
    pub fn forget(&mut self) {
        self.shown = None;
        self.placed.clear();
        // What the terminal keeps of pictures is not known to be there: all of it is given up with
        // the next frame, and the pixels are sent again when they are wanted.
        self.stored.forget();
        self.linked.clear();
        self.cursor = None;
    }

    /// What is to be written for `frame` (and the cursor at `cursor`, a cell from 0, or hidden):
    /// nothing where the terminal shows it already.
    #[must_use]
    pub fn draw(&mut self, frame: &Frame, cursor: Option<(u16, u16)>) -> Vec<u8> {
        let now = &frame.grid;
        let before = self
            .shown
            .take()
            .filter(|grid| grid.cols() == now.cols() && grid.rows() == now.rows());
        let mut body = String::new();
        let mut state = Sgr::new(self.depth);
        let links_changed = frame.links != self.linked;
        for (row, cells) in now.rows_of_cells().enumerate() {
            let at = u16::try_from(row).unwrap_or(0);
            let mut old = before
                .as_ref()
                .and_then(|grid| grid.rows_of_cells().nth(row));
            let links: Vec<&Link> = frame.links.iter().filter(|link| link.row == at).collect();
            let was_linked = self.linked.iter().any(|link| link.row == at);
            if (!links.is_empty() || was_linked) && (links_changed || old != Some(cells)) {
                old = None;
            }
            spans(&mut body, &mut state, at, cells, old, &links);
        }
        let layers = layers(&frame.placements);
        let pictures = self.stored.wipe
            || frame.placements.len() != self.placed.len()
            || frame
                .placements
                .iter()
                .zip(&layers)
                .zip(&self.placed)
                .any(|((placement, z), put)| put.placement != *placement || put.z != *z);
        let moved = self.cursor != Some(cursor);
        self.shown = Some(now.clone());
        self.cursor = Some(cursor);
        self.linked.clone_from(&frame.links);
        if before.is_some() && body.is_empty() && !pictures && !moved {
            return Vec::new();
        }
        // Hidden while it is drawn, so that it does not flicker over the cells it passes.
        let mut out = String::from("\x1b[?25l");
        if before.is_none() {
            out.push_str("\x1b[0m\x1b[2J");
        }
        out.push_str(&body);
        out.push_str("\x1b[0m");
        if pictures {
            self.pictures(&mut out, &frame.placements, &layers);
        }
        if let Some((col, row)) = cursor {
            let _ = write!(out, "\x1b[{};{}H\x1b[?25h", row + 1, col + 1);
        }
        out.into_bytes()
    }
}

/// The most pixels the terminal is asked to keep: Kitty's own limit is 320 MiB for a screen, and
/// what is kept for pictures that are not shown is given up, the least lately shown first, before
/// this is passed.
const KEPT_BYTES: usize = 128 * 1024 * 1024;

/// The first number a picture is kept under: the terminal's questions use small ones.
const FIRST_ID: u32 = 100;

/// A picture the terminal was told to put on the screen.
#[derive(Debug)]
struct Put {
    placement: Placement,
    /// The layer it is in on the terminal (see [`layers`]).
    z: i32,
    /// The number of the pixels it shows, and its own.
    id: u32,
    number: u32,
}

/// The pixels the terminal keeps, by what names them.
#[derive(Debug)]
struct Store {
    kept: HashMap<Name, Kept>,
    /// How many bytes of pixels the terminal is asked to keep ([`KEPT_BYTES`]).
    limit: usize,
    /// The next number, from [`FIRST_ID`], and the next placement's.
    next: u32,
    number: u32,
    bytes: usize,
    /// Counts the frames that put pictures, for what was shown last.
    tick: u64,
    /// Something was sent that the terminal may still keep.
    used: bool,
    /// The terminal is to be told to give all of it up before the next picture.
    wipe: bool,
}

#[derive(Debug)]
struct Kept {
    id: u32,
    bytes: usize,
    shown: u64,
}

impl Store {
    fn new() -> Self {
        Self {
            kept: HashMap::new(),
            limit: KEPT_BYTES,
            next: 0,
            number: 0,
            bytes: 0,
            tick: 0,
            used: false,
            wipe: false,
        }
    }

    fn forget(&mut self) {
        self.wipe = self.used;
        self.kept.clear();
        self.bytes = 0;
        self.used = false;
    }

    fn id(&mut self) -> u32 {
        self.next = self.next.max(FIRST_ID - 1) + 1;
        self.next
    }

    fn number(&mut self) -> u32 {
        self.number += 1;
        self.number
    }
}

/// The layers pictures are put in on the terminal, for `placements` in the order they were drawn:
/// the terminal draws those of one layer in the order of the numbers it keeps them under, which are
/// not the order they were drawn in once the pixels are kept, so that each placement gets a layer of
/// its own, a step above the one before it, as high as its own layer was for the last.
fn layers(placements: &[Placement]) -> Vec<i32> {
    let mut later: HashMap<i32, i32> = HashMap::new();
    let mut layers: Vec<i32> = placements
        .iter()
        .rev()
        .map(|placement| {
            let after = later.entry(placement.z).or_insert(0);
            let z = placement.z.saturating_sub(*after);
            *after += 1;
            z
        })
        .collect();
    layers.reverse();
    layers
}

impl Screen {
    /// The pictures of a frame put on the terminal: what is on it still is left, what is not wanted
    /// any more is taken off, and what is new is put (the pixels first sent where the terminal does
    /// not keep them yet).
    fn pictures(&mut self, out: &mut String, placements: &[Placement], layers: &[i32]) {
        if self.stored.wipe {
            out.push_str(output::FREE_ALL);
            self.stored.wipe = false;
            self.placed.clear();
        }
        // Which of what is put is wanted as it is.
        let mut old: Vec<Option<Put>> = std::mem::take(&mut self.placed)
            .into_iter()
            .map(Some)
            .collect();
        let mut kept: Vec<Put> = Vec::with_capacity(placements.len());
        let mut fresh: Vec<(usize, &Placement, i32)> = Vec::new();
        for (at, (placement, z)) in placements.iter().zip(layers).enumerate() {
            let same = old.iter_mut().find(|put| {
                put.as_ref()
                    .is_some_and(|put| put.placement == *placement && put.z == *z)
            });
            match same.and_then(Option::take) {
                Some(put) => kept.push(put),
                None => fresh.push((at, placement, *z)),
            }
        }
        for put in old.into_iter().flatten() {
            output::lift(out, put.id, put.number);
        }
        self.stored.tick += 1;
        let tick = self.stored.tick;
        for put in &kept {
            if let Some(kept) = self.stored.kept.get_mut(&put.placement.pixels.name()) {
                kept.shown = tick;
            }
        }
        // What is wanted is not given up to make room for what is new.
        let wanted: Vec<Name> = placements
            .iter()
            .map(|placement| placement.pixels.name())
            .collect();
        let mut put: Vec<(usize, Put)> = Vec::new();
        for (at, placement, z) in fresh {
            let name = placement.pixels.name();
            let id = match self.stored.kept.get_mut(&name) {
                Some(kept) => {
                    kept.shown = tick;
                    kept.id
                }
                None => {
                    let bytes = placement.pixels.bytes();
                    self.make_room(out, bytes, &wanted);
                    let id = self.stored.id();
                    output::store(out, id, &placement.pixels);
                    self.stored.bytes += bytes;
                    self.stored.used = true;
                    self.stored.kept.insert(
                        name,
                        Kept {
                            id,
                            bytes,
                            shown: tick,
                        },
                    );
                    id
                }
            };
            let number = self.stored.number();
            let _ = write!(out, "\x1b[{};{}H", placement.row + 1, placement.col + 1);
            output::put(out, id, number, placement, z);
            put.push((
                at,
                Put {
                    placement: placement.clone(),
                    z,
                    id,
                    number,
                },
            ));
        }
        // In the order they were drawn.
        let mut put = put.into_iter();
        let mut next = put.next();
        let mut kept = kept.into_iter();
        let mut next_kept = kept.next();
        let mut placed = Vec::with_capacity(placements.len());
        for at in 0..placements.len() {
            if next.as_ref().is_some_and(|(drawn, _)| *drawn == at) {
                if let Some((_, fresh)) = next.take() {
                    placed.push(fresh);
                }
                next = put.next();
            } else if let Some(old) = next_kept.take() {
                placed.push(old);
                next_kept = kept.next();
            }
        }
        self.placed = placed;
    }

    /// Gives up pixels the terminal keeps, those shown least lately first and none that are wanted,
    /// until `bytes` more fit.
    fn make_room(&mut self, out: &mut String, bytes: usize, wanted: &[Name]) {
        while self.stored.bytes + bytes > self.stored.limit {
            let oldest = self
                .stored
                .kept
                .iter()
                .filter(|(name, _)| !wanted.contains(name))
                .min_by_key(|(_, kept)| kept.shown)
                .map(|(name, kept)| (*name, kept.id, kept.bytes));
            let Some((name, id, size)) = oldest else {
                return;
            };
            output::free(out, id);
            self.stored.kept.remove(&name);
            self.stored.bytes = self.stored.bytes.saturating_sub(size);
        }
    }
}

/// The cells of `row` that differ from `old` (all, where there is none), as runs.
fn spans(
    out: &mut String,
    state: &mut Sgr,
    row: u16,
    now: &[Cell],
    old: Option<&[Cell]>,
    links: &[&Link],
) {
    let same = |at: usize| match (old, now.get(at)) {
        (Some(old), Some(cell)) => old.get(at) == Some(cell),
        _ => false,
    };
    let mut at = 0;
    while at < now.len() {
        if same(at) {
            at += 1;
            continue;
        }
        let mut start = at;
        // A change in the right half of a wide character is the character's.
        if now.get(start).is_some_and(|cell| cell.tail) {
            start = start.saturating_sub(1);
        }
        let mut end = at;
        let mut next = at + 1;
        while next < now.len() {
            if !same(next) {
                end = next;
                next += 1;
                continue;
            }
            let ahead = (next..now.len().min(next + GAP)).find(|index| !same(*index));
            match ahead {
                Some(index) => {
                    end = index;
                    next = index + 1;
                }
                None => break,
            }
        }
        let _ = write!(out, "\x1b[{};{}H", row + 1, start + 1);
        for (offset, cell) in now.get(start..=end).unwrap_or_default().iter().enumerate() {
            let index = start + offset;
            let column = u16::try_from(index).unwrap_or(u16::MAX);
            if !cell.tail {
                if let Some(link) = links.iter().find(|link| link.from == column) {
                    // The cell's own attributes are set first: the link is what the cells written
                    // while it is open belong to.
                    state.set(out, cell);
                    let _ = write!(out, "\x1b]8;;{}\x1b\\", link.url);
                }
                state.set(out, cell);
                out.push(cell.ch);
            }
            if links.iter().any(|link| link.to == column + 1) {
                out.push_str("\x1b]8;;\x1b\\");
            }
        }
        at = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, Theme, render};
    use pencere_view::DrawList;

    fn frame(cols: u16, rows: u16) -> Frame {
        render(
            &DrawList::default(),
            cols,
            rows,
            &Theme::dark(),
            &crate::NoPictures,
            &Options::default(),
        )
    }

    #[test]
    fn a_frame_that_is_the_same_writes_nothing() {
        let mut screen = Screen::new(Depth::Indexed);
        let one = frame(10, 3);
        let first = screen.draw(&one, Some((1, 1)));
        assert!(first.len() > 30);
        assert!(screen.draw(&one, Some((1, 1))).is_empty());
        // The cursor alone moved: it is all that is written.
        let moved = String::from_utf8(screen.draw(&one, Some((2, 1)))).unwrap();
        assert!(moved.ends_with("\x1b[2;3H\x1b[?25h") && !moved.contains("\x1b[2J"));
    }

    #[test]
    fn a_new_size_clears_and_draws_all() {
        let mut screen = Screen::new(Depth::True);
        let _ = screen.draw(&frame(10, 3), None);
        let bytes = String::from_utf8(screen.draw(&frame(12, 3), None)).unwrap();
        assert!(bytes.contains("\x1b[2J"));
    }

    #[test]
    fn a_colour_is_one_of_256_where_the_terminal_has_no_more() {
        let mut screen = Screen::new(Depth::Indexed);
        let bytes = String::from_utf8(screen.draw(&frame(2, 1), Some((1, 0)))).unwrap();
        assert!(bytes.contains("48;5;"), "{bytes:?}");
        assert!(!bytes.contains("38;2;") && !bytes.contains("48;2;"));
        assert!(bytes.ends_with("\x1b[1;2H\x1b[?25h"));
    }

    #[test]
    fn a_link_is_written_over_the_cells_it_is_and_the_row_again_when_it_goes() {
        let mut screen = Screen::new(Depth::True);
        let mut linked = frame(12, 3);
        linked.links.push(Link {
            row: 1,
            from: 2,
            to: 7,
            url: "https://example.org/a".to_owned(),
        });
        let first = String::from_utf8(screen.draw(&linked, None)).unwrap();
        let open = first
            .find("\x1b]8;;https://example.org/a\x1b\\")
            .expect("the link opens");
        let close = first.rfind("\x1b]8;;\x1b\\").expect("the link closes");
        assert!(open < close);
        assert!(
            screen.draw(&linked, None).is_empty(),
            "the same frame writes nothing"
        );
        // The link goes and the cells are the same: the row is written again, without it.
        let plain = frame(12, 3);
        let again = String::from_utf8(screen.draw(&plain, None)).unwrap();
        assert!(again.contains("\x1b[2;1H"), "{again:?}");
        assert!(!again.contains("\x1b]8;;https"), "{again:?}");
    }

    /// A frame with a picture of `size` pixels across at the cell (`col`, `row`), named `id`.
    fn picture(id: u64, size: u32, col: u16, row: u16, z: i32) -> Placement {
        let raster = std::sync::Arc::new(crate::Raster {
            id: Some(id),
            width: size,
            height: size,
            pixels: vec![200; (4 * size * size) as usize],
            crop: pencere_view::Rect::new(0.0, 0.0, 1.0, 1.0),
        });
        Placement {
            col,
            row,
            cols: 4,
            rows: 2,
            pixels: crate::output::Pixels::of(raster, None, 1.0),
            x: 0,
            y: 2,
            width: size,
            height: size - 2,
            z,
        }
    }

    fn with(placements: Vec<Placement>) -> Frame {
        let mut with = frame(20, 10);
        with.placements = placements;
        with
    }

    fn written(screen: &mut Screen, frame: &Frame) -> String {
        String::from_utf8(screen.draw(frame, None)).unwrap()
    }

    #[test]
    fn a_picture_is_sent_once_and_put_again_where_it_moves() {
        let mut screen = Screen::new(Depth::True);
        let first = written(&mut screen, &with(vec![picture(7, 4, 2, 1, -1)]));
        assert_eq!(first.matches("\x1b_Ga=t,").count(), 1);
        assert_eq!(first.matches("\x1b_Ga=p,").count(), 1);
        // A part of the pixels, which the terminal crops.
        assert!(first.contains(",x=0,y=2,w=4,h=2"), "{first:?}");
        // The same: nothing.
        assert!(
            screen
                .draw(&with(vec![picture(7, 4, 2, 1, -1)]), None)
                .is_empty()
        );
        // Moved: the old placement is taken off and a new one put, with no pixels.
        let moved = written(&mut screen, &with(vec![picture(7, 4, 2, 0, -1)]));
        assert_eq!(moved.matches("\x1b_Ga=t,").count(), 0, "{moved:?}");
        assert_eq!(moved.matches("\x1b_Ga=d,d=i,").count(), 1);
        assert_eq!(moved.matches("\x1b_Ga=p,").count(), 1);
        assert!(!moved.contains("d=I"));
        // Gone: taken off, and the pixels are kept for when it is back.
        let gone = written(&mut screen, &with(Vec::new()));
        assert_eq!(gone.matches("\x1b_Ga=d,d=i,").count(), 1);
        assert!(!gone.contains("d=I") && !gone.contains("d=A"));
        let back = written(&mut screen, &with(vec![picture(7, 4, 3, 3, -1)]));
        assert_eq!(back.matches("\x1b_Ga=t,").count(), 0);
        assert_eq!(back.matches("\x1b_Ga=p,").count(), 1);
    }

    #[test]
    fn pictures_that_overlap_in_a_layer_stay_in_the_order_they_were_drawn() {
        let mut screen = Screen::new(Depth::True);
        let both = written(
            &mut screen,
            &with(vec![picture(1, 4, 0, 0, -1), picture(2, 4, 0, 0, -1)]),
        );
        let layers: Vec<i32> = both
            .split("a=p,")
            .skip(1)
            .map(|put| {
                let z = put.split(",z=").nth(1).unwrap();
                z.split(',').next().unwrap().parse().unwrap()
            })
            .collect();
        assert_eq!(layers.len(), 2);
        assert!(layers[0] < layers[1], "{layers:?}");
        assert!(layers.iter().all(|z| *z < 0));
    }

    #[test]
    fn what_the_terminal_keeps_is_all_given_up_when_it_is_not_known_to_be_there_and_sent_again() {
        let mut screen = Screen::new(Depth::True);
        let _ = written(&mut screen, &with(vec![picture(7, 4, 2, 1, -1)]));
        screen.forget();
        let again = written(&mut screen, &with(vec![picture(7, 4, 2, 1, -1)]));
        assert!(again.contains("\x1b_Ga=d,d=A,"), "{again:?}");
        assert_eq!(again.matches("\x1b_Ga=t,").count(), 1);
        // Nothing was ever sent: nothing is said to a terminal that may not know the protocol.
        let mut plain = Screen::new(Depth::True);
        let _ = written(&mut plain, &with(Vec::new()));
        plain.forget();
        assert!(!written(&mut plain, &with(Vec::new())).contains("a=d"));
    }

    #[test]
    fn pixels_that_are_not_shown_are_given_up_the_least_lately_shown_first_to_make_room() {
        let mut screen = Screen::new(Depth::True);
        // Room for two pictures of 4 x 4 pixels.
        screen.stored.limit = 2 * 4 * 4 * 4;
        let _ = written(&mut screen, &with(vec![picture(1, 4, 0, 0, -1)]));
        let _ = written(&mut screen, &with(vec![picture(2, 4, 0, 0, -1)]));
        let _ = written(&mut screen, &with(vec![picture(2, 4, 0, 0, -1)]));
        // The third needs room: the first, which was shown least lately, is given up.
        let third = written(&mut screen, &with(vec![picture(3, 4, 0, 0, -1)]));
        assert_eq!(third.matches("\x1b_Ga=d,d=I,").count(), 1, "{third:?}");
        assert_eq!(third.matches("\x1b_Ga=t,").count(), 1);
        // The second is still kept: put, not sent.
        let second = written(&mut screen, &with(vec![picture(2, 4, 0, 0, -1)]));
        assert_eq!(second.matches("\x1b_Ga=t,").count(), 0, "{second:?}");
        // What is shown is not given up for what is new, though there is no room.
        let both = written(
            &mut screen,
            &with(vec![picture(2, 4, 0, 0, -1), picture(4, 4, 5, 0, -1)]),
        );
        assert_eq!(both.matches("\x1b_Ga=t,").count(), 1);
        assert!(!both.contains(&format!("d=I,i={}", 101)), "{both:?}");
    }
}
