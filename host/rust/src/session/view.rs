//! The host as an MCP Apps host of the view (specification §9.3 to §9.5, §11):
//! what the view may send, what it is told, and what is passed on to the program.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::{
    Action, Cause, Forward, Live, MAX_IN_FLIGHT, MAX_VIEW_MESSAGE, Phase, Session, Size, ViewState,
};
use crate::bundle::Store;
use crate::jsonrpc::{
    self, DENIED, Envelope, INVALID_PARAMS, INVALID_REQUEST, Id, Kind, METHOD_NOT_FOUND,
};
use crate::message::HostInfo;
use crate::sandbox;
use crate::theme::Variables;
use crate::token::Entropy;

/// The revision of MCP Apps the host speaks with its views.
pub const UI_PROTOCOL_VERSION: &str = "2026-01-26";

/// The key under which a view says it was written for the pane, and the host
/// says it knows (specification §7.2).
pub const PANE_CAPABILITY: &str = "pencere/pane";

#[derive(Serialize)]
struct Empty {}

#[derive(Serialize)]
struct InitializeResult<'a> {
    #[serde(rename = "protocolVersion")]
    protocol_version: &'a str,
    #[serde(rename = "hostInfo")]
    host_info: &'a HostInfo,
    #[serde(rename = "hostCapabilities")]
    host_capabilities: HostCapabilities,
    #[serde(rename = "hostContext")]
    host_context: HostContext<'a>,
}

#[derive(Serialize)]
struct HostCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    experimental: Option<Value>,
    #[serde(rename = "openLinks", skip_serializing_if = "Option::is_none")]
    open_links: Option<Empty>,
    #[serde(rename = "serverTools")]
    server_tools: Proxy,
    #[serde(rename = "serverResources")]
    server_resources: Proxy,
    #[serde(skip_serializing_if = "Option::is_none")]
    logging: Option<Empty>,
    sandbox: Value,
}

/// What the host does for the view with the program's tools or resources: it
/// passes the view's requests on (§9.4), and the program's notifications that the
/// list changed when the program announces them (`listChanged`, §9.6).
#[derive(Serialize)]
struct Proxy {
    #[serde(rename = "listChanged", skip_serializing_if = "std::ops::Not::not")]
    list_changed: bool,
}

/// The parameters of `notifications/cancelled` (MCP § Cancellation).
#[derive(Deserialize)]
struct Cancelled {
    #[serde(rename = "requestId")]
    request_id: Id,
    #[serde(default)]
    reason: Option<Value>,
}

/// The part of `notifications/progress` the host reads to know whose it is (MCP §
/// Progress); the rest goes to the view as the program wrote it.
#[derive(Deserialize)]
struct Progress {
    #[serde(rename = "progressToken", default)]
    progress_token: Option<Value>,
}

/// What the host writes the program when the view gives up a request: the id
/// the host gave it, and the view's reason when it is text.
#[derive(Serialize)]
struct Cancelling<'a> {
    #[serde(rename = "requestId")]
    request_id: &'a Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
struct HostContext<'a> {
    #[serde(rename = "toolInfo")]
    tool_info: ToolInfo<'a>,
    theme: &'static str,
    styles: Styles,
    #[serde(rename = "displayMode")]
    display_mode: &'static str,
    #[serde(rename = "availableDisplayModes")]
    available_display_modes: [&'static str; 1],
    #[serde(rename = "containerDimensions")]
    container_dimensions: Dimensions,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<&'a str>,
    #[serde(rename = "timeZone", skip_serializing_if = "Option::is_none")]
    time_zone: Option<&'a str>,
    #[serde(rename = "userAgent")]
    user_agent: &'a str,
    platform: &'static str,
}

#[derive(Serialize)]
struct ToolInfo<'a> {
    tool: &'a RawValue,
}

#[derive(Serialize)]
struct Styles {
    variables: Variables,
}

#[derive(Serialize)]
struct Dimensions {
    width: u32,
    height: u32,
}

impl From<Size> for Dimensions {
    fn from(size: Size) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

/// The mode a view is told (specification §7.2): `inline`, or, when the view does
/// not list it, the first of `fullscreen` and `pip` it lists.
fn display_mode(capabilities: Option<&Value>) -> &'static str {
    let Some(listed) = capabilities
        .and_then(|c| c.get("availableDisplayModes"))
        .and_then(Value::as_array)
    else {
        return "inline";
    };
    let has = |mode: &str| listed.iter().any(|m| m.as_str() == Some(mode));
    if has("inline") {
        "inline"
    } else if has("fullscreen") {
        "fullscreen"
    } else if has("pip") {
        "pip"
    } else {
        "inline"
    }
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// A message from the view, as JSON text. Messages over [`MAX_VIEW_MESSAGE`]
    /// bytes, and messages that are not JSON-RPC 2.0, are dropped.
    pub fn view_message(&mut self, text: &str) {
        if text.len() > MAX_VIEW_MESSAGE {
            self.ignored();
            return;
        }
        let Some(envelope) = Envelope::parse(text) else {
            self.ignored();
            return;
        };
        let kind = envelope.kind();
        // A WebAssembly view does not speak MCP Apps: what reaches here for one is not
        // its message (§8.8).
        if matches!(&self.phase, Phase::Live(live) if live.kind == super::ViewKind::Wasm) {
            self.ignored();
            return;
        }
        match self.take() {
            Phase::Live(mut live) => {
                let end = self.live_message(&mut live, kind, &envelope);
                self.phase = Phase::Live(live);
                if let Some(cause) = end {
                    self.close(cause, None);
                }
            }
            Phase::Closing(closing) => {
                let answered = matches!(&kind, Some(Kind::Result { id } | Kind::Failure { id }) if *id == closing.request);
                self.phase = Phase::Closing(closing);
                if answered {
                    self.finish_close();
                }
            }
            other => {
                self.phase = other;
                self.ignored();
            }
        }
    }

    fn live_message(
        &mut self,
        live: &mut Live,
        kind: Option<Kind>,
        envelope: &Envelope,
    ) -> Option<Cause> {
        match kind? {
            Kind::Request { id, method } => {
                self.view_request(live, id, &method, envelope);
                None
            }
            Kind::Notification { method } => self.view_notification(live, &method, envelope),
            // The only request the host makes of a view is the teardown.
            Kind::Result { .. } | Kind::Failure { .. } => {
                self.ignored();
                None
            }
        }
    }

    fn view_request(&mut self, live: &mut Live, id: Id, method: &str, envelope: &Envelope) {
        if method == "ui/initialize" {
            self.view_initialize(live, &id, envelope);
            return;
        }
        if live.state == ViewState::AwaitInitialize {
            let text = jsonrpc::failure(&id, INVALID_REQUEST, "ui/initialize comes first");
            self.tell_view(text);
            return;
        }
        let params: Option<Value> = envelope.params_as();
        match method {
            "ping" => self.tell_view(jsonrpc::result(&id, json!({}))),
            "tools/call" => {
                let name = params
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str);
                // A view reaches the tools whose visibility has `app` and no others
                // (MCP Apps § Resource Discovery → Visibility).
                let callable =
                    name.is_some_and(|name| live.tools.iter().any(|t| t.name == name && t.app));
                if callable {
                    self.forward(live, id, method, params);
                } else {
                    let text =
                        jsonrpc::failure(&id, INVALID_PARAMS, "no such tool is callable by a view");
                    self.tell_view(text);
                }
            }
            "resources/read" => {
                if params
                    .as_ref()
                    .and_then(|p| p.get("uri"))
                    .is_some_and(Value::is_string)
                {
                    self.forward(live, id, method, params);
                } else {
                    let text = jsonrpc::failure(&id, INVALID_PARAMS, "a uri is needed");
                    self.tell_view(text);
                }
            }
            // What the program has: how a view learns what a notification of the
            // program announced (§9.4, §9.6).
            "tools/list" | "resources/list" | "resources/templates/list" | "prompts/list" => {
                self.forward(live, id, method, params);
            }
            "ui/open-link" => self.view_open_link(live, id, params.as_ref()),
            // A terminal has no conversation with a model (§9.5).
            "ui/message" | "ui/update-model-context" => {
                let text = jsonrpc::failure(&id, DENIED, "this host has no conversation to add to");
                self.tell_view(text);
            }
            "ui/request-display-mode" => {
                let text = jsonrpc::result(&id, json!({ "mode": live.display_mode }));
                self.tell_view(text);
            }
            _ => {
                let text = jsonrpc::failure(&id, METHOD_NOT_FOUND, "method not found");
                self.tell_view(text);
            }
        }
    }

    fn view_notification(
        &mut self,
        live: &mut Live,
        method: &str,
        envelope: &Envelope,
    ) -> Option<Cause> {
        match method {
            "ui/notifications/initialized" if live.state == ViewState::AwaitInitialized => {
                live.state = ViewState::Ready;
                self.view_ready(live);
                None
            }
            "notifications/cancelled" => {
                self.view_cancelled(live, envelope);
                None
            }
            "notifications/message" => {
                if self.config.logging {
                    if let Some(params) = envelope.params.as_deref() {
                        self.actions.push(Action::ViewLog(params.get().to_owned()));
                    }
                }
                None
            }
            // The draft of MCP Apps has it and the revision this host follows does
            // not; accepting it is the host's choice (§10.6).
            "ui/notifications/request-teardown" => Some(Cause::View),
            // The container is fixed: `ui/notifications/size-changed`, with which a
            // view asks for a container that follows its content, changes nothing
            // (§7.2).
            _ => None,
        }
    }

    /// The answer to `ui/initialize` (MCP Apps § Lifecycle), shaped by what the
    /// view declared.
    fn view_initialize(&mut self, live: &mut Live, id: &Id, envelope: &Envelope) {
        if live.state != ViewState::AwaitInitialize {
            let text = jsonrpc::failure(id, INVALID_REQUEST, "already initialized");
            self.tell_view(text);
            return;
        }
        let params: Option<Value> = envelope.params_as();
        let capabilities = params.as_ref().and_then(|p| p.get("appCapabilities"));
        live.pane_aware = capabilities
            .and_then(|c| c.get("experimental"))
            .and_then(|e| e.get(PANE_CAPABILITY))
            .is_some_and(|v| !v.is_null());
        live.display_mode = display_mode(capabilities);
        let context = &self.context;
        let result = InitializeResult {
            protocol_version: UI_PROTOCOL_VERSION,
            host_info: &self.config.host,
            host_capabilities: HostCapabilities {
                experimental: live.pane_aware.then(|| json!({ PANE_CAPABILITY: {} })),
                open_links: self.config.open_links.then_some(Empty {}),
                server_tools: Proxy {
                    list_changed: live.announces.tools,
                },
                server_resources: Proxy {
                    list_changed: live.announces.resources,
                },
                logging: self.config.logging.then_some(Empty {}),
                sandbox: live.grant.capability(),
            },
            host_context: HostContext {
                tool_info: ToolInfo {
                    tool: &live.found.tool,
                },
                theme: context.theme.mode(),
                styles: Styles {
                    variables: context.theme.variables(),
                },
                display_mode: live.display_mode,
                available_display_modes: [live.display_mode],
                container_dimensions: context.size.into(),
                locale: context.locale.as_deref(),
                time_zone: context.time_zone.as_deref(),
                user_agent: &context.user_agent,
                platform: "desktop",
            },
        };
        let text = jsonrpc::result(id, result);
        live.state = ViewState::AwaitInitialized;
        self.tell_view(text);
    }

    /// The view is initialized: it is given the tool's input, the tool is called
    /// on the program, and the result follows when it comes (§9.3 step 4).
    fn view_ready(&mut self, live: &mut Live) {
        let arguments = live.tool.arguments.clone().unwrap_or_default();
        let input = jsonrpc::notification(
            "ui/notifications/tool-input",
            Some(json!({ "arguments": arguments.clone() })),
        );
        self.tell_view(input);
        let call = self.next_rpc();
        let params = json!({ "name": live.tool.name, "arguments": arguments });
        let text = jsonrpc::request(&call, "tools/call", Some(params));
        if self.send_json(&text) {
            live.open_call = Some(call);
        } else {
            let cancelled = jsonrpc::notification(
                "ui/notifications/tool-cancelled",
                Some(json!({ "reason": "the call was too large to send" })),
            );
            self.tell_view(cancelled);
        }
    }

    /// Passes a request of the view to the program under an id of the host's own
    /// (§9.4).
    fn forward(&mut self, live: &mut Live, id: Id, method: &str, params: Option<Value>) {
        if live.forwards.len() >= MAX_IN_FLIGHT {
            let text = jsonrpc::failure(&id, DENIED, "too many requests in flight");
            self.tell_view(text);
            return;
        }
        // The parameters go as they are, the view's progress token with them: the
        // program's progress for it is passed back (§9.6).
        let progress = params
            .as_ref()
            .and_then(|p| p.get("_meta"))
            .and_then(|m| m.get("progressToken"))
            .filter(|t| t.is_string() || t.is_i64() || t.is_u64())
            .cloned();
        let program = self.next_rpc();
        let text = jsonrpc::request(&program, method, params.as_ref());
        if self.send_json(&text) {
            live.forwards.push(Forward {
                program,
                view: id,
                progress,
            });
        } else {
            let text = jsonrpc::failure(&id, DENIED, "the request is too large to send");
            self.tell_view(text);
        }
    }

    /// The view gives up a request it passed on (MCP § Cancellation, §9.4): the
    /// program is told under the host's id, and its answer, if it still comes,
    /// has nobody to go to. The view is not answered; it has given up waiting.
    fn view_cancelled(&mut self, live: &mut Live, envelope: &Envelope) {
        let cancelled: Option<Cancelled> = envelope
            .params
            .as_deref()
            .and_then(|params| crate::json::from_object(params.get()));
        let at = cancelled.as_ref().and_then(|cancelled| {
            live.forwards
                .iter()
                .position(|f| f.view == cancelled.request_id)
        });
        let (Some(cancelled), Some(at)) = (cancelled, at) else {
            self.ignored();
            return;
        };
        let forward = live.forwards.remove(at);
        let params = Cancelling {
            request_id: &forward.program,
            reason: cancelled
                .reason
                .as_ref()
                .and_then(Value::as_str)
                .map(|reason| reason.chars().take(200).collect()),
        };
        let text = jsonrpc::notification("notifications/cancelled", Some(params));
        self.send_json(&text);
    }

    /// A notification of the program (specification §9.6): that one of its lists
    /// changed, or the progress of a request of the view. The host drops anything
    /// else, and a list's change the program did not say it announces.
    pub(super) fn program_notification(&mut self, method: &str, envelope: &Envelope) {
        let Some(mut live) = self.take_live() else {
            self.ignored();
            return;
        };
        // A view is sent nothing before it has initialized, and lists what it
        // needs once it has.
        let ready = live.state == ViewState::Ready;
        let announces = live.announces;
        match method {
            "notifications/tools/list_changed" if announces.tools => {
                self.tools_changed(&mut live);
            }
            "notifications/resources/list_changed" if announces.resources && ready => {
                self.pass_to_view(method, envelope);
            }
            "notifications/prompts/list_changed" if announces.prompts && ready => {
                self.pass_to_view(method, envelope);
            }
            "notifications/progress" => {
                let token = envelope
                    .params
                    .as_deref()
                    .and_then(|params| crate::json::from_object::<Progress>(params.get()))
                    .and_then(|progress| progress.progress_token);
                let the_views = token.is_some_and(|token| {
                    live.forwards
                        .iter()
                        .any(|f| f.progress.as_ref() == Some(&token))
                });
                if the_views {
                    self.pass_to_view(method, envelope);
                } else {
                    self.ignored();
                }
            }
            _ => self.ignored(),
        }
        self.phase = Phase::Live(live);
    }

    /// A notification of the program, passed to the view as the program wrote it.
    fn pass_to_view(&mut self, method: &str, envelope: &Envelope) {
        let text = match envelope.params.as_deref() {
            Some(params) => jsonrpc::notification_raw(method, params),
            None => jsonrpc::notification(method, None::<()>),
        };
        self.tell_view(text);
    }

    fn view_open_link(&mut self, live: &mut Live, id: Id, params: Option<&Value>) {
        let url = params.and_then(|p| p.get("url")).and_then(Value::as_str);
        match url {
            Some(url) if self.config.open_links && sandbox::is_openable_link(url) => {
                self.links = self.links.wrapping_add(1);
                live.links.push((self.links, id));
                self.actions.push(Action::OpenLink {
                    link: self.links,
                    url: url.to_owned(),
                });
            }
            _ => {
                let text = jsonrpc::failure(&id, DENIED, "Invalid URL");
                self.tell_view(text);
            }
        }
    }

    /// The person's answer to [`Action::OpenLink`].
    pub fn link_decision(&mut self, link: u32, allow: bool) {
        let view = match &mut self.phase {
            Phase::Live(live) => live
                .links
                .iter()
                .position(|(known, _)| *known == link)
                .map(|at| live.links.remove(at).1),
            _ => None,
        };
        let Some(view) = view else {
            return;
        };
        let text = if allow {
            jsonrpc::result(&view, json!({}))
        } else {
            jsonrpc::failure(&view, DENIED, "Link opening denied by user")
        };
        self.tell_view(text);
    }

    /// The terminal's wait for the program's answers has run out: every request
    /// of the view that was passed on and has no answer fails the way MCP
    /// defines, and a call of the tool the view is for that is still out is
    /// cancelled (specification §9.4). The crate has no clock, so how long to
    /// wait is the terminal's.
    pub fn requests_timed_out(&mut self) {
        let Some(mut live) = self.take_live() else {
            return;
        };
        for forward in std::mem::take(&mut live.forwards) {
            let text =
                jsonrpc::failure(&forward.view, DENIED, "the program did not answer in time");
            self.tell_view(text);
        }
        if live.open_call.take().is_some() {
            let text = jsonrpc::notification(
                "ui/notifications/tool-cancelled",
                Some(json!({ "reason": "the program did not answer in time" })),
            );
            self.tell_view(text);
        }
        // A reading of the tools that is out is given up: the host keeps the tools
        // it knew, and the view still hears that they changed.
        if live.refresh.take().is_some() {
            self.tools_read(&mut live, None, false);
        }
        self.phase = Phase::Live(live);
    }

    /// An answer of the program to a request of the host: the call of the tool
    /// the view is for, or a request of the view that was passed on.
    pub(super) fn live_response(&mut self, id: &Id, envelope: &Envelope) {
        let Some(mut live) = self.take_live() else {
            return;
        };
        if live.refresh.as_ref().is_some_and(|r| &r.request == id) {
            self.refresh_page(&mut live, envelope);
        } else if live.open_call.as_ref() == Some(id) {
            live.open_call = None;
            let text = match (envelope.result.as_deref(), envelope.kind()) {
                (Some(result), Some(Kind::Result { .. })) => {
                    jsonrpc::notification_raw("ui/notifications/tool-result", result)
                }
                _ => {
                    let reason = envelope.error_message().unwrap_or_default();
                    let reason: String = reason.chars().take(200).collect();
                    jsonrpc::notification(
                        "ui/notifications/tool-cancelled",
                        Some(json!({ "reason": format!("the tool call failed: {reason}") })),
                    )
                }
            };
            self.tell_view(text);
        } else if let Some(at) = live.forwards.iter().position(|f| &f.program == id) {
            let forward = live.forwards.remove(at);
            match jsonrpc::relay(envelope, &forward.view) {
                Some(text) => self.tell_view(text),
                None => self.ignored(),
            }
        } else {
            self.ignored();
        }
        self.phase = Phase::Live(live);
    }

    /// The pane's size or the terminal's look changed: a view that runs is told
    /// (specification §7.2, §11.1).
    pub(super) fn context_changed(&mut self, size: bool, theme: bool) {
        if !matches!(&self.phase, Phase::Live(live) if live.state == ViewState::Ready) {
            return;
        }
        let mut params = serde_json::Map::new();
        if size {
            params.insert(
                "containerDimensions".to_owned(),
                json!({ "width": self.context.size.width, "height": self.context.size.height }),
            );
        }
        if theme {
            params.insert("theme".to_owned(), json!(self.context.theme.mode()));
            params.insert(
                "styles".to_owned(),
                json!({ "variables": self.context.theme.variables() }),
            );
        }
        let text = jsonrpc::notification(
            "ui/notifications/host-context-changed",
            Some(Value::Object(params)),
        );
        self.tell_view(text);
    }
}
