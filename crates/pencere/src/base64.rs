//! Base64, the two forms the wire uses and nothing more: standard with padding
//! (RFC 4648 §4) for the bytes of a `file`, URL-safe without padding (§5) for a
//! token. Hand-written because the wire needs exactly these two and a strict
//! decoder; every byte it reads comes from a stream anyone can write.

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn symbol(table: &[u8; 64], value: u8) -> char {
    table
        .get(usize::from(value & 0x3f))
        .map_or('=', |&byte| char::from(byte))
}

/// Encodes `bytes` as standard base64 with padding (RFC 4648 §4).
pub fn encode(bytes: &[u8]) -> String {
    encode_with(bytes, STANDARD, true)
}

/// Encodes `bytes` as base64url without padding (RFC 4648 §5).
pub fn encode_url(bytes: &[u8]) -> String {
    encode_with(bytes, URL, false)
}

fn encode_with(bytes: &[u8], table: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let a = group.first().copied().unwrap_or(0);
        let b = group.get(1).copied();
        let c = group.get(2).copied();
        let word =
            (u32::from(a) << 16) | (u32::from(b.unwrap_or(0)) << 8) | u32::from(c.unwrap_or(0));
        out.push(symbol(table, (word >> 18) as u8));
        out.push(symbol(table, (word >> 12) as u8));
        match (b, c) {
            (Some(_), Some(_)) => {
                out.push(symbol(table, (word >> 6) as u8));
                out.push(symbol(table, word as u8));
            }
            (Some(_), None) => {
                out.push(symbol(table, (word >> 6) as u8));
                if pad {
                    out.push('=');
                }
            }
            _ => {
                if pad {
                    out.push_str("==");
                }
            }
        }
    }
    out
}

fn value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes standard base64 with padding (RFC 4648 §4), strictly: the length is
/// a multiple of four, the alphabet is the standard one, padding is only at the
/// end and the bits it discards are zero. Anything else is `None`.
pub fn decode(text: &[u8]) -> Option<Vec<u8>> {
    if text.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut groups = text.chunks_exact(4).peekable();
    while let Some(group) = groups.next() {
        let last = groups.peek().is_none();
        let [a, b, c, d] = *group else { return None };
        let (a, b) = (value(a)?, value(b)?);
        if last && c == b'=' {
            if d != b'=' || b & 0x0f != 0 {
                return None;
            }
            out.push((a << 2) | (b >> 4));
        } else if last && d == b'=' {
            let c = value(c)?;
            if c & 0x03 != 0 {
                return None;
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        } else {
            let (c, d) = (value(c)?, value(d)?);
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_4648_vectors_round_trip() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded.as_bytes()).as_deref(), Some(plain.as_bytes()));
        }
    }

    #[test]
    fn the_url_form_has_no_padding_and_its_own_symbols() {
        assert_eq!(encode_url(&[0xfb, 0xff]), "-_8");
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
        assert_eq!(encode_url(&[0u8; 16]).len(), 22);
    }

    #[test]
    fn a_decoder_for_untrusted_text_is_strict() {
        for bad in [
            "Zg=",      // not a multiple of four
            "Zg==Zg==", // padding in the middle
            "Zh==",     // discarded bits are not zero
            "Zm9=",     // discarded bits are not zero
            "Zm9v!A==", // outside the alphabet
            "Zg=A",     // padding followed by data
            "=g==",     // padding where data is due
            "Zm9v\n",   // not the alphabet
            "Zm-_",     // the URL alphabet is not this one
        ] {
            assert_eq!(decode(bad.as_bytes()), None, "{bad:?}");
        }
    }

    #[test]
    fn every_length_survives_a_round_trip() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        for len in 0..bytes.len() {
            let slice = &bytes[..len];
            assert_eq!(decode(encode(slice).as_bytes()).as_deref(), Some(slice));
        }
    }
}
