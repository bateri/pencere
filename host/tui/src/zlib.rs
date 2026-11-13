//! Pixels made smaller for a terminal that takes them so: zlib (RFC 1950) around one block of deflate
//! (RFC 1951) with the fixed codes and matches found by hash chains. A picture of a page or of a
//! screen is long runs of a colour and rows that are the rows before them, which this packs to a
//! fraction; what it does not pack is left as it was by the caller.

/// The window deflate matches reach back into.
const WINDOW: usize = 32 * 1024;

/// The longest match deflate has and the shortest it is worth.
const LONGEST: usize = 258;
const SHORTEST: usize = 3;

/// How many places a hash chain is followed for a longer match.
const CHAIN: usize = 24;

const HASH_BITS: u32 = 15;

/// The end of a chain.
const NONE: usize = usize::MAX;

/// The base length and the extra bits of each length code (257 to 285).
const LENGTHS: [(u16, u8); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// The base distance and the extra bits of each distance code.
const DISTANCES: [(u16, u8); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// Bits as deflate writes them: the first into the low end of a byte.
struct Bits {
    out: Vec<u8>,
    held: u32,
    count: u32,
}

impl Bits {
    /// `count` bits of `value`, the lowest first (the extra bits of a code).
    fn put(&mut self, value: u32, count: u32) {
        self.held |= value << self.count;
        self.count += count;
        while self.count >= 8 {
            self.out.push((self.held & 0xff) as u8);
            self.held >>= 8;
            self.count -= 8;
        }
    }

    /// A Huffman code of `count` bits, which goes the other way about: its highest bit first.
    fn code(&mut self, code: u32, count: u32) {
        let mut reversed = 0;
        for bit in 0..count {
            reversed |= ((code >> bit) & 1) << (count - 1 - bit);
        }
        self.put(reversed, count);
    }

    /// A literal or a length, by the fixed code of its number (0 to 287).
    fn symbol(&mut self, symbol: u32) {
        match symbol {
            0..=143 => self.code(0x30 + symbol, 8),
            144..=255 => self.code(0x190 + symbol - 144, 9),
            256..=279 => self.code(symbol - 256, 7),
            _ => self.code(0xc0 + symbol - 280, 8),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.out.push((self.held & 0xff) as u8);
        }
        self.out
    }
}

/// The Adler-32 checksum zlib ends with.
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for chunk in data.chunks(5552) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Where the places that begin with three bytes alike are, newest first: `head` has the latest of each
/// hash, `before` the one that came before each place (kept for a window of them).
struct Chains {
    head: Vec<usize>,
    before: Vec<usize>,
}

impl Chains {
    fn new() -> Self {
        Self {
            head: vec![NONE; 1 << HASH_BITS],
            before: vec![NONE; WINDOW],
        }
    }

    /// The slot of the three bytes at `at`, if there are three.
    fn slot(data: &[u8], at: usize) -> Option<usize> {
        let &[a, b, c] = data.get(at..at.checked_add(SHORTEST)?)? else {
            return None;
        };
        let word = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        Some((word.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize)
    }

    fn insert(&mut self, data: &[u8], at: usize) {
        if let Some(slot) = Self::slot(data, at)
            && let (Some(head), Some(before)) =
                (self.head.get_mut(slot), self.before.get_mut(at % WINDOW))
        {
            *before = *head;
            *head = at;
        }
    }

    /// The longest match for what is at `at`, as its length and how far back it is.
    fn longest(&self, data: &[u8], at: usize) -> (usize, usize) {
        let mut best = (0, 0);
        let (Some(slot), Some(rest)) = (Self::slot(data, at), data.get(at..)) else {
            return best;
        };
        let rest = rest.get(..LONGEST).unwrap_or(rest);
        let mut candidate = self.head.get(slot).copied().unwrap_or(NONE);
        let mut tried = 0;
        while candidate != NONE && at - candidate <= WINDOW && tried < CHAIN {
            let length = data
                .get(candidate..)
                .unwrap_or_default()
                .iter()
                .zip(rest)
                .take_while(|(a, b)| a == b)
                .count();
            if length > best.0 {
                best = (length, at - candidate);
                if length == rest.len() {
                    break;
                }
            }
            let next = self.before.get(candidate % WINDOW).copied().unwrap_or(NONE);
            if next >= candidate {
                break;
            }
            candidate = next;
            tried += 1;
        }
        best
    }
}

/// The number of the code a length or a distance has in `table`, and its extra bits.
fn coded(table: &[(u16, u8)], value: usize) -> (usize, u32, u32) {
    let code = table
        .iter()
        .rposition(|(base, _)| usize::from(*base) <= value)
        .unwrap_or(0);
    let (base, extra) = table.get(code).copied().unwrap_or((1, 0));
    (code, (value - usize::from(base)) as u32, u32::from(extra))
}

/// `data` as a zlib stream.
#[must_use]
pub(crate) fn compress(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits {
        out: vec![0x78, 0x01],
        held: 0,
        count: 0,
    };
    // One last block, with the fixed codes.
    bits.put(1, 1);
    bits.put(1, 2);
    let mut chains = Chains::new();
    let mut at = 0;
    while let Some(&byte) = data.get(at) {
        let (length, distance) = chains.longest(data, at);
        if length >= SHORTEST {
            let (code, rest, extra) = coded(&LENGTHS, length);
            bits.symbol(257 + code as u32);
            bits.put(rest, extra);
            let (code, rest, extra) = coded(&DISTANCES, distance);
            bits.code(code as u32, 5);
            bits.put(rest, extra);
            for covered in at..at + length {
                chains.insert(data, covered);
            }
            at += length;
        } else {
            bits.symbol(u32::from(byte));
            chains.insert(data, at);
            at += 1;
        }
    }
    bits.symbol(256);
    let mut out = bits.finish();
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// What a zlib stream of one block with the fixed codes says (the other kinds of block are not
    /// read: they are not written).
    fn inflate(stream: &[u8]) -> Vec<u8> {
        assert_eq!(&stream[..2], &[0x78, 0x01]);
        let body = &stream[2..stream.len() - 4];
        let mut at = 0;
        let mut bit = |count: u32| -> u32 {
            let mut value = 0;
            for i in 0..count {
                let byte = body[at / 8];
                value |= u32::from((byte >> (at % 8)) & 1) << i;
                at += 1;
            }
            value
        };
        assert_eq!(bit(1), 1, "the last block");
        assert_eq!(bit(2), 1, "with the fixed codes");
        let mut out: Vec<u8> = Vec::new();
        loop {
            // The code is read a bit at a time, the highest first.
            let mut code = 0_u32;
            let mut length = 0;
            let symbol = loop {
                code = (code << 1) | bit(1);
                length += 1;
                match (length, code) {
                    (7, 0..=0x17) => break 256 + code,
                    (8, 0x30..=0xbf) => break code - 0x30,
                    (8, 0xc0..=0xc7) => break 280 + code - 0xc0,
                    (9, 0x190..=0x1ff) => break 144 + code - 0x190,
                    _ => {}
                }
                assert!(length < 9, "a code of the fixed table");
            };
            match symbol {
                0..=255 => out.push(symbol as u8),
                256 => break,
                _ => {
                    let (base, extra) = LENGTHS[(symbol - 257) as usize];
                    let length = usize::from(base) + bit(u32::from(extra)) as usize;
                    let mut distance_code = 0_u32;
                    for _ in 0..5 {
                        distance_code = (distance_code << 1) | bit(1);
                    }
                    let (base, extra) = DISTANCES[distance_code as usize];
                    let distance = usize::from(base) + bit(u32::from(extra)) as usize;
                    for _ in 0..length {
                        out.push(out[out.len() - distance]);
                    }
                }
            }
        }
        let sum = u32::from_be_bytes(stream[stream.len() - 4..].try_into().expect("four bytes"));
        assert_eq!(sum, adler32(&out), "the checksum is of what it says");
        out
    }

    fn patterned(len: usize) -> Vec<u8> {
        // Rows of pixels, runs of one colour, some noise, a row that is the one before.
        let mut data = Vec::new();
        let mut seed = 12345_u32;
        for row in 0..len / 400 {
            for x in 0..100 {
                let pixel = if row % 7 == 3 && x % 11 < 3 {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    [(seed >> 16) as u8, (seed >> 8) as u8, seed as u8, 255]
                } else {
                    [240, 240, 240, 255]
                };
                data.extend_from_slice(&pixel);
            }
        }
        data
    }

    #[test]
    fn what_is_packed_is_what_it_was() {
        for data in [
            Vec::new(),
            vec![7],
            b"abcabcabcabcabcabc".to_vec(),
            vec![0; 100_000],
            patterned(400_000),
            (0..70_000_u64).map(|n| ((n * n) >> 3) as u8).collect(),
        ] {
            let packed = compress(&data);
            assert_eq!(inflate(&packed), data, "{} bytes", data.len());
        }
    }

    #[test]
    fn runs_and_repeated_rows_are_a_small_part_of_what_they_were() {
        let flat = vec![255; 1_000_000];
        assert!(compress(&flat).len() < 10_000);
        let page = patterned(400_000);
        let packed = compress(&page);
        assert!(
            packed.len() < page.len() / 5,
            "{} of {}",
            packed.len(),
            page.len()
        );
    }
}
