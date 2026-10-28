//! The page of a test, as Lua holds it: the headless host with the program of the page, and
//! what a test may do to either.

use std::cell::RefCell;
use std::process::ExitStatus;
use std::rc::Rc;

use pencere_cli::mlua::{self, Lua, Table, UserData, UserDataMethods, Value};
use pencere_cli::{from_json, to_json};
use pencere_drive::headless::{Error, Focus, Headless};
use pencere_drive::shot::{Fonts, Palette};
use pencere_view::edit::EditCommand;
use pencere_view::{AccessibleRole, FieldKind, Id, Point, Rect};
use serde_json::Value as Json;

use crate::peer::Any;
use crate::program::App;

/// A page under the host with no screen, and everything that is running to serve it.
pub struct Open {
    /// The page; none once it is let go of.
    pub page: Option<Headless<Any>>,
    /// The app's own program, when the page's program is that.
    pub app: Option<App>,
    /// How to open the page again with the same program.
    pub remake: Option<Remake>,
    /// The program's words, for a failure: the file its `print` goes to.
    pub log: Option<std::path::PathBuf>,
    /// Where the pictures of the test go.
    pub shots: Rc<dyn Fn(&str) -> std::path::PathBuf>,
}

impl Open {
    /// Lets go of the page and of the program that served it.
    pub fn close(&mut self) {
        // The page first: it holds the conversation with the program, which the program's end waits
        // for.
        self.page = None;
        if let Some(app) = self.app.take() {
            app.stop();
        }
    }
}

impl Open {
    /// The program closed its view: its `serve` returns, as it does under a terminal.
    fn let_serve_end<R>(&mut self, done: &Result<R, Error>) {
        if let Err(Error::Closed(why)) = done
            && why.starts_with("the program closed its view")
            && let Some(app) = self.app.as_mut()
        {
            app.end_view();
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        self.close();
    }
}

/// How a page is made ready again with the settings it was opened with.
pub type Remake = Box<
    dyn Fn() -> Result<pencere_drive::headless::Opening<pencere_drive::headless::Calls>, String>,
>;

/// The handle Lua has of a page.
#[derive(Clone)]
pub struct Handle(pub Rc<RefCell<Open>>);

fn trouble(error: &Error) -> mlua::Error {
    mlua::Error::runtime(error.to_string())
}

fn words(text: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(text.into())
}

impl Handle {
    /// Does something to the page.
    fn act<R>(&self, f: impl FnOnce(&mut Headless<Any>) -> Result<R, Error>) -> mlua::Result<R> {
        let mut open = self.0.try_borrow_mut().map_err(|_| {
            words("the page is in use: a stand-in handler of a call cannot drive the page")
        })?;
        let page = open
            .page
            .as_mut()
            .ok_or_else(|| words("the page is over: the test that opened it has ended"))?;
        let done = f(page);
        open.let_serve_end(&done);
        done.map_err(|error| trouble(&error))
    }

    /// Takes a frame's time, as `Headless::tick` does, but a page whose view is over is no failure
    /// of it: what is left to wait for is the program, which goes on after its view and on its own
    /// time (what it does once its `serve` has returned). Why the view is over, if it is: the
    /// moment is given to the program all the same.
    fn wait(&self) -> mlua::Result<Option<String>> {
        let mut open = self.0.try_borrow_mut().map_err(|_| {
            words("the page is in use: a stand-in handler of a call cannot drive the page")
        })?;
        let page = open
            .page
            .as_mut()
            .ok_or_else(|| words("the page is over: the test that opened it has ended"))?;
        let done = page.tick();
        open.let_serve_end(&done);
        match done {
            Err(Error::Closed(why)) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                Ok(Some(why))
            }
            other => other.map(|()| None).map_err(|error| trouble(&error)),
        }
    }

    /// Looks at the page.
    fn look<R>(&self, f: impl FnOnce(&Headless<Any>) -> R) -> mlua::Result<R> {
        let open = self.0.try_borrow().map_err(|_| {
            words("the page is in use: a stand-in handler of a call cannot drive the page")
        })?;
        let page = open
            .page
            .as_ref()
            .ok_or_else(|| words("the page is over: the test that opened it has ended"))?;
        Ok(f(page))
    }
}

fn point(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

fn rect_table(lua: &Lua, rect: Rect) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("x", rect.x)?;
    table.set("y", rect.y)?;
    table.set("w", rect.w)?;
    table.set("h", rect.h)?;
    Ok(table)
}

fn role_name(role: AccessibleRole) -> (&'static str, Option<u8>) {
    match role {
        AccessibleRole::Button => ("button", None),
        AccessibleRole::Field(_) => ("field", None),
        AccessibleRole::Editor => ("editor", None),
        AccessibleRole::Text => ("text", None),
        AccessibleRole::Link => ("link", None),
        AccessibleRole::Checkbox => ("checkbox", None),
        AccessibleRole::Switch => ("switch", None),
        AccessibleRole::Radio => ("radio", None),
        AccessibleRole::Tab => ("tab", None),
        AccessibleRole::MenuItem => ("menuitem", None),
        AccessibleRole::Heading(level) => ("heading", Some(level)),
        AccessibleRole::Image => ("image", None),
        AccessibleRole::Progress => ("progress", None),
        AccessibleRole::Slider => ("slider", None),
    }
}

fn kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Line => "line",
        FieldKind::Lines => "lines",
        FieldKind::Secret => "secret",
    }
}

/// What the page has on it now, for the locators of the test (they look for things in Lua): what
/// a screen reader is told of it, its fields, and the words drawn with where they start.
fn snapshot(lua: &Lua, page: &Headless<Any>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let place = page.page().placement();
    let accessible = lua.create_table()?;
    let mut count = 0;
    for item in &page.outline().accessible {
        let table = rect_table(lua, item.rect)?;
        let (role, level) = role_name(item.role);
        table.set("role", role)?;
        table.set("level", level)?;
        table.set("name", item.words.as_str())?;
        table.set("words", item.words.as_str())?;
        let state = lua.create_table()?;
        state.set("checked", item.state.checked)?;
        state.set("selected", item.state.selected)?;
        state.set("expanded", item.state.expanded)?;
        state.set("disabled", item.state.disabled)?;
        state.set("value", item.state.value.as_deref())?;
        table.set("state", state)?;
        // Whether a press at its middle reaches it: only a thing with a name can be told.
        let middle = point(
            item.rect.x + item.rect.w / 2.0,
            item.rect.y + item.rect.h / 2.0,
        );
        let reachable = match (item.id, page.laid()) {
            (Some(id), Some(laid)) => laid.ids_at(place.to_page(middle)).contains(&id),
            _ => true,
        };
        table.set("reachable", reachable)?;
        count += 1;
        accessible.raw_set(count, table)?;
    }
    out.set("accessible", accessible)?;
    let fields = lua.create_table()?;
    for (index, field) in page.outline().fields.iter().enumerate() {
        let table = rect_table(lua, field.rect)?;
        table.set("kind", kind_name(field.kind))?;
        table.set("hint", field.hint.as_deref())?;
        table.set("label", field.label.as_deref())?;
        table.set("index", index + 1)?;
        fields.raw_set(index + 1, table)?;
    }
    out.set("fields", fields)?;
    let runs = lua.create_table()?;
    let mut count = 0;
    for item in &page.list().items {
        if let pencere_view::Paint::Text(run) = item {
            let size = run.font.size;
            let table = lua.create_table()?;
            table.set("text", run.text.as_str())?;
            table.set("x", run.origin.x)?;
            table.set("y", run.origin.y)?;
            table.set("w", (run.clip.x + run.clip.w - run.origin.x).max(1.0))?;
            table.set("h", size * 1.25)?;
            count += 1;
            runs.raw_set(count, table)?;
        }
    }
    out.set("runs", runs)?;
    Ok(out)
}

/// The parts of the messages the page sent that are calls: their numbers, methods and params, and
/// whether the program has answered.
fn calls(lua: &Lua, page: &Headless<Any>) -> mlua::Result<Table> {
    let answered: Vec<u64> = page
        .received()
        .iter()
        .filter_map(|text| serde_json::from_str::<Json>(text).ok())
        .filter_map(|message| message.get("response")?.get("id")?.as_u64())
        .collect();
    let cancelled: Vec<u64> = page
        .sent()
        .iter()
        .filter_map(|text| serde_json::from_str::<Json>(text).ok())
        .filter_map(|message| message.get("cancel")?.get("id")?.as_u64())
        .collect();
    let list = lua.create_table()?;
    let mut count = 0;
    for text in page.sent() {
        let Ok(message) = serde_json::from_str::<Json>(text) else {
            continue;
        };
        let Some(call) = message.get("call") else {
            continue;
        };
        let id = call.get("id").and_then(Json::as_u64).unwrap_or(0);
        let table = lua.create_table()?;
        table.set("id", id)?;
        table.set(
            "method",
            call.get("method").and_then(Json::as_str).unwrap_or(""),
        )?;
        table.set(
            "params",
            from_json(lua, call.get("params").unwrap_or(&Json::Null))?,
        )?;
        table.set("answered", answered.contains(&id))?;
        table.set("cancelled", cancelled.contains(&id))?;
        count += 1;
        list.raw_set(count, table)?;
    }
    Ok(list)
}

fn subscriptions(lua: &Lua, page: &Headless<Any>) -> mlua::Result<Table> {
    let cancelled: Vec<u64> = page
        .sent()
        .iter()
        .filter_map(|text| serde_json::from_str::<Json>(text).ok())
        .filter_map(|message| message.get("cancel")?.get("id")?.as_u64())
        .collect();
    let list = lua.create_table()?;
    let mut count = 0;
    for text in page.sent() {
        let Ok(message) = serde_json::from_str::<Json>(text) else {
            continue;
        };
        let Some(subscribe) = message.get("subscribe") else {
            continue;
        };
        let id = subscribe.get("id").and_then(Json::as_u64).unwrap_or(0);
        let table = lua.create_table()?;
        table.set("id", id)?;
        table.set(
            "topic",
            subscribe.get("topic").and_then(Json::as_str).unwrap_or(""),
        )?;
        table.set(
            "params",
            from_json(lua, subscribe.get("params").unwrap_or(&Json::Null))?,
        )?;
        table.set("live", !cancelled.contains(&id))?;
        count += 1;
        list.raw_set(count, table)?;
    }
    Ok(list)
}

/// The editor command a test names: `"select_all"`, `"undo"`, `"insert"` with its text.
fn edit_command(name: &str, argument: Option<String>) -> Result<EditCommand, String> {
    let need = |what: &str| {
        argument
            .clone()
            .ok_or_else(|| format!("edit {name:?} takes {what}"))
    };
    Ok(match name {
        "insert" => EditCommand::Insert(need("the text to insert")?),
        "newline" => EditCommand::Newline,
        "delete_backward" => EditCommand::DeleteBackward,
        "delete_forward" => EditCommand::DeleteForward,
        "delete_word_backward" => EditCommand::DeleteWordBackward,
        "delete_word_forward" => EditCommand::DeleteWordForward,
        "indent" => EditCommand::Indent,
        "outdent" => EditCommand::Outdent,
        "move_lines_up" => EditCommand::MoveLinesUp,
        "move_lines_down" => EditCommand::MoveLinesDown,
        "duplicate_lines" => EditCommand::DuplicateLines,
        "toggle_comment" => EditCommand::ToggleComment,
        "toggle_block_comment" => EditCommand::ToggleBlockComment,
        "select_all" => EditCommand::SelectAll,
        "add_caret_above" => EditCommand::AddCaretAbove,
        "add_caret_below" => EditCommand::AddCaretBelow,
        "select_next" => EditCommand::SelectNext,
        "single_selection" => EditCommand::SingleSelection,
        "undo" => EditCommand::Undo,
        "redo" => EditCommand::Redo,
        "fold" => EditCommand::Fold,
        "unfold" => EditCommand::Unfold,
        "toggle_fold" => EditCommand::ToggleFold,
        "unfold_all" => EditCommand::UnfoldAll,
        "find" => EditCommand::Find {
            pattern: need("the pattern to find")?,
            regex: false,
            case: false,
            word: false,
        },
        "find_next" => EditCommand::FindNext,
        "find_previous" => EditCommand::FindPrevious,
        "stop_finding" => EditCommand::StopFinding,
        "replace" => EditCommand::Replace(need("the text to put in its place")?),
        "replace_all" => EditCommand::ReplaceAll(need("the text to put in its place")?),
        "select_found" => EditCommand::SelectFound,
        "select_occurrences" => EditCommand::SelectOccurrences,
        other => {
            return Err(format!(
                "there is no editor command {other:?} (insert, newline, delete_backward, select_all, undo, redo, fold, toggle_fold, find, replace …); moves are keys: page:press(\"left\")"
            ));
        }
    })
}

fn to_f32(value: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let narrowed = value as f32;
    narrowed
}

fn index_of(value: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = value.max(0.0) as usize;
    index
}

impl UserData for Handle {
    #[allow(clippy::too_many_lines)]
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ---- the keyboard ----
        methods.add_method("press", |_, this, keys: String| {
            this.act(|page| page.press(&keys).map(drop))
        });
        methods.add_method("type_text", |_, this, text: String| {
            this.act(|page| page.type_text(&text))
        });
        methods.add_method("insert", |_, this, text: String| {
            this.act(|page| page.insert_text(&text))
        });
        methods.add_method("paste", |_, this, text: String| {
            this.act(|page| page.paste(&text))
        });
        methods.add_method(
            "compose",
            |_, this, (text, from, to, replace): (String, f64, f64, Option<Table>)| {
                let replace = match replace {
                    Some(range) => Some((
                        index_of(range.get::<f64>(1)?),
                        index_of(range.get::<f64>(2)?),
                    )),
                    None => None,
                };
                this.act(|page| page.compose(&text, (index_of(from), index_of(to)), replace))
            },
        );
        methods.add_method("unmark", |_, this, ()| this.act(Headless::unmark));
        methods.add_method("focus_editor", |_, this, name: String| {
            this.act(|page| page.focus_editor(&name))
        });
        methods.add_method("select", |_, this, (name, a, b): (String, f64, f64)| {
            this.act(|page| page.select(&name, index_of(a), index_of(b)))
        });
        methods.add_method("caret_to", |_, this, (name, at): (String, f64)| {
            this.act(|page| page.caret_to(&name, index_of(at)))
        });
        methods.add_method(
            "edit",
            |_, this, (name, argument): (String, Option<String>)| {
                let command = edit_command(&name, argument).map_err(words)?;
                this.act(|page| page.edit(command))
            },
        );
        methods.add_method("command", |_, this, name: String| {
            this.act(|page| page.command(&name))
        });
        methods.add_method("pick", |_, this, which: Value| match which {
            Value::String(title) => {
                let title = title.to_str()?.to_owned();
                this.act(|page| page.pick_titled(&title))
            }
            Value::Integer(_) | Value::Number(_) => {
                let number = match which {
                    Value::Integer(n) => f64::from(i32::try_from(n).unwrap_or(0)),
                    Value::Number(n) => n,
                    _ => 0.0,
                };
                // The test counts from 1, as Lua does.
                this.act(|page| page.pick(index_of(number).saturating_sub(1)))
            }
            _ => Err(words(
                "pick takes the number of an entry (from 1) or its title",
            )),
        });

        // ---- the pointer, by name and by place ----
        methods.add_method("click_id", |_, this, name: String| {
            this.act(|page| page.click(&name))
        });
        methods.add_method("double_click_id", |_, this, name: String| {
            this.act(|page| page.double_click(&name))
        });
        methods.add_method("right_click_id", |_, this, name: String| {
            this.act(|page| page.right_click(&name))
        });
        methods.add_method("hover_id", |_, this, name: String| {
            this.act(|page| page.hover(&name))
        });
        methods.add_method(
            "click_gutter",
            |_, this, (editor, column, line, button): (String, String, usize, Option<String>)| {
                let button = match button.as_deref() {
                    None | Some("left") => pencere_view::Button::Left,
                    Some("right") => pencere_view::Button::Right,
                    Some("middle") => pencere_view::Button::Middle,
                    Some(other) => {
                        return Err(words(format!(
                            "click_gutter takes the button \"left\", \"right\" or \"middle\", not \"{other}\""
                        )));
                    }
                };
                this.act(|page| page.click_gutter(&editor, &column, line, button))
            },
        );
        methods.add_method(
            "hover_gutter",
            |_, this, (editor, column, line): (String, String, usize)| {
                this.act(|page| page.hover_gutter(&editor, &column, line))
            },
        );
        methods.add_method(
            "gutter_cell",
            |lua, this, (editor, column, line): (String, String, usize)| match this
                .look(|page| page.gutter_cell(&editor, &column, line))?
            {
                Some(rect) => Ok(Value::Table(rect_table(lua, rect)?)),
                None => Ok(Value::Nil),
            },
        );
        methods.add_method(
            "zone",
            |lua, this, (editor, zone): (String, String)| match this
                .look(|page| page.zone(&editor, &zone))?
            {
                Some(rect) => Ok(Value::Table(rect_table(lua, rect)?)),
                None => Ok(Value::Nil),
            },
        );
        methods.add_method("click_at", |_, this, (x, y): (f64, f64)| {
            this.act(|page| page.click_at(point(to_f32(x), to_f32(y))))
        });
        methods.add_method("double_click_at", |_, this, (x, y): (f64, f64)| {
            this.act(|page| {
                use pencere_view::{Button, Input, Mods};
                let at = point(to_f32(x), to_f32(y));
                for count in [1, 2] {
                    page.pointer(Input::Press {
                        at,
                        button: Button::Left,
                        mods: Mods::default(),
                        count,
                    })?;
                    page.pointer(Input::Release {
                        at,
                        button: Button::Left,
                    })?;
                }
                Ok(())
            })
        });
        methods.add_method("right_click_at", |_, this, (x, y): (f64, f64)| {
            this.act(|page| {
                use pencere_view::{Button, Input, Mods};
                let at = point(to_f32(x), to_f32(y));
                page.pointer(Input::Press {
                    at,
                    button: Button::Right,
                    mods: Mods::default(),
                    count: 1,
                })?;
                page.pointer(Input::Release {
                    at,
                    button: Button::Right,
                })
            })
        });
        methods.add_method("move_to", |_, this, (x, y): (f64, f64)| {
            this.act(|page| page.move_to(point(to_f32(x), to_f32(y))))
        });
        methods.add_method("leave", |_, this, ()| this.act(Headless::leave));
        methods.add_method("drag_ids", |_, this, (from, to): (String, String)| {
            this.act(|page| page.drag(&from, &to))
        });
        methods.add_method(
            "drag_between",
            |_, this, (x1, y1, x2, y2): (f64, f64, f64, f64)| {
                this.act(|page| {
                    page.drag_between(point(to_f32(x1), to_f32(y1)), point(to_f32(x2), to_f32(y2)))
                })
            },
        );
        methods.add_method("wheel_id", |_, this, (name, dx, dy): (String, f64, f64)| {
            this.act(|page| page.wheel(&name, to_f32(dx), to_f32(dy)))
        });
        methods.add_method(
            "wheel_at",
            |_, this, (x, y, dx, dy): (f64, f64, f64, f64)| {
                this.act(|page| page.wheel_at(point(to_f32(x), to_f32(y)), to_f32(dx), to_f32(dy)))
            },
        );
        methods.add_method("scroll_to", |_, this, (name, y): (String, f64)| {
            this.act(|page| page.scroll_to(&name, to_f32(y)))
        });
        methods.add_method("fill_id", |_, this, (name, text): (String, String)| {
            this.act(|page| page.fill(&name, &text))
        });
        methods.add_method("fill_field", |_, this, (index, text): (f64, String)| {
            this.act(|page| {
                let id = page
                    .outline()
                    .fields
                    .get(index_of(index).saturating_sub(1))
                    .map(|field| field.id)
                    .ok_or_else(|| Error::NoSuch("the field".to_owned()))?;
                page.fill_field(id, &text)
            })
        });
        methods.add_method(
            "drop_files",
            |_, this, (name, paths): (String, Vec<String>)| {
                this.act(|page| page.drop_files(&name, paths))
            },
        );

        // ---- the view and the clock ----
        methods.add_method("resize", |_, this, (w, h): (f64, f64)| {
            this.act(|page| page.resize(to_f32(w), to_f32(h)))
        });
        methods.add_method("zoom", |_, this, zoom: f64| {
            this.act(|page| page.zoom(to_f32(zoom)))
        });
        methods.add_method("set_active", |_, this, active: bool| {
            this.act(|page| page.set_active(active))
        });
        methods.add_method("refresh", |_, this, ()| this.act(Headless::refresh));
        methods.add_method("settle", |_, this, ()| this.act(Headless::settle));
        methods.add_method("advance", |_, this, seconds: f64| {
            this.act(|page| page.advance(seconds))
        });
        methods.add_method("step", |_, this, ()| this.act(Headless::step));
        methods.add_method("idle", |_, this, ()| this.act(Headless::idle));
        methods.add_method("hide", |_, this, seconds: f64| {
            this.act(|page| page.hide(seconds))
        });
        methods.add_method("tick", |_, this, ()| this.act(Headless::tick));
        methods.add_method("wait", |_, this, ()| this.wait());
        methods.add_method("now", |_, this, ()| this.look(Headless::now));
        methods.add_method("asks_at", |_, this, ()| this.look(Headless::asks_at));
        methods.add_method("tell", |_, this, message: String| {
            this.act(|page| page.tell(&message))
        });

        // ---- what the page shows ----
        methods.add_method("words", |_, this, ()| {
            this.look(|page| {
                page.words()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
        });
        methods.add_method("says", |_, this, text: String| {
            this.look(|page| page.says(&text))
        });
        methods.add_method("words_in", |_, this, name: String| {
            this.look(|page| page.words_in(&name))
        });
        methods.add_method("tree_words", |_, this, ()| this.look(Headless::tree_words));
        methods.add_method("bounds", |lua, this, name: String| {
            match this.look(|page| page.bounds(&name))? {
                Some(rect) => Ok(Value::Table(rect_table(lua, rect)?)),
                None => Ok(Value::Nil),
            }
        });
        methods.add_method("reaches", |_, this, name: String| {
            this.look(|page| {
                let Some(rect) = page.bounds(&name) else {
                    return false;
                };
                let middle = point(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                let place = page.page().placement();
                page.laid().is_some_and(|laid| {
                    laid.ids_at(place.to_page(middle))
                        .contains(&Id::named(&name))
                })
            })
        });
        methods.add_method("editor", |lua, this, name: String| {
            let found = this.look(|page| {
                page.editor(&name).map(|buffer| {
                    (
                        buffer.text(),
                        buffer.version(),
                        buffer
                            .ranges()
                            .iter()
                            .map(|range| (range.anchor, range.head))
                            .collect::<Vec<_>>(),
                    )
                })
            })?;
            let Some((text, version, ranges)) = found else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("text", text)?;
            table.set("version", version)?;
            let selections = lua.create_table()?;
            for (index, (anchor, head)) in ranges.into_iter().enumerate() {
                let range = lua.create_table()?;
                range.set("anchor", anchor)?;
                range.set("head", head)?;
                selections.raw_set(index + 1, range)?;
            }
            table.set("selections", selections)?;
            Ok(Value::Table(table))
        });
        methods.add_method("editor_text", |_, this, name: String| {
            this.look(|page| page.editor_text(&name))
        });
        methods.add_method("focus", |lua, this, ()| {
            let focus = this.look(Headless::focus)?;
            let table = lua.create_table()?;
            table.set(
                "kind",
                match focus {
                    Focus::Page => "page",
                    Focus::Field(_) => "field",
                    Focus::Editor(_) => "editor",
                },
            )?;
            Ok(table)
        });
        methods.add_method("has_focus", |_, this, name: String| {
            this.look(|page| match page.focus() {
                Focus::Page => name == "page",
                Focus::Field(id) | Focus::Editor(id) => id == Id::named(&name),
            })
        });
        methods.add_method("field", |_, this, name: String| {
            this.look(|page| page.field(&name).map(str::to_owned))
        });
        methods.add_method("outline", |lua, this, ()| {
            let (fields, tips, commands) = this.look(|page| {
                (
                    page.outline().fields.clone(),
                    page.outline().tips.clone(),
                    page.outline().commands.clone(),
                )
            })?;
            let out = lua.create_table()?;
            let list = lua.create_table()?;
            for (index, field) in fields.iter().enumerate() {
                let table = rect_table(lua, field.rect)?;
                table.set("kind", kind_name(field.kind))?;
                table.set("hint", field.hint.as_deref())?;
                table.set("label", field.label.as_deref())?;
                list.raw_set(index + 1, table)?;
            }
            out.set("fields", list)?;
            let list = lua.create_table()?;
            for (index, tip) in tips.iter().enumerate() {
                let table = rect_table(lua, tip.rect)?;
                table.set("words", tip.words.as_str())?;
                list.raw_set(index + 1, table)?;
            }
            out.set("tips", list)?;
            let list = lua.create_table()?;
            for (index, command) in commands.iter().enumerate() {
                let table = lua.create_table()?;
                table.set("title", command.title.as_str())?;
                table.set("enabled", command.enabled)?;
                list.raw_set(index + 1, table)?;
            }
            out.set("commands", list)?;
            Ok(out)
        });
        methods.add_method("snapshot", |lua, this, ()| {
            this.look(|page| snapshot(lua, page))?
        });
        methods.add_method("cursor", |_, this, ()| {
            this.look(|page| {
                page.cursor()
                    .map(|cursor| format!("{cursor:?}").to_lowercase())
            })
        });
        methods.add_method("pasteboard", |_, this, ()| {
            this.look(|page| page.pasteboard().map(str::to_owned))
        });
        methods.add_method("links", |_, this, ()| {
            this.look(|page| page.links().to_vec())
        });
        methods.add_method("menu", |lua, this, ()| {
            let menu = this.look(|page| page.menu().cloned())?;
            let Some(menu) = menu else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("x", menu.at.x)?;
            table.set("y", menu.at.y)?;
            let entries = lua.create_table()?;
            for (index, entry) in menu.entries.iter().enumerate() {
                let item = lua.create_table()?;
                match entry {
                    pencere_view::MenuEntry::Item {
                        title,
                        keys,
                        enabled,
                    } => {
                        item.set("title", title.as_str())?;
                        item.set("keys", keys.as_ref().map(|keys| format!("{keys:?}")))?;
                        item.set("enabled", *enabled)?;
                    }
                    pencere_view::MenuEntry::Separator => item.set("separator", true)?,
                }
                entries.raw_set(index + 1, item)?;
            }
            table.set("entries", entries)?;
            Ok(Value::Table(table))
        });
        methods.add_method("closed", |_, this, ()| this.look(Headless::closed));
        methods.add_method("diagnostics", |_, this, ()| {
            this.look(|page| page.diagnostics().to_vec())
        });
        methods.add_method("ended", |_, this, ()| {
            this.look(|page| page.ended().map(ToString::to_string))
        });
        methods.add_method("sent", |_, this, ()| this.look(|page| page.sent().to_vec()));
        methods.add_method("received", |_, this, ()| {
            this.look(|page| page.received().to_vec())
        });
        methods.add_method("calls", |lua, this, ()| {
            this.look(|page| calls(lua, page))?
        });
        methods.add_method("subscriptions", |lua, this, ()| {
            this.look(|page| subscriptions(lua, page))?
        });

        // ---- the program ----
        methods.add_method("waits", |_, this, ()| this.look(|page| page.peer().waits()));
        methods.add_method("push", |lua, this, (topic, value): (String, Value)| {
            let json = to_json(lua, &value).map_err(words)?;
            this.act(|page| {
                let Some(calls) = page.peer_mut().calls() else {
                    return Err(Error::NoSuch(
                        "a stand-in program (open the page with `calls`)".to_owned(),
                    ));
                };
                let pushed = calls.push(&topic, &json);
                page.settle()?;
                Ok(pushed)
            })
        });
        methods.add_method("answer", |lua, this, (method, value): (String, Value)| {
            let json = to_json(lua, &value).map_err(words)?;
            this.act(|page| {
                let Some(calls) = page.peer_mut().calls() else {
                    return Err(Error::NoSuch(
                        "a stand-in program (open the page with `calls`)".to_owned(),
                    ));
                };
                let answered = calls.answer(&method, Ok(json));
                page.settle()?;
                Ok(answered)
            })
        });
        methods.add_method(
            "fail",
            |_, this, (method, code, message): (String, String, Option<String>)| {
                this.act(|page| {
                    let Some(calls) = page.peer_mut().calls() else {
                        return Err(Error::NoSuch(
                            "a stand-in program (open the page with `calls`)".to_owned(),
                        ));
                    };
                    let answered = calls.answer(
                        &method,
                        Err(pencere_drive::headless::Failure {
                            code,
                            message: message.unwrap_or_default(),
                        }),
                    );
                    page.settle()?;
                    Ok(answered)
                })
            },
        );
        methods.add_method("lent", |lua, this, ()| {
            let command = this.act(|page| {
                Ok(page.peer_mut().in_process().and_then(|program| {
                    program.lent().map(|command| {
                        (
                            command.get_program().to_string_lossy().into_owned(),
                            command
                                .get_args()
                                .map(|arg| arg.to_string_lossy().into_owned())
                                .collect::<Vec<_>>(),
                        )
                    })
                }))
            })?;
            let Some((program, args)) = command else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("program", program)?;
            table.set("args", args)?;
            Ok(Value::Table(table))
        });
        methods.add_method("give_back", |_, this, code: Option<f64>| {
            use std::os::unix::process::ExitStatusExt as _;
            let status = ExitStatus::from_raw(
                i32::try_from(index_of(code.unwrap_or(0.0))).unwrap_or(0) << 8,
            );
            this.act(|page| {
                Ok(page
                    .peer_mut()
                    .in_process()
                    .is_some_and(|program| program.give_back(status)))
            })
        });
        methods.add_method("reopen", |_, this, ()| {
            let mut open = this
                .0
                .try_borrow_mut()
                .map_err(|_| words("the page is in use"))?;
            let Some(remake) = open.remake.as_ref() else {
                return Err(words(
                    "reopen: a program on a pty shows its own view again to a page it opens: open the page again with `test.open`",
                ));
            };
            let opening = remake().map_err(words)?;
            let page = open
                .page
                .take()
                .ok_or_else(|| words("the page is over: the test that opened it has ended"))?;
            let peer = page.into_peer();
            let reopened = opening.peer(peer).open().map_err(|error| trouble(&error))?;
            open.page = Some(reopened);
            Ok(())
        });
        methods.add_method("log", |_, this, ()| {
            let log = this
                .0
                .try_borrow()
                .map_err(|_| words("the page is in use"))?
                .log
                .clone();
            Ok(log
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default())
        });
        methods.add_method(
            "shot",
            |_, this, (name, theme): (String, Option<String>)| {
                let palette = if theme.as_deref() == Some("light") {
                    Palette::Light
                } else {
                    Palette::Dark
                };
                let png = this.look(|page| page.shot_in(&Fonts::none(), palette))?;
                let png = png.map_err(|error| words(format!("shot: {error}")))?;
                let shots = this
                    .0
                    .try_borrow()
                    .map_err(|_| words("the page is in use"))?
                    .shots
                    .clone();
                let path = shots(&name);
                if let Some(directory) = path.parent() {
                    std::fs::create_dir_all(directory).map_err(|error| {
                        words(format!("shot: {}: {error}", directory.display()))
                    })?;
                }
                std::fs::write(&path, png)
                    .map_err(|error| words(format!("shot: {}: {error}", path.display())))?;
                Ok(path.display().to_string())
            },
        );
    }
}
