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
//! any of them, so upgrading a vendor CLI is a change to that manifest and a
//! rebuild — not an edit to this file.
//!
//! The manifest is `include_str!`'d rather than read at run time: it is
//! application data compiled into the binary, so it cannot be swapped by
//! whoever can write next to the executable, and a malformed manifest fails the
//! build rather than the first launch.

#[cfg(test)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::OnceLock;

#[cfg(test)]
use serde::Deserialize;

use super::protocol::{AgentResponseEnvelope, EnvelopeInvalid, ResultContract};

pub mod claude_code;
pub mod codex;

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
        let repository = self
            .image_ref
            .rsplit_once(':')
            .map_or(self.image_ref.as_str(), |(repo, _)| repo);
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
/// The environment variable Claude Code reads its OAuth token from (EAC-FR-15).
pub const CLAUDE_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

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
    JsonlLastItem {
        event_type: &'static str,
        item_type: &'static str,
        text_key: &'static str,
    },
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
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
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
    ReportedByEvent {
        event_type: &'static str,
        key: &'static str,
    },
}

/// How a vendor's session state survives the container (EAC-FR-31).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStateMount {
    /// A Synthesis-owned directory mounted at `target` and addressed by `env`.
    /// The credential arrives separately, so this mount carries transcripts and
    /// nothing else.
    Dedicated {
        env: &'static str,
        target: &'static str,
    },
    /// The vendor's credential mount *is* its session directory, so the two
    /// coincide and that mount is read/write. Its target is whatever AIC
    /// supplied, so there is none to name here.
    SharedWithCredentialMount { env: &'static str },
}

/// How a vendor's credential reaches its container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialForm {
    /// A named environment variable, whose value never reaches an argv.
    EnvironmentVariable(&'static str),
    /// A directory mounted at the target AIC supplied.
    Mount,
}

/// What extraction is handed.
pub struct ExtractContext<'a> {
    pub stdout: &'a str,
    /// The identifier the executor assigned before the run, for a vendor that
    /// permits one. `None` on a resumed turn and for a vendor that does not.
    pub assigned_session_id: Option<&'a str>,
}

/// What extraction produces.
pub struct Extracted {
    pub envelope: AgentResponseEnvelope,
    /// Whatever session identity the run itself reported, which is what makes a
    /// later resume possible.
    pub session_id: Option<String>,
}

/// The whole vendor argument vector for a fresh turn.
///
/// EAC-FR-43: the result contract the task named is handed to every vendor.
/// What a vendor does with it is that vendor's own protocol — Claude Code
/// enforces it inside the run (CCP-FR-28), Codex carries it in the task document
/// alone (CDX-FR-14).
type FreshArgv = fn(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    session_id: &str,
    result_contract: Option<ResultContract>,
) -> Vec<String>;
/// The whole vendor argument vector for a resumed turn. Generated separately
/// rather than derived from the fresh one, because a resumed turn is not a fresh
/// turn with an addition (CDX-FR-09).
type ResumeArgv = fn(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    resume_session_id: &str,
    result_contract: Option<ResultContract>,
) -> Vec<String>;
type Extractor = fn(&ExtractContext<'_>) -> Result<Extracted, EnvelopeInvalid>;
type EffortSupported = fn(&str) -> bool;
/// One line of this vendor's stdout, read as an activity (EAC-FR-33).
type ActivityReader = fn(&str) -> VendorActivity;

/// Everything one vendor's invocation needs, as immutable application data
/// transcribed from that vendor's own protocol specification (EAC-FR-10).
#[derive(Clone, Copy)]
pub struct VendorExecutionDescriptor {
    pub vendor: &'static str,
    /// The specification this descriptor transcribes. Named so that a reader who
    /// finds a flag here knows which document justifies it.
    pub protocol_spec: &'static str,
    /// The CLI version that specification was asserted against, compared with
    /// the manifest's own `cli_version` by a test.
    pub pinned_cli_version: &'static str,
    fresh_argv: FreshArgv,
    resume_argv: Option<ResumeArgv>,
    effort_supported: Option<EffortSupported>,
    extractor: Extractor,
    activity: ActivityReader,
    /// EAC-FR-19's ordered positions, as data.
    pub response_sources: &'static [ResponseSource],
    pub session_source: SessionSource,
    pub session_state: SessionStateMount,
    pub credential_form: CredentialForm,
}

impl std::fmt::Debug for VendorExecutionDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VendorExecutionDescriptor")
            .field("vendor", &self.vendor)
            .field("protocol_spec", &self.protocol_spec)
            .field("pinned_cli_version", &self.pinned_cli_version)
            .field("response_sources", &self.response_sources)
            .field("session_source", &self.session_source)
            .field("session_state", &self.session_state)
            .field("credential_form", &self.credential_form)
            .finish()
    }
}

/// EAC-FR-11: Claude Code's descriptor, transcribing `CCP`.
pub static CLAUDE_CODE: VendorExecutionDescriptor = VendorExecutionDescriptor {
    vendor: "claude_code",
    protocol_spec: claude_code::SPEC,
    pinned_cli_version: claude_code::PINNED_CLI_VERSION,
    fresh_argv: claude_code::fresh_argv,
    resume_argv: Some(claude_code::resume_argv),
    effort_supported: Some(claude_code::effort_supported),
    extractor: claude_code::extract,
    activity: claude_code::activity,
    // CCP-FR-09: the schema-validated object first, the text form second.
    response_sources: &[
        ResponseSource::JsonObjectField {
            key: "structured_output",
        },
        ResponseSource::JsonStringField { key: "result" },
    ],
    // CCP-FR-16: assigned before the run, asserted back afterwards.
    session_source: SessionSource::AssignedThenAsserted { key: "session_id" },
    session_state: SessionStateMount::Dedicated {
        env: claude_code::SESSION_STATE_ENV,
        target: claude_code::SESSION_STATE_TARGET,
    },
    credential_form: CredentialForm::EnvironmentVariable(CLAUDE_TOKEN_ENV),
};

/// EAC-FR-12: Codex's descriptor, transcribing `CDX`.
pub static CODEX: VendorExecutionDescriptor = VendorExecutionDescriptor {
    vendor: "codex",
    protocol_spec: codex::SPEC,
    pinned_cli_version: codex::PINNED_CLI_VERSION,
    // This vendor cannot be told which session id to use for a new session, so
    // the assigned identifier it is handed is ignored (CDX-FR-20).
    // CDX-FR-14: this vendor's CLI enforces no schema, so a named result
    // contract changes nothing about its vector; the document reaches its agent
    // in the task alone.
    fresh_argv: |model, effort, _assigned, _contract| codex::fresh_argv(model, effort),
    resume_argv: Some(|model, effort, session, _contract| codex::resume_argv(model, effort, session)),
    // The CLI validates the identifier itself; what this checks is only that the
    // value can be carried inside a quoted config override without changing what
    // the override means.
    effort_supported: Some(codex::effort_supported),
    extractor: codex::extract,
    activity: codex::activity,
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
    // CDX-FR-23: login directory and session directory are the same directory.
    session_state: SessionStateMount::SharedWithCredentialMount {
        env: codex::SESSION_STATE_ENV,
    },
    credential_form: CredentialForm::Mount,
};

pub fn descriptor_for(vendor: &str) -> Option<&'static VendorExecutionDescriptor> {
    match vendor {
        "claude_code" => Some(&CLAUDE_CODE),
        "codex" => Some(&CODEX),
        _ => None,
    }
}

impl VendorExecutionDescriptor {
    /// The complete vendor argument vector for one turn.
    ///
    /// A fresh turn and a resumed turn are generated by different functions
    /// rather than by one with a branch, because for at least one pinned vendor
    /// they are different grammars accepting different options (CDX-FR-09).
    pub fn vendor_args(
        &self,
        model_id: Option<&str>,
        effort_id: Option<&str>,
        assigned_session_id: &str,
        resume_session: Option<&str>,
        result_contract: Option<ResultContract>,
    ) -> Vec<String> {
        match resume_session {
            Some(session) => match self.resume_argv {
                Some(build) => build(model_id, effort_id, session, result_contract),
                // Unreachable in practice: a vendor that cannot resume is
                // refused before launch (EAC-FR-30). Falling back to a fresh
                // vector rather than panicking keeps a programming error from
                // taking the process down.
                None => {
                    (self.fresh_argv)(model_id, effort_id, assigned_session_id, result_contract)
                }
            },
            None => (self.fresh_argv)(model_id, effort_id, assigned_session_id, result_contract),
        }
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
        let extracted = (self.extractor)(&ExtractContext {
            stdout,
            assigned_session_id,
        })?;
        Ok((extracted.envelope, extracted.session_id))
    }

    /// Read one line of this vendor's stdout as an activity (EAC-FR-33).
    ///
    /// Total: every line yields an activity, including one that is not JSON and
    /// one carrying an event this build has never heard of. A vendor that adds
    /// an event must not make a run go quiet.
    pub fn activity(&self, line: &str) -> VendorActivity {
        (self.activity)(line)
    }
}

// ---------------------------------------------------------------------------
// The Docker invocation
// ---------------------------------------------------------------------------

/// What one container launch needs, as a mount the invocation will express.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindMount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

impl BindMount {
    fn as_argument(&self) -> String {
        let mut value = format!("type=bind,source={},target={}", self.source, self.target);
        if self.read_only {
            value.push_str(",readonly");
        }
        value
    }
}

/// Build the complete `docker run` argument vector (EAC-FR-13).
///
/// `env_names` names variables whose *values* the runtime takes from the docker
/// client's own environment. That indirection is the whole reason Claude's
/// token never reaches an argument vector (EAC-FR-15): `--env NAME` is a name,
/// and a name is not a secret.
///
/// `env_literals` carry their values in the argv instead, which is correct only
/// because every one of them is a container path this application chose — a
/// mount target, never a secret. The two mechanisms are separate so that
/// "forwarded by name" and "written in the vector" are decisions a reader makes
/// per variable rather than one the function makes for all of them.
pub fn docker_run_args(
    container_name: &str,
    image: &str,
    host_uid: u32,
    host_gid: u32,
    workdir: &str,
    mounts: &[BindMount],
    env_names: &[&str],
    env_literals: &[(String, String)],
    vendor_args: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        // EAC-FR-26: removed when the agent exits, on every path.
        "--rm".into(),
        // EAC-FR-09: the task reaches the agent on the container's stdin and
        // nowhere else, and `docker run` connects the client's stdin to the
        // container only when asked. Without this the payload is written to a
        // stream nothing reads, and both pinned CLIs — each of which takes its
        // whole prompt from stdin — exit non-zero on an empty one. Not `--tty`:
        // a terminal would let the CLI print progress decoration into the very
        // stream EAC-FR-19 parses for an envelope.
        "--interactive".into(),
        "--name".into(),
        container_name.into(),
        "--network".into(),
        NETWORK.into(),
        // EAC-FR-14: the container's non-root user is mapped to the host user,
        // so a file the agent creates in the mount belongs to whoever launched
        // it rather than to a uid that exists only inside the container.
        "--user".into(),
        format!("{host_uid}:{host_gid}"),
        "--workdir".into(),
        workdir.into(),
    ];

    for option in SECURITY_OPTS {
        args.push((*option).to_string());
    }
    for mount in mounts {
        args.push("--mount".into());
        args.push(mount.as_argument());
    }
    for name in env_names {
        args.push("--env".into());
        args.push((*name).to_string());
    }
    for (name, value) in env_literals {
        args.push("--env".into());
        args.push(format!("{name}={value}"));
    }

    args.push(image.to_string());
    args.extend(vendor_args.iter().cloned());
    args
}

/// EAC-FR-10 / EAC-FR-39: one container launch, described whole.
///
/// Every field is a named constant, a field of the vendor's descriptor, or the
/// one resolved value that is not — the image, which is the project's own
/// (EAC-FR-38). It exists because two backends have to create the *same*
/// container: the Docker CLI backend renders it into the `docker run` vector
/// below, and the Docker Engine backend creates and starts it over Bollard's
/// connection. Neither composes a parameter of its own, so which backend a
/// machine uses changes how Docker is reached and nothing about what the agent
/// runs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub host_uid: u32,
    pub host_gid: u32,
    /// EAC-FR-ZKMR: the directory the turn stands in — the execution
    /// directory's own host path, or [`WORKSPACE_TARGET`] in the fallback shape.
    pub workdir: String,
    pub mounts: Vec<BindMount>,
    /// Variables the runtime forwards **by name**, taking their values from the
    /// client's own environment, so a secret never reaches an argument vector
    /// (EAC-FR-15).
    pub env_names: Vec<String>,
    /// Variables carried with their values, every one of them a container path
    /// this application chose rather than a secret.
    pub env_literals: Vec<(String, String)>,
    pub vendor_args: Vec<String>,
}

impl ContainerSpec {
    /// The Docker CLI backend's rendering of this specification.
    pub fn docker_run_args(&self) -> Vec<String> {
        let names: Vec<&str> = self.env_names.iter().map(String::as_str).collect();
        docker_run_args(
            &self.name,
            &self.image,
            self.host_uid,
            self.host_gid,
            &self.workdir,
            &self.mounts,
            &names,
            &self.env_literals,
            &self.vendor_args,
        )
    }

    /// `--user` as the Engine API expects it, which is the same string the CLI
    /// vector carries.
    pub fn user(&self) -> String {
        format!("{}:{}", self.host_uid, self.host_gid)
    }

    /// The security options of [`SECURITY_OPTS`], as the Engine API's own
    /// `HostConfig` spells them: `--cap-drop ALL` is a dropped capability list
    /// and `--security-opt no-new-privileges` is a security option, where the
    /// CLI takes both as flags.
    pub fn dropped_capabilities(&self) -> Vec<String> {
        vec!["ALL".to_string()]
    }

    pub fn security_options(&self) -> Vec<String> {
        vec!["no-new-privileges".to_string()]
    }
}

/// A container name no other request has used or will reuse (EAC-FR-13).
///
/// Process id, a monotonic counter, and the nanoseconds since the epoch
/// together: the counter separates two launches in one process, the pid
/// separates two processes, and the timestamp separates a relaunch that
/// happened to be given a recycled pid.
pub fn unique_container_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("synthesis-agent-{}-{}-{}", std::process::id(), nanos, sequence)
}

/// A session identifier for a vendor that lets the executor assign one
/// (EAC-FR-21, CCP-FR-16).
///
/// Shaped as a v4 UUID because the pinned CLI that accepts one requires that
/// shape. The bytes are derived from a SHA-256 over the process id, the
/// nanosecond clock, and a monotonic counter rather than from a random source:
/// what this value has to be is *unique*, not unguessable. It names a
/// conversation inside a container this application launched, it is not a
/// credential, and nothing authenticates with it — so a counter that cannot
/// repeat is exactly the property worth buying, and it is bought without adding
/// a dependency.
pub fn new_session_id() -> String {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());

    let mut hasher = Sha256::new();
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(nanos.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Version 4 and the RFC 4122 variant, so the value parses as the UUID the
    // CLI asks for rather than merely looking like one.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The host directory a vendor's session state lives in (EAC-FR-31).
///
/// Per vendor and per project: two projects driving the same vendor keep
/// separate transcripts, and one vendor's state is never visible to the other.
/// The project key is hashed rather than used as a path component — it is an
/// absolute path, so it cannot be one, and hashing also keeps a project's
/// location off the filesystem under `app_data_dir()`.
pub fn session_state_dir(root: &Path, vendor: &str, project_key: &str) -> PathBuf {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(project_key.as_bytes());
    let digest = hasher.finalize();
    let slug: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();

    root.join(vendor).join(slug)
}

/// EAC-FR-FNFV: a short, stable name for a path, which is what distinguishes the
/// generated files one execution's repository access is made of.
///
/// Those files are a function of the execution directory alone: two launches
/// against one directory write the same bytes, and two against different ones
/// write different files. Without the distinction they share one path, and one
/// launch replaces the other's between the write and the moment the runtime
/// resolves the mount — leaving a turn standing on a `.git` that names some
/// other working copy.
pub fn path_slug(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The host user the container runs as (EAC-FR-14).
#[cfg(unix)]
pub fn host_ids() -> (u32, u32) {
    // SAFETY: `getuid`/`getgid` cannot fail and touch no memory the caller owns.
    unsafe { (libc_getuid(), libc_getgid()) }
}

#[cfg(unix)]
extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
    #[link_name = "getgid"]
    fn libc_getgid() -> u32;
}

/// Windows has no uid/gid to map, and Docker Desktop handles ownership in the
/// mount layer, so the manifest's own ids are used unchanged.
#[cfg(not(unix))]
pub fn host_ids() -> (u32, u32) {
    (0, 0)
}

/// Whether a path is usable as a bind-mount source, as a string.
///
/// Docker's `--mount` value is comma-separated, so a source containing a comma
/// would be read as a second option. Rejecting it here turns a confusing
/// runtime failure into a typed refusal.
pub fn mount_source(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.contains(',') || text.is_empty() {
        return None;
    }
    Some(text.to_string())
}
