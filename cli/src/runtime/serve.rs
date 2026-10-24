//! `serve`: the view of an app. The library's `serve` runs on a thread of its own, and every call
//! the view makes comes in on yet another; each hands the call to the loop, which runs the Lua
//! handler on its thread and sends the answer back.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::thread::JoinHandle;

use mlua::{Function, Lua, Table, UserData, UserDataFields, UserDataMethods, Value};
use pencere::{
    Bundle, Call, EndedBy, File, Icon, Kind, NoViewReason, Options, Outcome, Subscription, View,
    ViewError,
};

use super::convert::{from_json, to_json};
use super::core::{Core, ServePlan, ServeRun, ViewSpec};
use super::events::{CallEvent, Event, SubscribeEvent};
use crate::files::{ASSETS, Files};

/// The version of the page API this runner shows views of (`lua` 1.0).
const VERSION: &str = "1.0";

/// What the page is called when `serve` is not told.
const DEFAULT_PAGE: &str = "page.lua";

/// The directory of a web view's files, when `serve` is not told which.
const DEFAULT_WEB: &str = "web";

/// The numbers subscriptions are told apart by, across `serve`s.
static SUBSCRIPTIONS: AtomicU64 = AtomicU64::new(1);

/// A view made from the app's files: a bundle of the kind the spec says.
pub fn view_of(files: &Files, spec: &ViewSpec) -> Result<View, String> {
    match spec {
        ViewSpec::Lua { page, assets } => {
            let script = files
                .read(page)
                .map_err(|why| format!("the page {page:?} cannot be read: {why}"))?;
            let mut list = vec![File::new(page.clone(), script)];
            for path in files.under(assets).map_err(|why| why.to_string())? {
                let bytes = files.read(&path).map_err(|why| format!("{path}: {why}"))?;
                list.push(File::new(path, bytes));
            }
            let bundle = Bundle::new(page, list).map_err(|why| why.to_string())?;
            Ok(View::new(Kind::Lua, &[VERSION], bundle))
        }
        ViewSpec::Web { dir, access } => {
            let prefix = format!("{}/", dir.trim_end_matches('/'));
            let mut list = Vec::new();
            for path in files.under(dir).map_err(|why| why.to_string())? {
                let bytes = files.read(&path).map_err(|why| format!("{path}: {why}"))?;
                let relative = path.strip_prefix(&prefix).unwrap_or(&path).to_owned();
                list.push(File::new(relative, bytes));
            }
            if list.is_empty() {
                return Err(format!(
                    "the web view's directory {dir:?} has no files in the app"
                ));
            }
            let bundle = Bundle::new("index.html", list)
                .map_err(|why| format!("the web view {dir:?}: {why}"))?;
            let view = View::web(bundle);
            Ok(match access {
                Some(access) => view.access(access.clone()),
                None => view,
            })
        }
    }
}

/// The views of a plan, in the order the app prefers them.
pub fn views_of(files: &Files, specs: &[ViewSpec]) -> Result<Vec<View>, String> {
    specs.iter().map(|spec| view_of(files, spec)).collect()
}

/// A web page's `access`, from the Lua table: the origins it may reach (`connect`, `resources`,
/// `frames`, `base`) and the capabilities it may ask for (`permissions`).
fn access_of(table: &Table) -> mlua::Result<pencere::Access> {
    let list = |name: &str| -> mlua::Result<Vec<String>> {
        Ok(table.get::<Option<Vec<String>>>(name)?.unwrap_or_default())
    };
    let mut permissions = Vec::new();
    for name in list("permissions")? {
        permissions.push(match name.as_str() {
            "camera" => pencere::Permission::Camera,
            "microphone" => pencere::Permission::Microphone,
            "geolocation" => pencere::Permission::Geolocation,
            "clipboardWrite" => pencere::Permission::ClipboardWrite,
            other => {
                return Err(super::fault(format!(
                    "access.permissions: {other:?} is not camera, microphone, geolocation or clipboardWrite"
                )));
            }
        });
    }
    Ok(pencere::Access {
        connect: list("connect")?,
        resources: list("resources")?,
        frames: list("frames")?,
        base: list("base")?,
        permissions,
    })
}

/// The views `serve` was asked for. `views = { {kind = "lua", page = …}, {kind = "web", dir = …} }`
/// names them in the order the app prefers them; short of that, `page` is a Lua view and `web` a
/// web one (the Lua one first), and `access` is the web one's.
pub fn specs_of(options: &Table) -> mlua::Result<Vec<ViewSpec>> {
    if let Some(list) = options.get::<Option<Table>>("views")? {
        for name in ["page", "web", "access"] {
            if options.get::<Value>(name)? != Value::Nil {
                return Err(super::fault(format!(
                    "serve: `views` lists the views, so `{name}` goes in its entries"
                )));
            }
        }
        let mut specs = Vec::new();
        for entry in list.sequence_values::<Table>() {
            let entry = entry?;
            let kind: String = entry.get::<Option<String>>("kind")?.unwrap_or_default();
            specs.push(match kind.as_str() {
                "lua" => ViewSpec::Lua {
                    page: entry
                        .get::<Option<String>>("page")?
                        .unwrap_or_else(|| DEFAULT_PAGE.to_owned()),
                    assets: entry
                        .get::<Option<String>>("assets")?
                        .unwrap_or_else(|| ASSETS.to_owned()),
                },
                "web" => ViewSpec::Web {
                    dir: entry
                        .get::<Option<String>>("dir")?
                        .unwrap_or_else(|| DEFAULT_WEB.to_owned()),
                    access: entry
                        .get::<Option<Table>>("access")?
                        .map(|table| access_of(&table))
                        .transpose()?,
                },
                other => {
                    return Err(super::fault(format!(
                        "serve: a view is of kind \"lua\" or \"web\", not {other:?}"
                    )));
                }
            });
        }
        if specs.is_empty() {
            return Err(super::fault(
                "serve: `views` is empty: an app offers a view",
            ));
        }
        return Ok(specs);
    }
    let page = options.get::<Option<String>>("page")?;
    let web = options.get::<Option<String>>("web")?;
    let mut specs = Vec::new();
    if page.is_some() || web.is_none() {
        specs.push(ViewSpec::Lua {
            page: page.unwrap_or_else(|| DEFAULT_PAGE.to_owned()),
            assets: options
                .get::<Option<String>>("assets")?
                .unwrap_or_else(|| ASSETS.to_owned()),
        });
    }
    if let Some(dir) = web {
        specs.push(ViewSpec::Web {
            dir,
            access: options
                .get::<Option<Table>>("access")?
                .map(|table| access_of(&table))
                .transpose()?,
        });
    }
    Ok(specs)
}

/// An icon from a file of the app, as the `data:` URI the protocol wants.
fn icon_of(files: &Files, path: &str) -> Result<Icon, String> {
    let bytes = files
        .read(path)
        .map_err(|why| format!("the icon {path:?} cannot be read: {why}"))?;
    let mime = match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        // A terminal hands an icon to an image decoder of its own, and takes only these.
        _ => {
            return Err(format!(
                "the icon {path:?} is not a PNG, a JPEG or a WebP picture"
            ));
        }
    };
    let mut icon = Icon::new(format!(
        "data:{mime};base64,{}",
        pencere_wire::base64::encode(&bytes)
    ));
    icon.mime_type = Some(mime.to_owned());
    Ok(icon)
}

/// `serve{ page =, web =, views =, assets =, access =, title =, program =, icon =, calls =, topics = }`,
/// from the prelude: starts the library's `serve` on a thread, and calls `done` with how it ended.
pub fn start(core: &Rc<Core>, options: Table, done: Function) -> mlua::Result<()> {
    if core.serve.borrow().is_some() {
        return Err(super::fault(
            "a view is already being shown: an app shows one at a time",
        ));
    }
    let (serving, calls, topics, specs) = prepare(core, &options)?;
    if let Ok(mut plan) = core.plan.lock() {
        *plan = Some(ServePlan { views: specs });
    }
    let thread = spawn(core, serving)?;
    *core.serve.borrow_mut() = Some(ServeRun {
        calls,
        topics,
        done,
        thread: Some(thread),
        options,
    });
    Ok(())
}

type Handlers = HashMap<String, Function>;

/// The library's options for what `serve` was given, and the Lua handlers they call.
fn prepare(
    core: &Rc<Core>,
    options: &Table,
) -> mlua::Result<(Options, Handlers, Handlers, Vec<ViewSpec>)> {
    let fail = |why: String| super::fault(why);
    let specs = specs_of(options)?;
    let mut serving = Options::new()
        .program(
            options
                .get::<Option<String>>("program")?
                .unwrap_or_else(|| core.name.clone()),
        )
        .control(core.control.clone());
    for view in views_of(&core.files, &specs).map_err(fail)? {
        serving = serving.view(view);
    }
    if let Some(title) = options.get::<Option<String>>("title")? {
        serving = serving.title(title);
    }
    if let Some(icon) = options.get::<Option<String>>("icon")? {
        serving = serving.icon(icon_of(&core.files, &icon).map_err(super::fault)?);
    }
    let mut calls = HashMap::new();
    if let Some(table) = options.get::<Option<Table>>("calls")? {
        for pair in table.pairs::<String, Function>() {
            let (name, handler) = pair?;
            let events = core.events.clone();
            let method = name.clone();
            serving = serving.call(name.clone(), move |params, call: &Call| {
                let (reply, answer) = channel();
                let sent = events.send(Event::Call(CallEvent {
                    method: method.clone(),
                    params,
                    call: call.clone(),
                    reply,
                }));
                if !sent {
                    return Err(ViewError::with_message("unavailable", "the app is ending"));
                }
                answer
                    .recv()
                    .unwrap_or_else(|_| Err(ViewError::with_message("unavailable", "no answer")))
            });
            calls.insert(name, handler);
        }
    }
    let mut topics = HashMap::new();
    if let Some(table) = options.get::<Option<Table>>("topics")? {
        for pair in table.pairs::<String, Function>() {
            let (name, handler) = pair?;
            let events = core.events.clone();
            serving = serving.topic(name.clone(), move |params, subscription: &Subscription| {
                let id = SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst);
                let ended = events.clone();
                subscription.on_end(move || {
                    ended.send(Event::SubscriptionEnded(id));
                });
                let (started, answer) = channel();
                let sent = events.send(Event::Subscribe(SubscribeEvent {
                    id,
                    topic: subscription.topic().to_owned(),
                    params,
                    subscription: subscription.clone(),
                    started,
                }));
                if !sent {
                    return Err(ViewError::with_message("unavailable", "the app is ending"));
                }
                answer
                    .recv()
                    .unwrap_or_else(|_| Err(ViewError::with_message("unavailable", "no answer")))
            });
            topics.insert(name, handler);
        }
    }
    Ok((serving, calls, topics, specs))
}

/// Runs the library's `serve` on a thread of its own, and tells the loop how it ended.
fn spawn(core: &Rc<Core>, serving: Options) -> mlua::Result<JoinHandle<()>> {
    let events = core.events.clone();
    std::thread::Builder::new()
        .name("pencere-serve".into())
        .spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pencere::serve(serving)))
                    .unwrap_or_else(|_| {
                        Err(pencere::Error::Io(std::io::Error::other(
                            "the view's thread panicked",
                        )))
                    });
            events.send(Event::Served(result));
        })
        .map_err(|why| super::fault(format!("the view cannot be started: {why}")))
}

/// Shows the view again with what `serve` was given, after the terminal was handed to a program:
/// the same handlers, the files as they are now.
pub fn serve_again(core: &Rc<Core>) -> mlua::Result<()> {
    let options = core
        .serve
        .borrow()
        .as_ref()
        .map(|run| run.options.clone())
        .ok_or_else(|| super::fault("there is no view to show again"))?;
    let (serving, calls, topics, specs) = prepare(core, &options)?;
    if let Ok(mut plan) = core.plan.lock() {
        *plan = Some(ServePlan { views: specs });
    }
    let thread = spawn(core, serving)?;
    if let Some(run) = core.serve.borrow_mut().as_mut() {
        run.calls = calls;
        run.topics = topics;
        run.thread = Some(thread);
    }
    Ok(())
}

/// What the loop does with what another thread handed it.
pub fn handle(core: &Core, event: Event) {
    let lua = core.lua();
    match event {
        Event::Call(call) => on_call(core, lua, call),
        Event::Subscribe(subscribe) => on_subscribe(core, lua, subscribe),
        Event::SubscriptionEnded(id) => {
            let cleanups = core.sub_ends.borrow_mut().remove(&id).unwrap_or_default();
            for cleanup in cleanups {
                core.callback(cleanup, Vec::new());
            }
        }
        Event::Served(result) => on_served(core, lua, result),
        Event::Restart => core.end(super::core::Exit::Restart),
        Event::Watched { id, changes } => on_watched(core, lua, id, &changes),
    }
}

/// A list of changes for the function of a watch: `{ path =, relative =, kind =, dir = }` each.
fn on_watched(core: &Core, lua: &Lua, id: u64, changes: &[super::watch::Change]) {
    let Some(func) = core.watches.borrow().get(&id).map(|(func, _)| func.clone()) else {
        return;
    };
    let built = (|| -> mlua::Result<Table> {
        let list = lua.create_table()?;
        for (index, change) in changes.iter().enumerate() {
            let item = lua.create_table()?;
            item.set("path", change.path.display().to_string())?;
            item.set("kind", change.kind)?;
            item.set("dir", change.dir)?;
            list.raw_set(index + 1, item)?;
        }
        Ok(list)
    })();
    match built {
        Ok(list) => core.callback(func, vec![Value::Table(list)]),
        Err(why) => core.log.error(&format!("pencere: {why}")),
    }
}

fn on_call(core: &Core, lua: &Lua, event: CallEvent) {
    let handler = core
        .serve
        .borrow()
        .as_ref()
        .and_then(|run| run.calls.get(&event.method).cloned());
    let Some(handler) = handler else {
        let _ = event.reply.send(Err(ViewError::with_message(
            "not-found",
            "there is nothing to call by that name",
        )));
        return;
    };
    // The view gave it up while it waited for the loop: nobody wants the answer.
    if event.call.is_cancelled() {
        return;
    }
    let reply = event.reply;
    let built = (|| -> mlua::Result<(Table, Vec<Value>)> {
        let params = from_json(lua, &event.params)?;
        let call = lua.create_userdata(CallHandle(event.call))?;
        let answer = reply.clone();
        let done = lua.create_function(move |lua, value: Value| {
            let result =
                to_json(lua, &value).map_err(|why| ViewError::with_message("internal", why));
            let _ = answer.send(result);
            Ok(())
        })?;
        let answer = reply.clone();
        let fail = lua.create_function(move |_, error: Value| {
            let _ = answer.send(Err(view_error(&error)));
            Ok(())
        })?;
        let handlers = lua.create_table()?;
        handlers.set("done", done)?;
        handlers.set("fail", fail)?;
        Ok((handlers, vec![params, Value::UserData(call)]))
    })();
    match built {
        Ok((handlers, args)) => core.run_task(handler, Value::Table(handlers), args),
        Err(why) => {
            let _ = reply.send(Err(ViewError::with_message("internal", why.to_string())));
        }
    }
}

fn on_subscribe(core: &Core, lua: &Lua, event: SubscribeEvent) {
    let handler = core
        .serve
        .borrow()
        .as_ref()
        .and_then(|run| run.topics.get(&event.topic).cloned());
    let Some(handler) = handler else {
        let _ = event.started.send(Err(ViewError::with_message(
            "not-found",
            "there is nothing to subscribe to by that name",
        )));
        return;
    };
    if event.subscription.is_cancelled() {
        let _ = event.started.send(Ok(()));
        return;
    }
    let started = event.started;
    let built = (|| -> mlua::Result<(Table, Vec<Value>)> {
        let params = from_json(lua, &event.params)?;
        let me = core.rc();
        let subscription = lua.create_userdata(SubscriptionHandle {
            id: event.id,
            subscription: event.subscription,
            core: me,
        })?;
        let answer = started.clone();
        let done = lua.create_function(move |_, _: Value| {
            let _ = answer.send(Ok(()));
            Ok(())
        })?;
        let answer = started.clone();
        let fail = lua.create_function(move |_, error: Value| {
            let _ = answer.send(Err(view_error(&error)));
            Ok(())
        })?;
        let handlers = lua.create_table()?;
        handlers.set("done", done)?;
        handlers.set("fail", fail)?;
        Ok((handlers, vec![params, Value::UserData(subscription)]))
    })();
    match built {
        Ok((handlers, args)) => core.run_task(handler, Value::Table(handlers), args),
        Err(why) => {
            let _ = started.send(Err(ViewError::with_message("internal", why.to_string())));
        }
    }
}

fn on_served(core: &Core, lua: &Lua, result: Result<Outcome, pencere::Error>) {
    let run = core.serve.borrow_mut().take();
    if let Ok(mut plan) = core.plan.lock() {
        *plan = None;
    }
    let Some(mut run) = run else {
        return;
    };
    if let Some(thread) = run.thread.take() {
        let _ = thread.join();
    }
    let (outcome, failure) = match result {
        Ok(outcome) => {
            core.status.set(i32::from(outcome.shell_status()));
            match outcome_table(lua, &outcome) {
                Ok(table) => (Value::Table(table), Value::Nil),
                Err(why) => (Value::Nil, text(lua, &why.to_string())),
            }
        }
        Err(why) => (Value::Nil, text(lua, &why.to_string())),
    };
    if let Err(why) = run.done.call::<()>((outcome, failure)) {
        core.log.error(&format!("pencere: {why}"));
    }
}

fn text(lua: &Lua, text: &str) -> Value {
    lua.create_string(text).map_or(Value::Nil, Value::String)
}

/// What `serve` came to, as the app sees it.
fn outcome_table(lua: &Lua, outcome: &Outcome) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("status", i32::from(outcome.shell_status()))?;
    match outcome {
        Outcome::Closed(closed) => {
            table.set("shown", true)?;
            table.set("kind", closed.view.kind.name())?;
            table.set("version", closed.view.version.as_str())?;
            let ended = match closed.by {
                EndedBy::User => "user",
                EndedBy::View => "view",
                EndedBy::Program => "program",
                EndedBy::Error => "error",
                EndedBy::Signal(number) => {
                    table.set("signal", number)?;
                    "signal"
                }
                EndedBy::Terminal => "terminal",
                _ => "other",
            };
            table.set("ended", ended)?;
            table.set("code", closed.code.as_deref())?;
            table.set("message", closed.message.as_deref())?;
        }
        Outcome::NoView(no_view) => {
            table.set("shown", false)?;
            let reason = match no_view.reason {
                NoViewReason::NoTty => "no-tty",
                NoViewReason::NoHost => "no-host",
                NoViewReason::NoCommonVersion => "no-common-version",
                NoViewReason::NoCommonView => "no-common-view",
                NoViewReason::Refused => "refused",
                NoViewReason::Failed => "failed",
                NoViewReason::Timeout => "timeout",
                NoViewReason::Interrupted(number) => {
                    table.set("signal", number)?;
                    "interrupted"
                }
                _ => "other",
            };
            table.set("reason", reason)?;
            table.set("code", no_view.code.as_deref())?;
            table.set("message", no_view.message.as_deref())?;
        }
    }
    Ok(table)
}

/// What a handler raised, as the view gets it: a table with a `code` (and a `message`) is the
/// handler's own answer, anything else is a failure of the handler.
fn view_error(error: &Value) -> ViewError {
    match error {
        Value::Table(table) => {
            let code = table.get::<Option<String>>("code").ok().flatten();
            let message = table.get::<Option<String>>("message").ok().flatten();
            match (code, message) {
                (Some(code), Some(message)) => ViewError::with_message(code, message),
                (Some(code), None) => ViewError::new(code),
                (None, message) => ViewError::with_message(
                    "internal",
                    message.unwrap_or_else(|| "the handler raised a table with no code".into()),
                ),
            }
        }
        Value::String(text) => {
            let text = text.to_string_lossy();
            // The first line: the rest is the traceback, for the log.
            let first = text.lines().next().unwrap_or("").to_owned();
            ViewError::with_message("internal", first)
        }
        _ => ViewError::new("internal"),
    }
}

// ---- what a handler is given ---------------------------------------------------------------

/// The call a handler is answering.
struct CallHandle(Call);

impl UserData for CallHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("method", |_, this| Ok(this.0.method().to_owned()));
        fields.add_field_method_get("cancelled", |_, this| Ok(this.0.is_cancelled()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // call:progress(done, total, note)
        methods.add_method(
            "progress",
            |_, this, (done, total, note): (f64, Option<f64>, Option<String>)| {
                this.0.progress(done, total, note.as_deref());
                Ok(())
            },
        );
    }
}

/// The subscription a topic's handler feeds.
struct SubscriptionHandle {
    id: u64,
    subscription: Subscription,
    core: Rc<Core>,
}

impl UserData for SubscriptionHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("topic", |_, this| Ok(this.subscription.topic().to_owned()));
        fields.add_field_method_get("cancelled", |_, this| Ok(this.subscription.is_cancelled()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // sub:push(value) -> whether it was sent. Waits while the terminal is catching up with
        // what was pushed before.
        methods.add_method("push", |lua, this, value: Value| {
            let json = to_json(lua, &value).map_err(mlua::Error::runtime)?;
            Ok(this.subscription.push(json))
        });
        // sub:on_end(function) runs when the subscription ends: the view cancelled it, or is over.
        methods.add_method("on_end", |_, this, cleanup: Function| {
            if this.subscription.is_cancelled() {
                this.core.callback(cleanup, Vec::new());
            } else {
                this.core
                    .sub_ends
                    .borrow_mut()
                    .entry(this.id)
                    .or_default()
                    .push(cleanup);
            }
            Ok(())
        });
    }
}
