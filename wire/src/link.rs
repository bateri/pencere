//! Links (specification §9.7, §9.8): what a view may ask its host to open in the person's
//! browser. A web page asks with its `link` message, a view of the kit with the `link` of a reply;
//! the rule is the same, and this is its one copy.

/// The most bytes a link may have (specification §9.7).
pub const MAX_LINK: usize = 8192;

/// Whether `url` may be opened for a view on the person's machine: it begins with `http://` or
/// `https://` (the scheme in either letter case), every byte of it is printable ASCII without the
/// space (0x21 to 0x7E), and it is at most [`MAX_LINK`] bytes long. A host drops any other link,
/// and does not tell the view.
pub fn is_openable(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && url.len() <= MAX_LINK
        && url.bytes().all(|b| (0x21..0x7f).contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_links_open() {
        for good in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "HTTPS://EXAMPLE.COM",
            "hTtP://example.com",
        ] {
            assert!(is_openable(good), "{good}");
        }
        for bad in [
            "",
            "http",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
            "mailto:a@b",
            "ssh://host",
            "https://exa mple.com",
            "https://example.com/\n",
            "https://example.com/\u{7f}",
            "https://example.com/ü",
            "//example.com",
            "https:/example.com",
            " https://example.com",
        ] {
            assert!(!is_openable(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_link_is_at_most_8192_bytes() {
        let at_most = format!("https://{}", "a".repeat(MAX_LINK - "https://".len()));
        assert_eq!(at_most.len(), MAX_LINK);
        assert!(is_openable(&at_most));
        assert!(!is_openable(&format!("{at_most}a")));
    }
}
