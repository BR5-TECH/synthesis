//! The launch credential of `../core/AIC-agentic-integrations.md` AIC-FR-30, in
//! the form a container gets it: the variables it forwards by name, the mount
//! it carries, and the session-state mount that credential leaves the vendor
//! (EAC-FR-05, EAC-FR-15, EAC-FR-16, EAC-FR-31, EAC-FR-QHIU).
//!
//! One composition for a turn and for an interactive terminal (EAC-FR-DQZP), so
//! the two cannot come to disagree about what a credential gives a container.

use std::collections::{BTreeMap, BTreeSet};

use super::descriptor::{self, BindMount, SessionStateMount, VendorExecutionDescriptor};
use crate::agentic::{self, AgentLaunchCredential, CodexAuthMode, SecretString};

/// What one credential gives one container.
#[derive(Default)]
pub struct CredentialParts {
    /// EAC-FR-15: forwarded by name; a value never reaches an argument vector.
    pub env_names: Vec<String>,
    pub env: BTreeMap<String, SecretString>,
    /// AIC-FR-ISOC / EAC-FR-QHIU: the variables whose values a record may not
    /// carry.
    pub masked_env: BTreeSet<String>,
    pub mounts: Vec<BindMount>,
    /// Container paths this application chose, never a secret.
    pub env_literals: Vec<(String, String)>,
    /// EAC-FR-29: a credential's location, which a record may not carry either:
    /// the host spelling and the container spelling of a credential mount.
    pub masked_paths: Vec<String>,
    /// EAC-FR-31 / EAC-FR-QHIU: the session-state mount this credential leaves
    /// the vendor. `None` where the credential names no executable vendor.
    pub session_state: Option<SessionStateMount>,
}

/// The descriptor of the vendor a credential belongs to.
fn descriptor_of(credential: &AgentLaunchCredential) -> Option<&'static VendorExecutionDescriptor> {
    match credential {
        AgentLaunchCredential::ClaudeEnvironment(_) => descriptor::descriptor_for("claude_code"),
        AgentLaunchCredential::CodexConfigMount { .. }
        | AgentLaunchCredential::CodexEnvironment(_) => descriptor::descriptor_for("codex"),
    }
}

/// EAC-FR-15 / EAC-FR-16 / EAC-FR-QHIU: `credential` in the form a container
/// gets it. The only refusal is a Codex login directory that cannot be a mount
/// source, which is `codex_config_missing`.
pub fn credential_parts(credential: AgentLaunchCredential) -> Result<CredentialParts, String> {
    let credential_is_mount = matches!(credential, AgentLaunchCredential::CodexConfigMount { .. });
    let session_state = descriptor_of(&credential)
        .map(|descriptor| descriptor.session_state.for_credential(credential_is_mount));
    let mut parts = CredentialParts { session_state, ..Default::default() };

    match credential {
        AgentLaunchCredential::ClaudeEnvironment(variables)
        | AgentLaunchCredential::CodexEnvironment(variables) => {
            // EAC-FR-15: the *name* goes in the argv, the value goes in the
            // client process's environment, and the runtime forwards it.
            for variable in variables {
                if variable.masked {
                    parts.masked_env.insert(variable.name.clone());
                }
                parts.env_names.push(variable.name.clone());
                parts.env.insert(variable.name, variable.value);
            }
        }
        AgentLaunchCredential::CodexConfigMount { source, target } => {
            let source = descriptor::mount_source(&source)
                .ok_or_else(|| agentic::ERR_CODEX_CONFIG_MISSING.to_string())?;
            let target = target.to_string_lossy().into_owned();
            // EAC-FR-16 / CDX-FR-23: read/write, because this vendor writes its
            // session rollout files beneath the same directory it reads its
            // login from. Nothing here reads, copies, or logs a byte of it.
            // Both spellings are masked: the CLI names its own directory by the
            // container path, and a Docker diagnostic names it by the host path.
            parts.masked_paths.push(source.clone());
            parts.masked_paths.push(target.clone());
            parts.mounts.push(BindMount { source, target: target.clone(), read_only: false });
            // CDX-FR-24: the same directory, named to the CLI.
            if let Some(SessionStateMount::SharedWithCredentialMount { env }) = parts.session_state
            {
                parts.env_literals.push((env.to_string(), target));
            }
        }
    }
    Ok(parts)
}

/// EAC-FR-IGXZ: the Codex mode a recorded session ran under. A `null`
/// continuation token reads as `login_folder`, and so does a value that names
/// no mode, which only a build before the modes existed could have recorded.
pub fn recorded_codex_mode(continuation_token: Option<&str>) -> CodexAuthMode {
    match continuation_token {
        Some(token) if token == CodexAuthMode::AccessToken.as_str() => CodexAuthMode::AccessToken,
        _ => CodexAuthMode::LoginFolder,
    }
}
