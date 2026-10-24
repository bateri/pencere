//! The cage of a web view (specification §12): what the page asks to reach in the `access`
//! of its `open`, what the person is asked, and the content security policy built from what
//! the person allowed.
//!
//! A page is untrusted code that runs on the person's machine, so what it asks for is a
//! request, not a grant. The person is asked before any origin or permission it names is
//! usable, and the policy is built from the answer, never from the request.

use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;

/// The policy of a page that is allowed no origin (specification §12.1): the one a page
/// without an `access` gets, and one whose person allowed nothing.
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

/// The directive a requested origin belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Directive {
    /// `connect`: `connect-src`, network requests.
    Connect,
    /// `resources`: scripts, styles, images, fonts and media.
    Resource,
    /// `frames`: `frame-src`, nested frames.
    Frame,
    /// `base`: `base-uri`.
    BaseUri,
}

impl Directive {
    /// The name the `access` of an `open` gives the list (specification §6.3).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Resource => "resources",
            Self::Frame => "frames",
            Self::BaseUri => "base",
        }
    }

    const ALL: [Self; 4] = [Self::Connect, Self::Resource, Self::Frame, Self::BaseUri];
}

/// The origins of a page, by directive.
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

/// A browser capability a page may ask for.
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

    /// The name the `access` of an `open` gives it.
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

/// Why an `access` could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessError;

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the access a page asks for is malformed")
    }
}

impl std::error::Error for AccessError {}

/// What a page asks to reach: the `access` of its `open` (specification §6.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    domains: Csp,
    permissions: Permissions,
    rejected: Vec<String>,
}

impl Access {
    /// Reads an `access`. An origin that is not one this crate accepts is not an error and is
    /// not allowed: it is left out and listed in [`rejected`](Self::rejected), because leaving
    /// it out can only narrow what the page reaches. A permission name that is not on the closed
    /// list is ignored the same way. A member of the wrong type is an error.
    ///
    /// # Errors
    ///
    /// [`AccessError`] when the value, one of the lists of origins or `permissions` is not the
    /// kind of JSON value it should be, or a list holds an item that is not a string.
    pub fn from_value(value: &Value) -> Result<Self, AccessError> {
        let object = value.as_object().ok_or(AccessError)?;
        let mut access = Self::default();
        for directive in Directive::ALL {
            let Some(list) = object.get(directive.key()) else {
                continue;
            };
            for item in list.as_array().ok_or(AccessError)? {
                let text = item.as_str().ok_or(AccessError)?;
                match Domain::parse(text) {
                    Some(domain) if !access.domains.domains(directive).contains(&domain) => {
                        access.domains.list_mut(directive).push(domain);
                    }
                    Some(_) => {}
                    None => access.rejected.push(text.chars().take(100).collect()),
                }
            }
        }
        if let Some(list) = object.get("permissions") {
            for item in list.as_array().ok_or(AccessError)? {
                let name = item.as_str().ok_or(AccessError)?;
                if let Some(permission) = Permission::ALL.into_iter().find(|p| p.key() == name) {
                    access.permissions.set(permission);
                }
            }
        }
        Ok(access)
    }

    /// The origins the page asks for.
    pub fn domains(&self) -> &Csp {
        &self.domains
    }

    /// The permissions the page asks for.
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// The origins that were left out for not being ones this crate accepts, as written (cut at
    /// 100 characters), for the host's log.
    pub fn rejected(&self) -> &[String] {
        &self.rejected
    }

    /// What the person has to be asked, or `None` when the page asks for nothing that needs
    /// asking. The question it returns has no [`Consent::id`] yet, and names neither the program
    /// nor the view: the session numbers the question and names both before it asks.
    pub fn consent(&self) -> Option<Consent> {
        let mut domains = Vec::new();
        for directive in Directive::ALL {
            for domain in self.domains.domains(directive) {
                domains.push(Declared {
                    directive,
                    domain: domain.clone(),
                });
            }
        }
        let permissions = self.permissions.list();
        (!domains.is_empty() || !permissions.is_empty()).then_some(Consent {
            id: 0,
            title: None,
            program: None,
            domains,
            permissions,
        })
    }
}

/// One origin a page asked for, with the directive it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The directive the origin is asked for under.
    pub directive: Directive,
    /// The origin.
    pub domain: Domain,
}

/// What the person is asked: the origins and the permissions a page asked for, and which
/// program asked. The question names them, says where the program runs (the host knows that,
/// not this crate) and says which origins are on this machine or a private network
/// ([`Domain::locality`]).
///
/// A host that remembers an answer finds what it was an answer about here: the program and what
/// was asked. How long an answer is remembered, and for which programs, is the host's policy
/// (specification §12.2); `program` is what the program calls itself, and tells two programs
/// apart no better than its say-so does.
///
/// This is not `#[non_exhaustive]`: a terminal that builds one itself, in a test or a stand-in
/// host, names the question and the two words as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consent {
    /// Which question this is: the session numbers each one it asks, and
    /// [`Session::consent`](crate::session::Session::consent) takes the number back
    /// with the answer, so that the answer to a question that is no longer asked
    /// (the open it was about ended, [`Action::WithdrawConsent`](crate::session::Action::WithdrawConsent))
    /// cannot settle another. `0` on what [`Access::consent`] returns, which no session has
    /// numbered.
    pub id: u32,
    /// What the program calls the view: the same text, cleaned the same way, as
    /// [`ViewStart::title`](crate::session::ViewStart::title).
    pub title: Option<String>,
    /// What the program calls itself: the same text as
    /// [`ViewStart::program`](crate::session::ViewStart::program). The program wrote it and a
    /// terminal draws the question in its own chrome, so the session cuts it to
    /// [`MAX_LABEL_CHARS`](crate::session::MAX_LABEL_CHARS) characters and leaves out control
    /// characters, the bidirectional overrides and the invisible format characters.
    pub program: Option<String>,
    /// The origins asked for.
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
    /// Nothing allowed: the answer when the page asks for nothing, or the person refuses.
    pub fn none() -> Self {
        Self::default()
    }

    /// Allows an origin for a directive.
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

    /// This grant cut down to what `access` asks for: nothing the page did not ask for is ever
    /// allowed, whatever the answer says.
    pub fn within(&self, access: &Access) -> Self {
        let mut grant = Self::none();
        for directive in Directive::ALL {
            for domain in self.csp.domains(directive) {
                if access.domains().domains(directive).contains(domain) {
                    grant.allow_domain(directive, domain.clone());
                }
            }
        }
        for permission in self.permissions.list() {
            if access.permissions().has(permission) {
                grant.allow_permission(permission);
            }
        }
        grant
    }

    /// The allowed origins.
    pub fn csp(&self) -> &Csp {
        &self.csp
    }

    /// The allowed permissions.
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// Whether nothing is allowed.
    pub fn is_empty(&self) -> bool {
        self.csp.is_empty() && self.permissions.list().is_empty()
    }

    /// The value of the `allow` attribute of the page's frame: the Permissions
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
}

/// A grant serialises in the shape of the `access` of an `open`, which is how the page is told
/// what it was allowed (the `access` of its `context`, specification §9.5): the lists that are
/// not empty, in the order of the specification, and nothing else.
impl Serialize for Grant {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let lists = Directive::ALL
            .into_iter()
            .filter(|d| !self.csp.domains(*d).is_empty())
            .count();
        let permissions = self.permissions.list();
        let mut map =
            serializer.serialize_map(Some(lists + usize::from(!permissions.is_empty())))?;
        for directive in Directive::ALL {
            let domains = self.csp.domains(directive);
            if !domains.is_empty() {
                let list: Vec<&str> = domains.iter().map(Domain::as_str).collect();
                map.serialize_entry(directive.key(), &list)?;
            }
        }
        if !permissions.is_empty() {
            let list: Vec<&str> = permissions.iter().map(|p| p.key()).collect();
            map.serialize_entry("permissions", &list)?;
        }
        map.end()
    }
}

/// The content security policy of a page (specification §12.1): the restrictive one when the
/// `open` has no `access` or the person allowed nothing of it, otherwise that policy with what
/// the person allowed added to the directive each list is for. A page never gets an origin the
/// person did not approve, whatever it asked.
pub fn content_security_policy(access: Option<&Access>, grant: &Grant) -> String {
    let Some(access) = access else {
        return DEFAULT_POLICY.to_owned();
    };
    let allowed = grant.within(access);
    if allowed.csp.is_empty() {
        return DEFAULT_POLICY.to_owned();
    }
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
    use serde_json::json;

    fn access(value: Value) -> Access {
        Access::from_value(&value).unwrap()
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
    fn an_access_is_read_and_what_is_not_an_origin_is_left_out_and_reported() {
        let a = access(json!({
            "connect": ["https://api.example.com", "https://API.example.com", "not an origin", "https://b.example"],
            "resources": ["https://cdn.example.com"],
            "frames": [],
            "permissions": ["camera", "geolocation", "teleport"],
            "unknown": 1,
        }));
        assert_eq!(
            a.domains()
                .domains(Directive::Connect)
                .iter()
                .map(Domain::as_str)
                .collect::<Vec<_>>(),
            vec!["https://api.example.com", "https://b.example"]
        );
        assert_eq!(a.domains().domains(Directive::Resource).len(), 1);
        assert!(a.domains().domains(Directive::Frame).is_empty());
        assert_eq!(a.rejected(), ["not an origin"]);
        // A name that is not on the closed list is ignored, not an error.
        assert_eq!(
            a.permissions().list(),
            vec![Permission::Camera, Permission::Geolocation]
        );
        // Members of the wrong type, and items that are not strings, are errors.
        for bad in [
            json!(5),
            json!([]),
            json!({"connect": 5}),
            json!({"connect": "https://a.example"}),
            json!({"connect": null}),
            json!({"connect": [5]}),
            json!({"resources": [["https://a.example"]]}),
            json!({"permissions": {}}),
            json!({"permissions": "camera"}),
            json!({"permissions": [true]}),
            json!({"permissions": [null]}),
        ] {
            assert_eq!(Access::from_value(&bad), Err(AccessError), "{bad}");
        }
        assert_eq!(Access::from_value(&json!({})).unwrap(), Access::default());
    }

    #[test]
    fn a_page_that_asks_for_nothing_gets_the_default_policy_and_nothing_to_ask() {
        let a = access(json!({"permissions": ["teleport"]}));
        assert!(a.consent().is_none());
        assert_eq!(
            content_security_policy(Some(&a), &Grant::none()),
            DEFAULT_POLICY
        );
        // Without an `access` at all the policy is the same.
        assert_eq!(
            content_security_policy(None, &Grant::none()),
            DEFAULT_POLICY
        );
        // Even a grant for something the page did not ask for changes nothing.
        let mut grant = Grant::none();
        grant.allow_domain(
            Directive::Connect,
            Domain::parse("https://evil.example").unwrap(),
        );
        assert_eq!(content_security_policy(Some(&a), &grant), DEFAULT_POLICY);
        assert_eq!(content_security_policy(None, &grant), DEFAULT_POLICY);
        assert!(DEFAULT_POLICY.contains("connect-src 'none'"));
    }

    #[test]
    fn the_policy_is_built_from_what_the_person_allowed() {
        let a = access(json!({
            "connect": ["https://api.example.com", "wss://live.example.com"],
            "resources": ["https://cdn.example.com"],
            "frames": ["https://video.example.com"],
            "base": ["https://base.example.com"],
            "permissions": ["camera", "clipboardWrite"],
        }));
        let consent = a.consent().unwrap();
        // The access alone names no program: the session does, when it asks.
        assert_eq!((&consent.title, &consent.program), (&None, &None));
        assert_eq!(consent.domains.len(), 5);
        assert_eq!(
            consent.permissions,
            vec![Permission::Camera, Permission::ClipboardWrite]
        );
        let all = consent.grant_all();
        assert_eq!(
            content_security_policy(Some(&a), &all),
            "default-src 'none'; script-src 'self' 'unsafe-inline' https://cdn.example.com; \
             style-src 'self' 'unsafe-inline' https://cdn.example.com; \
             connect-src 'self' https://api.example.com wss://live.example.com; \
             img-src 'self' data: https://cdn.example.com; font-src 'self' https://cdn.example.com; \
             media-src 'self' data: https://cdn.example.com; frame-src https://video.example.com; \
             object-src 'none'; base-uri https://base.example.com;"
        );
        assert_eq!(all.allow_attribute(), "camera clipboard-write");
        // Nothing allowed: the restrictive policy, whatever was asked.
        assert_eq!(
            content_security_policy(Some(&a), &Grant::none()),
            DEFAULT_POLICY
        );
        // Only a permission allowed: no origin, so the same policy again.
        let mut only_permission = Grant::none();
        only_permission.allow_permission(Permission::Camera);
        assert_eq!(
            content_security_policy(Some(&a), &only_permission),
            DEFAULT_POLICY
        );
        // A grant for an origin the page did not ask for is cut away.
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
        let within = greedy.within(&a);
        assert_eq!(within.csp().domains(Directive::Connect).len(), 1);
        assert!(within.csp().domains(Directive::Resource).is_empty());
        assert!(within.permissions().list().is_empty());
        let policy = content_security_policy(Some(&a), &greedy);
        assert!(
            policy.contains("connect-src 'self' https://api.example.com;"),
            "{policy}"
        );
        assert!(!policy.contains("evil"));
    }

    #[test]
    fn an_empty_access_is_an_access_that_asks_for_nothing() {
        let a = access(json!({}));
        assert!(a.consent().is_none());
        assert_eq!(
            content_security_policy(Some(&a), &Grant::none()),
            DEFAULT_POLICY
        );
    }

    #[test]
    fn what_was_allowed_is_written_in_the_shape_of_an_access_and_only_that() {
        let a = access(json!({
            "connect": ["https://api.example.com"],
            "resources": ["https://cdn.example.com"],
            "base": ["https://base.example.com"],
            "permissions": ["camera", "clipboardWrite"],
        }));
        let mut grant = Grant::none();
        assert!(grant.is_empty());
        assert_eq!(serde_json::to_string(&grant).unwrap(), "{}");
        grant.allow_domain(
            Directive::Connect,
            Domain::parse("https://api.example.com").unwrap(),
        );
        grant.allow_permission(Permission::ClipboardWrite);
        grant.allow_permission(Permission::Camera);
        assert!(!grant.is_empty());
        assert_eq!(
            serde_json::to_string(&grant.within(&a)).unwrap(),
            r#"{"connect":["https://api.example.com"],"permissions":["camera","clipboardWrite"]}"#
        );
        // The members come in the order of the specification, whatever order they were allowed in.
        let mut all = a.consent().unwrap().grant_all();
        all.allow_domain(
            Directive::Frame,
            Domain::parse("https://video.example.com").unwrap(),
        );
        assert_eq!(
            serde_json::to_string(&all).unwrap(),
            r#"{"connect":["https://api.example.com"],"resources":["https://cdn.example.com"],"frames":["https://video.example.com"],"base":["https://base.example.com"],"permissions":["camera","clipboardWrite"]}"#
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
