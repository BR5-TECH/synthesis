//! Execute agent CLI (`EAC-execute-agent-cli.md`).
//!
//! The execution boundary a loop crosses to make an agentic CLI do one turn of
//! work. Handed an existing directory and a structured task, it resolves the
//! project's own CLI-kind agentic integration, launches that vendor's pinned
//! container with the directory mounted as its working tree, hands the task to
//! the agent on stdin, and returns one normalized result.
//!
//! ## The one loop-facing tool
//!
//! Every other member of this group is a `rig::tool::PortableTool` a *model*
//! calls. This one is reached from loop code through a typed Rust API
//! (TLC-FR-20): it has no `NAME`, no `description()`, no `parameters()`, and
//! appears in no `ToolDefinition` any provider receives. That is deliberate
//! rather than incidental — launching another agent in a container is too
//! consequential to be selected by a model's judgement, and there is no path by
//! which a model can reach it.
//!
//! It is also the one tool that carries a credential (TLC-FR-18), which is why
//! the credential enters through an internal handoff rather than a caller's
//! argument, reaches exactly one container, and appears in no argument vector,
//! no log record, no error payload, and no returned value.
//!
//! ## What a valid response is, and is not
//!
//! An `AgentResponseEnvelope` is the agent's own report of its turn. It is not
//! proof that a file changed, that a suite passed, or that the work is complete
//! (EAC-FR-27). This module reads, writes, diffs, validates, and commits no
//! project file; its only effect on the execution directory is whatever the
//! invoked CLI did inside the mount. Inspecting that directory afterwards is
//! the caller's job, and nothing here does it on their behalf.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agentic::{
    self, AgenticIntegrations, AgenticInvocation, AgentLaunchCredential, SecretString,
};
use crate::fs::{EntryKind, FsAccess};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};

pub mod descriptor;
pub mod finish;
pub mod protocol;
pub mod runtime;

// The parts this file was split into. They hold no surface of their own: every
// item they carry is named through this module, exactly as before.
mod launch;
mod observer;
mod repository;

use observer::{bounded_payload, directory_problem, merge_session, LaunchObserver};
pub(crate) use repository::{repository_access, RepositoryAccess};
use repository::{
    repository_metadata_paths, validate_supplementary_mount, RepositoryMask,
    GIT_COMMON_DIR_TARGET, REPOSITORY_COMMONDIR_FILENAME, REPOSITORY_MASK_DIRNAME,
    REPOSITORY_MASK_FILENAME, REPOSITORY_POINTER_FILENAME,
};

#[cfg(test)]
use repository::repository_metadata_paths_within;

use descriptor::{BindMount, CLAUDE_TOKEN_ENV, WORKSPACE_TARGET};
use protocol::{AgentResponseEnvelope, AgentTaskRequest, EnvelopeInvalid, LimitName, TaskInvalid};
use runtime::{
    CancellationToken, CapturedStream, DockerRuntime, HostDockerCli, RunEnd, RunRequest,
    RuntimeError,
};

/// EAC-FR-31: the subdirectory of `app_data_dir()` agent session state lives
/// under. A dedicated directory rather than `app_data_dir()` itself, which
/// EAC-FR-13 forbids mounting.
const SESSION_STATE_DIRNAME: &str = "agent-sessions";

// ---------------------------------------------------------------------------
// The caller-visible surface (EAC contract surface)
// ---------------------------------------------------------------------------

/// One thing an agent CLI did, as it did it (EAC-FR-32).
///
/// Every field is safe to show and safe to keep: the text has been through
/// [`protocol::mask_secrets`] already, so nothing downstream has to mask
/// anything and nothing downstream is trusted to (EAC-FR-29).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivityEvent {
    /// RFC 3339 UTC, taken when the line arrived rather than when it is read.
    pub at: String,
    /// Which stream it came from, or the executor itself for the two events it
    /// writes about its own invocation.
    pub channel: &'static str,
    pub kind: &'static str,
    /// One line for a list.
    pub summary: String,
    /// The whole event, verbatim and masked — what the summary was read from,
    /// so a reader is never limited to this build's reading of it.
    pub payload: String,
    /// Whether `payload` is a prefix of what arrived.
    pub payload_truncated: bool,
}

/// Where a caller's live view of a run goes (EAC-FR-32).
///
/// The executor holds no store of its own: it knows what happened, and the
/// caller knows which piece of work it belongs to. A caller that supplies none
/// gets exactly the run it would have got before this existed.
pub trait AgentActivitySink: Send + Sync {
    fn activity(&self, event: AgentActivityEvent);
}

/// One safe activity a durable sink must keep (EAC-FR-VSNM, EAC-FR-DWGS).
///
/// It has **no payload field**: the verbatim event, the serialized task, the
/// generated invocation, and every full tool argument and result stay out of
/// it by shape (EAC-FR-IRKZ). Every text in it has been through
/// [`protocol::mask_secrets`] (EAC-FR-FKCN).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableActivity {
    /// Assigned once by the executor (EAC-FR-VSNM).
    pub record_id: String,
    /// RFC 3339 UTC, taken when the activity arrived.
    pub at: String,
    pub channel: &'static str,
    /// One of the safe kinds of [`is_safe_kind`].
    pub kind: &'static str,
    /// One masked line, bounded by [`descriptor::LIMIT_SUMMARY_LINE`].
    pub summary: String,
}

/// EAC-FR-ZVRP: why a durable sink could not keep an activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableOutputFailure {
    pub code: String,
    /// One sentence. It names the act that clears the failure and carries no
    /// activity text.
    pub message: String,
}

/// Who must keep a run (EAC-FR-VSNM).
///
/// The sink answers each activity once it is durable. The executor waits for
/// the answer, which is the bounded backpressure of EAC-FR-CXUE, and it ends
/// the run's delivery at the first failure (EAC-FR-DUTR).
pub trait DurableOutputSink: Send + Sync {
    fn record(&self, activity: DurableActivity) -> Result<(), DurableOutputFailure>;
}

/// EAC-FR-DWGS / EAC-FR-IRKZ: the kinds a durable sink may receive.
///
/// `invocation`, `task`, `reasoning`, and `unrecognized` are excluded. They
/// carry task input, invocation contents, internal reasoning, or an unread raw
/// protocol line.
pub fn is_safe_kind(kind: &str) -> bool {
    matches!(
        kind,
        "started"
            | "message"
            | "tool_call"
            | "tool_result"
            | "command"
            | "file_change"
            | "retry"
            | "usage"
            | "finished"
            | "error"
            | "diagnostic"
    )
}

/// The executor's own channel, for the two events it writes itself.
const CHANNEL_EXECUTOR: &str = "executor";

/// EAC-FR-02: what a caller supplies, and — just as importantly — what it
/// cannot.
///
/// There is no vendor, model, reasoning-effort, image, credential, or
/// credential-location field here, and no type reachable from here has one. A
/// caller that wanted to override the selected integration would have to change
/// this struct, which is the point: the restriction is enforced by shape rather
/// than by discipline at every call site.
pub struct AgentExecutionRequest {
    /// An existing directory. Mounted read/write at the path EAC-FR-ZKMR gives
    /// it — its own, wherever a container can be given it — and used as
    /// the container's working directory.
    pub execution_directory: PathBuf,
    pub task: AgentTaskRequest,
    pub cancellation: CancellationToken,
    /// EAC-FR-32: who is watching this run, if anyone. Still not a way to
    /// influence the launch — a sink receives and cannot supply.
    pub activity: Option<Arc<dyn AgentActivitySink>>,
    /// EAC-FR-VSNM: who must keep this run, if anyone. Like the activity sink it
    /// receives and cannot supply, and it may only answer whether it stored
    /// what it was given (EAC-FR-CXUE).
    pub durable_output: Option<Arc<dyn DurableOutputSink>>,
    /// EAC-FR-40: at most one typed supplementary mount, of one kind, for one
    /// run.
    ///
    /// Not an exception to EAC-FR-02: it is a closed enumeration of one member
    /// with one fixed container path and one fixed access mode, so what a
    /// caller may ask for is *that this run reads that bundle* and never where,
    /// how, or with what rights.
    pub supplementary_mount: Option<SupplementaryMount>,
    /// EAC-FR-IRRD: which kind of turn this is.
    ///
    /// Not an exception to EAC-FR-02 either: it names the kind of work a turn
    /// does, which is a fact the caller already holds about its own run, and
    /// every consequence of it is resolved here rather than supplied with it.
    /// What it decides is stated on [`TurnKind`] itself, where the rest of
    /// EAC-FR-IRRD lives.
    pub turn_kind: TurnKind,
}

/// EAC-FR-IRRD: the closed set of turn kinds a caller may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TurnKind {
    /// A turn that does the work the prompt asks for (EAC-FR-IRRD).
    #[default]
    Work,
    /// A turn that judges what a work turn wrote.
    Review,
    /// A semantic-rebase turn, which serves a stream update. The one kind
    /// EAC-FR-41 masks.
    SemanticRebase,
    /// The turn of a merge run that reconciles the merge. It resolves the
    /// author's `work` selections and is not masked.
    MergeWork,
    /// The turn of a merge run that judges the reconciled merge. It resolves the
    /// author's `review` selections and is not masked.
    MergeReview,
}

impl TurnKind {
    /// The identifier `../core/AIC-agentic-integrations.md` holds an effort
    /// against, and the one a log record names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Review => "review",
            Self::SemanticRebase => "semantic_rebase",
            Self::MergeWork => "merge_work",
            Self::MergeReview => "merge_review",
        }
    }

    /// EAC-FR-IRRD: the identifier the author's model and effort selections are
    /// held against for this kind.
    ///
    /// A merge kind has no selection of its own: `merge_work` is a work turn and
    /// `merge_review` is a review turn, so each takes what the author chose for
    /// that kind of work.
    pub fn selection_key(self) -> &'static str {
        match self {
            Self::Work | Self::MergeWork => "work",
            Self::Review | Self::MergeReview => "review",
            Self::SemanticRebase => "semantic_rebase",
        }
    }
}

/// EAC-FR-40: the closed enumeration a caller may supply, and its only member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupplementaryMount {
    /// `GRB-graduation-rebase.md`'s bundle for one semantic-rebase turn.
    SemanticRebaseArtifact { host_path: PathBuf },
}

impl SupplementaryMount {
    fn host_path(&self) -> &Path {
        match self {
            Self::SemanticRebaseArtifact { host_path } => host_path,
        }
    }

    /// The fixed container root. Not a caller's to choose.
    pub fn container_root(&self) -> &'static str {
        match self {
            Self::SemanticRebaseArtifact { .. } => SEMANTIC_REBASE_TARGET,
        }
    }
}

/// EAC-FR-40: the fixed container path a semantic-rebase bundle is mounted at.
pub const SEMANTIC_REBASE_TARGET: &str = "/rebase";
/// EAC-FR-08: the ceiling on any one file within the bundle, checked before any
/// container exists.
///
/// The bundle **whole** carries no ceiling, and a mount is never refused for
/// its total size: it is the caller's own generated material rather than a
/// payload anybody composed, its file count is the size of a change set, and
/// the agent reads it one file at a time out of three path-keyed mirrors (per
/// `../core/GRB-graduation-rebase.md` GRB-FR-YPEX). What this executor guards is
/// what one read can cost, and that is this limit.
pub const SEMANTIC_REBASE_FILE_LIMIT: u64 = 1024 * 1024;

/// How the process itself ended (EAC-FR-22).
///
/// Kept on a separate axis from the agent's own `outcome`, so "the agent says
/// it could not do this" never becomes indistinguishable from "Docker was not
/// installed" — opposite instructions to a caller deciding whether to retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// Exited cleanly *and* answered in the protocol. The only outcome that
    /// carries a `response`.
    Completed,
    NonZeroExit,
    InvalidStructuredOutput,
    Timeout,
    Cancelled,
    Terminated,
}

impl ProcessOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            ProcessOutcome::Completed => "completed",
            ProcessOutcome::NonZeroExit => "non_zero_exit",
            ProcessOutcome::InvalidStructuredOutput => "invalid_structured_output",
            ProcessOutcome::Timeout => "timeout",
            ProcessOutcome::Cancelled => "cancelled",
            ProcessOutcome::Terminated => "terminated",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExecution {
    pub process_outcome: ProcessOutcome,
    pub exit_code: Option<i32>,
    /// Present only for [`ProcessOutcome::Completed`].
    pub response: Option<AgentResponseEnvelope>,
    pub stdout: CapturedStream,
    pub stderr: CapturedStream,
    pub duration_ms: u64,
    /// EAC-FR-30: the caller asked to resume a session and the pinned vendor
    /// cannot, so a fresh session was *not* silently started — nothing ran.
    pub resume_unavailable: bool,
    /// The session reference to carry into a later turn, where the vendor
    /// supports one (EAC-FR-21). Never a credential.
    pub session: Option<protocol::SessionRef>,
    /// EAC-FR-26: whether the container's removal was confirmed. `false` means
    /// one may still be running under a name this application will never reuse
    /// — a fact a caller can act on, and one that would otherwise be visible
    /// only to whoever ran `docker ps`.
    pub container_removed: bool,
}

/// EAC's typed pre-launch failures. Every one is returned before any container
/// is created (EAC-FR-03 through EAC-FR-08, EAC-FR-17).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentExecutionError {
    /// AIC resolved no integration for this project. Carries the same typed
    /// distinction `get_project_agentic_integration` reports.
    IntegrationUnresolved(String),
    /// API-kind, or OpenCode: no execution protocol is pinned for it.
    UnsupportedIntegration(String),
    /// The AIC handoff refused, or the keychain would not answer.
    CredentialUnavailable(String),
    ExecutionDirectoryInvalid(DirectoryProblem),
    RequestTooLarge(LimitName),
    /// The task document itself is not usable.
    TaskInvalid(TaskInvalid),
    RuntimeUnavailable,
    ImageUnavailable(String),
    LaunchFailed(String),
    /// EAC-FR-38: the open project commits no usable image for the resolved
    /// vendor. Distinct from `ImageUnavailable`, which is an image the project
    /// *did* name that Docker could not find or pull: this one is a project
    /// that named none, and it is corrected in Project settings rather than by
    /// building or pulling anything.
    VendorImageUnresolved(String),
    /// EAC-FR-31: the vendor's session-state directory could not be established.
    /// Distinct because it is a fault on this machine the author can correct,
    /// and because launching anyway would produce a turn that succeeds while
    /// losing every session it creates.
    SessionStateUnavailable(SessionStateProblem),
    /// EAC-FR-40: the supplied bundle is missing, unreadable, over a limit, or
    /// in a refused location. Returned before any container is created.
    SupplementaryMountInvalid(String),
    /// EAC-FR-41: repository metadata inside the execution directory could not
    /// be masked. There is no launch-anyway path.
    RepositoryMaskingUnavailable(String),
    /// EAC-FR-ZVRP: the durable sink could not keep an activity. Unlike every
    /// other member this one is returned **after** the container is gone: the
    /// run was cancelled for it (EAC-FR-DUTR).
    DurableOutputFailed(DurableOutputFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStateProblem {
    /// `app_data_dir()` could not be resolved, so there is nowhere to put it.
    RootUnresolved,
    /// The directory does not exist and could not be created.
    NotCreatable,
    /// Something is already at the path and it is not a directory.
    Unusable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryProblem {
    Missing,
    NotADirectory,
    Inaccessible,
    /// Not expressible as a bind-mount source — a comma would be read as a
    /// second `--mount` option, and a non-UTF-8 path cannot be passed at all.
    Unmountable,
}

impl AgentExecutionError {
    /// A short, stable token for a log field. Carries no path, no credential,
    /// and no task data (EAC-FR-29).
    pub fn kind(&self) -> &'static str {
        match self {
            AgentExecutionError::IntegrationUnresolved(_) => "integration_unresolved",
            AgentExecutionError::UnsupportedIntegration(_) => "unsupported_integration",
            AgentExecutionError::CredentialUnavailable(_) => "credential_unavailable",
            AgentExecutionError::ExecutionDirectoryInvalid(_) => "execution_directory_invalid",
            AgentExecutionError::RequestTooLarge(_) => "request_too_large",
            AgentExecutionError::TaskInvalid(_) => "task_invalid",
            AgentExecutionError::RuntimeUnavailable => "runtime_unavailable",
            AgentExecutionError::ImageUnavailable(_) => "image_unavailable",
            AgentExecutionError::LaunchFailed(_) => "launch_failed",
            AgentExecutionError::VendorImageUnresolved(_) => "vendor_image_unresolved",
            AgentExecutionError::SessionStateUnavailable(_) => "session_state_unavailable",
            AgentExecutionError::SupplementaryMountInvalid(_) => "supplementary_mount_invalid",
            AgentExecutionError::RepositoryMaskingUnavailable(_) => {
                "repository_masking_unavailable"
            }
            AgentExecutionError::DurableOutputFailed(_) => "durable_output_failed",
        }
    }
}

/// The collaborators one launch resolves against.
///
/// Borrowed rather than owned: the executor holds no per-call state and is
/// cheap to construct (TLC-FR-15), so what a launch needs arrives with the
/// launch instead of being captured at construction.
pub struct LaunchContext<'a> {
    pub store: &'a GlobalSettingsStore,
    pub integrations: &'a AgenticIntegrations,
    /// Which project's integration to resolve (per AIC-FR-16).
    pub project_key: &'a str,
    /// EAC-FR-06: the guarded handle the execution directory is validated
    /// through. Nothing here reads or writes a byte of the directory itself.
    pub fs: &'a FsAccess,
    /// EAC-FR-38: where the image a container is created from comes from — the
    /// project's own committed configuration, and nowhere else.
    pub images: &'a dyn VendorImageSource,
}

/// EAC-FR-38: the image the open project commits for a vendor.
///
/// A trait rather than a direct call into `PSS-project-settings-storage.md` so
/// that the executor states the dependency it has — one image reference, per
/// vendor, per launch — rather than reaching for a store. Production binds it
/// to the project's committed configuration (PSS-FR-30).
pub trait VendorImageSource: Send + Sync {
    /// The reference to create the container from, or the typed refusal the
    /// project's own store returned.
    fn image_for(&self, vendor: &str) -> Result<String, String>;
}

/// Production image source: the open project's committed Docker image
/// configuration (PSS-FR-30). There is no fallback to a shipped vendor image,
/// so a project that configures none launches nothing.
pub struct ProjectVendorImages {
    root: crate::fs::RootFs,
}

impl ProjectVendorImages {
    pub fn new(root: crate::fs::RootFs) -> Self {
        ProjectVendorImages { root }
    }
}

impl VendorImageSource for ProjectVendorImages {
    fn image_for(&self, vendor: &str) -> Result<String, String> {
        crate::project_settings::resolve_project_vendor_image(&self.root, vendor)
    }
}

// ---------------------------------------------------------------------------
// The executor
// ---------------------------------------------------------------------------

/// EAC-FR-39: where the runtime seam is bound from.
///
/// Production binds it per launch from the machine's **verified** Docker
/// backend, so the executor probes for nothing of its own. A test — and
/// `../infra/ACM-agentic-cli-mock.md` standing in through the same seam — binds
/// one directly.
enum RuntimeBinding {
    FromVerifiedBackend,
    Fixed(Arc<dyn DockerRuntime>),
}

pub struct AgentCliExecutor {
    runtime: RuntimeBinding,
    /// EAC-FR-31: where per-vendor, per-project session state is kept on the
    /// host. `None` means resolve it under `app_data_dir()` at launch, which is
    /// what production does; a test names its own directory instead so that a
    /// suite never writes into the author's real application data.
    session_state_root: Option<PathBuf>,
}

impl Default for AgentCliExecutor {
    fn default() -> Self {
        AgentCliExecutor {
            runtime: RuntimeBinding::FromVerifiedBackend,
            session_state_root: None,
        }
    }
}

impl AgentCliExecutor {
    pub fn new(runtime: Arc<dyn DockerRuntime>) -> Self {
        AgentCliExecutor {
            runtime: RuntimeBinding::Fixed(runtime),
            session_state_root: None,
        }
    }

    /// EAC-FR-39: the runtime this launch goes through — the Docker backend the
    /// machine has **verified**, resolved once per launch and probed for
    /// nothing.
    ///
    /// An unverified or unresolvable backend is `RuntimeUnavailable` before any
    /// container is created, and nothing here verifies, re-verifies, or repairs
    /// one (GSS-FR-40).
    fn runtime_for(
        &self,
        store: &GlobalSettingsStore,
    ) -> Result<Arc<dyn DockerRuntime>, AgentExecutionError> {
        match &self.runtime {
            RuntimeBinding::Fixed(runtime) => Ok(runtime.clone()),
            RuntimeBinding::FromVerifiedBackend => {
                match crate::docker::resolve_docker_backend(store) {
                    Err(_) => Err(AgentExecutionError::RuntimeUnavailable),
                    Ok(crate::docker::ResolvedDockerBackend::Cli { path }) => {
                        // The executable the author verified, rather than
                        // whatever `PATH` happens to answer with.
                        Ok(Arc::new(HostDockerCli::with_program(path)))
                    }
                    Ok(crate::docker::ResolvedDockerBackend::Engine { endpoint }) => {
                        Ok(Arc::new(runtime::BollardDocker::new(endpoint)))
                    }
                }
            }
        }
    }

    /// The same executor keeping session state under `root` (EAC-FR-31).
    pub fn with_session_state_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.session_state_root = Some(root.into());
        self
    }

    /// The directory this launch's session state lives in, created if absent.
    ///
    /// Three outcomes, kept distinct on purpose. `Ok(None)` means this vendor has
    /// no dedicated mount because its credential mount is also its session
    /// directory (EAC-FR-16). `Ok(Some(..))` means the directory is there and
    /// mountable. An `Err` means it should have been there and is not — which is
    /// a failure to report rather than a mount to quietly omit, because a turn
    /// that runs without it looks entirely successful while every transcript it
    /// writes dies with the container (EAC-FR-31).
    /// EAC-FR-41: an empty, read-only directory to cover a `.git` path with, so a
    /// read of one fails and a write to one fails.
    ///
    /// It lives beside the executor's own session state rather than in either
    /// worktree: it is machine state this module owns, and it must exist on every
    /// machine a semantic-rebase turn can run on — a masking that could not be
    /// established is a refusal to launch rather than a launch without it.
    /// EAC-FR-40: every user-scoped root the application owns, which is what a
    /// supplied bundle must sit under.
    ///
    /// There are **two** of them and a bundle inside either one satisfies the
    /// check: the application data directory, and the short root of
    /// `FSA-filesystem-access.md` FSA-FR-KVWD — which is where
    /// `GSU-graduation-start.md` GSU-FR-RJRF puts a graduation run's store, and
    /// therefore every semantic-rebase bundle this mount exists to carry.
    /// Naming only the first is what made every semantic-rebase turn refuse to
    /// launch.
    ///
    /// A build under test roots it at the harness's own directory, exactly as it
    /// roots session state there, and that one root **replaces** both — a test
    /// exercises the root it named rather than the machine's.
    fn data_roots(&self) -> Result<Vec<PathBuf>, AgentExecutionError> {
        if let Some(root) = &self.session_state_root {
            return Ok(vec![root.clone()]);
        }
        let roots: Vec<PathBuf> = [crate::fs::short_data_dir(), crate::fs::app_data_dir()]
            .into_iter()
            .flatten()
            .collect();
        if roots.is_empty() {
            return Err(AgentExecutionError::SupplementaryMountInvalid(
                "no application data directory".to_string(),
            ));
        }
        Ok(roots)
    }

    /// EAC-FR-FNFV: the read-only repository mounts and Git environment for one
    /// launch, or `None` where this turn reaches no repository.
    ///
    /// **All of it or none of it.** A store mounted without the mask that covers
    /// its `config`, or a linked worktree whose pointer could not be pinned, is
    /// worse than no access: the first discloses a credential and the second
    /// lets a turn redirect the next turn's mount. So a part that cannot be
    /// expressed as a mount takes the whole grant with it, and the turn reads
    /// files instead — which is what it did before this existed.
    /// `directory` is the **canonical** execution directory of EAC-FR-02, which
    /// is the one spelling everything below is composed from: the shape was
    /// decided from it, and Git reports the repository in the same form.
    fn repository_mounts(
        &self,
        fs: &FsAccess,
        directory: &Path,
        workspace: &str,
        aligned: bool,
    ) -> Option<(Vec<BindMount>, Vec<(String, String)>, bool)> {
        let access = repository_access(directory)?;
        let mask = self.empty_mask_dir(fs).ok()?;
        // What the generated files below are named by. They are one execution's
        // own: a name shared with a concurrent launch is one that launch can
        // replace between this write and the moment the runtime resolves the
        // mount, which would stand this turn on another working copy's `.git`.
        let slug = descriptor::path_slug(directory);
        // EAC-FR-ZKMR: where the store stands, and what the worktree resolves
        // it through. Where the container names the host's paths the store is
        // at its own, the records the worktree already carries name paths the
        // container has, and nothing is generated — the pin is then the
        // worktree's own file. In the fallback shape the store stands somewhere
        // those records do not name, and both of them are generated.
        let (store, pin, record) = match &access {
            RepositoryAccess::Plain { .. } => (String::new(), mask.file.clone(), None),
            RepositoryAccess::Linked { common_dir, .. } if aligned => (
                // Withheld rather than half-granted: a store this launch cannot
                // express is the part EAC-FR-FNFV takes the whole grant with,
                // and the container keeps the paths it has already agreed on.
                descriptor::container_path(common_dir)?,
                directory.join(".git"),
                None,
            ),
            RepositoryAccess::Linked { .. } => {
                let pointer = mask
                    .dir
                    .with_file_name(format!("{REPOSITORY_POINTER_FILENAME}-{slug}"));
                fs.write_text_atomic(&pointer, &access.pointer_text()?).ok()?;
                let commondir = mask
                    .dir
                    .with_file_name(format!("{REPOSITORY_COMMONDIR_FILENAME}-{slug}"));
                fs.write_text_atomic(&commondir, &access.commondir_text()?)
                    .ok()?;
                (GIT_COMMON_DIR_TARGET.to_string(), pointer, Some(commondir))
            }
        };
        let mut out: Vec<BindMount> = Vec::new();
        for (source, target) in access.mounts(workspace, &store, &pin, record.as_deref(), &mask.file)
        {
            out.push(BindMount {
                source: descriptor::mount_source(&source)?,
                target,
                // Read-only is the whole of the grant: the turn reads history,
                // and it commits, checks out, stages, fetches, and rewrites
                // nothing.
                read_only: true,
            });
        }
        let env = access
            .env()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect();
        Some((out, env, access.linked()))
    }

    fn empty_mask_dir(&self, fs: &FsAccess) -> Result<RepositoryMask, AgentExecutionError> {
        use crate::fs::FsError;

        let root = match &self.session_state_root {
            Some(root) => root.clone(),
            None => crate::fs::app_data_dir()
                .map_err(|_| {
                    AgentExecutionError::RepositoryMaskingUnavailable(
                        "no application data directory".to_string(),
                    )
                })?
                .join(SESSION_STATE_DIRNAME),
        };
        let dir = root.join(REPOSITORY_MASK_DIRNAME);
        // Routed through `FsAccess` like every other backend filesystem operation.
        if let Err(err) = fs.create_dir(&dir) {
            match err {
                FsError::AlreadyExists { .. } => {}
                other => {
                    return Err(AgentExecutionError::RepositoryMaskingUnavailable(
                        other.to_string(),
                    ));
                }
            }
        }
        // EAC-FR-41: a `.git` is not always a directory. A **linked worktree**
        // — which is what every graduation run is given (`Repository::worktree`)
        // — carries a `.git` **file** holding a `gitdir:` pointer at its root,
        // and a bind mount of a directory onto a file is refused by the kernel
        // with `ENOTDIR`. So the mask is a pair, and each entry is masked by
        // the one of its own kind.
        let file = root.join(REPOSITORY_MASK_FILENAME);
        // Truncating, so a file left from an earlier launch is empty again
        // rather than trusted to still be: what this mounts must hold nothing.
        if let Err(err) = fs.create_file(&file, crate::fs::CreateMode::Truncate) {
            return Err(AgentExecutionError::RepositoryMaskingUnavailable(
                err.to_string(),
            ));
        }
        Ok(RepositoryMask { dir, file })
    }

    fn session_state_for(
        &self,
        fs: &FsAccess,
        descriptor: &descriptor::VendorExecutionDescriptor,
        project_key: &str,
    ) -> Result<Option<(PathBuf, &'static str, &'static str)>, AgentExecutionError> {
        use crate::fs::FsError;

        let descriptor::SessionStateMount::Dedicated { env, target } = descriptor.session_state
        else {
            return Ok(None);
        };

        let root = match &self.session_state_root {
            Some(root) => root.clone(),
            None => crate::fs::app_data_dir()
                .map_err(|_| {
                    AgentExecutionError::SessionStateUnavailable(SessionStateProblem::RootUnresolved)
                })?
                .join(SESSION_STATE_DIRNAME),
        };
        let dir = descriptor::session_state_dir(&root, descriptor.vendor, project_key);

        // Routed through `FsAccess` like every other backend filesystem
        // operation. The shared instance already allowlists `app_data_dir()`,
        // which is what this directory lives under, so nothing here needs a
        // bootstrap exemption.
        match fs.create_dir(&dir) {
            Ok(()) => {}
            // `create_dir` refuses to adopt an existing entry, so the second and
            // every later launch for a project lands here. That is the normal
            // path, not a failure — but only where what is already there is a
            // directory.
            Err(FsError::AlreadyExists { .. }) => {
                let info = fs.file_info(&dir).map_err(|_| {
                    AgentExecutionError::SessionStateUnavailable(SessionStateProblem::Unusable)
                })?;
                if info.kind != EntryKind::Dir {
                    return Err(AgentExecutionError::SessionStateUnavailable(
                        SessionStateProblem::Unusable,
                    ));
                }
            }
            Err(_) => {
                return Err(AgentExecutionError::SessionStateUnavailable(
                    SessionStateProblem::NotCreatable,
                ));
            }
        }

        Ok(Some((dir, env, target)))
    }

    /// EAC-FR-01: the one entry point. One call is one container.
    ///
    /// Not a `#[tauri::command]`, not registered in `generate_handler!`, and
    /// unreachable from `src/**`.
    pub async fn execute_agent_cli<S>(
        &self,
        sink: &S,
        context: &LaunchContext<'_>,
        request: AgentExecutionRequest,
    ) -> Result<AgentExecution, AgentExecutionError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let started = Instant::now();
        let result = self.launch(sink, context, request, started).await;

        if let Err(error) = &result {
            // Every error path is logged, including the ones a caller handles —
            // a launch that never happened is otherwise invisible in the panel.
            logging::log_error(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "agent execution refused before launch",
                log_fields! {
                    "reason" => error.kind(),
                    "duration_ms" => started.elapsed().as_millis() as u64,
                },
            );
        }
        result
    }
}

/// Re-exported so a caller names one type rather than reaching into the seam.
pub use runtime::CancellationToken as ExecutionCancellation;

/// The buffer this module logs into, named so a test can read it back.
pub fn log_buffer() -> &'static LogBuffer {
    &BUFFER
}

// Declared at the foot rather than beside the other modules: `FSA-FR-*`'s sweep
// splits a file at its first `#[cfg(test)] mod`, and a declaration near the top
// made the whole of this file read as test code — so nothing in it was ever
// checked for reaching the filesystem outside `FsAccess`. The sweep's own
// boundary assertion is what caught that.
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod tests;
