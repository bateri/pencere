//! The host as an MCP client of the program (specification §9.1 to §9.3): the
//! start-up of the MCP session, up to the moment a view can start.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use super::{
    Action, Found, Live, MAX_LABEL_CHARS, MAX_TOOL_PAGES, MAX_TOOLS, Opening, Phase, Session,
    Stage, ToolEntry, ViewSource, ViewStart, ViewState,
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

/// The part of an `initialize` result the host reads, and only to name the
/// program for people. A member of the wrong type is not text, and is no reason
/// to stop the open, so the members are kept as raw JSON and read as strings
/// where they are wanted.
#[derive(Deserialize)]
struct InitializeResult {
    #[serde(rename = "serverInfo", default)]
    server_info: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
struct ServerInfo {
    #[serde(default)]
    title: Option<Box<RawValue>>,
    #[serde(default)]
    name: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
struct ToolHead {
    name: String,
    #[serde(default)]
    title: Option<Box<RawValue>>,
    #[serde(default)]
    annotations: Option<Box<RawValue>>,
    #[serde(rename = "_meta", default)]
    meta: Option<ToolMeta>,
}

#[derive(Deserialize)]
struct ToolAnnotations {
    #[serde(default)]
    title: Option<Box<RawValue>>,
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

    /// What the program calls the tool for people: `title`, else
    /// `annotations.title` (MCP § Tool). A title that is no text, or nothing
    /// once cleaned, leaves the next one its turn.
    fn title(&self) -> Option<String> {
        let annotated: Option<ToolAnnotations> = self
            .annotations
            .as_deref()
            .and_then(|raw| crate::json::from_object(raw.get()));
        [
            self.title.as_deref(),
            annotated.as_ref().and_then(|a| a.title.as_deref()),
        ]
        .into_iter()
        .find_map(|raw| as_text(raw).as_deref().and_then(label))
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

/// A member read as a string: whatever else it is, it is no text.
fn as_text(raw: Option<&RawValue>) -> Option<String> {
    serde_json::from_str(raw?.get()).ok()
}

/// Whether `c` is a format character (Unicode general category Cf): a character
/// that is no mark on the screen but does something to what is next to it, or to
/// nothing at all. The ranges are Unicode 16's, written out because the standard
/// library has no general categories and the crate takes no data table for one.
fn is_format(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Whether `c` stays out of a name the terminal draws: control characters, which
/// let a program type into the terminal or end a line, the line and paragraph
/// separators, and the format characters, which let it show text other than what
/// it wrote (the bidirectional embeddings, overrides and isolates) or make two
/// names that differ look like one (the zero-width space, the word joiner, the
/// byte order mark, the Arabic letter mark, the soft hyphen, the tag characters).
///
/// Four format characters stay, because text needs them:
///
/// - the zero-width joiner U+200D, which makes an emoji sequence one emoji (a
///   person at a laptop) and shapes several scripts, and the zero-width non-joiner
///   U+200C, which Persian, Kurdish and others write inside words;
/// - the left-to-right and right-to-left marks U+200E and U+200F, which a name
///   or title in a right-to-left script needs for the punctuation beside it to
///   land where its author put it. They only settle how neutral characters at the
///   edge of a run are placed, and, unlike the embeddings and overrides, cannot
///   make characters read in another order or hide them.
///
/// These four are as invisible as what is left out, so a program can still give
/// two tools names that differ and look alike: a name is for people to read, and
/// tells two programs apart no better than the program's say-so does. What this
/// closes is the rest of the invisible characters, which have no use in a name.
/// The three flags of England, Scotland and Wales are written with tag characters
/// and lose them, and show as a black flag.
fn is_hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}')
        || (is_format(c) && !matches!(c, '\u{200c}'..='\u{200f}'))
}

/// A name the program supplied, as the host passes it to the terminal: without
/// the characters of [`is_hostile`], at most [`MAX_LABEL_CHARS`] characters (cut
/// at a character, not at a grapheme) and without the blanks at its ends. `None`
/// when nothing is left.
fn label(text: &str) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|c| !is_hostile(*c))
        .take(MAX_LABEL_CHARS)
        .collect();
    let kept = kept.trim();
    (!kept.is_empty()).then(|| kept.to_owned())
}

/// What the program calls itself: `serverInfo.title`, else `serverInfo.name`.
fn program_name(envelope: &Envelope) -> Option<String> {
    let result: InitializeResult = envelope.result_as()?;
    let info: ServerInfo = crate::json::from_object(result.server_info?.get())?;
    [info.title.as_deref(), info.name.as_deref()]
        .into_iter()
        .find_map(|raw| as_text(raw).as_deref().and_then(label))
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
                open.program = program_name(envelope);
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
                found = Some(Found {
                    tool: raw,
                    uri,
                    title: head.title(),
                    program: open.program.clone(),
                });
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
            Some(mut consent) => {
                // The question is about the tool the person is about to see, in
                // the words the view starts with: one source, one cleaning.
                consent.tool = label(&open.tool.name);
                consent.title = found.title.clone();
                consent.program = found.program.clone();
                // Numbered by the session, one for each question ever asked, so that an
                // answer can be told from the answer to an earlier one: the program's own
                // numbers (the `open`'s id) would let it name the id a stale answer carries.
                self.questions = self.questions.wrapping_add(1);
                consent.id = self.questions;
                open.stage = Stage::Consent {
                    id: consent.id,
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
            tool_label: label(&tool.name),
            title: found.title.clone(),
            program: found.program.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_keeps_what_people_write() {
        assert_eq!(label("Containers").as_deref(), Some("Containers"));
        assert_eq!(
            label("  Café \u{2014} cache  ").as_deref(),
            Some("Café \u{2014} cache")
        );
        // A joiner is part of an emoji and of several scripts; the marks of
        // right-to-left text stay too.
        assert_eq!(
            label("a\u{200d}b\u{200f}").as_deref(),
            Some("a\u{200d}b\u{200f}")
        );
    }

    /// Every character of the Unicode 16 general category Cf, as ranges, from the Unicode
    /// Character Database: written out here from the source, not from `is_format`.
    const CF: [(u32, u32); 21] = [
        (0x00AD, 0x00AD),
        (0x0600, 0x0605),
        (0x061C, 0x061C),
        (0x06DD, 0x06DD),
        (0x070F, 0x070F),
        (0x0890, 0x0891),
        (0x08E2, 0x08E2),
        (0x180E, 0x180E),
        (0x200B, 0x200F),
        (0x202A, 0x202E),
        (0x2060, 0x2064),
        (0x2066, 0x206F),
        (0xFEFF, 0xFEFF),
        (0xFFF9, 0xFFFB),
        (0x110BD, 0x110BD),
        (0x110CD, 0x110CD),
        (0x13430, 0x1343F),
        (0x1BCA0, 0x1BCA3),
        (0x1D173, 0x1D17A),
        (0xE0001, 0xE0001),
        (0xE0020, 0xE007F),
    ];

    #[test]
    fn a_label_loses_the_invisible_characters_that_make_two_names_look_like_one() {
        // The word joiner, the zero-width space, the byte order mark and the Arabic letter
        // mark among them: a name with one in it looks like the name without.
        for (first, last) in CF {
            for code in first..=last {
                let c = char::from_u32(code).unwrap();
                if matches!(c, '\u{200c}'..='\u{200f}') {
                    continue;
                }
                let text = format!("Cont{c}ainers");
                assert_eq!(label(&text).as_deref(), Some("Containers"), "U+{code:04X}");
            }
        }
        // What the ends of a name hold is no part of it either.
        assert_eq!(
            label("\u{feff}\u{200b}Containers\u{2060}").as_deref(),
            Some("Containers")
        );
        assert_eq!(label("\u{200b}\u{2060}\u{feff}"), None);
    }

    #[test]
    fn a_label_keeps_the_joiners_and_the_marks_that_text_needs() {
        // An emoji sequence is one emoji by its joiners; Persian is written with the non-joiner.
        let emoji = "\u{1f469}\u{200d}\u{1f4bb} laptop";
        assert_eq!(label(emoji).as_deref(), Some(emoji));
        let persian = "\u{645}\u{6cc}\u{200c}\u{62e}\u{648}\u{627}\u{647}\u{645}";
        assert_eq!(label(persian).as_deref(), Some(persian));
        // A right-to-left name with its punctuation placed by the mark.
        let hebrew = "\u{5e9}\u{5dc}\u{5d5}\u{5dd}\u{200f} (beta)";
        assert_eq!(label(hebrew).as_deref(), Some(hebrew));
        let ltr = "a\u{200e}b";
        assert_eq!(label(ltr).as_deref(), Some(ltr));
    }

    #[test]
    fn a_label_loses_what_would_act_in_the_terminal_or_hide_what_it_says() {
        for hostile in [
            "\u{1b}", "\u{7}", "\n", "\r", "\t", "\u{0}", "\u{7f}", "\u{9b}", "\u{2028}",
            "\u{2029}", "\u{202a}", "\u{202d}", "\u{202e}", "\u{2066}", "\u{2069}",
        ] {
            let text = format!("a{hostile}b");
            assert_eq!(label(&text).as_deref(), Some("ab"), "{hostile:?}");
        }
        assert_eq!(label("\u{1b}[2J").as_deref(), Some("[2J"));
    }

    #[test]
    fn a_label_is_cut_at_a_character_and_what_it_loses_does_not_count() {
        let long = "\u{e9}".repeat(MAX_LABEL_CHARS + 50);
        let cut = label(&long).unwrap();
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        let buried = format!("{}ok", "\u{1}".repeat(10_000));
        assert_eq!(label(&buried).as_deref(), Some("ok"));
        // The blanks the cut leaves at its end go too.
        let spaced = format!("{}{}", "a".repeat(MAX_LABEL_CHARS - 1), " b");
        assert_eq!(label(&spaced), Some("a".repeat(MAX_LABEL_CHARS - 1)));
    }

    #[test]
    fn nothing_left_is_no_label() {
        for blank in ["", " ", "\u{1b}\u{7}", " \n\t "] {
            assert_eq!(label(blank), None, "{blank:?}");
        }
    }
}
