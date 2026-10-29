//! What a test says of the host it runs the page under: how its terminal draws its cursor and its
//! selection (`terminal_look`), and what it draws that not every host does (`draws`).

use pencere_cli::mlua::{self, Table, Value};
use pencere_view::edit::{CaretMotion, CaretShape, CaretUnfocused};
use pencere_view::{Drawing, TerminalCaret, TerminalLook, TerminalSelection};

fn words(text: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(text.into())
}

/// The word `name` of `table` as one of `choices`, or none where it says none.
fn choice<T: Copy>(table: &Table, name: &str, choices: &[(&str, T)]) -> mlua::Result<Option<T>> {
    let Some(said) = table.get::<Option<String>>(name)? else {
        return Ok(None);
    };
    choices
        .iter()
        .find(|(word, _)| *word == said)
        .map(|(_, value)| Some(*value))
        .ok_or_else(|| {
            let all: Vec<String> = choices
                .iter()
                .map(|(word, _)| format!("\"{word}\""))
                .collect();
            words(format!("{name}: \"{said}\" is none of {}", all.join(", ")))
        })
}

#[allow(clippy::cast_possible_truncation)]
fn number(table: &Table, name: &str) -> mlua::Result<Option<f32>> {
    Ok(table.get::<Option<f64>>(name)?.map(|number| number as f32))
}

/// A caret's motion: `"snap"`, `"ease"`, `"spring"` (a terminal's numbers), `{ ease = seconds }` or
/// `{ spring = stiffness }`.
fn motion(table: &Table) -> mlua::Result<Option<CaretMotion>> {
    match table.get::<Value>("motion")? {
        Value::Nil => Ok(None),
        Value::String(word) => match &*word.to_str()? {
            "snap" => Ok(Some(CaretMotion::Snap)),
            "ease" => Ok(Some(CaretMotion::Ease(CaretMotion::EASE))),
            "spring" => Ok(Some(CaretMotion::Spring(CaretMotion::SPRING))),
            other => Err(words(format!(
                "motion: \"{other}\" is none of \"snap\", \"ease\", \"spring\""
            ))),
        },
        Value::Table(table) => match (number(&table, "ease")?, number(&table, "spring")?) {
            (Some(seconds), None) => Ok(Some(CaretMotion::Ease(seconds))),
            (None, Some(stiffness)) => Ok(Some(CaretMotion::Spring(stiffness))),
            _ => Err(words(
                "motion is { ease = seconds } or { spring = stiffness }",
            )),
        },
        _ => Err(words(
            "motion is \"snap\", \"ease\", \"spring\", { ease = seconds } or { spring = stiffness }",
        )),
    }
}

/// How the host's terminal draws its cursor and its selection, from a test's table, as the page reads
/// it (`pencere.terminal_look()`): `{ caret = { shape =, width =, radius =, glow =, blink =,
/// blink_interval =, unfocused =, motion = }, selection = { radius = } }`.
pub fn terminal_look(table: &Table) -> mlua::Result<TerminalLook> {
    let mut look = TerminalLook::default();
    if let Some(caret) = table.get::<Option<Table>>("caret")? {
        look.caret = TerminalCaret {
            shape: choice(
                &caret,
                "shape",
                &[
                    ("bar", CaretShape::Bar),
                    ("block", CaretShape::Block),
                    ("underline", CaretShape::Underline),
                ],
            )?,
            width: number(&caret, "width")?,
            radius: number(&caret, "radius")?,
            glow: number(&caret, "glow")?,
            blink: caret.get::<Option<bool>>("blink")?,
            blink_interval: number(&caret, "blink_interval")?,
            unfocused: choice(
                &caret,
                "unfocused",
                &[
                    ("hidden", CaretUnfocused::Hidden),
                    ("hollow", CaretUnfocused::Hollow),
                    ("solid", CaretUnfocused::Solid),
                ],
            )?,
            motion: motion(&caret)?,
        };
    }
    if let Some(selection) = table.get::<Option<Table>>("selection")? {
        look.selection = TerminalSelection {
            radius: number(&selection, "radius")?,
        };
    }
    Ok(look)
}

/// What the host draws beyond what every host does, from a test's list of words (`{ "shadow" }`).
pub fn draws(names: &[String]) -> mlua::Result<Vec<Drawing>> {
    names
        .iter()
        .map(|name| match name.as_str() {
            "shadow" => Ok(Drawing::Shadow),
            other => Err(words(format!("draws: \"{other}\" is none of \"shadow\""))),
        })
        .collect()
}
