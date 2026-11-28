//! Vendor execution descriptors and the invocation they generate (EAC-FR-10).
//!
//! Every Docker parameter and every vendor argument fragment is a named
//! constant or a field of an immutable descriptor. No call site assembles an
//! argument string of its own, which is what makes the complete generated
//! invocation derivable from this file alone — and therefore assertable
//! byte-for-byte by a test (EAC-FR-35, EAC-FR-10, EAC-FR-11, EAC-FR-13, EAC-FR-22, EAC-FR-12).
//!
//! ## Where the pinned identity comes from
//!
//! The image reference, its digest, the installed CLI version, and the
//! container UID/GID are read from `docker/agent-images/manifest.toml`, which
//! `../infra/AVI-agent-vendor-images.md` owns (AVI-FR-07). Nothing here hardcodes
//! any of them, so upgrading a vendor CLI is an edit of its Dockerfile and a
//! rebuild — not an edit to this file.
//!
//! The manifest is `include_str!`'d rather than read at run time: it is
//! application data compiled into the binary, so it cannot be swapped by
//! whoever can write next to the executable, and a malformed manifest fails the
//! build rather than the first launch.

#[cfg(test)]
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(test)]
use std::sync::OnceLock;

#[cfg(test)]
use serde::Deserialize;

use super::protocol::{AgentResponseEnvelope, EnvelopeInvalid, ResultContract};

pub mod claude_code;
pub mod codex;
mod container;

pub use container::*;

/// AVI-FR-07: the one record of what each **shipped** image is.
///
/// Compiled in under `cfg(test)` alone, and deliberately so. AVI-FR-07 makes
/// the manifest the record of the shipped pair and names the publish and verify
/// commands as what read it; no launch reads it any more, because the image a
/// container is created from is the one the open project commits (EAC-FR-38).
/// Gating it here is what makes "the repository outside `docker/agent-images/`
/// holds no shipped image reference or digest" (AVI-FR-07) a property of the
/// shipped binary rather than a convention, while the checks that stand in for
/// `verify` still read it.
#[cfg(test)]
const MANIFEST_TOML: &str = include_str!("../../../../docker/agent-images/manifest.toml");

/// AVI-FR-08's "defined but not yet published" value.
///
/// `verify` fails on it by design. A **launch** does not: [`ImageManifestEntry::launch_reference`]
/// addresses such an entry by its tag instead, because a repository digest is
/// something only a registry issues and a locally built image therefore has none
/// however correct it is. What that costs is stated there, and it is the reason
/// the tag an unpublished entry names has to be a tag somebody actually builds.
#[cfg(test)]
pub const UNPUBLISHED_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[cfg(test)]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ImageManifestEntry {
    pub image_ref: String,
    pub image_digest: String,
    pub cli_version: String,
    pub uid: u32,
    pub gid: u32,
}

#[cfg(test)]
impl ImageManifestEntry {
    /// The image as the shipped-image checks address it.
    ///
    /// A **published** image is addressed by the repository without its tag,
    /// pinned by digest (AVI-FR-08): a tag that has moved since publication
    /// cannot resolve to a different image through this, which is the whole
    /// protection the digest buys.
    ///
    /// An image carrying the unpublished sentinel is addressed by its `image_ref`
    /// instead — tag and all. There is no digest to pin to, and a repository
    /// digest is something only a registry issues, so a locally built image has
    /// none however correct it is. `verify` fails on such an entry by design
    /// (AVI-FR-08); nothing launches from here at all.
    pub fn launch_reference(&self) -> String {
        if !self.is_published() {
            return self.image_ref.clone();
        }
        let repository =
            self.image_ref.rsplit_once(':').map_or(self.image_ref.as_str(), |(repo, _)| repo);
        format!("{repository}@{}", self.image_digest)
    }

    pub fn is_published(&self) -> bool {
        self.image_digest != UNPUBLISHED_DIGEST
    }
}

#[cfg(test)]
fn manifest() -> &'static BTreeMap<String, ImageManifestEntry> {
    static PARSED: OnceLock<BTreeMap<String, ImageManifestEntry>> = OnceLock::new();
    PARSED.get_or_init(|| {
        toml::from_str(MANIFEST_TOML).expect("docker/agent-images/manifest.toml is malformed")
    })
}

#[cfg(test)]
pub fn manifest_entry(vendor: &str) -> Option<&'static ImageManifestEntry> {
    manifest().get(vendor)
}

// ---------------------------------------------------------------------------
// Docker parameters (EAC-FR-13)
// ---------------------------------------------------------------------------

/// The host program the runtime seam invokes in production.
pub const DOCKER_PROGRAM: &str = "docker";

/// EAC-FR-37: where the runtime is looked for when `PATH` does not hold it.
///
/// An application bundle started the way a user starts an application inherits
/// the session launcher's `PATH` rather than the one their shell builds — on
/// macOS that is `/usr/bin:/bin:/usr/sbin:/sbin`, which holds no directory a
/// container runtime installs into. A `PATH` lookup by itself therefore reports
/// no runtime on a machine whose runtime is installed and running, and the
/// author is told to install what they already have.
///
/// `$HOME` in an entry is the one the process was given; an entry that needs a
/// home and has none is passed over.
pub const DOCKER_SEARCH_PATHS: &[&str] = &[
    // Docker Desktop's own symlink, and where most Linux packages land.
    "/usr/local/bin/docker",
    // Homebrew on Apple silicon.
    "/opt/homebrew/bin/docker",
    // Docker Desktop's per-user CLI, which recent versions prefer.
    "$HOME/.docker/bin/docker",
    // Rancher Desktop, which supplies a Docker-compatible CLI of its own.
    "$HOME/.rd/bin/docker",
    // The application bundle, where nothing else has been linked.
    "/Applications/Docker.app/Contents/Resources/bin/docker",
];

/// EAC-FR-37: [`DOCKER_SEARCH_PATHS`] with `$HOME` resolved, in order.
///
/// Resolved here rather than at each use so the seam that tries them and the
/// record that names them cannot disagree about what was looked at.
pub fn docker_search_paths() -> Vec<String> {
    let home = std::env::var("HOME").ok();
    DOCKER_SEARCH_PATHS
        .iter()
        .filter_map(|entry| match entry.strip_prefix("$HOME/") {
            Some(rest) => home.as_ref().map(|home| format!("{home}/{rest}")),
            None => Some((*entry).to_string()),
        })
        .collect()
}
/// EAC-FR-ZKMR: where the execution directory is mounted, and the container's
/// working directory, in the **fallback shape** — the shape an execution takes
/// where its own host path is not one a container can be given.
pub const WORKSPACE_TARGET: &str = "/workspace";

/// EAC-FR-ZKMR: a host path a container can be given, or `None` where it cannot.
///
/// Where it can, the container names the path its host names, and what the
/// application calls a file and what the turn inside the container calls it are
/// one string. Two host paths cannot be container paths, and each is a fallback
/// rather than a refusal: one that is not absolute in the form the runtime takes
/// — a Windows path names a drive, and a container has no drive to name — and
/// one holding a comma, which the mount syntax separates its own fields with.
pub fn container_path(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if !text.starts_with('/') || text.contains(',') {
        return None;
    }
    Some(text.to_string())
}
/// The container's network. Named rather than defaulted: both vendor CLIs reach
/// their own services, so `none` would break every launch — but the value is a
/// decision worth reading in a diff rather than an omission.
pub const NETWORK: &str = "bridge";
/// EAC-FR-13: the least privileges the pinned CLIs work under. `--cap-drop ALL`
/// because a Node process editing files in a bind mount needs no Linux
/// capability at all, and `no-new-privileges` because nothing in either image is
/// setuid. Never `--privileged`.
pub const SECURITY_OPTS: &[&str] = &["--cap-drop", "ALL", "--security-opt", "no-new-privileges"];
/// EAC-FR-DKRS: where a container sees the host Docker socket, whatever its
/// host path is.
pub const DOCKER_SOCKET_TARGET: &str = "/var/run/docker.sock";
/// EAC-FR-GKSW: the network of the helper container that reads the socket
/// group. It reads a file and needs no network.
pub const HELPER_NETWORK: &str = "none";
/// EAC-FR-GKSW: the program the helper runs in place of the image entrypoint.
pub const SOCKET_GROUP_ENTRYPOINT: &str = "stat";
/// EAC-FR-GKSW: the arguments of [`SOCKET_GROUP_ENTRYPOINT`]: print the numeric
/// group of the socket at its container path.
pub const SOCKET_GROUP_ARGS: [&str; 3] = ["-c", "%g", DOCKER_SOCKET_TARGET];
/// EAC-FR-DKRS: the host mapping of the host gateway toggle.
pub const DOCKER_HOST_GATEWAY_MAPPING: &str = "host.docker.internal:host-gateway";
/// EAC-FR-DKRS: the variable Testcontainers reads to reach the host, set where
/// the host gateway toggle is on.
pub const TESTCONTAINERS_HOST_OVERRIDE_ENV: &str = "TESTCONTAINERS_HOST_OVERRIDE";
/// EAC-FR-DKRS: the value of [`TESTCONTAINERS_HOST_OVERRIDE_ENV`].
pub const TESTCONTAINERS_HOST_OVERRIDE_VALUE: &str = "host.docker.internal";
/// EAC-FR-QMRW: the variable the sandbox process reaches the Docker daemon of
/// its VM through.
pub const DOCKER_HOST_ENV: &str = "DOCKER_HOST";
/// EAC-FR-QMRW: the value of [`DOCKER_HOST_ENV`], the guest socket of the
/// daemon in the VM.
pub const VM_DOCKER_HOST_VALUE: &str = "unix:///var/run/docker.sock";
/// The environment variable Claude Code reads its OAuth token from (EAC-FR-15).
pub const CLAUDE_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";
/// EAC-FR-WQNH: the two Bash timeouts of the Claude Code CLI.
pub const BASH_TIMEOUT_ENVS: [&str; 2] = ["BASH_DEFAULT_TIMEOUT_MS", "BASH_MAX_TIMEOUT_MS"];
/// EAC-FR-WQNH: the value of each of [`BASH_TIMEOUT_ENVS`], 15 minutes.
pub const BASH_TIMEOUT_MS: &str = "900000";

/// EAC-FR-WQNH: the Bash timeouts the executor sets on a launch of `vendor`.
/// A Claude Code launch gets both, except a variable that the launch
/// environment names: the author's value then stands. Other vendors get none.
pub fn bash_timeouts(vendor: &str, env_names: &[String]) -> Vec<(String, String)> {
    if vendor != CLAUDE_CODE.vendor {
        return Vec::new();
    }
    BASH_TIMEOUT_ENVS
        .iter()
        .filter(|name| !env_names.iter().any(|named| named == *name))
        .map(|name| (name.to_string(), BASH_TIMEOUT_MS.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Vendor descriptors
// ---------------------------------------------------------------------------

/// Where in a vendor's structured output the response envelope may sit
/// (EAC-FR-19).
///
/// A descriptor names these in order, and extraction takes the first that yields
/// a document. Declarative rather than executable: it is what lets a test
/// compare a descriptor against the specification it transcribes without running
/// a container (EAC-FR-19, EAC-FR-10, EAC-FR-11, EAC-FR-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseSource {
    /// One JSON object on stdout; the envelope is the object at this key.
    JsonObjectField { key: &'static str },
    /// One JSON object on stdout; the envelope is the string at this key,
    /// parsed as JSON text.
    JsonStringField { key: &'static str },
    /// JSONL events; the envelope is the text of the last completed item of
    /// this type, parsed as JSON text.
    JsonlLastItem { event_type: &'static str, item_type: &'static str, text_key: &'static str },
}

/// What one line of a vendor's output means, in terms every vendor shares
/// (EAC-FR-33).
///
/// Normalized rather than passed through, because a reader watching a run should
/// not have to know that one CLI calls a tool call `item.started` and another
/// calls it a content block of type `tool_use`. The verbatim line is kept beside
/// this, so normalization never becomes the only account of what happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    /// The command line the container was launched with. Not something a vendor
    /// emits — it is what the executor did, recorded first so a reader sees the
    /// invocation above the run it produced.
    Invocation,
    /// The task document handed to the agent on stdin. Also the executor's, and
    /// the other half of "what was this agent actually asked to do".
    Task,
    /// The CLI announced itself: its session, model, and what it can reach.
    Started,
    /// The agent's own thinking, where the vendor emits it.
    Reasoning,
    /// Prose the agent wrote.
    Message,
    /// One valid milestone marker the agent wrote in its prose (EAC-FR-ZNBD).
    /// The executor makes it from a message line. A vendor reader never does.
    Milestone,
    /// The agent asked for a tool.
    ToolCall,
    /// A tool answered.
    ToolResult,
    /// A shell command the agent ran.
    Command,
    /// The agent wrote to the working tree.
    FileChange,
    /// The CLI is retrying its own service.
    Retry,
    /// Token or cost accounting.
    Usage,
    /// The turn ended, however it ended.
    Finished,
    /// The CLI reported a failure.
    Error,
    /// Anything on stderr, which both pinned protocols define as diagnostics.
    Diagnostic,
    /// A line no rule matched. Never dropped: an event this build does not know
    /// about is exactly what a reader needs to see when a vendor changes.
    Unrecognized,
}

impl ActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivityKind::Invocation => "invocation",
            ActivityKind::Task => "task",
            ActivityKind::Started => "started",
            ActivityKind::Reasoning => "reasoning",
            ActivityKind::Message => "message",
            ActivityKind::Milestone => "milestone",
            ActivityKind::ToolCall => "tool_call",
            ActivityKind::ToolResult => "tool_result",
            ActivityKind::Command => "command",
            ActivityKind::FileChange => "file_change",
            ActivityKind::Retry => "retry",
            ActivityKind::Usage => "usage",
            ActivityKind::Finished => "finished",
            ActivityKind::Error => "error",
            ActivityKind::Diagnostic => "diagnostic",
            ActivityKind::Unrecognized => "unrecognized",
        }
    }
}

/// The longest summary a normalized activity carries.
///
/// A summary is a row in a list. The whole of what the vendor wrote is kept
/// verbatim beside it, so nothing is lost by keeping this short.
pub const LIMIT_SUMMARY_LINE: usize = 300;

/// Collapse text to one bounded line, for a summary.
///
/// Shared rather than transcribed per vendor: how a row is shortened is a
/// property of the row, not of the CLI that filled it, and a vendor module that
/// reached into another's would break the one-file-per-protocol rule EAC-FR-11
/// and EAC-FR-12 rest on.
pub fn summary_line(text: &str) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= LIMIT_SUMMARY_LINE {
        return flat;
    }
    let kept: String = flat.chars().take(LIMIT_SUMMARY_LINE).collect();
    format!("{kept}…")
}

/// One observed line, read (EAC-FR-33).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorActivity {
    pub kind: ActivityKind,
    /// One line a person can read, composed from the event's own fields. Never
    /// the whole event: that is carried verbatim beside it.
    pub summary: String,
}

/// Where a vendor's session identity comes from (EAC-FR-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionSource {
    /// The executor assigns the identifier before the run and the run must
    /// report the same one back at `key`; a mismatch is invalid output.
    AssignedThenAsserted { key: &'static str },
    /// The vendor reports an identifier of its own at `event_type`.`key`, and
    /// the executor cannot preassign one.
    ReportedByEvent { event_type: &'static str, key: &'static str },
}

/// How a vendor's session state survives the container (EAC-FR-31).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStateMount {
    /// A Synthesis-owned directory mounted at `target` and addressed by `env`.
    /// The credential arrives separately, so this mount carries transcripts and
    /// nothing else.
    Dedicated { env: &'static str, target: &'static str },
    /// The vendor's credential mount *is* its session directory, so the two
    /// coincide and that mount is read/write. Its target is whatever AIC
    /// supplied, so there is none to name here.
    SharedWithCredentialMount { env: &'static str },
    /// EAC-FR-QHIU: the credential decides. A credential that is a mount is
    /// also the session directory, as [`Self::SharedWithCredentialMount`]; a
    /// credential in the environment leaves a dedicated directory at
    /// `dedicated_target`, as [`Self::Dedicated`].
    ByCredential { env: &'static str, dedicated_target: &'static str },
}

impl SessionStateMount {
    /// EAC-FR-31 / EAC-FR-QHIU: the mount a launch gets, for a credential that
    /// is a mount (`true`) or one in the environment (`false`). Never
    /// [`Self::ByCredential`].
    pub fn for_credential(self, credential_is_mount: bool) -> SessionStateMount {
        match self {
            SessionStateMount::ByCredential { env, .. } if credential_is_mount => {
                SessionStateMount::SharedWithCredentialMount { env }
            }
            SessionStateMount::ByCredential { env, dedicated_target } => {
                SessionStateMount::Dedicated { env, target: dedicated_target }
            }
            other => other,
        }
    }
}

/// How a vendor's credential reaches its container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialForm {
    /// A named environment variable, whose value never reaches an argv.
    EnvironmentVariable(&'static str),
    /// A directory mounted at the target AIC supplied.
    Mount,
    /// EAC-FR-16 / EAC-FR-QHIU: a directory mount in one mode, and this named
    /// environment variable in the other.
    MountOrEnvironmentVariable(&'static str),
}

/// What extraction is handed.
pub struct ExtractContext<'a> {
    pub stdout: &'a str,
    /// The identifier the executor assigned before the run, for a vendor that
    /// permits one. `None` on a resumed turn and for a vendor that does not.
    pub assigned_session_id: Option<&'a str>,
    /// EAC-FR-43: the result contract the task named, for a vendor whose
    /// reading of the answer depends on it (EAC-FR-RBLI).
    pub result_contract: Option<ResultContract>,
}

/// What extraction produces.
pub struct Extracted {
    pub envelope: AgentResponseEnvelope,
    /// Whatever session identity the run itself reported, which is what makes a
    /// later resume possible.
    pub session_id: Option<String>,
}

/// What one turn's vendor argument vector, and its launch files, are made
/// from (EAC-FR-10, EAC-FR-BOXF).
#[derive(Clone, Copy, Debug, Default)]
pub struct ArgvRequest<'a> {
    pub model_id: Option<&'a str>,
    pub effort_id: Option<&'a str>,
    /// EAC-FR-21: the identifier the executor generated, for a vendor that
    /// takes one. A vendor that reports its own ignores it.
    pub assigned_session_id: &'a str,
    /// EAC-FR-30: the session a resumed turn continues. `None` for a fresh turn.
    pub resume_session: Option<&'a str>,
    /// EAC-FR-43: the result contract the task named. What a vendor does with
    /// it is that vendor's own protocol.
    pub result_contract: Option<ResultContract>,
    /// EAC-FR-WBIF: whether this launch carries the vendor's milestone reminder.
    pub milestone_reminder: bool,
}

/// EAC-FR-BOXF / EAC-FR-VTLG: one fixed file a vendor's argument vector names.
/// It holds fixed text compiled into the binary and nothing of the task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchFile {
    pub name: &'static str,
    pub contents: Vec<u8>,
}

/// EAC-FR-BOXF: where a launch's files are mounted, read-only, in the container.
pub const LAUNCH_FILES_TARGET: &str = "/opt/synthesis/vendor";

/// EAC-FR-BOXF: the SHA-256 of what `files` hold, as the name of the directory
/// that holds them. Each file's name and length are hashed before its bytes, in
/// name order, so two different sets never share a name.
pub fn launch_files_digest(files: &[LaunchFile]) -> String {
    use sha2::{Digest, Sha256};

    let mut sorted: Vec<&LaunchFile> = files.iter().collect();
    sorted.sort_by_key(|file| file.name);
    let mut hasher = Sha256::new();
    for file in sorted {
        hasher.update(file.name.as_bytes());
        hasher.update([0u8]);
        hasher.update((file.contents.len() as u64).to_le_bytes());
        hasher.update(&file.contents);
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The whole vendor argument vector for a fresh turn.
type FreshArgv = fn(&ArgvRequest<'_>) -> Vec<String>;
/// The whole vendor argument vector for a resumed turn, given the session it
/// continues. Generated separately rather than derived from the fresh one,
/// because a resumed turn is not a fresh turn with an addition (CDX-FR-09).
type ResumeArgv = fn(&ArgvRequest<'_>, &str) -> Vec<String>;
/// EAC-FR-BOXF: the full fixed set of files the vendor's vectors can name.
type LaunchFiles = fn() -> Vec<LaunchFile>;
type Extractor = fn(&ExtractContext<'_>) -> Result<Extracted, EnvelopeInvalid>;
type EffortSupported = fn(&str) -> bool;
/// One line of this vendor's stdout, read as an activity (EAC-FR-33).
type ActivityReader = fn(&str, TextFilter<'_>) -> VendorActivity;

/// What a reader does to each agent text block before it composes a summary
/// from it. The identity filter reads the line as it came (EAC-FR-HTSN).
pub type TextFilter<'a> = &'a dyn Fn(&str) -> String;

/// Everything one vendor's invocation needs, as immutable application data
/// transcribed from that vendor's own protocol specification (EAC-FR-10).
#[derive(Clone, Copy)]
pub struct VendorExecutionDescriptor {
    pub vendor: &'static str,
    /// The specification this descriptor transcribes. Named so that a reader who
    /// finds a flag here knows which document justifies it.
    pub protocol_spec: &'static str,
    fresh_argv: FreshArgv,
    resume_argv: Option<ResumeArgv>,
    effort_supported: Option<EffortSupported>,
    /// EAC-FR-WBIF: whether this vendor's protocol states a milestone reminder.
    /// The vector functions place it, because where it goes is that protocol's.
    pub milestone_reminder: bool,
    launch_files: LaunchFiles,
    extractor: Extractor,
    /// EAC-FR-21: the session the stream reports, at the position
    /// `session_source` names.
    session_reader: fn(&str) -> Option<String>,
    activity: ActivityReader,
    /// EAC-FR-19's ordered positions, as data.
    pub response_sources: &'static [ResponseSource],
    pub session_source: SessionSource,
    pub session_state: SessionStateMount,
    pub credential_form: CredentialForm,
    /// EAC-FR-HZIE: the stdout line that ends this vendor's turn. `None` for a
    /// vendor that declares none, whose run ends only by exit, cancellation, or
    /// the deadline (EAC-FR-QHAY).
    pub final_event: Option<super::final_event::FinalEvent>,
}

impl std::fmt::Debug for VendorExecutionDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VendorExecutionDescriptor")
            .field("vendor", &self.vendor)
            .field("protocol_spec", &self.protocol_spec)
            .field("response_sources", &self.response_sources)
            .field("session_source", &self.session_source)
            .field("session_state", &self.session_state)
            .field("credential_form", &self.credential_form)
            .field("final_event", &self.final_event.is_some())
            .finish()
    }
}

/// EAC-FR-11: Claude Code's descriptor, transcribing `CCP`.
pub static CLAUDE_CODE: VendorExecutionDescriptor = VendorExecutionDescriptor {
    vendor: "claude_code",
    protocol_spec: claude_code::SPEC,
    fresh_argv: |request| {
        with_claude_reminder(
            claude_code::fresh_argv(
                request.model_id,
                request.effort_id,
                request.assigned_session_id,
                request.result_contract,
            ),
            request,
        )
    },
    resume_argv: Some(|request, session| {
        with_claude_reminder(
            claude_code::resume_argv(
                request.model_id,
                request.effort_id,
                session,
                request.result_contract,
            ),
            request,
        )
    }),
    effort_supported: Some(claude_code::effort_supported),
    // CCP-FR-WZJS: a `PostToolUse` hook through `--settings`.
    milestone_reminder: true,
    // CCP-FR-07: the schema rides on the vector, so no file is needed.
    launch_files: Vec::new,
    extractor: claude_code::extract,
    session_reader: claude_code::reported_session,
    activity: claude_code::activity_filtered,
    // CCP-FR-09: the schema-validated object first, the text form second.
    response_sources: &[
        ResponseSource::JsonObjectField { key: "structured_output" },
        ResponseSource::JsonStringField { key: "result" },
    ],
    // CCP-FR-16: assigned before the run, asserted back afterwards.
    session_source: SessionSource::AssignedThenAsserted { key: "session_id" },
    session_state: SessionStateMount::Dedicated {
        env: claude_code::SESSION_STATE_ENV,
        target: claude_code::SESSION_STATE_TARGET,
    },
    credential_form: CredentialForm::EnvironmentVariable(CLAUDE_TOKEN_ENV),
    // CCP-FR-RGVA: the `result` line.
    final_event: Some(claude_code::is_final_event),
};

/// EAC-FR-12: Codex's descriptor, transcribing `CDX`.
pub static CODEX: VendorExecutionDescriptor = VendorExecutionDescriptor {
    vendor: "codex",
    protocol_spec: codex::SPEC,
    // This vendor cannot be told which session id to use for a new session, so
    // the assigned identifier it is handed is ignored (CDX-FR-20).
    fresh_argv: codex::fresh_argv,
    resume_argv: Some(codex::resume_argv),
    // CDX-FR-SDOV: the six levels this CLI accepts.
    effort_supported: Some(codex::effort_supported),
    // CDX-FR-OQVA: a `PreToolUse` hook through an inline override.
    milestone_reminder: true,
    // CDX-FR-14 / CDX-FR-WJZY: the strict schema and the reminder output.
    launch_files: codex::launch_files,
    extractor: codex::extract,
    session_reader: codex::reported_session,
    activity: codex::activity_filtered,
    // CDX-FR-12: exactly one position.
    response_sources: &[ResponseSource::JsonlLastItem {
        event_type: "item.completed",
        item_type: "agent_message",
        text_key: "text",
    }],
    // CDX-FR-20: reported, never assigned.
    session_source: SessionSource::ReportedByEvent {
        event_type: "thread.started",
        key: "thread_id",
    },
    // CDX-FR-23 / CDX-FR-EUBZ: the login directory is the session directory in
    // the `login_folder` mode; the `access_token` mode has a dedicated one.
    session_state: SessionStateMount::ByCredential {
        env: codex::SESSION_STATE_ENV,
        dedicated_target: codex::SESSION_STATE_TARGET,
    },
    credential_form: CredentialForm::MountOrEnvironmentVariable(codex::ACCESS_TOKEN_ENV),
    // CDX-FR-RJPF: `turn.completed` or `turn.failed`.
    final_event: Some(codex::is_final_event),
};

/// CCP-FR-WZJS: Claude Code's reminder stands at the end of its vector.
fn with_claude_reminder(mut args: Vec<String>, request: &ArgvRequest<'_>) -> Vec<String> {
    if request.milestone_reminder {
        args.extend(claude_code::milestone_reminder_args());
    }
    args
}

pub fn descriptor_for(vendor: &str) -> Option<&'static VendorExecutionDescriptor> {
    match vendor {
        "claude_code" => Some(&CLAUDE_CODE),
        "codex" => Some(&CODEX),
        _ => None,
    }
}

impl VendorExecutionDescriptor {
    /// The complete vendor argument vector for one turn, with no milestone
    /// reminder.
    pub fn vendor_args(
        &self,
        model_id: Option<&str>,
        effort_id: Option<&str>,
        assigned_session_id: &str,
        resume_session: Option<&str>,
        result_contract: Option<ResultContract>,
    ) -> Vec<String> {
        self.argv(&ArgvRequest {
            model_id,
            effort_id,
            assigned_session_id,
            resume_session,
            result_contract,
            milestone_reminder: false,
        })
    }

    /// The complete vendor argument vector for one turn.
    ///
    /// A fresh turn and a resumed turn are generated by different functions
    /// rather than by one with a branch, because for at least one pinned vendor
    /// they are different grammars accepting different options (CDX-FR-09). A
    /// vendor with no reminder ignores `milestone_reminder` (EAC-FR-WBIF).
    pub fn argv(&self, request: &ArgvRequest<'_>) -> Vec<String> {
        match (request.resume_session, self.resume_argv) {
            (Some(session), Some(build)) => build(request, session),
            // Unreachable in practice: a vendor that cannot resume is refused
            // before launch (EAC-FR-30). Falling back to a fresh vector rather
            // than panicking keeps a programming error from taking the process
            // down.
            _ => (self.fresh_argv)(request),
        }
    }

    /// EAC-FR-BOXF: the fixed files this turn's vector names. Empty where the
    /// vector names none.
    pub fn launch_files(&self) -> Vec<LaunchFile> {
        (self.launch_files)()
    }

    /// Whether the pinned CLI can resume a session at all (EAC-FR-30).
    pub fn supports_resume(&self) -> bool {
        self.resume_argv.is_some()
    }

    /// Whether this vendor can preassign a session identifier (EAC-FR-21).
    pub fn assigns_session_id(&self) -> bool {
        matches!(self.session_source, SessionSource::AssignedThenAsserted { .. })
    }

    /// Whether a resolved reasoning effort is one the pinned CLI accepts.
    ///
    /// A vendor whose CLI validates the value itself declares none here, and
    /// every identifier reaches it unchanged.
    pub fn effort_supported(&self, effort_id: &str) -> bool {
        match self.effort_supported {
            Some(check) => check(effort_id),
            None => true,
        }
    }

    /// Pull the envelope out of the vendor's structured output.
    ///
    /// Returns the envelope and, separately, whatever session id the run itself
    /// reported — which is what makes a later resume possible, since an agent
    /// does not reliably know the id its own CLI assigned.
    pub fn extract(
        &self,
        stdout: &str,
        assigned_session_id: Option<&str>,
    ) -> Result<(AgentResponseEnvelope, Option<String>), EnvelopeInvalid> {
        self.extract_for(stdout, assigned_session_id, None)
    }

    /// [`Self::extract`] for a task that named `result_contract` (EAC-FR-RBLI).
    pub fn extract_for(
        &self,
        stdout: &str,
        assigned_session_id: Option<&str>,
        result_contract: Option<ResultContract>,
    ) -> Result<(AgentResponseEnvelope, Option<String>), EnvelopeInvalid> {
        let extracted =
            (self.extractor)(&ExtractContext { stdout, assigned_session_id, result_contract })?;
        Ok((extracted.envelope, extracted.session_id))
    }

    /// EAC-FR-21: the session a turn reported, read without the envelope.
    ///
    /// A refused document has no envelope, and the caller still needs the
    /// session to ask the turn again. A vendor that cannot resume reports none.
    pub fn reported_session(&self, stdout: &str) -> Option<String> {
        self.supports_resume().then(|| (self.session_reader)(stdout)).flatten()
    }

    /// Read one line of this vendor's stdout as an activity (EAC-FR-33).
    ///
    /// Total: every line yields an activity, including one that is not JSON and
    /// one carrying an event this build has never heard of. A vendor that adds
    /// an event must not make a run go quiet.
    pub fn activity(&self, line: &str) -> VendorActivity {
        (self.activity)(line, &|text| text.to_string())
    }

    /// EAC-FR-HTSN: the same reading, with each agent text block passed through
    /// `filter` before it reaches the summary. The filter is called once for
    /// each text block, in the order the vendor wrote them.
    pub fn activity_filtered(&self, line: &str, filter: TextFilter<'_>) -> VendorActivity {
        (self.activity)(line, filter)
    }
}
