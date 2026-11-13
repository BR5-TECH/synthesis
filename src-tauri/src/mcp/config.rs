//! The MCP server records of a project (MCS-FR-EKPA to MCS-FR-UOIO).
//!
//! A record lives in the `mcpServers` array of tables of
//! `.synthesis/project.toml`. On disk a record is flat: its transport kind is
//! the `transport` key and the fields of that transport sit beside it. On the
//! wire to the frontend the transport is a nested object with a `kind`
//! (MCS contract surface).

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::project_settings::public_store::{load_public, save_public};

/// MCS-FR-EKPA: the project-public array the records live in.
pub const MCP_SERVERS_KEY: &str = "mcpServers";

/// MCS-FR-LLQD: the longest name, counted in characters.
const MAX_NAME_CHARS: usize = 80;

/// One MCP server record (MCS-FR-XOSW).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    pub transport: McpTransport,
}

/// MCS-FR-XOSW: `stdio` or `http`, with the fields of that transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum McpTransport {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

/// MCS-FR-KALQ, MCS-FR-KLQS.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdioTransport {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub secret_env: Vec<String>,
}

/// MCS-FR-DINZ, MCS-FR-UAAN.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpTransport {
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub secret_headers: Vec<String>,
    pub auth: McpAuth,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub callback_port: Option<u16>,
}

/// MCS-FR-DINZ: the auth mode of an `http` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpAuth {
    None,
    Bearer,
    Oauth2,
}

impl McpAuth {
    pub fn as_str(self) -> &'static str {
        match self {
            McpAuth::None => "none",
            McpAuth::Bearer => "bearer",
            McpAuth::Oauth2 => "oauth2",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(McpAuth::None),
            "bearer" => Some(McpAuth::Bearer),
            "oauth2" => Some(McpAuth::Oauth2),
            _ => None,
        }
    }
}

impl McpServer {
    pub fn transport_name(&self) -> &'static str {
        match self.transport {
            McpTransport::Stdio(_) => "stdio",
            McpTransport::Http(_) => "http",
        }
    }

    /// The auth mode, `none` for a `stdio` record.
    pub fn auth(&self) -> McpAuth {
        match &self.transport {
            McpTransport::Stdio(_) => McpAuth::None,
            McpTransport::Http(http) => http.auth,
        }
    }
}

/// The validation codes of the MCS contract surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpConfigError {
    InvalidId,
    DuplicateId,
    InvalidName,
    InvalidTransport,
    InvalidCommand,
    InvalidCwd,
    InvalidUrl,
    InvalidEnvName,
    InvalidHeaderName,
    InvalidHeaderValue,
    DuplicateName,
    AuthorizationHeaderConflict,
    InvalidCallbackPort,
    InvalidAuth,
}

impl McpConfigError {
    pub fn code(self) -> &'static str {
        match self {
            McpConfigError::InvalidId => "invalid_id",
            McpConfigError::DuplicateId => "duplicate_id",
            McpConfigError::InvalidName => "invalid_name",
            McpConfigError::InvalidTransport => "invalid_transport",
            McpConfigError::InvalidCommand => "invalid_command",
            McpConfigError::InvalidCwd => "invalid_cwd",
            McpConfigError::InvalidUrl => "invalid_url",
            McpConfigError::InvalidEnvName => "invalid_env_name",
            McpConfigError::InvalidHeaderName => "invalid_header_name",
            McpConfigError::InvalidHeaderValue => "invalid_header_value",
            McpConfigError::DuplicateName => "duplicate_name",
            McpConfigError::AuthorizationHeaderConflict => "authorization_header_conflict",
            McpConfigError::InvalidCallbackPort => "invalid_callback_port",
            McpConfigError::InvalidAuth => "invalid_auth",
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// MCS-FR-LLQD: `^[a-z0-9][a-z0-9-]{0,31}$`.
pub fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// MCS-FR-KALQ: `^[A-Za-z_][A-Za-z0-9_]*$`.
fn valid_env_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// MCS-FR-DINZ: a header name is an HTTP token.
fn valid_header_name(name: &str) -> bool {
    http::HeaderName::from_bytes(name.as_bytes()).is_ok()
}

/// MCS-FR-KLQS: a relative path that stays inside the worktree.
fn valid_cwd(cwd: &str) -> bool {
    let mut depth: i64 = 0;
    for component in Path::new(cwd).components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// MCS-FR-XDGN: `https`, or `http` to a loopback host.
fn valid_url(text: &str) -> bool {
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    match url.scheme() {
        "https" => url.host().is_some(),
        "http" => matches!(
            url.host(),
            Some(url::Host::Domain("localhost"))
                | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
                | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
        ),
        _ => false,
    }
}

/// Names that occur twice, compared as `fold` decides.
fn has_duplicate<'a>(names: impl Iterator<Item = &'a str>, fold: fn(&str) -> String) -> bool {
    let mut seen = std::collections::HashSet::new();
    names.into_iter().any(|name| !seen.insert(fold(name)))
}

/// MCS-FR-XCOS: validate a record, and return it with its text trimmed and the
/// fields its transport does not use cleared.
pub fn validate(mut server: McpServer) -> Result<McpServer, McpConfigError> {
    if !valid_id(&server.id) {
        return Err(McpConfigError::InvalidId);
    }
    server.name = server.name.trim().to_string();
    let name_chars = server.name.chars().count();
    if name_chars == 0 || name_chars > MAX_NAME_CHARS {
        return Err(McpConfigError::InvalidName);
    }
    match &mut server.transport {
        McpTransport::Stdio(stdio) => validate_stdio(stdio)?,
        McpTransport::Http(http) => validate_http(http)?,
    }
    Ok(server)
}

fn validate_stdio(stdio: &mut StdioTransport) -> Result<(), McpConfigError> {
    stdio.command = stdio.command.trim().to_string();
    if stdio.command.is_empty() {
        return Err(McpConfigError::InvalidCommand);
    }
    if stdio.cwd.as_deref().is_some_and(|cwd| cwd.trim().is_empty()) {
        stdio.cwd = None;
    }
    if stdio.cwd.as_deref().is_some_and(|cwd| !valid_cwd(cwd)) {
        return Err(McpConfigError::InvalidCwd);
    }
    let names =
        stdio.env.keys().map(String::as_str).chain(stdio.secret_env.iter().map(String::as_str));
    if !names.clone().all(valid_env_name) {
        return Err(McpConfigError::InvalidEnvName);
    }
    if has_duplicate(names, str::to_string) {
        return Err(McpConfigError::DuplicateName);
    }
    Ok(())
}

fn validate_http(http: &mut HttpTransport) -> Result<(), McpConfigError> {
    http.url = http.url.trim().to_string();
    if !valid_url(&http.url) {
        return Err(McpConfigError::InvalidUrl);
    }
    let names = http
        .headers
        .keys()
        .map(String::as_str)
        .chain(http.secret_headers.iter().map(String::as_str));
    if !names.clone().all(valid_header_name) {
        return Err(McpConfigError::InvalidHeaderName);
    }
    if http.headers.values().any(|value| http::HeaderValue::from_str(value).is_err()) {
        return Err(McpConfigError::InvalidHeaderValue);
    }
    if has_duplicate(names.clone(), str::to_ascii_lowercase) {
        return Err(McpConfigError::DuplicateName);
    }
    if http.auth != McpAuth::None
        && names.clone().any(|name| name.eq_ignore_ascii_case("authorization"))
    {
        return Err(McpConfigError::AuthorizationHeaderConflict);
    }
    if http.auth == McpAuth::Oauth2 {
        if http.client_id.as_deref().is_some_and(|id| id.trim().is_empty()) {
            http.client_id = None;
        }
        http.client_id = http.client_id.as_deref().map(|id| id.trim().to_string());
        http.scopes = http
            .scopes
            .iter()
            .map(|scope| scope.trim().to_string())
            .filter(|scope| !scope.is_empty())
            .collect();
        if http.callback_port.is_some_and(|port| port < 1024) {
            return Err(McpConfigError::InvalidCallbackPort);
        }
    } else {
        http.client_id = None;
        http.scopes.clear();
        http.callback_port = None;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The command fingerprint (MCS-FR-KWCN)
// ---------------------------------------------------------------------------

/// MCS-FR-KXYO: an `http` record uses a credential.
pub fn uses_credential(http: &HttpTransport) -> bool {
    http.auth != McpAuth::None || !http.secret_headers.is_empty()
}

/// MCS-FR-PPPL, MCS-FR-KWCN, MCS-FR-RISC: the trust fingerprint of a record, or
/// `None` for a record that needs no trust.
pub fn trust_fingerprint(server: &McpServer) -> Option<String> {
    match &server.transport {
        McpTransport::Stdio(stdio) => Some(command_fingerprint(stdio)),
        McpTransport::Http(http) if uses_credential(http) => Some(credential_fingerprint(http)),
        McpTransport::Http(_) => None,
    }
}

/// MCS-FR-KWCN: the SHA-256 of a canonical JSON object of the four values that
/// decide where a credential goes.
pub fn credential_fingerprint(http: &HttpTransport) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        url: &'a str,
        auth: McpAuth,
        #[serde(rename = "clientId")]
        client_id: Option<&'a str>,
        #[serde(rename = "secretHeaders")]
        secret_headers: &'a [String],
    }
    let canonical = Canonical {
        url: &http.url,
        auth: http.auth,
        client_id: http.client_id.as_deref(),
        secret_headers: &http.secret_headers,
    };
    sha256_hex(&serde_json::to_string(&canonical).unwrap_or_default())
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// MCS-FR-KWCN: the SHA-256 of a canonical JSON object of the five values that
/// decide what runs. `serde_json` writes the fields in declaration order and a
/// `BTreeMap` in key order, so equal values always give equal text.
pub fn command_fingerprint(stdio: &StdioTransport) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        command: &'a str,
        args: &'a [String],
        cwd: Option<&'a str>,
        env: &'a BTreeMap<String, String>,
        #[serde(rename = "secretEnv")]
        secret_env: &'a [String],
    }
    let canonical = Canonical {
        command: &stdio.command,
        args: &stdio.args,
        cwd: stdio.cwd.as_deref(),
        env: &stdio.env,
        secret_env: &stdio.secret_env,
    };
    sha256_hex(&serde_json::to_string(&canonical).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// TOML
// ---------------------------------------------------------------------------

/// One record as the file holds it: its id, and the record or the code of the
/// first rule it fails (MCS-FR-HBOD).
#[derive(Clone, Debug, PartialEq)]
pub struct ListedRecord {
    pub id: String,
    pub record: Result<McpServer, McpConfigError>,
}

fn str_of(table: &toml::Table, key: &str) -> Option<String> {
    table.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn strings_of(table: &toml::Table, key: &str) -> Result<Vec<String>, ()> {
    match table.get(key) {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(items)) => {
            items.iter().map(|v| v.as_str().map(str::to_string).ok_or(())).collect()
        }
        Some(_) => Err(()),
    }
}

fn map_of(table: &toml::Table, key: &str) -> Result<BTreeMap<String, String>, ()> {
    match table.get(key) {
        None => Ok(BTreeMap::new()),
        Some(toml::Value::Table(map)) => map
            .iter()
            .map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())).ok_or(()))
            .collect(),
        Some(_) => Err(()),
    }
}

/// Read one table into a record, then validate it.
fn from_toml(table: &toml::Table) -> Result<McpServer, McpConfigError> {
    let id = str_of(table, "id").ok_or(McpConfigError::InvalidId)?;
    let name = str_of(table, "name").ok_or(McpConfigError::InvalidName)?;
    // An absent `enabled` is true; a value that is not a boolean is false, so
    // a server the author meant to turn off does not start.
    let enabled = table.get("enabled").map(|v| v.as_bool().unwrap_or(false)).unwrap_or(true);
    let disabled_tools = strings_of(table, "disabledTools").unwrap_or_default();
    let transport = match str_of(table, "transport").as_deref() {
        Some("stdio") => McpTransport::Stdio(StdioTransport {
            command: str_of(table, "command").ok_or(McpConfigError::InvalidCommand)?,
            args: strings_of(table, "args").map_err(|_| McpConfigError::InvalidCommand)?,
            cwd: str_of(table, "cwd"),
            env: map_of(table, "env").map_err(|_| McpConfigError::InvalidEnvName)?,
            secret_env: strings_of(table, "secretEnv")
                .map_err(|_| McpConfigError::InvalidEnvName)?,
        }),
        Some("http") => McpTransport::Http(HttpTransport {
            url: str_of(table, "url").ok_or(McpConfigError::InvalidUrl)?,
            headers: map_of(table, "headers").map_err(|_| McpConfigError::InvalidHeaderName)?,
            secret_headers: strings_of(table, "secretHeaders")
                .map_err(|_| McpConfigError::InvalidHeaderName)?,
            auth: match str_of(table, "auth") {
                None => McpAuth::None,
                Some(text) => McpAuth::parse(&text).ok_or(McpConfigError::InvalidAuth)?,
            },
            client_id: str_of(table, "clientId"),
            scopes: strings_of(table, "scopes").unwrap_or_default(),
            callback_port: match table.get("callbackPort") {
                None => None,
                Some(value) => Some(
                    value
                        .as_integer()
                        .and_then(|port| u16::try_from(port).ok())
                        .ok_or(McpConfigError::InvalidCallbackPort)?,
                ),
            },
        }),
        _ => return Err(McpConfigError::InvalidTransport),
    };
    validate(McpServer { id, name, enabled, disabled_tools, transport })
}

fn strings_value(items: &[String]) -> toml::Value {
    toml::Value::Array(items.iter().cloned().map(toml::Value::String).collect())
}

fn map_value(map: &BTreeMap<String, String>) -> toml::Value {
    toml::Value::Table(
        map.iter().map(|(k, v)| (k.clone(), toml::Value::String(v.clone()))).collect(),
    )
}

/// One record as the file holds it. Empty optional values are left out.
fn to_toml(server: &McpServer) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("id".into(), toml::Value::String(server.id.clone()));
    table.insert("name".into(), toml::Value::String(server.name.clone()));
    table.insert("enabled".into(), toml::Value::Boolean(server.enabled));
    table.insert("transport".into(), toml::Value::String(server.transport_name().into()));
    match &server.transport {
        McpTransport::Stdio(stdio) => {
            table.insert("command".into(), toml::Value::String(stdio.command.clone()));
            table.insert("args".into(), strings_value(&stdio.args));
            if let Some(cwd) = &stdio.cwd {
                table.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !stdio.secret_env.is_empty() {
                table.insert("secretEnv".into(), strings_value(&stdio.secret_env));
            }
            if !stdio.env.is_empty() {
                table.insert("env".into(), map_value(&stdio.env));
            }
        }
        McpTransport::Http(http) => {
            table.insert("url".into(), toml::Value::String(http.url.clone()));
            table.insert("auth".into(), toml::Value::String(http.auth.as_str().into()));
            if let Some(client_id) = &http.client_id {
                table.insert("clientId".into(), toml::Value::String(client_id.clone()));
            }
            if !http.scopes.is_empty() {
                table.insert("scopes".into(), strings_value(&http.scopes));
            }
            if let Some(port) = http.callback_port {
                table.insert("callbackPort".into(), toml::Value::Integer(i64::from(port)));
            }
            if !http.secret_headers.is_empty() {
                table.insert("secretHeaders".into(), strings_value(&http.secret_headers));
            }
            if !http.headers.is_empty() {
                table.insert("headers".into(), map_value(&http.headers));
            }
        }
    }
    if !server.disabled_tools.is_empty() {
        table.insert("disabledTools".into(), strings_value(&server.disabled_tools));
    }
    table
}

/// The tables of the array, or none when the key is absent or not an array.
fn tables_of(public: &toml::Table) -> Vec<toml::Table> {
    match public.get(MCP_SERVERS_KEY) {
        Some(toml::Value::Array(items)) => {
            items.iter().map(|item| item.as_table().cloned().unwrap_or_default()).collect()
        }
        _ => Vec::new(),
    }
}

/// MCS-FR-FIEE, MCS-FR-HBOD: every record in file order. A later record with
/// the id of an earlier one is `duplicate_id`.
pub fn list_records(root: &crate::fs::RootFs) -> Result<Vec<ListedRecord>, String> {
    let public = load_public(root)?;
    Ok(records_of(&public))
}

pub fn records_of(public: &toml::Table) -> Vec<ListedRecord> {
    let mut seen = std::collections::HashSet::new();
    tables_of(public)
        .iter()
        .map(|table| {
            let id = str_of(table, "id").unwrap_or_default();
            let record = if !id.is_empty() && !seen.insert(id.clone()) {
                Err(McpConfigError::DuplicateId)
            } else {
                from_toml(table)
            };
            ListedRecord { id, record }
        })
        .collect()
}

/// MCS-FR-XCOS, MCS-FR-JZDF: replace the record with the same id, or add the
/// record at the end. Every other key of the file is kept.
pub fn save_record(root: &crate::fs::RootFs, server: &McpServer) -> Result<(), String> {
    let mut public = load_public(root)?;
    let mut tables = tables_of(&public);
    let incoming = to_toml(server);
    match tables.iter().position(|t| str_of(t, "id").as_deref() == Some(server.id.as_str())) {
        Some(index) => tables[index] = incoming,
        None => tables.push(incoming),
    }
    write_tables(&mut public, tables);
    save_public(root, &public)
}

/// MCS-FR-UOIO, MCS-FR-JZDF: remove every record with this id. `Ok(false)` when
/// there is none.
pub fn remove_record(root: &crate::fs::RootFs, id: &str) -> Result<bool, String> {
    let mut public = load_public(root)?;
    let mut tables = tables_of(&public);
    let before = tables.len();
    tables.retain(|t| str_of(t, "id").as_deref() != Some(id));
    if tables.len() == before {
        return Ok(false);
    }
    write_tables(&mut public, tables);
    save_public(root, &public)?;
    Ok(true)
}

fn write_tables(public: &mut toml::Table, tables: Vec<toml::Table>) {
    if tables.is_empty() {
        public.remove(MCP_SERVERS_KEY);
    } else {
        public.insert(
            MCP_SERVERS_KEY.to_string(),
            toml::Value::Array(tables.into_iter().map(toml::Value::Table).collect()),
        );
    }
}
