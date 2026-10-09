//! Agentic integrations — `specifications/core/AIC-agentic-integrations.md`.
//!
//! The backend home for the agent backends Synthesis may drive. An agentic
//! integration comes in one of two kinds: a **CLI** launched on this machine
//! (Claude Code, Codex, OpenCode) or an **API** agent-execution endpoint reached
//! over HTTP (Claude Agent API, Custom agent API). Whichever kind, this module
//! locates or reaches it, verifies it by asking what it is, learns which models
//! it offers, records the model and reasoning effort the author chose, and
//! resolves which integration applies to the open project.
//!
//! Four properties shape the module:
//!
//! - **Verification is what commits a configuration** (AIC-FR-06), for both
//!   kinds. The fields in the UI hold candidates; nothing reaches the registry
//!   until the backend has answered. A failed verification therefore persists
//!   *nothing*, so an integration that was working is never degraded by an
//!   attempt against a different path, URL, or key.
//! - **A record carries the fields of its own kind and no others** (AIC-FR-25).
//!   That is what lets one tab strip render both kinds side by side without the
//!   UI having to guess which fields are meaningful.
//! - **Every secret here lives in the keychain** (AIC-FR-20) — an API-kind
//!   vendor's key and Claude Code's OAuth token, one entry per vendor. Each is
//!   written by a successful verification and never crosses the IPC boundary
//!   except as `masked_hint`. Codex and OpenCode carry no credential at all,
//!   each authenticating itself through a mechanism this module neither performs
//!   nor observes. Claude Code is the exception because it is expected to run in
//!   an isolated container with no session of its own, and its token is written
//!   *after* the path verification succeeds rather than before (AIC-FR-28) — a
//!   token is worth storing only for an installation that has proved to exist.
//! - **This module never runs an agent.** The only child process it spawns is a
//!   version probe, under a bounded timeout, with no argument derived from the
//!   open project (AIC-FR-04). `resolve_agentic_invocation` hands out the facts
//!   an invocation needs and is deliberately not a `#[tauri::command]`, so no
//!   frontend call can reach it (AIC-FR-19).
//!
//! All three outside dependencies — the filesystem, the child process, and the
//! network — are behind traits (`FileProbe`, `CliRunner`, `EndpointProber`) so
//! every rule below is unit-testable without a CLI installed or a network. CI
//! has neither, and a test that needed one would either fail there or be deleted
//! until it passed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ai_shared::{
    mask_hint, normalize_base_url, redacted, AuthStyle, BaseUrlError, EffortOption, EndpointProber,
    HttpEndpointProber, KeyPresence, ModelOption, ModelsOrigin, ProbeError, ProbeRequest,
    SecretStore,
};
use crate::global_settings::{now_iso8601, GlobalSettingsStore};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};
use crate::project::ProjectState;

// ---------------------------------------------------------------------------
// Typed errors
// ---------------------------------------------------------------------------
//
// The codebase's error channel is `Result<_, String>`, so a "typed" error is a
// stable string the frontend matches on. Constants rather than inline literals
// because both sides depend on the exact spelling: `src/types.ts` mirrors this
// list, and a silent divergence would leave the UI unable to tell a wrong path
// from a permissions problem.

/// Verify was asked to check an empty path (AIC-FR-05).
pub const ERR_PATH_EMPTY: &str = "path_empty";
/// Nothing exists at the path (AIC-FR-05).
pub const ERR_NOT_FOUND: &str = "not_found";
/// Something exists but cannot be executed — a permissions problem, distinct
/// from a wrong path (AIC-FR-05).
pub const ERR_NOT_EXECUTABLE: &str = "not_executable";
/// It ran, but what answered is not the vendor being verified (AIC-FR-05).
pub const ERR_NOT_THE_EXPECTED_CLI: &str = "not_the_expected_cli";
/// It could not be run at all.
pub const ERR_EXECUTION_FAILED: &str = "execution_failed";
/// A CLI ran, or an endpoint was called, past the bounded timeout
/// (AIC-FR-04 / AIC-FR-22).
pub const ERR_TIMED_OUT: &str = "timed_out";
/// Verify was asked to check an empty base URL (AIC-FR-22).
pub const ERR_BASE_URL_EMPTY: &str = "base_url_empty";
/// The base URL is not a well-formed absolute URL (AIC-FR-22).
pub const ERR_BASE_URL_INVALID: &str = "base_url_invalid";
/// No key was supplied where the vendor requires one (AIC-FR-22).
pub const ERR_KEY_MISSING: &str = "key_missing";
/// The host could not be reached (AIC-FR-22).
pub const ERR_UNREACHABLE: &str = "unreachable";
/// The TLS check refused the host's certificate. The error text is
/// `tls_untrusted:<cause>:<host>` (AIC-FR-22, AAP-FR-HZTB).
pub const ERR_TLS_UNTRUSTED: &str = "tls_untrusted";
/// The endpoint answered and refused the credentials (AIC-FR-22).
pub const ERR_REJECTED: &str = "rejected";
/// The URL answered, but not as an agent-execution endpoint (AIC-FR-22).
pub const ERR_NOT_AN_AGENT_ENDPOINT: &str = "not_an_agent_endpoint";
/// A CLI-shaped config was supplied for an API vendor, or the reverse.
pub const ERR_WRONG_CONFIG_KIND: &str = "wrong_config_kind";
/// Detection was asked for a vendor that has no binary to find (AIC-FR-03).
pub const ERR_NOT_A_CLI_INTEGRATION: &str = "not_a_cli_integration";
/// A `{ path }` payload for a vendor that holds an OAuth token, with no token
/// already stored (AIC-FR-26).
pub const ERR_TOKEN_MISSING: &str = "token_missing";
/// A supplied OAuth token that is not shaped like one (AIC-FR-27).
pub const ERR_TOKEN_MALFORMED: &str = "token_malformed";
/// The OS credential store would not answer (AIC-FR-24).
pub const ERR_KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
/// A model id absent from the record's current list (AIC-FR-10).
pub const ERR_UNKNOWN_MODEL: &str = "unknown_model";
/// An effort id absent from the vendor's declared levels (AIC-FR-10).
pub const ERR_UNKNOWN_EFFORT: &str = "unknown_effort";
/// AIC-FR-10: a turn kind outside the closed set. Never stored.
pub const ERR_UNKNOWN_TURN_KIND: &str = "unknown_turn_kind";
/// Activation, or an override, naming an integration that is not verified
/// (AIC-FR-12 / AIC-FR-17).
pub const ERR_NOT_VERIFIED: &str = "not_verified";
/// No vendor carries the requested id.
pub const ERR_UNKNOWN_VENDOR: &str = "unknown_vendor";
/// An override can only be set while a project is open.
pub const ERR_NO_PROJECT: &str = "no project is open";
/// `resolve_agentic_invocation` was asked for an integration when none is
/// configured at all (AIC-FR-19).
pub const ERR_NONE_CONFIGURED: &str = "none_configured";
/// `resolve_agentic_invocation` was asked when integrations exist but none is
/// chosen (AIC-FR-19).
pub const ERR_NONE_SELECTED: &str = "none_selected";

/// A variable name that the Docker client process or the executor owns (AIC-FR-XTEZ).
pub const ERR_ENV_VAR_RESERVED: &str = "env_var_reserved";
/// An `env_vars` entry is not `NAME=value` (AIC-FR-XTEZ).
pub const ERR_ENV_VAR_INVALID: &str = "env_var_invalid";
/// The token variable name is not usable (AIC-FR-CVPW).
pub const ERR_TOKEN_VAR_INVALID: &str = "token_var_invalid";
/// The gateway answered with a status that is not 2xx (AIC-FR-DRPC). The error
/// text is `gateway_status:<code>`.
pub const ERR_GATEWAY_STATUS: &str = "gateway_status";
/// The gateway could not be reached (AIC-FR-DRPC). The error text is
/// `gateway_unreachable:<cause>`.
pub const ERR_GATEWAY_UNREACHABLE: &str = "gateway_unreachable";
/// The gateway answered 2xx with a body that is not a model list (AIC-FR-DRPC).
pub const ERR_GATEWAY_NOT_A_MODEL_LIST: &str = "gateway_not_a_model_list";

/// AIC-FR-04: how long a version probe may take before the child is killed.
/// Generous enough for a cold start of a Node-based CLI, short enough that a
/// hung binary cannot wedge the settings surface.
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// AIC-FR-32: the service name of the **earlier** one-entry-per-vendor keyring
/// layout.
///
/// Nothing writes to it any more — every agent credential lives in the
/// application secret vault (`ASV-application-secret-vault.md` ASV-FR-01) — but
/// it is what a migration candidate names, so a machine that stored credentials
/// under the old layout has them adopted into the one entry on the next launch.
pub const LEGACY_KEYCHAIN_SERVICE: &str = "com.synthesis.agentic-integration";

/// AIC-FR-20: the vault namespace this module owns
/// (`ASV-application-secret-vault.md` ASV-FR-03). An agent credential — an
/// API-kind vendor's key or Claude Code's OAuth token — is addressed at
/// `["agentic", "vendors", <vendor_id>]` and nowhere else. Codex's own login
/// directory is outside the vault entirely (ASV-FR-32).
pub const VAULT_NAMESPACE: &[&str] = &["agentic", "vendors"];

/// The one prefix an OAuth token may carry (AIC-FR-27).
const OAUTH_TOKEN_PREFIX: &str = "sk-ant-oat01-";

/// AIC-FR-27: `^sk-ant-oat01-[A-Za-z0-9_-]+$`, and nothing else.
///
/// Spelled out rather than compiled as a regex because the pattern is anchored
/// at both ends and has one repetition group, which `strip_prefix` plus a
/// character predicate expresses exactly — and because a hand-rolled check
/// cannot be mis-anchored by a later edit the way a regex literal can.
///
/// The check is **structural and complete**: it decides whether a value is
/// shaped like a token and nothing more. Ownership, expiry, authenticity, and
/// reachability are deliberately not tested — no service is contacted and no CLI
/// is run — so a well-formed token that has been revoked passes here and fails
/// wherever it is eventually presented.
pub fn is_valid_oauth_token(token: &str) -> bool {
    match token.strip_prefix(OAUTH_TOKEN_PREFIX) {
        // `[A-Za-z0-9_-]+` — one or more, so a bare prefix does not pass.
        Some(rest) => {
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }
        None => false,
    }
}

mod catalog;
mod commands;
mod gateway;
mod launch;
mod logs;
mod operations;
mod records;
mod resolution;
mod runner;

pub use catalog::*;
pub use commands::*;
pub use gateway::*;
pub use launch::*;
use logs::*;
pub use operations::*;
pub use records::*;
pub use resolution::*;
pub use runner::*;

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

/// The module's collaborators, as Tauri-managed state. The registry itself is
/// not here — it lives in the user-global store (GSS-FR-14), which
/// `GlobalSettingsStore` owns.
pub struct AgenticIntegrations {
    probe: Box<dyn FileProbe>,
    runner: Box<dyn CliRunner>,
    prober: Box<dyn EndpointProber>,
    secrets: Box<dyn SecretStore>,
    /// Where this machine's user home is, which is what a vendor that
    /// authenticates through its own login directory is resolved against
    /// (AIC-FR-30). Held here rather than read at the call site so the location
    /// stays this module's to own — and so a test can point it at a scratch
    /// directory without touching the real one.
    home: Option<PathBuf>,
    /// Serialises read-modify-write cycles over the registry. Each mutating
    /// command reads the registry, edits it, and writes it back; without this
    /// two concurrent `invoke`s could interleave and one would silently discard
    /// the other's record.
    write_lock: Mutex<()>,
}

impl Default for AgenticIntegrations {
    fn default() -> Self {
        Self::new(
            Box::new(RealFileProbe),
            Box::new(RealCliRunner),
            Box::new(HttpEndpointProber),
            Box::new(crate::secret_vault::VaultSecrets::new(
                crate::secret_vault::global(),
                VAULT_NAMESPACE,
            )),
        )
    }
}

impl AgenticIntegrations {
    pub fn new(
        probe: Box<dyn FileProbe>,
        runner: Box<dyn CliRunner>,
        prober: Box<dyn EndpointProber>,
        secrets: Box<dyn SecretStore>,
    ) -> Self {
        Self {
            probe,
            runner,
            prober,
            secrets,
            home: dirs::home_dir(),
            write_lock: Mutex::new(()),
        }
    }

    /// The same collaborators with the user home pointed somewhere else, so a
    /// test can exercise Codex's login-directory resolution (AIC-FR-30) against
    /// a scratch directory rather than against the machine's real one.
    pub fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.home = Some(home.into());
        self
    }
}


#[cfg(test)]
mod tests;
