//! What is wrong with a page's reading, found without a person to read it: the checks that every
//! pen's tests hold the page to (`Reading::audit`), so that the faults a screen reader's user and an
//! agent meet first (a control that has no name, the same words said twice, a control that cannot be
//! told by its id, one that answers the pointer and says it is not a control) are found in a test and
//! not by them.

use std::fmt;

use pencere_view::Id;

use crate::reading::{Item, Reading, reference};

/// What is wrong with a line of a reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flaw {
    /// A control (a button, a field, a checkbox, a tab, a row of choices, anything that answers the
    /// pointer) with no name: a person is told "button" and nothing else.
    Nameless,
    /// The same words twice in a row: the element and the text inside it, said both.
    Twice,
    /// A control that has no id: a press, a click or a key cannot be sent to it by name.
    NoId,
    /// An element that answers the pointer and whose role says it only shows (a text, a picture, a
    /// progress): it is a control read as words.
    Misfit,
    /// An item of the page behind a modal layer that is in front of it, read: a person who reads by ear
    /// meets what they cannot reach, and an agent presses what is covered.
    Behind,
    /// An item, a field or an editor that is read and has something over it that answers the pointer, so
    /// that a press where it shows goes to that instead (a scrim of a layer that was not made a modal
    /// one, so the page behind it is read in full): read, and not to be reached. Not where what is over
    /// it is in a layer that is not modal (a popover, a toast), nor a box that does not answer the
    /// pointer. Found on the page itself ([`crate::Headless::audit`]): the reading alone does not know
    /// what lies over what.
    Covered,
}

impl Flaw {
    /// What the flaw is called in a report.
    pub fn name(self) -> &'static str {
        match self {
            Self::Nameless => "nameless",
            Self::Twice => "twice",
            Self::NoId => "no id",
            Self::Misfit => "misfit",
            Self::Behind => "behind",
            Self::Covered => "covered",
        }
    }
}

/// A flaw, and where it is: the line of [`Reading::text`] it is on and the name that line begins with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub flaw: Flaw,
    /// The line of the reading's text, from 1.
    pub line: usize,
    /// The element's id, where it has one.
    pub at: Option<Id>,
    pub role: String,
    pub words: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.at.map_or_else(|| "-".to_owned(), reference);
        write!(
            out,
            "line {}: {name} {}: {} {:?}",
            self.line,
            self.flaw.name(),
            self.role,
            self.words
        )
    }
}

/// The findings, a line each: what a test prints where it finds some, and the empty text where it
/// finds none.
pub fn report(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| format!("{finding}\n"))
        .collect()
}

/// What a person works on: a role that is a control.
fn controls(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "link"
            | "checkbox"
            | "switch"
            | "radio"
            | "tab"
            | "menuitem"
            | "slider"
            | "field"
            | "editor"
            | "browser"
    )
}

/// A role that is words or a picture, which shows and does nothing.
fn only_shows(role: &str) -> bool {
    matches!(role, "text" | "image" | "progress")
}

impl Reading {
    /// The flaws of this reading, in the order of their lines: a control with no name, the same words
    /// said twice in a row, a control with no id, an element that answers the pointer and is read as
    /// words. None where the reading is clean.
    pub fn audit(&self) -> Vec<Finding> {
        let at = self.written();
        let mut found: Vec<Finding> = Vec::new();
        let of = |flaw: Flaw, line: usize, item: &Item| Finding {
            flaw,
            line,
            at: item.id,
            role: item.role.clone(),
            words: item.words.clone(),
        };
        let mut before: Option<&Item> = None;
        for (item, line) in self.items.iter().zip(&at.items) {
            let control = controls(&item.role) || (item.pressable && !only_shows(&item.role));
            if control && item.words.trim().is_empty() {
                found.push(of(Flaw::Nameless, *line, item));
            }
            if control && item.id.is_none() {
                found.push(of(Flaw::NoId, *line, item));
            }
            if item.pressable && only_shows(&item.role) {
                found.push(of(Flaw::Misfit, *line, item));
            }
            if item.behind {
                found.push(of(Flaw::Behind, *line, item));
            }
            if let Some(last) = before
                && !item.words.trim().is_empty()
                && last.words.trim() == item.words.trim()
            {
                found.push(of(Flaw::Twice, *line, item));
            }
            before = Some(item);
        }
        for (field, line) in self.fields.iter().zip(&at.fields) {
            let name = field.label.as_deref().or(field.hint.as_deref());
            if name.is_none_or(|name| name.trim().is_empty()) {
                found.push(Finding {
                    flaw: Flaw::Nameless,
                    line: *line,
                    at: Some(field.id),
                    role: "field".to_owned(),
                    words: String::new(),
                });
            }
        }
        for (editor, line) in self.editors.iter().zip(&at.editors) {
            if editor
                .label
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
            {
                found.push(Finding {
                    flaw: Flaw::Nameless,
                    line: *line,
                    at: Some(editor.id),
                    role: "editor".to_owned(),
                    words: String::new(),
                });
            }
        }
        found.sort_by_key(|finding| finding.line);
        found
    }
}
