//! A terminal that keeps its window as pictures: it follows the stream as the graphics protocol says (a
//! picture sent and put, a frame of it edited in place, a rectangle of it copied, a picture given up,
//! the cursor that says where a picture is put) and says what the window shows. The unit tests of the
//! pixels and the tests that run a program in a pty read the stream with it.
#![allow(dead_code)]

use std::collections::HashMap;

fn base64_value(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn unbase64(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut bits = 0_u32;
    let mut have = 0;
    for &c in text {
        let Some(value) = base64_value(c) else {
            continue;
        };
        bits = (bits << 6) | value;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push(u8::try_from((bits >> have) & 0xff).expect("a byte"));
        }
    }
    out
}

/// What the terminal did, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Done {
    /// A picture sent and put, at the cell (row, column from 0), in the layer.
    Put {
        id: u32,
        size: (u32, u32),
        at: (u32, u32),
        z: i32,
    },
    /// A rectangle of a picture's frame edited, in place.
    Edit {
        id: u32,
        at: (u32, u32),
        size: (u32, u32),
    },
    /// A rectangle of a picture's frame copied to another place in the same frame.
    Copy {
        id: u32,
        from: (u32, u32),
        to: (u32, u32),
        size: (u32, u32),
    },
    /// A picture given up.
    Free(u32),
    /// Every picture given up.
    FreeAll,
}

pub struct Image {
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

pub struct Placed {
    pub id: u32,
    pub at: (u32, u32),
    pub z: i32,
}

/// A terminal that draws pictures, with a window of cells of the size `shows` is told.
#[derive(Default)]
pub struct Window {
    pub cursor: (u32, u32),
    pub images: HashMap<u32, Image>,
    pub placed: Vec<Placed>,
    /// A picture whose pieces are still coming: its keys, and what has come.
    partial: Option<(HashMap<String, String>, Vec<u8>)>,
    pub done: Vec<Done>,
    /// What each sequence of a picture asked the terminal to say of it (`q`).
    pub quiet: Vec<String>,
}

impl Window {
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut at = 0;
        while at < bytes.len() {
            if bytes.get(at) != Some(&0x1b) {
                at += 1;
                continue;
            }
            match bytes.get(at + 1) {
                Some(b'[') => {
                    let mut end = at + 2;
                    while bytes.get(end).is_some_and(|b| !(0x40..=0x7e).contains(b)) {
                        end += 1;
                    }
                    let body = String::from_utf8_lossy(&bytes[at + 2..end]).into_owned();
                    if bytes.get(end) == Some(&b'H') {
                        let mut parts = body.split(';').map(|p| p.parse::<u32>().unwrap_or(1));
                        let (row, col) = (parts.next().unwrap_or(1), parts.next().unwrap_or(1));
                        self.cursor = (row.saturating_sub(1), col.saturating_sub(1));
                    }
                    at = end + 1;
                }
                Some(b'_') => {
                    let mut end = at + 2;
                    while !(bytes.get(end) == Some(&0x1b) && bytes.get(end + 1) == Some(&b'\\')) {
                        end += 1;
                        assert!(end < bytes.len(), "a picture's sequence that does not end");
                    }
                    self.graphics(&bytes[at + 2..end]);
                    at = end + 2;
                }
                _ => at += 1,
            }
        }
    }

    fn graphics(&mut self, body: &[u8]) {
        if body.first() != Some(&b'G') {
            // Another protocol's sequence (the program's own messages travel in these too).
            return;
        }
        let split = body.iter().position(|b| *b == b';').unwrap_or(body.len());
        let keys: HashMap<String, String> = String::from_utf8_lossy(&body[1..split])
            .split(',')
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let payload = unbase64(body.get(split + 1..).unwrap_or_default());
        let (keys, mut data) = match self.partial.take() {
            Some((first, data)) => (first, data),
            None => {
                if let Some(q) = keys.get("q") {
                    self.quiet.push(q.clone());
                }
                (keys, Vec::new())
            }
        };
        data.extend(payload);
        if keys_more(&body[1..split]) {
            self.partial = Some((keys, data));
            return;
        }
        self.act(&keys, data);
    }

    fn act(&mut self, keys: &HashMap<String, String>, data: Vec<u8>) {
        let number = |key: &str| keys.get(key).and_then(|v| v.parse::<u32>().ok());
        let id = number("i").unwrap_or(0);
        // The questions asked of the terminal at the start (a picture sent to be asked about, and
        // given up again) are not pictures of the window.
        if matches!(keys.get("a").map(String::as_str), Some("q" | "t")) || (1..100).contains(&id) {
            return;
        }
        let data = if keys.get("o").map(String::as_str) == Some("z") {
            miniz_oxide::inflate::decompress_to_vec_zlib(&data).expect("zlib")
        } else {
            data
        };
        match keys.get("a").map(String::as_str) {
            Some("T") => {
                let size = (number("s").expect("s"), number("v").expect("v"));
                assert_eq!(keys.get("f").map(String::as_str), Some("32"));
                assert_eq!(
                    data.len(),
                    4 * size.0 as usize * size.1 as usize,
                    "the pixels of a picture are what it says it is"
                );
                assert_eq!(
                    keys.get("C").map(String::as_str),
                    Some("1"),
                    "the cursor stays"
                );
                self.images.insert(id, Image { size, rgba: data });
                let z = keys.get("z").and_then(|v| v.parse().ok()).unwrap_or(0);
                self.placed.push(Placed {
                    id,
                    at: self.cursor,
                    z,
                });
                self.done.push(Done::Put {
                    id,
                    size,
                    at: self.cursor,
                    z,
                });
            }
            Some("f") => {
                assert_eq!(number("r"), Some(1), "the picture's own frame");
                let (x, y) = (number("x").unwrap_or(0), number("y").unwrap_or(0));
                let size = (number("s").expect("s"), number("v").expect("v"));
                assert_eq!(data.len(), 4 * size.0 as usize * size.1 as usize);
                let image = self.images.get_mut(&id).expect("a picture to edit");
                assert!(
                    x + size.0 <= image.size.0 && y + size.1 <= image.size.1,
                    "an edit inside the picture"
                );
                for row in 0..size.1 {
                    let from = 4 * (row * size.0) as usize;
                    let to = 4 * ((y + row) * image.size.0 + x) as usize;
                    let len = 4 * size.0 as usize;
                    image.rgba[to..to + len].copy_from_slice(&data[from..from + len]);
                }
                self.done.push(Done::Edit {
                    id,
                    at: (x, y),
                    size,
                });
            }
            Some("c") => {
                // A rectangle of the frame copied to another place in it: the rectangles of the
                // two places do not meet (what a terminal that copies a frame onto itself is
                // given to be sure of).
                assert_eq!((number("r"), number("c")), (Some(1), Some(1)));
                assert!(data.is_empty(), "a copy has no pixels of its own");
                let from = (number("X").unwrap_or(0), number("Y").unwrap_or(0));
                let to = (number("x").unwrap_or(0), number("y").unwrap_or(0));
                let size = (number("w").expect("w"), number("h").expect("h"));
                let image = self.images.get_mut(&id).expect("a picture to copy in");
                assert!(
                    from.0 + size.0 <= image.size.0
                        && from.1 + size.1 <= image.size.1
                        && to.0 + size.0 <= image.size.0
                        && to.1 + size.1 <= image.size.1,
                    "a copy inside the picture"
                );
                assert!(
                    from.0 + size.0 <= to.0
                        || to.0 + size.0 <= from.0
                        || from.1 + size.1 <= to.1
                        || to.1 + size.1 <= from.1,
                    "the rectangles of a copy do not meet: {from:?} {to:?} {size:?}"
                );
                for row in 0..size.1 {
                    let a = 4 * ((from.1 + row) * image.size.0 + from.0) as usize;
                    let b = 4 * ((to.1 + row) * image.size.0 + to.0) as usize;
                    image.rgba.copy_within(a..a + 4 * size.0 as usize, b);
                }
                self.done.push(Done::Copy { id, from, to, size });
            }
            Some("d") => match keys.get("d").map(String::as_str) {
                Some("I") => {
                    self.images.remove(&id);
                    self.placed.retain(|p| p.id != id);
                    self.done.push(Done::Free(id));
                }
                Some("A") => {
                    self.images.clear();
                    self.placed.clear();
                    self.done.push(Done::FreeAll);
                }
                other => panic!("a deletion this terminal does not know: {other:?}"),
            },
            other => panic!("an action this terminal does not know: {other:?}"),
        }
    }

    /// What the window shows: the pictures, from the lowest layer up, each at its cell.
    pub fn shows(&self, size: (u32, u32), cell: (u32, u32)) -> Vec<u8> {
        let mut shown = vec![0_u8; 4 * size.0 as usize * size.1 as usize];
        let mut order: Vec<&Placed> = self.placed.iter().collect();
        order.sort_by_key(|p| p.z);
        for placed in order {
            let image = &self.images[&placed.id];
            let (x0, y0) = (placed.at.1 * cell.0, placed.at.0 * cell.1);
            for row in 0..image.size.1 {
                let y = y0 + row;
                if y >= size.1 {
                    break;
                }
                let width = image.size.0.min(size.0.saturating_sub(x0)) as usize;
                let from = 4 * (row * image.size.0) as usize;
                let to = 4 * (y * size.0 + x0) as usize;
                shown[to..to + 4 * width].copy_from_slice(&image.rgba[from..from + 4 * width]);
            }
        }
        shown
    }
}

fn keys_more(head: &[u8]) -> bool {
    String::from_utf8_lossy(head)
        .split(',')
        .any(|pair| pair == "m=1")
}
