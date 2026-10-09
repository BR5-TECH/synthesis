//! The executor-only launch handoff (`AIC-agentic-integration-config.md`
//! AIC-FR-30, AIC-FR-31).

use super::*;

// ---------------------------------------------------------------------------
// The executor-only launch handoff (AIC-FR-30, AIC-FR-31)
// ---------------------------------------------------------------------------
//
// AIC-FR-29 used to say this module presents Claude Code's token to nothing at
// all. It now has exactly one destination: the agent-CLI executor
// (`../tools/EAC-execute-agent-cli.md`), which needs the token to authenticate
// the agent it launches. Everything about the shape below exists to keep that
// one destination from becoming two.
//
// - The type carries no `Serialize`, no `Deserialize`, and no derived `Debug`,
//   so it cannot cross the IPC boundary, land in a log field, or be rendered
//   into an error by accident (AIC-FR-31).
// - It is returned by no `#[tauri::command]` and by no function a loop caller
//   holds; the executor asks for it at the moment of a launch and drops it when
//   the container is gone.
// - It answers *only* with launch material. Which integration is selected is
//   `resolve_agentic_invocation`'s answer and deliberately not this one's, so a
//   caller cannot use the credential path as a second way to resolve a vendor.
// - `binary_path` is never part of it: a stored path is a verification and
//   detection datum (AIC-FR-05, AIC-FR-07), and the executor runs a pinned
//   image rather than whatever is installed on this machine.

/// A credential held only for the length of one launch.
///
/// Wraps the string so that the obvious mistakes are compile errors rather than
/// review findings: it does not implement `Serialize`, `Display`, or a `Debug`
/// that renders what it holds, and it overwrites its bytes on drop.
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        SecretString(value)
    }

    /// The one way to read the secret. Named to make a call site conspicuous in
    /// a diff — nothing should reach for this except the code that is about to
    /// present the credential to the thing it belongs to.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Not the value, and not its length either: a length is a fact about a
        // credential that a record has no reason to carry.
        f.write_str("SecretString(<redacted>)")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // `zeroize` rather than a hand-rolled overwrite: it writes through
        // volatile stores the optimiser is not allowed to elide, and it clears
        // the `String`'s current allocation rather than leaving a stale copy
        // behind a reallocation.
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

/// What one container launch needs from this module, and nothing else
/// (AIC-FR-30).
///
/// Deliberately not `Serialize`/`Deserialize`/`Clone`: a credential that can be
/// copied is a credential with an untracked lifetime, and one that can be
/// serialised is one an error payload can carry.
///
/// `Debug` is hand-rolled below rather than derived, and for both variants
/// rather than only the obvious one. The token is protected by `SecretString`'s
/// own redaction, but a derived `Debug` would print `CodexConfigMount`'s paths
/// verbatim — and the source path is the machine user's home directory, which
/// names the person at the keyboard. AIC-FR-31 forbids a rendering that carries
/// either.
pub enum AgentLaunchCredential {
    /// Claude Code authenticates through environment variables on one
    /// container: its subscription token, or its gateway URL and token, and
    /// the author's own variables (AIC-FR-XZCS, EAC-FR-15).
    ClaudeEnvironment(Vec<LaunchVariable>),
    /// Codex authenticates through the login directory its own CLI wrote on
    /// this machine, mounted read-only (EAC-FR-16). Not a secret this module
    /// holds — a location it resolves and never opens.
    CodexConfigMount { source: PathBuf, target: PathBuf },
}

impl std::fmt::Debug for AgentLaunchCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Which vendor's material this is, and nothing about what it holds.
        // That is the whole of what a diagnostic has any use for.
        match self {
            Self::ClaudeEnvironment(variables) => write!(
                f,
                "AgentLaunchCredential::ClaudeEnvironment({} variables)",
                variables.len()
            ),
            Self::CodexConfigMount { .. } => {
                f.write_str("AgentLaunchCredential::CodexConfigMount(<redacted>)")
            }
        }
    }
}

/// The vendor is not one this application can execute in a container:
/// `opencode`, or either API-kind vendor (AIC-FR-30).
pub const ERR_NOT_AN_EXECUTABLE_CLI: &str = "not_an_executable_cli";
/// The registry could not be read, so the authentication mode is unknown
/// (AIC-FR-30).
pub const ERR_REGISTRY_UNAVAILABLE: &str = "registry_unavailable";
/// Codex resolves, but the login directory its CLI writes is not on this
/// machine — the author has not signed in to Codex here (AIC-FR-30).
pub const ERR_CODEX_CONFIG_MISSING: &str = "codex_config_missing";

/// AIC-FR-30: the executor-only handoff.
///
/// Not a `#[tauri::command]`, not registered in `generate_handler!`, and not
/// reachable from the frontend. Its only caller is the agent-CLI executor.
pub fn resolve_agent_launch_credential(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
) -> Result<AgentLaunchCredential, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_NOT_AN_EXECUTABLE_CLI)?;

    // The gate is "can this application execute it", not "is it a CLI":
    // OpenCode is CLI-kind and still refused, because no execution protocol is
    // pinned for it (EAC-FR-04).
    if descriptor.kind != VendorKind::Cli || !descriptor.container_executable {
        return Err(ERR_NOT_AN_EXECUTABLE_CLI.into());
    }

    if descriptor.requires_oauth_token() {
        // AIC-FR-WNQR: the stored mode decides which credential this launch
        // reads. A registry that cannot be read is a refusal, never a quiet
        // fall back to the other mode's credential.
        let (records, _) = store
            .load_agentic_registry()
            .map_err(|_| ERR_REGISTRY_UNAVAILABLE.to_string())?;
        let record = records
            .into_iter()
            .find(|r| r.vendor == vendor)
            .unwrap_or_else(|| AgenticRecord::empty(vendor));
        let secret_id = match record.auth_mode {
            AuthMode::Subscription => vendor,
            AuthMode::CustomGateway => GATEWAY_SECRET_ID,
        };
        let token = ai
            .secrets
            .get(secret_id)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            .ok_or(ERR_TOKEN_MISSING)?;
        let credential = SecretString::new(token);
        return Ok(AgentLaunchCredential::ClaudeEnvironment(
            compose_launch_environment(&record, &credential)?,
        ));
    }

    let mount = descriptor
        .config_mount
        .ok_or(ERR_NOT_AN_EXECUTABLE_CLI)?;
    // The location is this module's to own (AIC-FR-30): no caller supplies it,
    // and no caller can substitute one.
    let home = ai.home.as_deref().ok_or(ERR_CODEX_CONFIG_MISSING)?;
    let source = home.join(mount.host_home_relative);
    if !ai.probe.dir_exists(&source) {
        // The path is not in the error: a refusal describes what is wrong, and
        // a home-directory path names the machine's user (AIC-FR-31).
        return Err(ERR_CODEX_CONFIG_MISSING.into());
    }
    Ok(AgentLaunchCredential::CodexConfigMount {
        source,
        target: PathBuf::from(mount.container_target),
    })
}

