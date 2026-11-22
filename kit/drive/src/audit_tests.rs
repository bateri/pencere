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
        parent: None,
        detail: Default::default(),
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
fn an_item_of_the_page_behind_a_modal_that_is_read_is_found_and_the_models_own_reading_never_has_one()
 {
    let mut reading = good();
    reading.items[5].behind = true;
    let found = reading.audit();
    assert_eq!(found.len(), 1, "{}", report(&found));
    assert_eq!(found[0].flaw, Flaw::Behind);
    assert!(report(&found).contains("behind"), "{}", report(&found));
    // The model's own reading has none: it has taken the page behind a modal out.
    assert!(good().items.iter().all(|item| !item.behind));
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

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_is_read_under_a_scrim_that_was_not_made_a_layer_is_covered_and_is_not_where_it_is_one()
-> Result<(), Error> {
    // A page behind a scrim that answers the pointer: read in full, and a press on it goes to the scrim.
    let page = |marks: &str| {
        format!(
            r#"
return {{
    render = function(size, now, reduce_motion)
        local scrim = pencere.div():id("scrim"):press("scrim"):overlay():inset(0, 0, 0, 0):items_center():pt(40)
            :child(pencere.div():id("card"):press("card"):role("dialog"):label("Delete it?"):w(200):h(60))
        {marks}
        return pencere.div():size_full():p(8):gap(8)
            :child(pencere.div():id("behind"):press("behind"):label("Behind"):size(80, 24))
            :child(scrim)
    end,
    settle = function() end,
    handle = function(event) return true end,
}}
"#
        )
    };
    let mut forgotten = Headless::script(page("")).size(400.0, 300.0).open()?;
    let found = forgotten.audit();
    assert!(
        found
            .iter()
            .any(|finding| finding.flaw == Flaw::Covered && finding.words == "Behind"),
        "{}",
        report(&found)
    );
    // The same page with its scrim marked: the page behind is not read, and nothing is found.
    let mut marked = Headless::script(page("scrim:role(\"none\"):layer(true)"))
        .size(400.0, 300.0)
        .open()?;
    assert_eq!(marked.audit(), [], "{}", report(&marked.audit()));
    Ok(())
}

/// A page whose `render` returns what `body` makes of `root` (Lua statements, `root` is the page's box).
fn page_with(body: &str) -> Result<Headless, Error> {
    Headless::script(format!(
        r#"
return {{
    render = function(size, now, reduce_motion)
        local root = pencere.div():size_full():p(8):gap(8)
            :child(pencere.div():id("behind"):press("behind"):label("Behind"):size(120, 24))
            :child(pencere.div():id("name"):text_field("name"):label("Name"):size(120, 24))
        {body}
        return root
    end,
    settle = function() end,
    handle = function(event) return true end,
}}
"#
    ))
    .size(400.0, 300.0)
    .open()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_box_over_what_is_read_covers_it_only_where_it_answers_the_pointer_and_is_in_no_passing_layer()
-> Result<(), Error> {
    let covers = |marks: &str| {
        format!(
            r#"root:child(pencere.div():id("over"):press("over"):overlay():inset(0, 0, 0, 0){marks}:label("Over"))"#
        )
    };
    // A scrim over the page that answers the pointer covers a button and a field behind it.
    let mut page = page_with(&covers(""))?;
    let found = page.audit();
    let covered: Vec<&str> = found
        .iter()
        .filter(|finding| finding.flaw == Flaw::Covered)
        .map(|finding| finding.words.as_str())
        .collect();
    assert!(
        covered.contains(&"Behind") && covered.contains(&"Name"),
        "{}",
        report(&found)
    );
    // A box that does not answer the pointer (a shade, a fade at an edge) covers nothing.
    let mut shade =
        page_with(r#"root:child(pencere.div():overlay():inset(0, 0, 0, 0):bg("shadow"))"#)?;
    assert_eq!(shade.audit(), [], "{}", report(&shade.audit()));
    // A layer that is not modal (a popover, a toast) covers what it stands over, which is read as well.
    let mut popover = page_with(&covers(":layer(false)"))?;
    assert_eq!(popover.audit(), [], "{}", report(&popover.audit()));
    // A small box that answers the pointer over the middle of a wide one leaves the rest of it reached
    // (the badge is over the middle point and the two points above and below it, not the two at its sides).
    let mut floating = page_with(
        r#"root:child(pencere.div():id("badge"):press("badge"):label("Badge"):overlay():inset(10, nil, nil, 58):size(20, 20))"#,
    )?;
    assert_eq!(floating.audit(), [], "{}", report(&floating.audit()));
    // A box that answers the pointer and is disabled does not answer it, and covers nothing.
    let mut disabled = page_with(&covers(":disabled(true)"))?;
    assert_eq!(disabled.audit(), [], "{}", report(&disabled.audit()));
    Ok(())
}
