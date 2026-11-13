//! The status of an MCP server and the shapes `list_mcp_servers` returns
//! (MCS-FR-RXZG, MCS-FR-JNXV, MCS contract surface).

use serde::Serialize;

use super::config::{McpAuth, McpConfigError, McpServer};

/// MCS-FR-JNXV: why a connection failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpFailure {
    CommandNotFound,
    ProcessExited,
    Unreachable,
    TlsUntrusted,
    Rejected,
    ProtocolError,
    TimedOut,
    OauthFailed,
    KeychainUnavailable,
}

impl McpFailure {
    pub fn code(self) -> &'static str {
        match self {
            McpFailure::CommandNotFound => "command_not_found",
            McpFailure::ProcessExited => "process_exited",
            McpFailure::Unreachable => "unreachable",
            McpFailure::TlsUntrusted => "tls_untrusted",
            McpFailure::Rejected => "rejected",
            McpFailure::ProtocolError => "protocol_error",
            McpFailure::TimedOut => "timed_out",
            McpFailure::OauthFailed => "oauth_failed",
            McpFailure::KeychainUnavailable => "keychain_unavailable",
        }
    }
}

/// MCS-FR-RXZG: the status of one server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum McpServerStatus {
    Disabled,
    Invalid,
    Untrusted,
    NeedsSecret,
    NeedsSignIn,
    SigningIn,
    Connecting,
    Ready { tool_count: usize },
    Failed { failure: McpFailure },
}

impl McpServerStatus {
    pub fn code(&self) -> &'static str {
        match self {
            McpServerStatus::Disabled => "disabled",
            McpServerStatus::Invalid => "invalid",
            McpServerStatus::Untrusted => "untrusted",
            McpServerStatus::NeedsSecret => "needs_secret",
            McpServerStatus::NeedsSignIn => "needs_sign_in",
            McpServerStatus::SigningIn => "signing_in",
            McpServerStatus::Connecting => "connecting",
            McpServerStatus::Ready { .. } => "ready",
            McpServerStatus::Failed { .. } => "failed",
        }
    }
}

/// What this machine knows about a record before it connects it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineFacts {
    /// MCS-FR-HQJQ: the trusted fingerprint equals the record's now.
    pub trusted: bool,
    /// MCS-FR-PXRJ: every required slot holds a value.
    pub secrets_present: bool,
    /// MCS-FR-MGCJ: the `oauth` slot holds an access or a refresh token.
    pub signed_in: bool,
    /// MCS-FR-JNXV: the vault could not be read, so the other facts are unknown.
    pub vault_unavailable: bool,
}

/// MCS-FR-UXPP: whether a record may connect now. `Err` is the status the
/// server takes instead. The order of the checks decides which status a server
/// with several problems shows: the first one the author can act on.
pub fn gate(record: &McpServer, facts: &MachineFacts) -> Result<(), McpServerStatus> {
    if !record.enabled {
        return Err(McpServerStatus::Disabled);
    }
    // MCS-FR-HQJQ: a record that needs trust and has none.
    if crate::mcp::config::trust_fingerprint(record).is_some() && !facts.trusted {
        return Err(McpServerStatus::Untrusted);
    }
    if facts.vault_unavailable {
        return Err(McpServerStatus::Failed { failure: McpFailure::KeychainUnavailable });
    }
    if !facts.secrets_present {
        return Err(McpServerStatus::NeedsSecret);
    }
    if record.auth() == McpAuth::Oauth2 && !facts.signed_in {
        return Err(McpServerStatus::NeedsSignIn);
    }
    Ok(())
}

/// The presence of one settable slot (MCS-FR-FIEE).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SlotPresence {
    pub slot: String,
    pub present: bool,
}

/// One tool of a server, as the settings section shows it (MCS-FR-FIEE).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub disabled: bool,
}

/// MCS-FR-FIEE: one row of `list_mcp_servers`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct McpServerEntry {
    pub id: String,
    pub record: Option<McpServer>,
    pub invalid: Option<McpConfigError>,
    pub status: McpServerStatus,
    pub secrets: Vec<SlotPresence>,
    pub tools: Vec<McpToolInfo>,
}

/// MCS-FR-ETIS, MCS-FR-ERQP: the result of `test_mcp_server`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum McpTestResult {
    Passed {
        ok: bool,
        tools: Vec<McpToolInfo>,
    },
    Refused {
        ok: bool,
        failure: String,
        #[serde(rename = "stderrTail")]
        stderr_tail: Vec<String>,
    },
}

impl McpTestResult {
    pub fn passed(tools: Vec<McpToolInfo>) -> Self {
        McpTestResult::Passed { ok: true, tools }
    }

    pub fn refused(failure: &str, stderr_tail: Vec<String>) -> Self {
        McpTestResult::Refused { ok: false, failure: failure.to_string(), stderr_tail }
    }
}

/// MCS-FR-NOKX: the payload of `mcp-server-status-changed`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusChanged {
    pub id: String,
    pub status: McpServerStatus,
}

/// MCS-FR-NOKX: the name of the event.
pub const STATUS_EVENT: &str = "mcp-server-status-changed";
