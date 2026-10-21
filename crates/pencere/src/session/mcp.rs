//! The host as an MCP client of the program (specification §9.1 to §9.3): the
//! start-up of the MCP session, up to the moment a view can start.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use super::{
    Action, Found, Live, MAX_TOOL_PAGES, MAX_TOOLS, Opening, Phase, Session, Stage, ToolEntry,
    ViewSource, ViewStart, ViewState,
};
use crate::base64;
use crate::bundle::Store;
use crate::frame::Message;
use crate::jsonrpc::{self, Envelope, Id, Kind, METHOD_NOT_FOUND};
use crate::message::{HostInfo, ToolCall};
use crate::sandbox::{self, Grant, UiMeta};
use crate::token::Entropy;

/// The MCP protocol revision the host asks for as a client.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// The media type of a UI resource.
pub const UI_MIME_TYPE: &str = "text/html;profile=mcp-app";

#[derive(Serialize)]
struct InitializeParams<'a> {
    #[serde(rename = "protocolVersion")]
    protocol_version: &'a str,
    capabilities: Capabilities,
    #[serde(rename = "clientInfo")]
    client_info: &'a HostInfo,
}

#[derive(Serialize)]
struct Capabilities {
    extensions: Extensions,
}

#[derive(Serialize)]
struct Extensions {
    #[serde(rename = "io.modelcontextprotocol/ui")]
    ui: UiExtension,
}

#[derive(Serialize)]
struct UiExtension {
    #[serde(rename = "mimeTypes")]
    mime_types: [&'static str; 1],
}

#[derive(Serialize)]
struct ToolsListParams<'a> {
    cursor: &'a str,
}

#[derive(Serialize)]
struct ReadParams<'a> {
    uri: &'a str,
}

#[derive(Deserialize)]
struct ToolsPage {
    tools: Vec<Box<RawValue>>,
    #[serde(rename = "nextCursor", default)]
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct ToolHead {
    name: String,
    #[serde(rename = "_meta", default)]
    meta: Option<ToolMeta>,
}

#[derive(Deserialize)]
struct ToolMeta {
    #[serde(default)]
    ui: Option<ToolUi>,
    #[serde(rename = "ui/resourceUri", default)]
    legacy_uri: Option<String>,
}

#[derive(Deserialize)]
struct ToolUi {
    #[serde(rename = "resourceUri", default)]
    resource_uri: Option<String>,
    #[serde(default)]
    visibility: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ReadResult {
    contents: Vec<Content>,
}

#[derive(Deserialize)]
struct Content {
    #[serde(default)]
    uri: Option<String>,
    #[serde(rename = "mimeType", default)]
    mime_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    blob: Option<String>,
    #[serde(rename = "_meta", default)]
    meta: Option<Value>,
}

impl ToolHead {
    /// The view the tool names (`_meta.ui.resourceUri`, or the deprecated flat
    /// `_meta["ui/resourceUri"]`).
    fn uri(&self) -> Option<&str> {
        let meta = self.meta.as_ref()?;
        meta.ui
            .as_ref()
            .and_then(|ui| ui.resource_uri.as_deref())
            .or(meta.legacy_uri.as_deref())
    }

    /// Whether the view may call the tool: `app` is in its visibility, which
    /// defaults to `["model", "app"]` (MCP Apps § Resource Discovery).
    fn app_visible(&self) -> bool {
        let visibility = self
            .meta
            .as_ref()
            .and_then(|meta| meta.ui.as_ref())
            .and_then(|ui| ui.visibility.as_ref());
        visibility.is_none_or(|list| list.iter().any(|v| v == "app"))
    }
}

/// What reading a page of tools came to.
enum Page {
    /// The program has more pages: the request for the next is out.
    Next(Stage),
    /// Every tool is known, and the one the open asked for is among them.
    Done { tools: Vec<ToolEntry>, found: Found },
}

/// The document of a UI resource and its metadata, out of a `resources/read`
/// result.
fn document(result: &ReadResult, uri: &str) -> Option<(String, UiMeta)> {
    let content = result
        .contents
        .iter()
        .find(|c| c.uri.as_deref() == Some(uri))
        .or_else(|| result.contents.first())?;
    let mime: String = content
        .mime_type
        .as_deref()?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if mime != UI_MIME_TYPE {
        return None;
    }
    let html = match (&content.text, &content.blob) {
        (Some(text), _) => text.clone(),
        (None, Some(blob)) => String::from_utf8(base64::decode(blob.as_bytes())?).ok()?,
        (None, None) => return None,
    };
    let meta = match content.meta.as_ref().and_then(|meta| meta.get("ui")) {
        Some(ui) if !ui.is_null() => UiMeta::from_value(ui).ok()?,
        _ => UiMeta::default(),
    };
    Some((html, meta))
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// The files are here (or none were missing): the host becomes an MCP client
    /// of the program.
    pub(super) fn begin_mcp(&mut self) {
        let request = self.next_rpc();
        let params = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION,
            capabilities: Capabilities {
                extensions: Extensions {
                    ui: UiExtension {
                        mime_types: [UI_MIME_TYPE],
                    },
                },
            },
            client_info: &self.config.host,
        };
        let text = jsonrpc::request(&request, "initialize", Some(&params));
        if !self.send_json(&text) {
            self.fail("internal");
            return;
        }
        if let Phase::Opening(open) = &mut self.phase {
            open.stage = Stage::Initialize(request);
        }
    }

    /// A `msg` from the program: one JSON-RPC message.
    pub(super) fn on_msg(&mut self, message: &Message) {
        if matches!(self.phase, Phase::Idle) {
            self.ignored();
            return;
        }
        let Some(envelope) = Envelope::parse(&message.payload) else {
            self.ignored();
            return;
        };
        match envelope.kind() {
            Some(Kind::Request { id, method }) => self.program_request(&id, &method),
            Some(Kind::Result { id } | Kind::Failure { id }) => {
                self.program_response(&id, &envelope)
            }
            Some(Kind::Notification { .. }) | None => self.ignored(),
        }
    }

    /// A request of the program to its client. The host offers it nothing but
    /// `ping`.
    fn program_request(&mut self, id: &Id, method: &str) {
        let text = if method == "ping" {
            jsonrpc::result(id, serde_json::json!({}))
        } else {
            jsonrpc::failure(id, METHOD_NOT_FOUND, "method not found")
        };
        self.send_json(&text);
    }

    fn program_response(&mut self, id: &Id, envelope: &Envelope) {
        match &self.phase {
            Phase::Opening(_) => self.startup_response(id, envelope),
            Phase::Live(_) => self.live_response(id, envelope),
            Phase::Idle | Phase::Closing(_) => self.ignored(),
        }
    }

    fn startup_response(&mut self, id: &Id, envelope: &Envelope) {
        let Some(mut open) = self.take_opening() else {
            return;
        };
        let failed = matches!(envelope.kind(), Some(Kind::Failure { .. }));
        let stage = std::mem::replace(&mut open.stage, Stage::Ready);
        match stage {
            Stage::Initialize(request) if &request == id => {
                if failed {
                    self.phase = Phase::Opening(open);
                    self.fail("mcp");
                    return;
                }
                let initialized = jsonrpc::notification("notifications/initialized", None::<()>);
                let next = self.next_rpc();
                let list = jsonrpc::request(&next, "tools/list", None::<()>);
                if !(self.send_json(&initialized) && self.send_json(&list)) {
                    self.phase = Phase::Opening(open);
                    self.fail("internal");
                    return;
                }
                open.stage = Stage::Tools {
                    request: next,
                    pages: 1,
                    tools: Vec::new(),
                    found: None,
                };
                self.phase = Phase::Opening(open);
            }
            Stage::Tools {
                request,
                pages,
                tools,
                found,
            } if &request == id => {
                let outcome = if failed {
                    None
                } else {
                    self.tools_page(&mut open, envelope, pages, tools, found)
                };
                match outcome {
                    Some(Page::Next(stage)) => {
                        open.stage = stage;
                        self.phase = Phase::Opening(open);
                    }
                    Some(Page::Done { tools, found }) => {
                        self.phase = Phase::Opening(open);
                        self.document_of(tools, found);
                    }
                    None => {
                        self.phase = Phase::Opening(open);
                        self.fail("mcp");
                    }
                }
            }
            Stage::Resource {
                request,
                tools,
                found,
            } if &request == id => {
                let read = if failed {
                    None
                } else {
                    envelope.result_as::<ReadResult>()
                };
                let Some((html, meta)) = read.and_then(|read| document(&read, &found.uri)) else {
                    self.phase = Phase::Opening(open);
                    self.fail("mcp");
                    return;
                };
                open.stage = Stage::Ready;
                self.phase = Phase::Opening(open);
                self.present(ViewSource::Document(html), meta, tools, found);
            }
            other => {
                // Not the answer being waited for.
                open.stage = other;
                self.phase = Phase::Opening(open);
                self.ignored();
            }
        }
    }

    /// Reads one page of `tools/list`. Returns the next stage, or `None` when
    /// the page is not one or the open cannot go on.
    fn tools_page(
        &mut self,
        open: &mut Opening,
        envelope: &Envelope,
        pages: u32,
        mut tools: Vec<ToolEntry>,
        mut found: Option<Found>,
    ) -> Option<Page> {
        let page = envelope.result_as::<ToolsPage>()?;
        for raw in page.tools {
            let head: ToolHead = crate::json::from_object(raw.get())?;
            if tools.len() >= MAX_TOOLS {
                return None;
            }
            if found.is_none() && head.name == open.tool.name {
                // A tool without a view cannot be opened.
                let uri = head
                    .uri()
                    .filter(|uri| uri.starts_with("ui://"))?
                    .to_owned();
                found = Some(Found { tool: raw, uri });
            }
            tools.push(ToolEntry {
                app: head.app_visible(),
                name: head.name,
            });
        }
        match page.next_cursor {
            Some(cursor) if pages < MAX_TOOL_PAGES => {
                let next = self.next_rpc();
                let text = jsonrpc::request(
                    &next,
                    "tools/list",
                    Some(ToolsListParams { cursor: &cursor }),
                );
                self.send_json(&text).then_some(Page::Next(Stage::Tools {
                    request: next,
                    pages: pages + 1,
                    tools,
                    found,
                }))
            }
            Some(_) => None,
            // The program has said all of its tools; one of them is ours.
            None => Some(Page::Done {
                tools,
                found: found?,
            }),
        }
    }

    /// The tools are known: the view's document comes from the bundle, or from
    /// the program.
    fn document_of(&mut self, tools: Vec<ToolEntry>, found: Found) {
        let bundle_meta = match &self.phase {
            Phase::Opening(open) => open
                .bundle
                .as_ref()
                .map(|bundle| bundle.meta().cloned().unwrap_or_default()),
            _ => return,
        };
        if let Some(meta) = bundle_meta {
            // The document and its metadata come from the bundle and the
            // program is not asked for them (§8.7).
            self.present(ViewSource::Bundle, meta, tools, found);
            return;
        }
        let request = self.next_rpc();
        let text = jsonrpc::request(
            &request,
            "resources/read",
            Some(ReadParams { uri: &found.uri }),
        );
        let sent = self.send_json(&text);
        if let Phase::Opening(open) = &mut self.phase {
            open.stage = Stage::Resource {
                request,
                tools,
                found,
            };
        }
        if !sent {
            self.fail("internal");
        }
    }

    /// The view's document and metadata are known: the person is asked what the
    /// view declared, or the view starts at once.
    fn present(&mut self, source: ViewSource, meta: UiMeta, tools: Vec<ToolEntry>, found: Found) {
        let Some(mut open) = self.take_opening() else {
            return;
        };
        match meta.consent() {
            Some(consent) => {
                open.stage = Stage::Consent {
                    source,
                    meta,
                    tools,
                    found,
                };
                self.phase = Phase::Opening(open);
                self.actions.push(Action::Consent(consent));
            }
            None => {
                let Opening { tool, bundle, .. } = *open;
                self.start_view(tool, bundle, source, meta, Grant::none(), tools, found);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_view(
        &mut self,
        tool: ToolCall,
        bundle: Option<crate::bundle::Manifest>,
        source: ViewSource,
        meta: UiMeta,
        grant: Grant,
        tools: Vec<ToolEntry>,
        found: Found,
    ) {
        let start = ViewStart {
            source,
            policy: sandbox::content_security_policy(&meta, &grant),
            allow: grant.allow_attribute(),
            tool: tool.name.clone(),
            meta: meta.clone(),
            grant: grant.clone(),
        };
        self.phase = Phase::Live(Box::new(Live {
            tool,
            found,
            tools,
            bundle,
            grant,
            state: ViewState::AwaitInitialize,
            pane_aware: false,
            display_mode: "inline",
            open_call: None,
            forwards: Vec::new(),
            links: Vec::new(),
        }));
        self.actions.push(Action::StartView(Box::new(start)));
    }
}
