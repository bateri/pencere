//! The token (specification §6.1): the secret that lets a program open a view.
//!
//! The crate does no I/O, so it cannot read a random generator; the terminal
//! lends one through [`Entropy`], and the crate turns its bytes into the token
//! the wire carries.

use crate::base64;

/// A source of unpredictable bytes. **It must be a cryptographically secure
/// generator** (the operating system's, `getentropy` or `/dev/urandom`): the
/// token it feeds is the only thing between a stream anyone can write and a view
/// that runs code on the person's machine.
pub trait Entropy {
    /// 16 fresh bytes.
    fn token_bytes(&mut self) -> [u8; 16];
}

impl<F: FnMut() -> [u8; 16]> Entropy for F {
    fn token_bytes(&mut self) -> [u8; 16] {
        self()
    }
}

/// A token: 16 bytes written as base64url without padding, 22 characters.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// The token that `bytes` make.
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self(base64::encode_url(bytes))
    }

    /// The token as the wire writes it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `presented` is this token. The comparison takes the same time
    /// wherever the two differ.
    pub fn matches(&self, presented: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), presented.as_bytes());
        let mut diff = a.len() ^ b.len();
        for (x, y) in a.iter().zip(b) {
            diff |= usize::from(x ^ y);
        }
        diff == 0
    }
}

// A token is a secret: it is never printed by accident.
impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_bytes_are_twenty_two_characters() {
        let token = Token::from_bytes(&[0xff; 16]);
        assert_eq!(token.as_str().len(), 22);
        assert!(
            token
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
    }

    #[test]
    fn a_token_matches_only_itself() {
        let token = Token::from_bytes(&[7; 16]);
        assert!(token.matches(token.as_str()));
        assert!(!token.matches(""));
        assert!(!token.matches(&format!("{}x", token.as_str())));
        let mut other = token.as_str().to_owned();
        other.replace_range(21.., "A");
        assert!(!token.matches(&other));
        assert_eq!(format!("{token:?}"), "Token(..)");
    }
}
