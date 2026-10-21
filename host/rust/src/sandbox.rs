//! The cage of a view (specification §12): what the view's metadata declares,
//! what the person is asked, and the content security policy built from what the
//! person allowed.
//!
//! The view is untrusted code that runs on the person's machine, so what it
//! declares is a request, not a grant. MCP Apps asks a host to warn; here the
//! person is asked before any declared domain or permission is usable, and the
//! policy is built from the answer, never from the declaration.

use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde_json::{Map, Value, json};

/// The policy of a view whose metadata declares no `csp` at all (MCP Apps § UI
/// Resource Format, "Restrictive Default").
pub const DEFAULT_POLICY: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; media-src 'self' data:; connect-src 'none';";

/// Where an address is, as far as its text says. A name can resolve to an
/// address its text does not suggest, so this is a hint for the question put to
/// the person, not a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locality {
    /// An address on this machine (`localhost`, `127.0.0.0/8`, `::1`).
    Loopback,
    /// An address on a private network or a name with no public suffix.
    Private,
    /// Anything else.
    Public,
}

/// An origin a view declares, in the form that goes into a content security
/// policy: `scheme://host[:port]`, lowercase, the host a name, an address, or
/// `*.` and a name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Domain {
    text: String,
    locality: Locality,
}

impl Domain {
    /// Reads a declared origin. `None` when it is not one: only `http`, `https`,
    /// `ws` and `wss` origins with a host and an optional port, no path, no
    /// credentials and nothing a policy would read as a second source or
    /// directive. A wildcard stands for the subdomains of a name of at least two
    /// labels.
    pub fn parse(text: &str) -> Option<Self> {
        let (scheme, rest) = text.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "ws" | "wss") {
            return None;
        }
        let (host, port) = split_port(rest)?;
        let host = host.to_ascii_lowercase();
        let locality = host_locality(&host)?;
        let mut normal = format!("{scheme}://{host}");
        if let Some(port) = port {
            normal.push(':');
            normal.push_str(port);
        }
        Some(Self {
            text: normal,
            locality,
        })
    }

    /// The origin as a policy writes it.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Where the origin is.
    pub fn locality(&self) -> Locality {
        self.locality
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

fn split_port(rest: &str) -> Option<(&str, Option<&str>)> {
    let (host, port) = if rest.starts_with('[') {
        let close = rest.find(']')?;
        let host = rest.get(..=close)?;
        match rest.get(close + 1..)? {
            "" => (host, None),
            tail => (host, Some(tail.strip_prefix(':')?)),
        }
    } else {
        match rest.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        }
    };
    if let Some(port) = port {
        let number: u32 = port
            .parse()
            .ok()
            .filter(|_| port.bytes().all(|b| b.is_ascii_digit()))?;
        if !(1..=65535).contains(&number) {
            return None;
        }
    }
    Some((host, port))
}

fn is_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn host_locality(host: &str) -> Option<Locality> {
    if let Some(inner) = host.strip_prefix('[') {
        let address: Ipv6Addr = inner.strip_suffix(']')?.parse().ok()?;
        return Some(match address.to_ipv4_mapped() {
            Some(v4) => ipv4_locality(v4),
            None => ipv6_locality(address),
        });
    }
    if host.len() > 253 {
        return None;
    }
    if let Ok(address) = host.parse::<Ipv4Addr>() {
        return Some(ipv4_locality(address));
    }
    let (wildcard, name) = match host.strip_prefix("*.") {
        Some(name) => (true, name),
        None => (false, host),
    };
    let labels: Vec<&str> = name.split('.').collect();
    if !labels.iter().all(|label| is_label(label)) || (wildcard && labels.len() < 2) {
        return None;
    }
    let last = *labels.last()?;
    if last == "localhost" {
        return Some(Locality::Loopback);
    }
    let private_suffix = matches!(
        last,
        "local" | "internal" | "lan" | "intranet" | "corp" | "home" | "arpa"
    );
    Some(if labels.len() == 1 || private_suffix {
        Locality::Private
    } else {
        Locality::Public
    })
}

fn ipv4_locality(address: Ipv4Addr) -> Locality {
    let [a, b, ..] = address.octets();
    if address.is_loopback() {
        Locality::Loopback
    } else if address.is_private()
        || address.is_link_local()
        || address.is_unspecified()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
    {
        Locality::Private
    } else {
        Locality::Public
    }
}

fn ipv6_locality(address: Ipv6Addr) -> Locality {
    if address.is_loopback() {
        Locality::Loopback
    } else if address.is_unspecified()
        || address.is_unique_local()
        || address.is_unicast_link_local()
    {
        Locality::Private
    } else {
        Locality::Public
    }
}

/// The directive a declared domain belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Directive {
    /// `connectDomains`: `connect-src`, network requests.
    Connect,
    /// `resourceDomains`: scripts, styles, images, fonts and media.
    Resource,
    /// `frameDomains`: `frame-src`, nested frames.
    Frame,
    /// `baseUriDomains`: `base-uri`.
    BaseUri,
}

impl Directive {
    /// The name the view's metadata gives the list.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Connect => "connectDomains",
            Self::Resource => "resourceDomains",
            Self::Frame => "frameDomains",
            Self::BaseUri => "baseUriDomains",
        }
    }

    const ALL: [Self; 4] = [Self::Connect, Self::Resource, Self::Frame, Self::BaseUri];
}

/// The domains a view declares, by directive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Csp {
    connect: Vec<Domain>,
    resource: Vec<Domain>,
    frame: Vec<Domain>,
    base_uri: Vec<Domain>,
}

impl Csp {
    /// The domains of a directive.
    pub fn domains(&self, directive: Directive) -> &[Domain] {
        match directive {
            Directive::Connect => &self.connect,
            Directive::Resource => &self.resource,
            Directive::Frame => &self.frame,
            Directive::BaseUri => &self.base_uri,
        }
    }

    fn list_mut(&mut self, directive: Directive) -> &mut Vec<Domain> {
        match directive {
            Directive::Connect => &mut self.connect,
            Directive::Resource => &mut self.resource,
            Directive::Frame => &mut self.frame,
            Directive::BaseUri => &mut self.base_uri,
        }
    }

    /// Whether no directive has a domain.
    pub fn is_empty(&self) -> bool {
        Directive::ALL.iter().all(|d| self.domains(*d).is_empty())
    }
}

/// A browser capability a view may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// `camera`.
    Camera,
    /// `microphone`.
    Microphone,
    /// `geolocation`.
    Geolocation,
    /// `clipboardWrite` (Permissions Policy `clipboard-write`).
    ClipboardWrite,
}

impl Permission {
    const ALL: [Self; 4] = [
        Self::Camera,
        Self::Microphone,
        Self::Geolocation,
        Self::ClipboardWrite,
    ];

    /// The name the view's metadata gives it.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Geolocation => "geolocation",
            Self::ClipboardWrite => "clipboardWrite",
        }
    }

    /// The Permissions Policy feature that goes into an `allow` attribute.
    pub const fn feature(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Geolocation => "geolocation",
            Self::ClipboardWrite => "clipboard-write",
        }
    }
}

/// A set of permissions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Permissions {
    camera: bool,
    microphone: bool,
    geolocation: bool,
    clipboard_write: bool,
}

impl Permissions {
    /// Whether `permission` is in the set.
    pub fn has(&self, permission: Permission) -> bool {
        match permission {
            Permission::Camera => self.camera,
            Permission::Microphone => self.microphone,
            Permission::Geolocation => self.geolocation,
            Permission::ClipboardWrite => self.clipboard_write,
        }
    }

    fn set(&mut self, permission: Permission) {
        match permission {
            Permission::Camera => self.camera = true,
            Permission::Microphone => self.microphone = true,
            Permission::Geolocation => self.geolocation = true,
            Permission::ClipboardWrite => self.clipboard_write = true,
        }
    }

    /// The permissions in the set, in a fixed order.
    pub fn list(&self) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|p| self.has(*p))
            .collect()
    }
}

/// Why a view's metadata could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaError;

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the view's metadata is malformed")
    }
}

impl std::error::Error for MetaError {}

/// The `_meta.ui` of a view (MCP Apps § UI Resource Format).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiMeta {
    csp: Option<Csp>,
    permissions: Permissions,
    domain: Option<String>,
    prefers_border: Option<bool>,
    rejected: Vec<String>,
}

fn optional<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key).filter(|value| !value.is_null())
}

impl UiMeta {
    /// Reads `_meta.ui`. A domain that is not an origin this crate accepts is
    /// not an error and is not allowed: it is left out and listed in
    /// [`rejected`](Self::rejected), because leaving it out can only narrow what
    /// the view reaches. A member of the wrong type is an error.
    ///
    /// # Errors
    ///
    /// [`MetaError`] when the value, `csp`, `permissions` or a list of domains is
    /// not the kind of JSON value it should be.
    pub fn from_value(value: &Value) -> Result<Self, MetaError> {
        let object = value.as_object().ok_or(MetaError)?;
        let mut meta = Self::default();
        if let Some(csp) = optional(object, "csp") {
            let csp = csp.as_object().ok_or(MetaError)?;
            let mut declared = Csp::default();
            for directive in Directive::ALL {
                let Some(list) = optional(csp, directive.key()) else {
                    continue;
                };
                for item in list.as_array().ok_or(MetaError)? {
                    let text = item.as_str().ok_or(MetaError)?;
                    match Domain::parse(text) {
                        Some(domain) if !declared.domains(directive).contains(&domain) => {
                            declared.list_mut(directive).push(domain);
                        }
                        Some(_) => {}
                        None => meta.rejected.push(text.chars().take(100).collect()),
                    }
                }
            }
            meta.csp = Some(declared);
        }
        if let Some(permissions) = optional(object, "permissions") {
            let permissions = permissions.as_object().ok_or(MetaError)?;
            for permission in Permission::ALL {
                if let Some(asked) = optional(permissions, permission.key()) {
                    asked.as_object().ok_or(MetaError)?;
                    meta.permissions.set(permission);
                }
            }
        }
        if let Some(domain) = optional(object, "domain") {
            meta.domain = Some(domain.as_str().ok_or(MetaError)?.to_owned());
        }
        if let Some(border) = optional(object, "prefersBorder") {
            meta.prefers_border = Some(border.as_bool().ok_or(MetaError)?);
        }
        Ok(meta)
    }

    /// The `csp` the view declares; `None` when it declares none at all, which
    /// gets the restrictive default policy.
    pub fn csp(&self) -> Option<&Csp> {
        self.csp.as_ref()
    }

    /// The permissions the view asks for.
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// The `domain` the view asks for, which a host may ignore.
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    /// Whether the view prefers a border, when it says.
    pub fn prefers_border(&self) -> Option<bool> {
        self.prefers_border
    }

    /// The declared domains that were left out for not being origins this crate
    /// accepts, as written (cut at 100 characters), for the host's log.
    pub fn rejected(&self) -> &[String] {
        &self.rejected
    }

    /// What the person has to be asked, or `None` when the view declares nothing
    /// that needs asking.
    pub fn consent(&self) -> Option<Consent> {
        let mut domains = Vec::new();
        if let Some(csp) = &self.csp {
            for directive in Directive::ALL {
                for domain in csp.domains(directive) {
                    domains.push(Declared {
                        directive,
                        domain: domain.clone(),
                    });
                }
            }
        }
        let permissions = self.permissions.list();
        (!domains.is_empty() || !permissions.is_empty()).then_some(Consent {
            domains,
            permissions,
        })
    }
}

/// One domain a view declared, with the directive it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The directive the domain is declared under.
    pub directive: Directive,
    /// The domain.
    pub domain: Domain,
}

/// What the person is asked: the domains and the permissions a view declared.
/// The question names them, says where the program runs (the host knows that,
/// not this crate) and says which ones are on this machine or a private network
/// ([`Domain::locality`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consent {
    /// The declared domains.
    pub domains: Vec<Declared>,
    /// The permissions asked for.
    pub permissions: Vec<Permission>,
}

impl Consent {
    /// The answer "allow everything that was asked".
    pub fn grant_all(&self) -> Grant {
        let mut grant = Grant::none();
        for declared in &self.domains {
            grant.allow_domain(declared.directive, declared.domain.clone());
        }
        for permission in &self.permissions {
            grant.allow_permission(*permission);
        }
        grant
    }
}

/// What the person allowed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grant {
    csp: Csp,
    permissions: Permissions,
}

impl Grant {
    /// Nothing allowed: the answer when the view declares nothing, or the person
    /// refuses.
    pub fn none() -> Self {
        Self::default()
    }

    /// Allows a domain for a directive.
    pub fn allow_domain(&mut self, directive: Directive, domain: Domain) {
        let list = self.csp.list_mut(directive);
        if !list.contains(&domain) {
            list.push(domain);
        }
    }

    /// Allows a permission.
    pub fn allow_permission(&mut self, permission: Permission) {
        self.permissions.set(permission);
    }

    /// This grant cut down to what `meta` declares: nothing the view did not ask
    /// for is ever allowed, whatever the answer says.
    pub fn within(&self, meta: &UiMeta) -> Self {
        let mut grant = Self::none();
        if let Some(declared) = meta.csp() {
            for directive in Directive::ALL {
                for domain in self.csp.domains(directive) {
                    if declared.domains(directive).contains(domain) {
                        grant.allow_domain(directive, domain.clone());
                    }
                }
            }
        }
        for permission in self.permissions.list() {
            if meta.permissions().has(permission) {
                grant.allow_permission(permission);
            }
        }
        grant
    }

    /// The allowed domains.
    pub fn csp(&self) -> &Csp {
        &self.csp
    }

    /// The allowed permissions.
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// The value of the `allow` attribute of the view's frame: the Permissions
    /// Policy features that were allowed, separated by spaces.
    pub fn allow_attribute(&self) -> String {
        let features: Vec<&str> = self
            .permissions
            .list()
            .into_iter()
            .map(Permission::feature)
            .collect();
        features.join(" ")
    }

    /// `hostCapabilities.sandbox` for the view (MCP Apps § Host Capabilities):
    /// what was allowed, and only that.
    pub fn capability(&self) -> Value {
        let mut permissions = Map::new();
        for permission in self.permissions.list() {
            permissions.insert(permission.key().to_owned(), json!({}));
        }
        let mut csp = Map::new();
        for directive in Directive::ALL {
            let domains = self.csp.domains(directive);
            if !domains.is_empty() {
                csp.insert(
                    directive.key().to_owned(),
                    Value::Array(domains.iter().map(|d| Value::from(d.as_str())).collect()),
                );
            }
        }
        let mut sandbox = Map::new();
        if !permissions.is_empty() {
            sandbox.insert("permissions".to_owned(), Value::Object(permissions));
        }
        if !csp.is_empty() {
            sandbox.insert("csp".to_owned(), Value::Object(csp));
        }
        Value::Object(sandbox)
    }
}

/// The content security policy of a view: the restrictive default when the view
/// declares no `csp`, otherwise MCP Apps' construction from the declared domains
/// **that were allowed** (§ Security Implications → Content Security Policy
/// Enforcement), with no domain the person did not approve.
pub fn content_security_policy(meta: &UiMeta, grant: &Grant) -> String {
    if meta.csp().is_none() {
        return DEFAULT_POLICY.to_owned();
    }
    let allowed = grant.within(meta);
    let list = |directive| -> Vec<String> {
        allowed
            .csp
            .domains(directive)
            .iter()
            .map(|d| d.as_str().to_owned())
            .collect()
    };
    let source = |head: &[&str], directive| -> String {
        let mut parts: Vec<String> = head.iter().map(|s| (*s).to_owned()).collect();
        parts.extend(list(directive));
        parts.join(" ")
    };
    let frame = list(Directive::Frame);
    let base = list(Directive::BaseUri);
    [
        "default-src 'none'".to_owned(),
        format!(
            "script-src {}",
            source(&["'self'", "'unsafe-inline'"], Directive::Resource)
        ),
        format!(
            "style-src {}",
            source(&["'self'", "'unsafe-inline'"], Directive::Resource)
        ),
        format!("connect-src {}", source(&["'self'"], Directive::Connect)),
        format!(
            "img-src {}",
            source(&["'self'", "data:"], Directive::Resource)
        ),
        format!("font-src {}", source(&["'self'"], Directive::Resource)),
        format!(
            "media-src {}",
            source(&["'self'", "data:"], Directive::Resource)
        ),
        format!(
            "frame-src {}",
            if frame.is_empty() {
                "'none'".to_owned()
            } else {
                frame.join(" ")
            }
        ),
        "object-src 'none'".to_owned(),
        format!(
            "base-uri {}",
            if base.is_empty() {
                "'self'".to_owned()
            } else {
                base.join(" ")
            }
        ),
    ]
    .join("; ")
        + ";"
}

/// Whether `url` may be opened for a view on the person's machine: `http` and
/// `https` only (specification §12.3).
pub fn is_openable_link(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && url.len() <= 8192
        && url.bytes().all(|b| (0x21..0x7f).contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(value: Value) -> UiMeta {
        UiMeta::from_value(&value).unwrap()
    }

    #[test]
    fn origins_are_read_into_the_form_a_policy_writes() {
        for (text, normal, locality) in [
            (
                "https://api.example.com",
                "https://api.example.com",
                Locality::Public,
            ),
            (
                "HTTPS://API.Example.COM:8443",
                "https://api.example.com:8443",
                Locality::Public,
            ),
            (
                "wss://realtime.example.com",
                "wss://realtime.example.com",
                Locality::Public,
            ),
            (
                "https://*.cloudflare.com",
                "https://*.cloudflare.com",
                Locality::Public,
            ),
            (
                "http://localhost:3000",
                "http://localhost:3000",
                Locality::Loopback,
            ),
            (
                "http://app.localhost",
                "http://app.localhost",
                Locality::Loopback,
            ),
            (
                "http://127.0.0.1:80",
                "http://127.0.0.1:80",
                Locality::Loopback,
            ),
            ("http://[::1]:9000", "http://[::1]:9000", Locality::Loopback),
            (
                "http://192.168.1.10",
                "http://192.168.1.10",
                Locality::Private,
            ),
            (
                "http://10.0.0.5:8080",
                "http://10.0.0.5:8080",
                Locality::Private,
            ),
            ("http://172.16.0.1", "http://172.16.0.1", Locality::Private),
            ("http://172.32.0.1", "http://172.32.0.1", Locality::Public),
            (
                "http://169.254.169.254",
                "http://169.254.169.254",
                Locality::Private,
            ),
            ("http://100.64.0.1", "http://100.64.0.1", Locality::Private),
            ("http://[fd00::1]", "http://[fd00::1]", Locality::Private),
            (
                "http://[::ffff:127.0.0.1]",
                "http://[::ffff:127.0.0.1]",
                Locality::Loopback,
            ),
            (
                "http://[2001:db8::1]",
                "http://[2001:db8::1]",
                Locality::Public,
            ),
            (
                "http://printer.local",
                "http://printer.local",
                Locality::Private,
            ),
            ("http://nas", "http://nas", Locality::Private),
        ] {
            let domain = Domain::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(domain.as_str(), normal, "{text}");
            assert_eq!(domain.locality(), locality, "{text}");
        }
    }

    #[test]
    fn what_could_be_read_as_more_than_one_origin_is_not_an_origin() {
        for bad in [
            "",
            "example.com",
            "https://",
            "https://example.com/",
            "https://example.com/path",
            "https://example.com?x",
            "https://user@example.com",
            "https://example.com;script-src *",
            "https://example.com https://evil.example",
            "https://example.com,https://evil.example",
            "https://exa'mple.com",
            "https://*",
            "https://*.com",
            "https://*.",
            "https://a*.example.com",
            "https://-a.example.com",
            "https://a..example.com",
            "https://example.com:0",
            "https://example.com:65536",
            "https://example.com:80a",
            "https://example.com:",
            "ftp://example.com",
            "data:text/html,x",
            "javascript:alert(1)",
            "https://[::1",
            "https://[nope]",
            "https://exa mple.com",
            "https://example.com\n",
            "https://caf\u{e9}.example",
            "'self'",
            "*",
        ] {
            assert!(Domain::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn metadata_is_read_and_what_is_not_an_origin_is_left_out_and_reported() {
        let m = meta(json!({
            "csp": {
                "connectDomains": ["https://api.example.com", "https://API.example.com", "not an origin", "https://b.example"],
                "resourceDomains": ["https://cdn.example.com"],
                "frameDomains": null,
            },
            "permissions": {"camera": {}, "geolocation": {}, "microphone": null},
            "domain": "abc.example",
            "prefersBorder": false,
            "unknown": 1,
        }));
        let csp = m.csp().unwrap();
        assert_eq!(
            csp.domains(Directive::Connect)
                .iter()
                .map(Domain::as_str)
                .collect::<Vec<_>>(),
            vec!["https://api.example.com", "https://b.example"]
        );
        assert_eq!(csp.domains(Directive::Resource).len(), 1);
        assert!(csp.domains(Directive::Frame).is_empty());
        assert_eq!(m.rejected(), ["not an origin"]);
        assert_eq!(
            m.permissions().list(),
            vec![Permission::Camera, Permission::Geolocation]
        );
        assert_eq!(
            (m.domain(), m.prefers_border()),
            (Some("abc.example"), Some(false))
        );
        // Members of the wrong type are errors.
        for bad in [
            json!(5),
            json!({"csp": 5}),
            json!({"csp": {"connectDomains": "https://a.example"}}),
            json!({"csp": {"connectDomains": [5]}}),
            json!({"permissions": []}),
            json!({"permissions": {"camera": true}}),
            json!({"domain": 5}),
            json!({"prefersBorder": "yes"}),
        ] {
            assert_eq!(UiMeta::from_value(&bad), Err(MetaError), "{bad}");
        }
        assert_eq!(UiMeta::from_value(&json!({})).unwrap(), UiMeta::default());
    }

    #[test]
    fn a_view_that_declares_nothing_gets_the_default_policy_and_nothing_to_ask() {
        let m = meta(json!({"prefersBorder": true}));
        assert!(m.consent().is_none());
        assert_eq!(content_security_policy(&m, &Grant::none()), DEFAULT_POLICY);
        // Even a grant for something the view did not declare changes nothing.
        let mut grant = Grant::none();
        grant.allow_domain(
            Directive::Connect,
            Domain::parse("https://evil.example").unwrap(),
        );
        assert_eq!(content_security_policy(&m, &grant), DEFAULT_POLICY);
        assert!(DEFAULT_POLICY.contains("connect-src 'none'"));
    }

    #[test]
    fn a_declared_csp_is_built_from_what_the_person_allowed() {
        let m = meta(json!({
            "csp": {
                "connectDomains": ["https://api.example.com", "wss://live.example.com"],
                "resourceDomains": ["https://cdn.example.com"],
                "frameDomains": ["https://video.example.com"],
                "baseUriDomains": ["https://base.example.com"],
            },
            "permissions": {"camera": {}, "clipboardWrite": {}},
        }));
        let consent = m.consent().unwrap();
        assert_eq!(consent.domains.len(), 5);
        assert_eq!(
            consent.permissions,
            vec![Permission::Camera, Permission::ClipboardWrite]
        );
        let all = consent.grant_all();
        assert_eq!(
            content_security_policy(&m, &all),
            "default-src 'none'; script-src 'self' 'unsafe-inline' https://cdn.example.com; \
             style-src 'self' 'unsafe-inline' https://cdn.example.com; \
             connect-src 'self' https://api.example.com wss://live.example.com; \
             img-src 'self' data: https://cdn.example.com; font-src 'self' https://cdn.example.com; \
             media-src 'self' data: https://cdn.example.com; frame-src https://video.example.com; \
             object-src 'none'; base-uri https://base.example.com;"
        );
        assert_eq!(all.allow_attribute(), "camera clipboard-write");
        // Nothing allowed: the construction with empty lists.
        assert_eq!(
            content_security_policy(&m, &Grant::none()),
            "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
             connect-src 'self'; img-src 'self' data:; font-src 'self'; media-src 'self' data:; \
             frame-src 'none'; object-src 'none'; base-uri 'self';"
        );
        // A grant for a domain the view did not declare is cut away.
        let mut greedy = Grant::none();
        greedy.allow_domain(
            Directive::Connect,
            Domain::parse("https://api.example.com").unwrap(),
        );
        greedy.allow_domain(
            Directive::Connect,
            Domain::parse("https://evil.example").unwrap(),
        );
        greedy.allow_domain(
            Directive::Resource,
            Domain::parse("https://api.example.com").unwrap(),
        );
        greedy.allow_permission(Permission::Microphone);
        let within = greedy.within(&m);
        assert_eq!(within.csp().domains(Directive::Connect).len(), 1);
        assert!(within.csp().domains(Directive::Resource).is_empty());
        assert!(within.permissions().list().is_empty());
        let policy = content_security_policy(&m, &greedy);
        assert!(
            policy.contains("connect-src 'self' https://api.example.com;"),
            "{policy}"
        );
        assert!(!policy.contains("evil"));
    }

    #[test]
    fn an_empty_csp_object_is_a_declared_csp() {
        let m = meta(json!({"csp": {}}));
        assert!(m.consent().is_none());
        assert!(content_security_policy(&m, &Grant::none()).contains("connect-src 'self';"));
    }

    #[test]
    fn the_capability_names_what_was_allowed_and_only_that() {
        let m = meta(json!({
            "csp": {"connectDomains": ["https://api.example.com"], "resourceDomains": ["https://cdn.example.com"]},
            "permissions": {"camera": {}},
        }));
        let mut grant = Grant::none();
        assert_eq!(grant.capability(), json!({}));
        grant.allow_domain(
            Directive::Connect,
            Domain::parse("https://api.example.com").unwrap(),
        );
        grant.allow_permission(Permission::Camera);
        assert_eq!(
            grant.within(&m).capability(),
            json!({"permissions": {"camera": {}}, "csp": {"connectDomains": ["https://api.example.com"]}})
        );
    }

    #[test]
    fn only_web_links_open() {
        for good in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "HTTPS://EXAMPLE.COM",
        ] {
            assert!(is_openable_link(good), "{good}");
        }
        for bad in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
            "mailto:a@b",
            "ssh://host",
            "https://exa mple.com",
            "https://example.com/\n",
            "//example.com",
            "https:/example.com",
        ] {
            assert!(!is_openable_link(bad), "{bad:?}");
        }
    }
}
