//! What names an element from one build of a page to the next.
//!
//! A page is built afresh for every frame (it is a function of its state), so what the pointer
//! is over, what is pressed, where a list is scrolled to and what a fade has reached cannot
//! live in the elements: it is kept by [`Id`], and the elements that need it carry one.

/// The name of an element, stable across builds. Made from a number or from words
/// ([`Id::named`]); two elements of a page with the same name are the same thing to the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(u64);

/// The role of an editor's name that is its caret's ([`Id::caret`]): one no page has a use for.
const CARET: u8 = 0xc7;

/// The role of an editor's name that a range of its text is named by ([`Id::anchor`]).
const ANCHOR: u8 = 0xa7;

impl Id {
    /// The id with this number.
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    /// The number of the id: what a page that lives apart from its host names an element by.
    pub const fn number(self) -> u64 {
        self.0
    }

    /// The id of these words (a hash of them: 64 bits of FNV-1a), so a row can be named for
    /// what it shows — `Id::named("container api")` — without a table of numbers.
    pub const fn named(words: &str) -> Self {
        let mut bytes = words.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        while let [byte, rest @ ..] = bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            bytes = rest;
        }
        Self(hash)
    }

    /// The caret of the editor named this ([`crate::Element::editor`]): what an overlay is anchored
    /// to (`anchor(id.caret(), Side::Below)`, a popup of completions) and what
    /// [`crate::Laid::bounds`] says the box of, while the editor has the room and the caret is
    /// in it. A name of the editor's, which no name of the page's is.
    #[must_use]
    pub const fn caret(self) -> Self {
        self.role(CARET)
    }

    /// The range named `name` of the editor named this ([`crate::edit::Mark::Anchor`]): what an
    /// overlay is anchored to, as to [`Id::caret`], and what [`crate::Laid::bounds`] says the box of,
    /// while the editor has the room and the range is on a line that shows.
    #[must_use]
    pub fn anchor(self, name: &str) -> Self {
        // Not symmetric: an editor named as a range and a range named as an editor are two names.
        Self(self.role(ANCHOR).0 ^ Self::named(name).role(ANCHOR).0.rotate_left(17))
    }

    /// An id related to this one — the same element in another role (a row, and the fade of
    /// its highlight). Different for every `role`, and never this id itself.
    #[must_use]
    pub const fn role(self, role: u8) -> Self {
        let mixed = (self.0 ^ ((role as u64 + 1) << 56)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        Self(mixed ^ (mixed >> 29))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_words_are_the_same_id_and_other_words_are_not() {
        assert_eq!(Id::named("container api"), Id::named("container api"));
        assert_ne!(Id::named("container api"), Id::named("container db"));
        assert_ne!(Id::new(1), Id::new(2));
    }

    #[test]
    fn a_role_of_an_id_is_another_id_for_each_role() {
        let id = Id::named("row");
        let roles: Vec<Id> = (0..8).map(|role| id.role(role)).collect();
        for (at, one) in roles.iter().enumerate() {
            assert_ne!(*one, id);
            assert!(roles[at + 1..].iter().all(|other| other != one));
        }
        assert_eq!(id.role(3), id.role(3));
    }
}
