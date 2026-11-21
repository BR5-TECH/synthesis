//! The payload shapes of the terminal session contract (`TRS-terminal-sessions.md`).

use serde::{Deserialize, Serialize};

/// TRS-FR-BGDL: what a session is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRef {
    Worktree {
        path: String,
    },
    WorkStream {
        #[serde(rename = "streamId", alias = "stream_id")]
        stream_id: String,
    },
}

impl TargetRef {
    /// TRS-FR-BGDL: the queue scope key, the same key as EAC's sandbox scope.
    pub fn queue_scope(&self) -> String {
        match self {
            TargetRef::Worktree { path } => format!("worktree:{path}"),
            TargetRef::WorkStream { stream_id } => stream_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    HostShell,
    ContainerShell,
    MicrosandboxShell,
    AgentCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliVendor {
    ClaudeCode,
    Codex,
    Opencode,
}

impl CliVendor {
    pub const ALL: [CliVendor; 3] = [CliVendor::ClaudeCode, CliVendor::Codex, CliVendor::Opencode];

    pub fn id(self) -> &'static str {
        match self {
            CliVendor::ClaudeCode => "claude_code",
            CliVendor::Codex => "codex",
            CliVendor::Opencode => "opencode",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|vendor| vendor.id() == id)
    }

    /// TRS-FR-LRYH: the display name a label starts with.
    pub fn display_name(self) -> &'static str {
        match self {
            CliVendor::ClaudeCode => "Claude Code",
            CliVendor::Codex => "Codex",
            CliVendor::Opencode => "OpenCode",
        }
    }

    /// EAC-FR-HTEB: the executable an interactive session runs.
    pub fn executable(self) -> &'static str {
        match self {
            CliVendor::ClaudeCode => "claude",
            CliVendor::Codex => "codex",
            CliVendor::Opencode => "opencode",
        }
    }
}

/// Where a session's process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunsIn {
    Host,
    Docker,
    Devcontainer,
    Microsandbox,
}

/// TRS-FR-XXPE: why a kind is not available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    DockerUnverified,
    VendorImageUnconfigured,
    DevcontainerNotReady,
    DevcontainerProject,
    IsolationNotMicrosandbox,
    NoVmRunning,
    VendorUnverified,
}

impl UnavailableReason {
    pub fn code(self) -> &'static str {
        match self {
            UnavailableReason::DockerUnverified => "docker_unverified",
            UnavailableReason::VendorImageUnconfigured => "vendor_image_unconfigured",
            UnavailableReason::DevcontainerNotReady => "devcontainer_not_ready",
            UnavailableReason::DevcontainerProject => "devcontainer_project",
            UnavailableReason::IsolationNotMicrosandbox => "isolation_not_microsandbox",
            UnavailableReason::NoVmRunning => "no_vm_running",
            UnavailableReason::VendorUnverified => "vendor_unverified",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindAvailability {
    pub kind: TerminalKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<CliVendor>,
    pub runs_in: RunsIn,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<UnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTarget {
    pub target: TargetRef,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_active_worktree: bool,
    pub busy_run_id: Option<String>,
    pub kinds: Vec<KindAvailability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Starting,
    Running,
    Exited,
    TargetGone,
}

impl TerminalState {
    /// Whether the process may still run.
    pub fn is_live(self) -> bool {
        matches!(self, TerminalState::Starting | TerminalState::Running)
    }

    pub fn code(self) -> &'static str {
        match self {
            TerminalState::Starting => "starting",
            TerminalState::Running => "running",
            TerminalState::Exited => "exited",
            TerminalState::TargetGone => "target_gone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    ProcessExited,
    StartFailed,
    VmStopped,
    TargetGone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSummary {
    pub id: String,
    pub label: String,
    pub target: TargetRef,
    pub target_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub kind: TerminalKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<CliVendor>,
    pub runs_in: RunsIn,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
    pub state: TerminalState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<EndReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_detail: Option<String>,
    pub pinned: bool,
    pub busy_run_id: Option<String>,
    pub has_history: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachment {
    pub state: TerminalState,
    pub history_bytes: usize,
}

/// The typed errors of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalError {
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<UnavailableReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TerminalError {
    pub fn new(code: &'static str) -> Self {
        Self { code, reason: None, message: None }
    }

    pub fn with_message(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, reason: None, message: Some(message.into()) }
    }

    pub fn kind_unavailable(reason: UnavailableReason) -> Self {
        Self { code: "kind_unavailable", reason: Some(reason), message: None }
    }
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.reason, &self.message) {
            (Some(reason), _) => write!(f, "{}: {}", self.code, reason.code()),
            (None, Some(message)) => write!(f, "{}: {message}", self.code),
            (None, None) => f.write_str(self.code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-BGDL: the queue scope matches EAC's sandbox scope key.
    #[test]
    fn queue_scope_of_a_worktree_and_of_a_stream() {
        let worktree = TargetRef::Worktree { path: "/repo/wt".into() };
        let stream = TargetRef::WorkStream { stream_id: "s-1".into() };
        assert_eq!(worktree.queue_scope(), "worktree:/repo/wt");
        assert_eq!(stream.queue_scope(), "s-1");
    }

    // TRS-FR-BGDL: a target is read from the frontend's shape.
    #[test]
    fn target_ref_reads_the_frontend_shape() {
        let stream: TargetRef =
            serde_json::from_str(r#"{"kind":"work_stream","streamId":"s-9"}"#).unwrap();
        assert_eq!(stream, TargetRef::WorkStream { stream_id: "s-9".into() });
        let worktree: TargetRef =
            serde_json::from_str(r#"{"kind":"worktree","path":"/a"}"#).unwrap();
        assert_eq!(worktree, TargetRef::Worktree { path: "/a".into() });
    }
}
