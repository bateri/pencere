//! The icons a program gives for itself in the `icons` of its `open`, read the way a terminal
//! may draw them (specification §7.3).
//!
//! An icon's `src` is a `data:` URI with base64 data. A terminal draws on the person's machine,
//! where a view has no network unless the person allowed it, and a program on the other end of
//! an ssh connection has no origin an icon could be checked against: fetching an icon would tell
//! the program's author who opened it, from where and when. So only `data:` URIs are read, and
//! the bytes that arrive in the terminal's own stream are all there is.
//!
//! The bytes are the program's, and a terminal hands them to an image decoder in
//! its own process. They are therefore taken only when their own first bytes say
//! PNG, JPEG or WebP (what the program says the type is only has to agree), when
//! they are at most [`MAX_ICON_BYTES`], when the image they describe is at most
//! [`MAX_ICON_SIDE`] pixels a side, and when it is a still: a small file can ask a
//! decoder for a picture of gigabytes, or for thousands of frames.

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::base64;

/// The most bytes of one icon, decoded.
pub const MAX_ICON_BYTES: usize = 64 * 1024;

/// The most pixels of either side of an icon.
pub const MAX_ICON_SIDE: u32 = 512;

/// The most icons kept of one `icons` list.
pub const MAX_ICONS: usize = 4;

/// The most entries of one `icons` list that are looked at.
pub const MAX_ICON_ENTRIES: usize = 16;

/// What an icon's bytes are, as their first bytes say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IconFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// WebP.
    Webp,
}

impl IconFormat {
    /// The media type of the format.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }

    /// The format a media type names, of those an icon may be. `image/jpg` is the
    /// alias some programs write beside `image/jpeg`.
    fn named(media_type: &str) -> Option<Self> {
        let named = |name: &str| media_type.eq_ignore_ascii_case(name);
        if named("image/png") {
            Some(Self::Png)
        } else if named("image/jpeg") || named("image/jpg") {
            Some(Self::Jpeg)
        } else if named("image/webp") {
            Some(Self::Webp)
        } else {
            None
        }
    }
}

/// The background an icon was drawn for (its `theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconTheme {
    /// A light background.
    Light,
    /// A dark background.
    Dark,
}

/// An icon a terminal may draw as it stands: its bytes are an image of
/// [`format`](Self::format), [`width`](Self::width) by [`height`](Self::height)
/// pixels, within the limits of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    /// What the bytes are.
    pub format: IconFormat,
    /// The width in pixels, as the image's header has it.
    pub width: u32,
    /// The height in pixels, as the image's header has it.
    pub height: u32,
    /// The background the icon was drawn for; `None` when it suits either.
    pub theme: Option<IconTheme>,
    /// The image.
    pub bytes: Vec<u8>,
}

/// Of `icons`, the one to draw at `side` pixels on a background that is dark or
/// not: one drawn for that background before one that suits either (an icon for
/// the other background is never chosen), then the smallest that is at least
/// `side` pixels on its shorter side, else the largest.
pub fn pick(icons: &[Icon], dark: bool, side: u32) -> Option<&Icon> {
    let wanted = if dark {
        IconTheme::Dark
    } else {
        IconTheme::Light
    };
    let fits = |icon: &&Icon| icon.theme.is_none_or(|theme| theme == wanted);
    let short = |icon: &Icon| icon.width.min(icon.height);
    icons.iter().filter(fits).min_by_key(|icon| {
        let matched = icon.theme == Some(wanted);
        let short = short(icon);
        // Matched first; then big enough before too small; then the closest.
        (
            !matched,
            short < side,
            if short < side {
                u32::MAX - short
            } else {
                short
            },
        )
    })
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    src: Option<Box<RawValue>>,
    #[serde(rename = "mimeType", default)]
    mime_type: Option<Box<RawValue>>,
    #[serde(default)]
    sizes: Option<Box<RawValue>>,
    #[serde(default)]
    theme: Option<Box<RawValue>>,
}

/// The icons of an `icons` member, at most [`MAX_ICONS`] of the first
/// [`MAX_ICON_ENTRIES`], in the program's order. A member that is not a list, and
/// an entry that is not an icon this module takes, says nothing; neither is a
/// reason to refuse what the icons were part of.
pub(crate) fn read(raw: Option<&RawValue>) -> Vec<Icon> {
    let Some(entries) = raw.and_then(|raw| serde_json::from_str::<Vec<&RawValue>>(raw.get()).ok())
    else {
        return Vec::new();
    };
    entries
        .into_iter()
        .take(MAX_ICON_ENTRIES)
        .filter_map(|raw| crate::json::from_object::<Entry>(raw.get()))
        .filter_map(|entry| icon(&entry))
        .take(MAX_ICONS)
        .collect()
}

fn icon(entry: &Entry) -> Option<Icon> {
    let text = |raw: &Option<Box<RawValue>>| -> Option<Option<String>> {
        match raw {
            None => Some(None),
            Some(raw) => serde_json::from_str::<String>(raw.get()).ok().map(Some),
        }
    };
    let src = text(&entry.src)??;
    let mime_type = text(&entry.mime_type)?;
    // A member of another shape makes the icon unusable (specification §7.3). `sizes` is only
    // read for that: the size an icon has is the image's own.
    if let Some(sizes) = &entry.sizes {
        serde_json::from_str::<Vec<String>>(sizes.get()).ok()?;
    }
    let theme = match text(&entry.theme)?.as_deref() {
        None => None,
        Some("light") => Some(IconTheme::Light),
        Some("dark") => Some(IconTheme::Dark),
        Some(_) => return None,
    };
    let (media_type, bytes) = data_uri(&src)?;
    let (format, width, height) = sniff(&bytes)?;
    // What the program says the bytes are is believed only when it agrees.
    for said in [media_type, mime_type.as_deref()].into_iter().flatten() {
        if IconFormat::named(said) != Some(format) {
            return None;
        }
    }
    if width == 0 || height == 0 || width > MAX_ICON_SIDE || height > MAX_ICON_SIDE {
        return None;
    }
    Some(Icon {
        format,
        width,
        height,
        theme,
        bytes,
    })
}

/// The media type and the bytes of a base64 `data:` URI (RFC 2397):
/// `data:[<media type>][;<parameter>]*;base64,<data>`. The media type is `None`
/// when the URI leaves it out. Anything else, and data over [`MAX_ICON_BYTES`], is
/// `None`, before any of it is decoded.
fn data_uri(src: &str) -> Option<(Option<&str>, Vec<u8>)> {
    let scheme = src.get(..5)?;
    if !scheme.eq_ignore_ascii_case("data:") {
        return None;
    }
    let (head, data) = src.get(5..)?.split_once(',')?;
    let (head, encoding) = head.rsplit_once(';')?;
    if !encoding.eq_ignore_ascii_case("base64") {
        return None;
    }
    if data.len() > MAX_ICON_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let media_type = head.split(';').next().filter(|t| !t.is_empty());
    let bytes = base64::decode(data.as_bytes())?;
    (bytes.len() <= MAX_ICON_BYTES).then_some((media_type, bytes))
}

/// The format and the size of a still image, from its own bytes.
fn sniff(bytes: &[u8]) -> Option<(IconFormat, u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes).map(|(w, h)| (IconFormat::Png, w, h))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg(bytes).map(|(w, h)| (IconFormat::Jpeg, w, h))
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        webp(bytes).map(|(w, h)| (IconFormat::Webp, w, h))
    } else {
        None
    }
}

/// The `N` bytes at `at`, when there are that many.
fn array<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    array(bytes, at).map(|b| u32::from(u16::from_be_bytes(b)))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    array(bytes, at).map(u32::from_be_bytes)
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    array(bytes, at).map(|b| u32::from(u16::from_le_bytes(b)))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    array::<3>(bytes, at).map(|[a, b, c]| u32::from_le_bytes([a, b, c, 0]))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    array(bytes, at).map(u32::from_le_bytes)
}

/// PNG: the size from `IHDR`, which comes first; an animation control chunk
/// (`acTL`, APNG) before the image data makes it no still.
fn png(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let size = (be32(bytes, 16)?, be32(bytes, 20)?);
    let mut at = 8usize;
    loop {
        let length = usize::try_from(be32(bytes, at)?).ok()?;
        match bytes.get(at + 4..at + 8)? {
            b"acTL" => return None,
            b"IDAT" => return Some(size),
            _ => at = at.checked_add(12)?.checked_add(length)?,
        }
    }
}

/// JPEG: the size from the first start-of-frame marker (SOF0 to SOF15, which
/// leaves out DHT, JPG and DAC among them), found by walking the markers before
/// the image data.
fn jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2usize;
    loop {
        if *bytes.get(at)? != 0xff {
            return None;
        }
        // Fill bytes before a marker.
        while *bytes.get(at + 1)? == 0xff {
            at += 1;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        match marker {
            // Markers without a length.
            0x01 | 0xd0..=0xd8 => continue,
            // The end of the image, or its data, before any frame.
            0xd9 | 0xda => return None,
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some((be16(bytes, at + 5)?, be16(bytes, at + 3)?));
            }
            _ => {
                let length = usize::try_from(be16(bytes, at)?).ok()?;
                if length < 2 {
                    return None;
                }
                at += length;
            }
        }
    }
}

/// WebP: the size from the first chunk, which is `VP8 ` (lossy), `VP8L`
/// (lossless) or `VP8X` (extended, whose animation flag makes it no still).
fn webp(bytes: &[u8]) -> Option<(u32, u32)> {
    const DATA: usize = 20;
    match bytes.get(12..16)? {
        b"VP8 " => {
            if bytes.get(DATA + 3..DATA + 6)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((
                le16(bytes, DATA + 6)? & 0x3fff,
                le16(bytes, DATA + 8)? & 0x3fff,
            ))
        }
        b"VP8L" => {
            if *bytes.get(DATA)? != 0x2f {
                return None;
            }
            let bits = le32(bytes, DATA + 1)?;
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => {
            const ANIMATION: u8 = 0x02;
            if *bytes.get(DATA)? & ANIMATION != 0 {
                return None;
            }
            Some((le24(bytes, DATA + 4)? + 1, le24(bytes, DATA + 7)? + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A PNG header, `chunks` after `IHDR`, and no image data beyond an empty
    /// `IDAT`: enough for a reader of sizes, which is all this module is.
    fn png_of(width: u32, height: u32, chunks: &[&[u8; 4]]) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8; 4], data: &[u8]| {
            bytes.extend((data.len() as u32).to_be_bytes());
            bytes.extend(kind);
            bytes.extend(data);
            bytes.extend([0, 0, 0, 0]);
        };
        let mut ihdr = Vec::new();
        ihdr.extend(width.to_be_bytes());
        ihdr.extend(height.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        for kind in chunks {
            chunk(kind, &[1, 2, 3]);
        }
        chunk(b"IDAT", &[]);
        bytes
    }

    fn jpeg_of(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        // An APP0 segment, a fill byte, a DHT (no frame), then SOF2.
        bytes.extend([0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46]);
        bytes.extend([0xff, 0xff, 0xc4, 0x00, 0x03, 0x00]);
        bytes.extend([0xff, 0xc2, 0x00, 0x0b, 0x08]);
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([0x01, 0x01, 0x11, 0x00]);
        bytes.extend([0xff, 0xd9]);
        bytes
    }

    fn riff(chunk: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(((data.len() + 12) as u32).to_le_bytes());
        bytes.extend(b"WEBP");
        bytes.extend(chunk);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes
    }

    fn data(media_type: &str, bytes: &[u8]) -> String {
        format!("data:{media_type};base64,{}", base64::encode(bytes))
    }

    fn read_json(value: serde_json::Value) -> Vec<Icon> {
        let raw = serde_json::value::to_raw_value(&value).unwrap();
        read(Some(&raw))
    }

    #[test]
    fn the_three_formats_are_known_by_their_bytes_with_their_size() {
        assert_eq!(sniff(&png_of(48, 32, &[])), Some((IconFormat::Png, 48, 32)));
        assert_eq!(sniff(&jpeg_of(64, 40)), Some((IconFormat::Jpeg, 64, 40)));
        let lossy = [0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a, 0x20, 0x00, 0x18, 0x00];
        assert_eq!(
            sniff(&riff(b"VP8 ", &lossy)),
            Some((IconFormat::Webp, 32, 24))
        );
        // 16 by 8: 15 and 7 in fourteen bits each.
        let bits: u32 = 15 | (7 << 14);
        let mut lossless = vec![0x2f];
        lossless.extend(bits.to_le_bytes());
        assert_eq!(
            sniff(&riff(b"VP8L", &lossless)),
            Some((IconFormat::Webp, 16, 8))
        );
        let extended = [0x10, 0, 0, 0, 99, 0, 0, 49, 0, 0];
        assert_eq!(
            sniff(&riff(b"VP8X", &extended)),
            Some((IconFormat::Webp, 100, 50))
        );
    }

    #[test]
    fn an_animation_is_no_icon() {
        assert_eq!(sniff(&png_of(32, 32, &[b"acTL"])), None);
        assert!(sniff(&png_of(32, 32, &[b"tEXt"])).is_some());
        let animated = [0x02, 0, 0, 0, 31, 0, 0, 31, 0, 0];
        assert_eq!(sniff(&riff(b"VP8X", &animated)), None);
    }

    #[test]
    fn other_bytes_and_cut_headers_are_no_image() {
        assert_eq!(sniff(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(sniff(b"GIF89a\x01\x00\x01\x00"), None);
        for whole in [
            png_of(32, 32, &[]),
            jpeg_of(32, 32),
            riff(b"VP8X", &[0; 10]),
        ] {
            for len in 0..whole.len() - 1 {
                // A cut file never panics; it is an image only once its size is in.
                let _ = sniff(&whole[..len]);
            }
        }
        assert_eq!(sniff(&png_of(32, 32, &[])[..20]), None);
        // A chunk before the image data whose length runs past the end.
        let mut long = png_of(32, 32, &[b"tEXt"]);
        long[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(sniff(&long), None);
        // A JPEG whose data comes before any frame.
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xda, 0x00, 0x02]), None);
    }

    #[test]
    fn an_icon_is_a_data_uri_of_an_image_within_the_limits() {
        let png = png_of(48, 48, &[]);
        let icons = read_json(json!([
            {"src": data("image/png", &png), "mimeType": "image/png", "sizes": ["48x48"], "theme": "dark"},
            {"src": data("", &jpeg_of(16, 16))},
        ]));
        assert_eq!(
            icons,
            vec![
                Icon {
                    format: IconFormat::Png,
                    width: 48,
                    height: 48,
                    theme: Some(IconTheme::Dark),
                    bytes: png,
                },
                Icon {
                    format: IconFormat::Jpeg,
                    width: 16,
                    height: 16,
                    theme: None,
                    bytes: jpeg_of(16, 16),
                },
            ]
        );
    }

    #[test]
    fn an_icon_from_elsewhere_is_never_fetched_and_never_kept() {
        let png = png_of(48, 48, &[]);
        let encoded = base64::encode(&png);
        for src in [
            "https://example.com/icon.png".to_owned(),
            "http://example.com/icon.png".to_owned(),
            "file:///etc/passwd".to_owned(),
            "javascript:alert(1)".to_owned(),
            format!("data:image/png,{encoded}"),
            format!("data:image/png;base64;x,{encoded}"),
            format!("data:image/png;base64,{encoded}\n"),
            format!("dat:image/png;base64,{encoded}"),
        ] {
            assert_eq!(read_json(json!([{"src": src}])), Vec::new(), "{src}");
        }
        // The scheme and the encoding ignore case, as RFC 2397 has them.
        assert_eq!(
            read_json(json!([{"src": format!("DATA:image/png;charset=x;BASE64,{encoded}")}])).len(),
            1
        );
    }

    #[test]
    fn what_the_program_says_the_type_is_has_to_agree_with_the_bytes() {
        let png = png_of(48, 48, &[]);
        for (media_type, mime_type) in [
            ("image/jpeg", None),
            ("image/svg+xml", None),
            ("image/png", Some("image/webp")),
            ("", Some("image/svg+xml")),
            ("image/gif", Some("image/png")),
        ] {
            let mut entry = json!({"src": data(media_type, &png)});
            if let Some(mime_type) = mime_type {
                entry["mimeType"] = json!(mime_type);
            }
            assert_eq!(read_json(json!([entry])), Vec::new(), "{entry}");
        }
        let jpeg = jpeg_of(16, 16);
        assert_eq!(
            read_json(json!([{"src": data("image/jpg", &jpeg), "mimeType": "IMAGE/JPEG"}])).len(),
            1
        );
    }

    #[test]
    fn a_big_file_or_a_big_picture_is_no_icon() {
        let side = MAX_ICON_SIDE;
        assert_eq!(
            read_json(json!([{"src": data("image/png", &png_of(side, side, &[]))}])).len(),
            1
        );
        for (w, h) in [
            (side + 1, 1),
            (1, side + 1),
            (0, 16),
            (16, 0),
            (u32::MAX, u32::MAX),
        ] {
            assert_eq!(
                read_json(json!([{"src": data("image/png", &png_of(w, h, &[]))}])),
                Vec::new()
            );
        }
        let mut big = png_of(16, 16, &[]);
        big.resize(MAX_ICON_BYTES, 0);
        assert_eq!(
            read_json(json!([{"src": data("image/png", &big)}])).len(),
            1
        );
        big.push(0);
        assert_eq!(
            read_json(json!([{"src": data("image/png", &big)}])),
            Vec::new()
        );
    }

    #[test]
    fn a_list_the_program_got_wrong_says_nothing_and_a_long_one_is_cut() {
        let good = json!({"src": data("image/png", &png_of(16, 16, &[]))});
        for wrong in [
            json!(null),
            json!("icons"),
            json!({"src": "x"}),
            json!([7, null, "x", {"src": 3}, {"theme": "dark"}]),
            json!([{"src": good["src"], "theme": "sepia"}]),
            json!([{"src": good["src"], "mimeType": 3}]),
            json!([{"src": good["src"], "sizes": "16x16"}]),
            json!([{"src": good["src"], "sizes": [16]}]),
        ] {
            assert_eq!(read_json(wrong.clone()), Vec::new(), "{wrong}");
        }
        assert_eq!(read_json(json!([7, good, {"src": 1}])).len(), 1);
        assert_eq!(read_json(json!(vec![good.clone(); 10])).len(), MAX_ICONS);
        let mut late = vec![json!({}); MAX_ICON_ENTRIES];
        late.push(good);
        assert_eq!(read_json(json!(late)), Vec::new());
        assert_eq!(read(None), Vec::new());
    }

    #[test]
    fn the_icon_picked_suits_the_background_then_the_size() {
        let icon = |side: u32, theme: Option<IconTheme>| Icon {
            format: IconFormat::Png,
            width: side,
            height: side,
            theme,
            bytes: Vec::new(),
        };
        let icons = [
            icon(16, None),
            icon(64, None),
            icon(32, Some(IconTheme::Light)),
            icon(128, Some(IconTheme::Dark)),
        ];
        // Drawn for the background first, however big.
        assert_eq!(pick(&icons, true, 32), Some(&icons[3]));
        assert_eq!(pick(&icons, false, 64), Some(&icons[2]));
        // Without one for the background: the smallest that is big enough, else the largest.
        let plain = [
            icon(16, None),
            icon(64, None),
            icon(32, Some(IconTheme::Light)),
        ];
        assert_eq!(pick(&plain, true, 24), Some(&plain[1]));
        assert_eq!(pick(&plain, true, 12), Some(&plain[0]));
        assert_eq!(pick(&plain, true, 200), Some(&plain[1]));
        // Never one drawn for the other background.
        assert_eq!(pick(&[icon(32, Some(IconTheme::Light))], true, 32), None);
        assert_eq!(pick(&[], false, 32), None);
    }
}
