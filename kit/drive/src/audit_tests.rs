//! The reading's audit: what it finds on a page that is read badly, all of it and where, and that it
//! finds nothing on one that is read well.

use pencere_view::{Accessible, AccessibleRole, FieldKind, Id, Rect, State};

use crate::audit::{Finding, Flaw, report};
use crate::headless::{Error, Headless};
use crate::reading::{EditorReading, FieldReading, Item, Place, Reading};

fn item(role: AccessibleRole, name: Option<&str>, words: &str, pressable: bool) -> Item {
    Item::of(&Accessible {
        role,
        id: name.map(Id::named),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        words: words.to_owned(),
        state: State::default(),
        pressable,
    })
}

fn field(name: &str, label: Option<&str>) -> FieldReading {
    FieldReading {
        id: Id::named(name),
        kind: FieldKind::Line,
        label: label.map(str::to_owned),
        hint: None,
        value: None,
        covered: false,
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
    }
}

fn editor(name: &str, label: Option<&str>) -> EditorReading {
    EditorReading {
        id: Id::named(name),
        label: label.map(str::to_owned),
        focused: false,
        first: 1,
        lines: vec!["a".to_owned()],
        total: 1,
        caret: Place { line: 1, column: 1 },
        selection: None,
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
    }
}

/// A reading with every flaw the audit knows, one of each (and a field and an editor with no name).
fn bad() -> Reading {
    Reading {
        items: vec![
            item(AccessibleRole::Button, Some("close"), "", true),
            item(AccessibleRole::Heading(2), None, "Settings", false),
            item(AccessibleRole::Text, None, "Settings", false),
            item(AccessibleRole::Checkbox, None, "Mark the row", false),
            item(AccessibleRole::Text, Some("logo"), "Logo", true),
            item(AccessibleRole::Button, Some("ok"), "OK", true),
        ],
        fields: vec![field("name", None)],
        editors: vec![editor("code", None)],
        ..Reading::default()
    }
}

/// A reading with none: the same elements, named, told apart by an id and each said once.
fn good() -> Reading {
    Reading {
        items: vec![
            item(AccessibleRole::Button, Some("close"), "Close", true),
            item(AccessibleRole::Heading(2), None, "Settings", false),
            item(
                AccessibleRole::Text,
                None,
                "Your changes will be lost.",
                false,
            ),
            item(
                AccessibleRole::Checkbox,
                Some("mark"),
                "Mark the row",
                false,
            ),
            item(AccessibleRole::Image, Some("logo"), "Logo", false),
            item(AccessibleRole::Button, Some("ok"), "OK", true),
        ],
        fields: vec![field("name", Some("Name"))],
        editors: vec![editor("code", Some("Source"))],
        ..Reading::default()
    }
}

#[test]
fn every_flaw_of_a_bad_reading_is_found_with_its_line_and_its_kind() {
    let reading = bad();
    let text = reading.text();
    let lines: Vec<&str> = text.lines().collect();
    let found = reading.audit();
    let kinds: Vec<(Flaw, &str)> = found
        .iter()
        .map(|finding| (finding.flaw, finding.role.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (Flaw::Nameless, "button"),
            (Flaw::Twice, "text"),
            (Flaw::NoId, "checkbox"),
            (Flaw::Misfit, "text"),
            (Flaw::Nameless, "field"),
            (Flaw::Nameless, "editor"),
        ],
        "{}",
        report(&found)
    );
    // Each is on the line of the reading that says it.
    let on = |finding: &Finding| lines[finding.line - 1].to_owned();
    assert!(on(&found[0]).contains("button"), "{}", on(&found[0]));
    assert!(on(&found[1]).contains("\"Settings\""), "{}", on(&found[1]));
    assert!(on(&found[2]).contains("checkbox"), "{}", on(&found[2]));
    assert!(on(&found[3]).contains("\"Logo\""), "{}", on(&found[3]));
    assert!(on(&found[4]).contains("field"), "{}", on(&found[4]));
    assert!(on(&found[5]).contains("editor"), "{}", on(&found[5]));
    // And a report says where, for a person to find it.
    assert!(report(&found).starts_with("line 3: "), "{}", report(&found));
}

#[test]
fn a_clean_reading_is_found_clean() {
    let reading = good();
    assert_eq!(reading.audit(), [], "{}", report(&reading.audit()));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_page_that_is_read_badly_says_is_found_on_the_page_itself() -> Result<(), Error> {
    // A button of no words, a text said twice, a checkbox of no id: written as a page writes them.
    let mut page = Headless::script(
        r#"
return {
    render = function(size, now, reduce_motion)
        return pencere.div():size_full():p(8):gap(8)
            :child(pencere.div():id("close"):press("close"):size(20, 20))
            :child(pencere.div():role("heading"):label("Settings"):child(pencere.text("Settings")))
            :child(pencere.div():role("checkbox"):label("Mark the row"):size(20, 20))
    end,
    settle = function() end,
    handle = function(event) return true end,
}
"#
        .to_owned(),
    )
    .size(400.0, 300.0)
    .open()?;
    let found = page.audit();
    let kinds: Vec<(Flaw, &str)> = found
        .iter()
        .map(|finding| (finding.flaw, finding.role.as_str()))
        .collect();
    assert!(
        kinds.contains(&(Flaw::Nameless, "button")) && kinds.contains(&(Flaw::NoId, "checkbox")),
        "{}",
        report(&found)
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_read_well_has_no_flaw() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
    render = function(size, now, reduce_motion)
        return pencere.div():size_full():p(8):gap(8)
            :child(pencere.div():id("close"):press("close"):label("Close"):size(20, 20))
            :child(pencere.text("Settings"))
            :child(pencere.div():id("mark"):role("checkbox"):label("Mark the row"):size(20, 20))
    end,
    settle = function() end,
    handle = function(event) return true end,
}
"#
        .to_owned(),
    )
    .size(400.0, 300.0)
    .open()?;
    assert_eq!(page.audit(), [], "{}", report(&page.audit()));
    Ok(())
}
