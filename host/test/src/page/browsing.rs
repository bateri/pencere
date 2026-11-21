//! What a test may do with the browsers of a page: read what the page asked of each, and tell the
//! page what its engine would (the host with no screen has none).

use pencere_cli::mlua::{self, Table, UserDataMethods};
use pencere_view::{BrowserAsk, BrowserError, BrowserState, BrowserWindows, Id, Key, Mods};

use super::{Handle, rect_table};

/// The state a test tells a browser: `{ status =, message =, url =, title =, loading =, back =,
/// forward =, error = { url =, code =, message = } }`, each member optional; a browser that is told
/// nothing of its status is ready.
fn state_of(table: &Table) -> mlua::Result<BrowserState> {
    let mut state = BrowserState::default();
    if let Some(status) = table.get::<Option<String>>("status")? {
        state.status = status;
    }
    state.message = table.get::<Option<String>>("message")?;
    state.url = table.get::<Option<String>>("url")?.unwrap_or_default();
    state.title = table.get::<Option<String>>("title")?.unwrap_or_default();
    state.loading = table.get::<Option<bool>>("loading")?.unwrap_or(false);
    state.back = table.get::<Option<bool>>("back")?.unwrap_or(false);
    state.forward = table.get::<Option<bool>>("forward")?.unwrap_or(false);
    if let Some(error) = table.get::<Option<Table>>("error")? {
        state.error = Some(BrowserError {
            url: error.get::<Option<String>>("url")?.unwrap_or_default(),
            code: error.get::<Option<i32>>("code")?.unwrap_or(0),
            message: error.get::<Option<String>>("message")?.unwrap_or_default(),
        });
    }
    Ok(state)
}

/// A key as a page writes one, `"cmd+l"`.
fn written(key: Key, mods: Mods) -> String {
    let mut out = String::new();
    for (held, word) in [
        (mods.control, "ctrl+"),
        (mods.alt, "alt+"),
        (mods.shift, "shift+"),
        (mods.command, "cmd+"),
    ] {
        if held {
            out.push_str(word);
        }
    }
    match key {
        Key::Character(' ') => out.push_str("space"),
        Key::Character(character) => out.push(character),
        Key::Function(number) => out.push_str(&format!("f{number}")),
        Key::Up => out.push_str("up"),
        Key::Down => out.push_str("down"),
        Key::Left => out.push_str("left"),
        Key::Right => out.push_str("right"),
        Key::PageUp => out.push_str("page_up"),
        Key::PageDown => out.push_str("page_down"),
        Key::Home => out.push_str("home"),
        Key::End => out.push_str("end"),
        Key::Enter => out.push_str("enter"),
        Key::Escape => out.push_str("escape"),
        Key::Tab => out.push_str("tab"),
        Key::Backspace => out.push_str("backspace"),
        Key::Delete => out.push_str("delete"),
    }
    out
}

pub(super) fn add<M: UserDataMethods<Handle>>(methods: &mut M) {
    // The browsers of the page: each its room and what the page made it by.
    methods.add_method("browsers", |lua, this, ()| {
        let rooms = this.look(|page| page.browsers().to_vec())?;
        let list = lua.create_table()?;
        for (index, room) in rooms.iter().enumerate() {
            let item = rect_table(lua, room.rect)?;
            item.set("profile", room.options.profile.as_deref())?;
            item.set(
                "windows",
                match room.options.windows {
                    BrowserWindows::Here => "here",
                    BrowserWindows::Page => "page",
                },
            )?;
            let keys = lua.create_table()?;
            for (at, kept) in room.options.keys.iter().enumerate() {
                keys.raw_set(at + 1, written(kept.key, kept.mods))?;
            }
            item.set("keys", keys)?;
            item.set("hidden", room.hidden)?;
            item.set("id", format!("{:?}", room.id))?;
            list.raw_set(index + 1, item)?;
        }
        Ok(list)
    });
    // What the page asked of the browser named `name` (every browser's, without a name), oldest
    // first: `{ go = "https://…" }` or `{ command = "back" }`.
    methods.add_method("browser_asks", |lua, this, name: Option<String>| {
        let asks = this.look(|page| page.browser_asks().to_vec())?;
        let want = name.as_deref().map(Id::named);
        let list = lua.create_table()?;
        for (browser, ask) in asks.iter().filter(|(id, _)| want.is_none_or(|w| w == *id)) {
            let item = lua.create_table()?;
            match ask {
                BrowserAsk::Go(url) => item.set("go", url.as_str())?,
                BrowserAsk::Back => item.set("command", "back")?,
                BrowserAsk::Forward => item.set("command", "forward")?,
                BrowserAsk::Reload => item.set("command", "reload")?,
                BrowserAsk::Stop => item.set("command", "stop")?,
            }
            if want.is_none() {
                item.set("browser", format!("{browser:?}"))?;
            }
            list.raw_push(item)?;
        }
        Ok(list)
    });
    // The keys the browser that had the keyboard took for itself, oldest first, as a page writes
    // them.
    methods.add_method("browser_keys", |lua, this, ()| {
        let keys = this.look(|page| page.browser_keys().to_vec())?;
        let list = lua.create_table()?;
        for (_, key, mods) in keys {
            list.raw_push(written(key, mods))?;
        }
        Ok(list)
    });
    // The browser named `name` is, or does, what the table says, as its engine would tell the page.
    methods.add_method(
        "browser_state",
        |_, this, (name, state): (String, Table)| {
            let state = state_of(&state)?;
            this.act(|page| page.browser_state(&name, state))
        },
    );
    // A link of the browser named `name` opens a new window at `url`.
    methods.add_method(
        "browser_window",
        |_, this, (name, url): (String, String)| this.act(|page| page.browser_window(&name, &url)),
    );
    // The page's browser named `name`, as a table, or nil: where it is and what it was made by.
    methods.add_method("has_browser", |_, this, name: String| {
        let id = Id::named(&name);
        this.look(|page| page.browsers().iter().any(|room| room.id == id))
    });
}
