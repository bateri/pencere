//! What a terminal shows of what it is written: the pictures it keeps and puts, over and under the
//! grounds of its cells, as the pixels a person sees. For the tests of a page and of this crate, which
//! say where a picture stands by looking at the colour of a pixel; no terminal needs it.
//!
//! It reads what [`crate::Screen`] writes (the Kitty graphics protocol: the pixels sent, packed or
//! not, the placements put and lifted) and puts it together with the grounds of the frame's cells,
//! in the layers the protocol has: under the grounds of the cells that have one, over them and under
//! the text, over the text.

use std::collections::HashMap;

use crate::color::Rgb;
use crate::grid::Bg;
use crate::paint::Frame;

/// The layer from which a picture is over the grounds of the cells.
const OVER_GROUNDS: i32 = -1_073_741_824;

/// Pixels a terminal was sent to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Load {
    /// The number they are kept under.
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// They were sent packed (`o=z`).
    pub packed: bool,
}

struct Stored {
    width: u32,
    height: u32,
    /// Straight (not premultiplied) RGBA, rows from the top.
    rgba: Vec<u8>,
}

/// A picture put on the screen.
#[derive(Clone)]
struct Put {
    source: Source,
    number: u32,
    /// The cell the cursor was in.
    at: (u32, u32),
    inside: (u32, u32),
    part: Option<(u32, u32, u32, u32)>,
    over: Option<(u32, u32)>,
    z: i32,
}

/// Where the pixels of a picture put on the screen are kept.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    /// Under a number the terminal was given.
    Kept(u32),
    /// With the sequence that put them.
    Inline(usize),
}

struct Pending {
    keys: HashMap<String, String>,
    data: String,
}

/// The screen of a terminal that has the graphics protocol: the pictures it was sent and where it
/// was told to put them.
pub struct Terminal {
    cell: (u32, u32),
    ground: Rgb,
    images: HashMap<u32, Stored>,
    puts: Vec<Put>,
    cursor: (u32, u32),
    pending: Option<Pending>,
    loads: Vec<Load>,
    /// Pictures sent in the escape sequence that puts them (`a=T`), which are kept under no number.
    anonymous: Vec<Stored>,
}

/// What a terminal shows: pixels, rows from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    rgb: Vec<u8>,
}

impl Image {
    /// The colour of the pixel `x`, `y`.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> Rgb {
        let at = ((y * self.width + x) * 3) as usize;
        match self.rgb.get(at..at + 3) {
            Some(&[r, g, b]) => Rgb::new(r, g, b),
            _ => Rgb::new(0, 0, 0),
        }
    }

    /// The image as a PPM file, which an image program reads.
    #[must_use]
    pub fn ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.rgb);
        out
    }
}

impl Terminal {
    /// A terminal whose cells are `cell` pixels across and down, and whose own ground (where no cell
    /// and no picture has one) is `ground`.
    #[must_use]
    pub fn new(cell: (u32, u32), ground: Rgb) -> Self {
        Self {
            cell,
            ground,
            images: HashMap::new(),
            puts: Vec::new(),
            cursor: (0, 0),
            pending: None,
            loads: Vec::new(),
            anonymous: Vec::new(),
        }
    }

    /// The pixels the terminal was sent so far, in the order they were.
    #[must_use]
    pub fn loads(&self) -> &[Load] {
        &self.loads
    }

    /// How many pictures are on the screen.
    #[must_use]
    pub fn on_screen(&self) -> usize {
        self.puts.len()
    }

    /// Takes what is written to the terminal.
    pub fn feed(&mut self, stream: &[u8]) {
        let mut at = 0;
        while at < stream.len() {
            if stream.get(at) != Some(&0x1b) {
                at += 1;
                continue;
            }
            match stream.get(at + 1) {
                Some(b'[') => {
                    let start = at + 2;
                    let end = stream
                        .get(start..)
                        .and_then(|rest| rest.iter().position(|b| (0x40..=0x7e).contains(b)))
                        .map_or(stream.len(), |n| start + n);
                    let params = String::from_utf8_lossy(stream.get(start..end).unwrap_or(&[]));
                    match stream.get(end) {
                        Some(b'H') => {
                            let mut numbers =
                                params.split(';').map(|n| n.parse::<u32>().unwrap_or(1));
                            let row = numbers.next().unwrap_or(1);
                            let col = numbers.next().unwrap_or(1);
                            self.cursor = (col.saturating_sub(1), row.saturating_sub(1));
                        }
                        Some(b'J') if params == "2" => self.puts.clear(),
                        _ => {}
                    }
                    at = end + 1;
                }
                Some(b'_') | Some(b']') => {
                    let start = at + 2;
                    let mut end = start;
                    while end < stream.len() {
                        if stream.get(end) == Some(&0x1b) && stream.get(end + 1) == Some(&b'\\') {
                            break;
                        }
                        if stream.get(end) == Some(&0x07) && stream.get(at + 1) == Some(&b']') {
                            break;
                        }
                        end += 1;
                    }
                    if stream.get(at + 1) == Some(&b'_') && stream.get(start) == Some(&b'G') {
                        let body =
                            String::from_utf8_lossy(stream.get(start + 1..end).unwrap_or(&[]))
                                .into_owned();
                        self.graphics(&body);
                    }
                    at = end + 2;
                }
                _ => at += 2,
            }
        }
    }

    fn graphics(&mut self, body: &str) {
        let (keys, payload) = body.split_once(';').unwrap_or((body, ""));
        let mut map: HashMap<String, String> = HashMap::new();
        for pair in keys.split(',') {
            if let Some((key, value)) = pair.split_once('=') {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
        let more = map.get("m").is_some_and(|m| m == "1");
        if let Some(pending) = self.pending.as_mut()
            && !map.contains_key("a")
        {
            pending.data.push_str(payload);
            if !more && let Some(done) = self.pending.take() {
                self.transmit(&done);
            }
            return;
        }
        let action = map.get("a").map_or("t", String::as_str);
        match action {
            "t" | "T" => {
                let pending = Pending {
                    keys: map,
                    data: payload.to_owned(),
                };
                if more {
                    self.pending = Some(pending);
                } else {
                    self.transmit(&pending);
                }
            }
            "p" => {
                let id = Self::number(&map, "i").unwrap_or(0);
                let put = self.put_of(&map, Source::Kept(id));
                self.set(put);
            }
            "d" => self.delete(&map),
            _ => {}
        }
    }

    fn number(map: &HashMap<String, String>, key: &str) -> Option<u32> {
        map.get(key).and_then(|value| value.parse().ok())
    }

    fn put_of(&self, map: &HashMap<String, String>, source: Source) -> Put {
        let number = |key: &str| Self::number(map, key);
        let part = match (number("x"), number("y"), number("w"), number("h")) {
            (None, None, None, None) => None,
            (x, y, w, h) => Some((
                x.unwrap_or(0),
                y.unwrap_or(0),
                w.unwrap_or(0),
                h.unwrap_or(0),
            )),
        };
        let over = match (number("c"), number("r")) {
            (Some(c), Some(r)) => Some((c, r)),
            _ => None,
        };
        Put {
            source,
            number: number("p").unwrap_or(0),
            at: self.cursor,
            inside: (number("X").unwrap_or(0), number("Y").unwrap_or(0)),
            part,
            over,
            z: map.get("z").and_then(|z| z.parse().ok()).unwrap_or(0),
        }
    }

    fn set(&mut self, put: Put) {
        self.puts
            .retain(|had| !(had.source == put.source && had.number == put.number));
        self.puts.push(put);
    }

    fn transmit(&mut self, pending: &Pending) {
        let keys = &pending.keys;
        let (Some(width), Some(height)) = (Self::number(keys, "s"), Self::number(keys, "v")) else {
            return;
        };
        let packed = keys.get("o").is_some_and(|o| o == "z");
        let Some(mut bytes) = unbase64(&pending.data) else {
            return;
        };
        if packed {
            let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes) else {
                return;
            };
            bytes = inflated;
        }
        if bytes.len() != 4 * width as usize * height as usize {
            return;
        }
        let stored = Stored {
            width,
            height,
            rgba: bytes,
        };
        let action = keys.get("a").map_or("t", String::as_str);
        let id = Self::number(keys, "i").unwrap_or(0);
        self.loads.push(Load {
            id,
            width,
            height,
            packed,
        });
        if action == "T" {
            self.anonymous.push(stored);
            let put = self.put_of(keys, Source::Inline(self.anonymous.len() - 1));
            self.set(put);
        } else {
            self.images.insert(id, stored);
        }
    }

    fn delete(&mut self, map: &HashMap<String, String>) {
        let what = map.get("d").map_or("a", String::as_str);
        let id = Self::number(map, "i");
        let number = Self::number(map, "p");
        match what {
            "i" | "I" => {
                self.puts.retain(|put| {
                    !(id.is_some_and(|id| put.source == Source::Kept(id))
                        && number.is_none_or(|number| number == put.number))
                });
                if what == "I"
                    && let Some(id) = id
                {
                    self.images.remove(&id);
                }
            }
            "a" => self.puts.clear(),
            "A" => {
                self.puts.clear();
                self.images.clear();
            }
            _ => {}
        }
    }

    /// What the terminal shows of `frame`'s cells with the pictures it holds: `cols` by `rows` cells.
    #[must_use]
    pub fn show(&self, frame: &Frame) -> Image {
        let (cw, ch) = self.cell;
        let (cols, rows) = (u32::from(frame.grid.cols()), u32::from(frame.grid.rows()));
        let (width, height) = (cols * cw, rows * ch);
        let mut image = Image {
            width,
            height,
            rgb: [self.ground.r, self.ground.g, self.ground.b].repeat((width * height) as usize),
        };
        let mut order: Vec<&Put> = self.puts.iter().collect();
        order.sort_by_key(|put| put.z);
        for put in order.iter().filter(|put| put.z < OVER_GROUNDS) {
            self.blit(&mut image, put);
        }
        for row in 0..rows {
            for col in 0..cols {
                let Some(cell) = frame.grid.cell(col as u16, row as u16) else {
                    continue;
                };
                let (top, bottom) = match (cell.ch, cell.bg) {
                    ('\u{2580}', Bg::Rgb(bottom)) => (Some(cell.fg), Some(bottom)),
                    (_, Bg::Rgb(ground)) => (Some(ground), Some(ground)),
                    _ => (None, None),
                };
                for y in 0..ch {
                    let color = if y < ch / 2 { top } else { bottom };
                    let Some(color) = color else { continue };
                    for x in 0..cw {
                        image.set(col * cw + x, row * ch + y, color);
                    }
                }
            }
        }
        for put in order.iter().filter(|put| put.z >= OVER_GROUNDS) {
            self.blit(&mut image, put);
        }
        image
    }

    fn blit(&self, image: &mut Image, put: &Put) {
        let stored = match put.source {
            Source::Kept(id) => self.images.get(&id),
            Source::Inline(at) => self.anonymous.get(at),
        };
        let Some(stored) = stored else { return };
        let (cw, ch) = self.cell;
        let (x, y, w, h) = put
            .part
            .map_or((0, 0, stored.width, stored.height), |(x, y, w, h)| {
                (
                    x,
                    y,
                    if w == 0 {
                        stored.width - x.min(stored.width)
                    } else {
                        w
                    },
                    if h == 0 {
                        stored.height - y.min(stored.height)
                    } else {
                        h
                    },
                )
            });
        let origin = (put.at.0 * cw + put.inside.0, put.at.1 * ch + put.inside.1);
        let size = put.over.map_or((w, h), |(c, r)| (c * cw, r * ch));
        for dy in 0..size.1 {
            for dx in 0..size.0 {
                let (px, py) = (origin.0 + dx, origin.1 + dy);
                if px >= image.width || py >= image.height {
                    continue;
                }
                let (sx, sy) = (
                    x + (u64::from(dx) * u64::from(w) / u64::from(size.0.max(1))) as u32,
                    y + (u64::from(dy) * u64::from(h) / u64::from(size.1.max(1))) as u32,
                );
                if sx >= stored.width || sy >= stored.height {
                    continue;
                }
                let at = ((sy * stored.width + sx) * 4) as usize;
                let Some(&[r, g, b, a]) = stored.rgba.get(at..at + 4) else {
                    continue;
                };
                let under = image.at(px, py);
                let mix = |over: u8, under: u8| {
                    ((u32::from(over) * u32::from(a)
                        + u32::from(under) * (255 - u32::from(a))
                        + 127)
                        / 255) as u8
                };
                image.set(
                    px,
                    py,
                    Rgb::new(mix(r, under.r), mix(g, under.g), mix(b, under.b)),
                );
            }
        }
    }
}

impl Image {
    fn set(&mut self, x: u32, y: u32, color: Rgb) {
        let at = ((y * self.width + x) * 3) as usize;
        if let Some(pixel) = self.rgb.get_mut(at..at + 3) {
            pixel.copy_from_slice(&[color.r, color.g, color.b]);
        }
    }
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut group = 0_u32;
    let mut count = 0;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        group = group << 6 | value(byte)?;
        count += 1;
        if count == 4 {
            out.extend_from_slice(&[(group >> 16) as u8, (group >> 8) as u8, group as u8]);
            group = 0;
            count = 0;
        }
    }
    match count {
        2 => out.push((group >> 4) as u8),
        3 => out.extend_from_slice(&[(group >> 10) as u8, (group >> 2) as u8]),
        _ => {}
    }
    Some(out)
}
