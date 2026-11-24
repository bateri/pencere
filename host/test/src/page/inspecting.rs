//! What a test reads of its page as the author built it: each element, the ones with no name too,
//! where it stands in the tree the page wrote, its box, its room, the style that placed it and what a
//! screen reader says for it, and what the frames cost; as a table to look into, or as the text an
//! agent reads (`pencere_drive::Inspection`).

use pencere_cli::mlua::{self, Lua, Table, UserDataMethods, Value};
use pencere_drive::reading::reference;
use pencere_drive::{Cost, Inspection, Item};
use pencere_view::{Length, Placing, Point, Track};

use super::{Handle, rect_table};

/// A list of numbers as a table `{ top =, right =, bottom =, left = }`.
fn sides(lua: &Lua, [top, right, bottom, left]: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("top", top)?;
    table.set("right", right)?;
    table.set("bottom", bottom)?;
    table.set("left", left)?;
    Ok(table)
}

/// A length as the page wrote it: points as a number, a share as `"50%"`, none (`auto`) as nothing.
fn length(lua: &Lua, length: Length) -> mlua::Result<Value> {
    Ok(match length {
        Length::Auto => Value::Nil,
        Length::Px(points) => Value::Number(f64::from(points)),
        Length::Percent(share) => Value::String(lua.create_string(format!("{share}%"))?),
    })
}

fn track(track: Track) -> String {
    match track {
        Track::Px(points) => format!("{points}px"),
        Track::Fr(share) => format!("{share}fr"),
        Track::Range { min, fr } => format!("minmax({min}px, {fr}fr)"),
        Track::Auto => "auto".to_owned(),
    }
}

/// A word of an enum with no data: `Between` is `"between"`.
fn word(value: &impl std::fmt::Debug) -> String {
    format!("{value:?}").to_lowercase()
}

fn placing_table(lua: &Lua, placing: &Placing) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("display", word(&placing.display))?;
    table.set("direction", word(&placing.direction))?;
    table.set("wrap", placing.wrap)?;
    table.set("absolute", placing.absolute)?;
    table.set("overlay", placing.overlay)?;
    let [top, right, bottom, left] = placing.inset;
    let inset = lua.create_table()?;
    inset.set("top", top)?;
    inset.set("right", right)?;
    inset.set("bottom", bottom)?;
    inset.set("left", left)?;
    table.set("inset", inset)?;
    table.set("width", length(lua, placing.width)?)?;
    table.set("height", length(lua, placing.height)?)?;
    table.set("min_width", length(lua, placing.min_width)?)?;
    table.set("min_height", length(lua, placing.min_height)?)?;
    table.set("max_width", length(lua, placing.max_width)?)?;
    table.set("max_height", length(lua, placing.max_height)?)?;
    table.set("padding", sides(lua, placing.padding)?)?;
    table.set("margin", sides(lua, placing.margin)?)?;
    let gap = lua.create_table()?;
    gap.set("x", placing.gap.0)?;
    gap.set("y", placing.gap.1)?;
    table.set("gap", gap)?;
    table.set("grow", placing.grow)?;
    table.set("shrink", placing.shrink)?;
    table.set("items", placing.items.as_ref().map(word))?;
    table.set("justify", placing.justify.as_ref().map(word))?;
    table.set(
        "columns",
        placing
            .columns
            .iter()
            .copied()
            .map(track)
            .collect::<Vec<_>>(),
    )?;
    table.set(
        "rows",
        placing.rows.iter().copied().map(track).collect::<Vec<_>>(),
    )?;
    table.set("column_span", placing.column_span)?;
    table.set("overflow", word(&placing.overflow))?;
    Ok(table)
}

/// The place of an element in the page's tree, as a list of the child's number at each step, from 1
/// as Lua counts.
fn path_table(lua: &Lua, path: &[usize]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (at, place) in path.iter().enumerate() {
        table.raw_set(at + 1, place + 1)?;
    }
    Ok(table)
}

fn item_table(lua: &Lua, item: &Item) -> mlua::Result<Table> {
    let element = &item.element;
    let table = lua.create_table()?;
    if let Some(path) = &element.path {
        table.set("path", path_table(lua, path)?)?;
    }
    // The name a reading gives it, where it has an id: what ties the element to a line of
    // `page:reading()`.
    table.set("name", element.id.map(reference))?;
    table.set("shape", element.shape.name())?;
    table.set(
        "field",
        element.field.map(|kind| match kind {
            pencere_view::FieldKind::Line => "line",
            pencere_view::FieldKind::Lines => "lines",
            pencere_view::FieldKind::Secret => "secret",
        }),
    )?;
    table.set("rect", rect_table(lua, element.rect)?)?;
    table.set("clip", rect_table(lua, element.clip)?)?;
    table.set("padding", sides(lua, element.padding)?)?;
    table.set("border", element.border.map(|border| border.width))?;
    if let Some(content) = element.content {
        let scroll = lua.create_table()?;
        scroll.set("x", element.scroll.0)?;
        scroll.set("y", element.scroll.1)?;
        scroll.set("content_w", content.w)?;
        scroll.set("content_h", content.h)?;
        table.set("scroll", scroll)?;
    }
    table.set("hidden", element.hidden)?;
    table.set("overlay", element.overlay)?;
    table.set("exiting", element.exiting)?;
    table.set("parent", element.parent.map(|parent| parent + 1))?;
    if let Some(placing) = &element.placing {
        table.set("placing", placing_table(lua, placing)?)?;
    }
    let said = lua.create_table()?;
    for (at, one) in item.said.iter().enumerate() {
        let line = lua.create_table()?;
        line.set("role", one.role.as_str())?;
        line.set("words", one.words.as_str())?;
        line.set("state", one.state.as_str())?;
        line.set("pressable", one.pressable)?;
        said.raw_set(at + 1, line)?;
    }
    table.set("said", said)?;
    Ok(table)
}

fn millis(took: std::time::Duration) -> f64 {
    took.as_secs_f64() * 1000.0
}

fn cost_table(lua: &Lua, cost: &Cost) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("elements", cost.elements)?;
    table.set("renders", cost.renders)?;
    table.set("layouts", cost.layouts)?;
    table.set("layout_ms", millis(cost.layout))?;
    table.set("paint_ms", millis(cost.paint))?;
    let script = lua.create_table()?;
    for (at, call) in cost.script.iter().enumerate() {
        let one = lua.create_table()?;
        one.set("kind", format!("{:?}", call.kind).to_lowercase())?;
        one.set("ms", millis(call.took))?;
        script.raw_set(at + 1, one)?;
    }
    table.set("script", script)?;
    Ok(table)
}

fn inspection_table(lua: &Lua, looked: &Inspection) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let size = lua.create_table()?;
    size.set("w", looked.size.w)?;
    size.set("h", looked.size.h)?;
    table.set("size", size)?;
    table.set("elements", looked.elements)?;
    let items = lua.create_table()?;
    for (at, item) in looked.items.iter().enumerate() {
        items.raw_set(at + 1, item_table(lua, item)?)?;
    }
    table.set("items", items)?;
    let frames = lua.create_table()?;
    for (at, cost) in looked.frames.iter().enumerate() {
        frames.raw_set(at + 1, cost_table(lua, cost)?)?;
    }
    table.set("frames", frames)?;
    Ok(table)
}

pub(super) fn add<M: UserDataMethods<Handle>>(methods: &mut M) {
    // page:measure(on): from now on (or no more), the frames' cost is measured and the page can be
    // inspected with it. Off, which it is to begin with, nothing is measured and no clock is read.
    methods.add_method("measure", |_, this, on: Option<bool>| {
        this.act(|page| {
            page.inspect(on.unwrap_or(true));
            Ok(())
        })
    });
    // page:inspect(): the page element by element (`size`, `elements`, `items`, `frames`). Asking turns
    // the measuring on if it is not, so that what comes after is measured; what came before is not.
    methods.add_method("inspect", |lua, this, ()| {
        let looked = this.act(|page| {
            if !page.page().inspecting() {
                page.inspect(true);
            }
            Ok(page.inspection())
        })?;
        match looked {
            Some(looked) => Ok(Value::Table(inspection_table(lua, &looked)?)),
            None => Ok(Value::Nil),
        }
    });
    // page:inspect_text(): the same as the text an agent reads (the same for every host).
    methods.add_method("inspect_text", |_, this, ()| {
        this.act(|page| {
            if !page.page().inspecting() {
                page.inspect(true);
            }
            Ok(page.inspection().map(|looked| looked.text()))
        })
    });
    // page:inspect_at(x, y): the topmost element that shows at a point of the page (the host's
    // points), named or not, as `inspect` says it; nothing where none shows. It does not turn the
    // measuring on: the page is looked at for the one answer and no more.
    methods.add_method("inspect_at", |lua, this, (x, y): (f64, f64)| {
        #[allow(clippy::cast_possible_truncation)]
        let at = Point::new(x as f32, y as f32);
        let found = this.act(|page| {
            let measuring = page.page().inspecting();
            if !measuring {
                page.inspect(true);
            }
            let path = page.inspect_at(at);
            let found = path.and_then(|path| {
                page.inspection().and_then(|looked| {
                    looked
                        .items
                        .into_iter()
                        .find(|item| item.element.path.as_deref() == Some(path.as_slice()))
                })
            });
            if !measuring {
                page.inspect(false);
            }
            Ok(found)
        })?;
        match found {
            Some(item) => Ok(Value::Table(item_table(lua, &item)?)),
            None => Ok(Value::Nil),
        }
    });
}
