//! What a test may do with the browsers of a page: read what the page asked of each, and tell the
//! page what its engine would (the host with no screen has none).

use pencere_cli::mlua::{self, Table, UserDataMethods};
use pencere_view::{
    BrowserAsk, BrowserError, BrowserFound, BrowserState, BrowserWindows, Id, Key, Mods,
};

use super::{Handle, rect_table};

/// The state a test tells a browser: `{ status =, message =, url =, title =, loading =, back =,
/// forward =, error = { url =, code =, message =, certificate = }, found = { count =, current = }, zoom = }`, each
/// member optional; a browser that is told nothing of its status is ready, and of its zoom is at the
/// page's size.
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
            certificate: error.get::<Option<String>>("certificate")?,
        });
    }
    if let Some(found) = table.get::<Option<Table>>("found")? {
        state.found = Some(BrowserFound {
            count: found.get::<Option<u32>>("count")?.unwrap_or(0),
            current: found.get::<Option<u32>>("current")?,
        });
    }
    if let Some(zoom) = table.get::<Option<f32>>("zoom")? {
        state.zoom = zoom;
    }
    Ok(state)
}

/// A zoom as a page reads one: to three places, so that 1.1 is 1.1 and not the single float nearest it.
fn rounded(zoom: f32) -> f64 {
    (f64::from(zoom) * 1000.0).round() / 1000.0
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
    // first: `{ go = "https://…" }`, `{ zoom = 1.5 }` or `{ command = "back" }` (and the rest of a
    // command's words: `{ command = "find", pattern =, case = }`, `{ command = "reload", cache =
    // false }`).
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
                BrowserAsk::Reload { cache } => {
                    item.set("command", "reload")?;
                    if !cache {
                        item.set("cache", false)?;
                    }
                }
                BrowserAsk::Stop => item.set("command", "stop")?,
                BrowserAsk::Find { pattern, case } => {
                    item.set("command", "find")?;
                    item.set("pattern", pattern.as_str())?;
                    item.set("case", *case)?;
                }
                BrowserAsk::FindNext => item.set("command", "find_next")?,
                BrowserAsk::FindPrevious => item.set("command", "find_previous")?,
                BrowserAsk::StopFinding => item.set("command", "stop_finding")?,
                BrowserAsk::Zoom(zoom) => item.set("zoom", rounded(*zoom))?,
                BrowserAsk::Devtools => item.set("command", "devtools")?,
                BrowserAsk::ClearCache => item.set("command", "clear_cache")?,
                BrowserAsk::ClearSiteData => item.set("command", "clear_site_data")?,
                BrowserAsk::Proceed => item.set("command", "proceed")?,
                // The drive gives a host none of this kind: the page was told it was refused.
                BrowserAsk::Unknown => item.set("command", "unknown")?,
            }
            if want.is_none() {
                item.set("browser", format!("{browser:?}"))?;
            }
            list.raw_push(item)?;
        }
        Ok(list)
    });
    // The keys the browser that had the keyboard was given, oldest first, as a page writes them.
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
    // The browser named `name` had no use for the keys, which the page gets as its own.
    methods.add_method(
        "browser_unused",
        |_, this, (name, keys): (String, String)| {
            this.act(|page| page.browser_unused(&name, &keys))
        },
    );
    // A link of the browser named `name` opens a new window at `url`, behind the page where
    // `background`.
    methods.add_method(
        "browser_window",
        |_, this, (name, url, background): (String, String, Option<bool>)| {
            this.act(|page| page.browser_window(&name, &url, background.unwrap_or(false)))
        },
    );
    // The page's browser named `name`, as a table, or nil: where it is and what it was made by.
    methods.add_method("has_browser", |_, this, name: String| {
        let id = Id::named(&name);
        this.look(|page| page.browsers().iter().any(|room| room.id == id))
    });
}
