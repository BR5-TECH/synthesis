//! The project's committed Docker image configuration
//! (`specifications/core/PSS-project-settings-storage.md` PSS-FR-22..PSS-FR-30).
//!
//! One entry per agentic CLI vendor, each an image name, an optional tag, and an
//! optional project-relative Dockerfile. It is **project-public**: the image a
//! project's agentic work runs in is a property of the project that everyone who
//! checks it out shares, so it is committed with `.synthesis/project.toml` and
//! resolves from the active worktree like every other project-public value
//! (PSS-FR-22). No Docker backend value is stored here — the mode, the endpoint,
//! and the CLI path are user-global and live in `crate::docker` (GSS-FR-35).
//!
//! A configured Dockerfile can be **built locally** (PSS-FR-26), through the
//! backend the author already verified and never through one this module probes
//! for. The build targets the entry's image reference, takes the project root as
//! its context, and pushes, publishes, and logs in to nothing: there is no code
//! path here that reaches a registry at all.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::docker::ResolvedDockerBackend;

// ---------------------------------------------------------------------------
// Vendors (PSS-FR-22)
// ---------------------------------------------------------------------------

/// The three agentic CLI vendors an entry can be configured for, in the stable
/// order `load_project_docker_images` returns them and the Docker section
/// renders them (`../ui/SET-project-settings.md` SET-FR-21).
pub const VENDORS: [&str; 3] = ["claude_code", "codex", "opencode"];

/// AIC-FR-33: the vendors this application can actually execute. `opencode` may
/// be configured, verified, and overridden to like any other CLI-kind vendor,
/// and it resolves no launch — so an image configured for it is an image
/// nothing can yet run.
pub const EXECUTABLE_VENDORS: [&str; 2] = ["claude_code", "codex"];

pub fn is_executable_vendor(vendor: &str) -> bool {
    EXECUTABLE_VENDORS.contains(&vendor)
}

fn is_known_vendor(vendor: &str) -> bool {
    VENDORS.contains(&vendor)
}

// ---------------------------------------------------------------------------
// Typed refusals
// ---------------------------------------------------------------------------

/// PSS-FR-23: an entry whose image name is empty once trimmed.
pub const ERR_IMAGE_NAME_EMPTY: &str = "image_name_empty";
/// PSS-FR-24: the three ways a configured Dockerfile path is refused.
pub const ERR_DOCKERFILE_ABSOLUTE: &str = "dockerfile_absolute";
pub const ERR_DOCKERFILE_ESCAPES_PROJECT_ROOT: &str = "dockerfile_escapes_project_root";
pub const ERR_DOCKERFILE_NOT_A_REGULAR_FILE: &str = "dockerfile_not_a_regular_file";
/// PSS-FR-26: a build asked for on a vendor that configures no Dockerfile.
pub const ERR_DOCKERFILE_UNSET: &str = "dockerfile_unset";
/// PSS-FR-29: a project builds one image at a time.
pub const ERR_BUILD_ALREADY_RUNNING: &str = "build_already_running";
/// PSS-FR-30: the project configures no usable image for this vendor.
pub const ERR_VENDOR_IMAGE_UNCONFIGURED: &str = "vendor_image_unconfigured";
/// A vendor id this application does not know.
pub const ERR_UNKNOWN_VENDOR: &str = "unknown_vendor";

/// The key the Docker image configuration lives under in
/// `.synthesis/project.toml` (PSS-FR-22).
pub const DOCKER_IMAGES_KEY: &str = "dockerImages";

// ---------------------------------------------------------------------------
// Wire shapes (PSS contract surface)
// ---------------------------------------------------------------------------

/// One vendor's committed entry, as it is written and as it sits on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectVendorImage {
    pub image_name: String,
    /// PSS-FR-23: optional. Where it is omitted the reference is the image name
    /// alone and Docker applies its own default tag, `latest`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// PSS-FR-24: optional, and project-relative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
}

impl ProjectVendorImage {
    /// PSS-FR-23: the single string a build targets and an execution resolves.
    /// Composed here so neither composes one of its own.
    pub fn image_reference(&self) -> Option<String> {
        let name = self.image_name.trim();
        if name.is_empty() {
            return None;
        }
        match self.tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(tag) => Some(format!("{name}:{tag}")),
            None => Some(name.to_string()),
        }
    }

    fn normalised(&self) -> ProjectVendorImage {
        ProjectVendorImage {
            image_name: self.image_name.trim().to_string(),
            tag: self
                .tag
                .as_ref()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty()),
            dockerfile: self
                .dockerfile
                .as_ref()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty()),
        }
    }
}

/// PSS-FR-25: whether the committed project holds an entry for this vendor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorImageConfiguration {
    #[default]
    Unset,
    Configured,
}

/// PSS-FR-25: an **absent** optional Dockerfile, told apart from a configured
/// one that is **invalid** for this project. The distinction is the whole point
/// of the field: the first configures no build and is not a problem, the second
/// is one the author corrects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerfileState {
    #[default]
    Absent,
    Valid,
    Invalid,
}

/// PSS-FR-25: which of PSS-FR-24's three refusals a configured Dockerfile fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerfileProblem {
    AbsolutePath,
    EscapesProjectRoot,
    NotARegularFile,
}

impl DockerfileProblem {
    /// The typed refusal a save returns for this problem (PSS-FR-24).
    pub fn refusal(self) -> &'static str {
        match self {
            DockerfileProblem::AbsolutePath => ERR_DOCKERFILE_ABSOLUTE,
            DockerfileProblem::EscapesProjectRoot => ERR_DOCKERFILE_ESCAPES_PROJECT_ROOT,
            DockerfileProblem::NotARegularFile => ERR_DOCKERFILE_NOT_A_REGULAR_FILE,
        }
    }
}

/// PSS-FR-25: what this vendor's entry is worth to a graduation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorGraduationState {
    Usable,
    #[default]
    ImageNameMissing,
    DockerfileInvalid,
    /// `opencode`: the entry may be perfectly well configured, and it is
    /// execution support that is missing (AIC-FR-33).
    ExecutionUnsupported,
}

/// PSS-FR-25: one vendor's entry and what it is worth, in fields a surface can
/// render without a second opinion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVendorImageStatus {
    pub vendor: String,
    pub configuration: VendorImageConfiguration,
    pub image_name: String,
    pub tag: Option<String>,
    pub image_reference: Option<String>,
    pub dockerfile: Option<String>,
    pub dockerfile_state: DockerfileState,
    pub dockerfile_problem: Option<DockerfileProblem>,
    pub graduation_state: VendorGraduationState,
}

/// What `build_project_vendor_image` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedImageBuild {
    pub operation_id: String,
}

/// PSS-FR-29: the build holding the project's one build slot right now, if any.
///
/// It exists because a build outlives the surface that started it: a section
/// that is left and returned to reads this rather than forgetting that a build
/// is running, which is what keeps it rendered against its own tab and keeps
/// every Build control disabled while it runs
/// (`../ui/SET-project-settings.md` SET-FR-25, SET-FR-26).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InFlightImageBuild {
    pub vendor: String,
    pub operation_id: String,
}

// ---------------------------------------------------------------------------
// Events (PSS contract surface)
// ---------------------------------------------------------------------------

/// PSS-FR-27: emitted while a build runs.
pub const PROJECT_IMAGE_BUILD_PROGRESS: &str = "project-image-build-progress";
/// PSS-FR-28: emitted exactly once per build.
pub const PROJECT_IMAGE_BUILD_FINISHED: &str = "project-image-build-finished";

/// PSS-FR-27: the vendor, the operation id, the phase, the optional completed
/// and total values, and a message written to be displayed to the author.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageBuildProgress {
    pub vendor: String,
    pub operation_id: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    pub message: String,
}

/// PSS-FR-28: exactly one of these per build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageBuildOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageBuildFinished {
    pub vendor: String,
    pub operation_id: String,
    pub outcome: ImageBuildOutcome,
    /// Present on a success: the reference that was built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_reference: Option<String>,
    /// Present on a failure: a safe, user-displayable account of what the
    /// backend reported (PSS-FR-28).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

// ---------------------------------------------------------------------------
// Reading and writing the entries (PSS-FR-22)
// ---------------------------------------------------------------------------

/// The entries as the committed project holds them, keyed by vendor.
///
/// A vendor the file names that this build does not know is dropped on the way
/// in rather than surfaced: a `project.toml` written by a later build must not
/// make this section unreadable, exactly as an unknown key elsewhere in the
/// store does not.
pub fn read_entries(table: &toml::Table) -> BTreeMap<String, ProjectVendorImage> {
    let mut out = BTreeMap::new();
    let Some(images) = table.get(DOCKER_IMAGES_KEY).and_then(|v| v.as_table()) else {
        return out;
    };
    for vendor in VENDORS {
        let Some(value) = images.get(vendor) else {
            continue;
        };
        if let Ok(entry) = value.clone().try_into::<ProjectVendorImage>() {
            out.insert(vendor.to_string(), entry.normalised());
        }
    }
    out
}

/// Write one vendor's entry into the project-public table, carrying every other
/// vendor's entry and every other section through unchanged (PSS-FR-22).
pub fn write_entry(table: &mut toml::Table, vendor: &str, entry: &ProjectVendorImage) {
    let images = table
        .entry(DOCKER_IMAGES_KEY.to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !images.is_table() {
        *images = toml::Value::Table(toml::Table::new());
    }
    let images = images.as_table_mut().expect("just made a table");
    match toml::Value::try_from(entry.normalised()) {
        Ok(value) => {
            images.insert(vendor.to_string(), value);
        }
        Err(_) => {
            // Nothing in the entry can fail to encode — three strings — but a
            // failure here must not leave a half-written section behind.
            images.remove(vendor);
        }
    }
}

// ---------------------------------------------------------------------------
// Dockerfile validation (PSS-FR-24)
// ---------------------------------------------------------------------------

/// Does this project-relative path stay inside the project once its `.` and
/// `..` components are applied?
///
/// Lexical, and deliberately so: a path is refused for *escaping* before
/// anything on disk is consulted, so a `../secrets/Dockerfile` is refused the
/// same way whether or not it happens to exist.
fn escapes_root(relative: &Path) -> bool {
    let mut depth: i32 = 0;
    for component in relative.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::CurDir => {}
            Component::Normal(_) => depth += 1,
            // A prefix or a root component means the path was not relative at
            // all, which the absolute check above already refused.
            Component::Prefix(_) | Component::RootDir => return true,
        }
    }
    false
}

/// PSS-FR-24: validate a configured Dockerfile path against the project root.
///
/// The disk-touching half goes through the governed instance rather than a bare
/// path predicate, so the check is subject to the same allowlist every other
/// read in the application is.
pub fn validate_dockerfile(
    root: &crate::fs::RootFs,
    dockerfile: &str,
) -> Result<(), DockerfileProblem> {
    let path = Path::new(dockerfile);
    if path.is_absolute() {
        return Err(DockerfileProblem::AbsolutePath);
    }
    if escapes_root(path) {
        return Err(DockerfileProblem::EscapesProjectRoot);
    }
    match root.access().file_info(root.path().join(path)) {
        Ok(info) if info.kind == crate::fs::EntryKind::File => Ok(()),
        // Absent, a directory, a symlink, or a path the instance refuses: none
        // of them identifies a regular file, which is the whole of what
        // PSS-FR-24 asks of it.
        _ => Err(DockerfileProblem::NotARegularFile),
    }
}

/// PSS-FR-25: one vendor's status, computed from its entry and the project.
///
/// Reads only: a Dockerfile deleted since it was configured is reported
/// `invalid` rather than removed from the entry.
pub fn status_for(
    root: &crate::fs::RootFs,
    vendor: &str,
    entry: Option<&ProjectVendorImage>,
) -> ProjectVendorImageStatus {
    let Some(entry) = entry else {
        return ProjectVendorImageStatus {
            vendor: vendor.to_string(),
            configuration: VendorImageConfiguration::Unset,
            image_name: String::new(),
            tag: None,
            image_reference: None,
            dockerfile: None,
            dockerfile_state: DockerfileState::Absent,
            dockerfile_problem: None,
            graduation_state: if is_executable_vendor(vendor) {
                VendorGraduationState::ImageNameMissing
            } else {
                VendorGraduationState::ExecutionUnsupported
            },
        };
    };

    let (dockerfile_state, dockerfile_problem) = match entry.dockerfile.as_deref() {
        None => (DockerfileState::Absent, None),
        Some(path) => match validate_dockerfile(root, path) {
            Ok(()) => (DockerfileState::Valid, None),
            Err(problem) => (DockerfileState::Invalid, Some(problem)),
        },
    };

    let image_reference = entry.image_reference();
    let graduation_state = if !is_executable_vendor(vendor) {
        // Checked first: an OpenCode entry that is missing an image name is
        // still an OpenCode entry, and what the author needs to be told is that
        // execution support does not exist rather than that the entry is wrong.
        VendorGraduationState::ExecutionUnsupported
    } else if image_reference.is_none() {
        VendorGraduationState::ImageNameMissing
    } else if dockerfile_state == DockerfileState::Invalid {
        VendorGraduationState::DockerfileInvalid
    } else {
        VendorGraduationState::Usable
    };

    ProjectVendorImageStatus {
        vendor: vendor.to_string(),
        configuration: VendorImageConfiguration::Configured,
        image_name: entry.image_name.clone(),
        tag: entry.tag.clone(),
        image_reference,
        dockerfile: entry.dockerfile.clone(),
        dockerfile_state,
        dockerfile_problem,
        graduation_state,
    }
}

/// PSS-FR-24: the validation a save performs before it writes anything.
pub fn validate_entry(
    root: &crate::fs::RootFs,
    entry: &ProjectVendorImage,
) -> Result<ProjectVendorImage, String> {
    let entry = entry.normalised();
    // The image name is required, and it is required on an entry that
    // configures a Dockerfile for the same reason it is required on one that
    // does not: it is the build target and the reference graduation runs.
    if entry.image_name.is_empty() {
        return Err(ERR_IMAGE_NAME_EMPTY.to_string());
    }
    if let Some(dockerfile) = entry.dockerfile.as_deref() {
        validate_dockerfile(root, dockerfile).map_err(|p| p.refusal().to_string())?;
    }
    Ok(entry)
}

// ---------------------------------------------------------------------------
// The build (PSS-FR-26..PSS-FR-29)
// ---------------------------------------------------------------------------

/// The `docker build` flags, named rather than spelled at the call site so a
/// test asserts the exact vector — and so that the absence of `--push` is a
/// property of one list rather than of a string somebody assembled (PSS-FR-26).
pub const BUILD_SUBCOMMAND: &str = "build";
pub const BUILD_FILE_FLAG: &str = "--file";
pub const BUILD_TAG_FLAG: &str = "--tag";

/// PSS-FR-26: the complete Docker CLI argument vector for a local build.
///
/// `docker build --file <project-relative-Dockerfile> --tag <image[:tag]> <project-root>`
/// and nothing else. There is no `--push`, no `login`, and no publish
/// subcommand anywhere in this module, so a build cannot reach a registry by
/// any argument this function could be asked to produce.
pub fn docker_build_args(dockerfile: &str, image_reference: &str, context_root: &Path) -> Vec<String> {
    vec![
        BUILD_SUBCOMMAND.to_string(),
        BUILD_FILE_FLAG.to_string(),
        dockerfile.to_string(),
        BUILD_TAG_FLAG.to_string(),
        image_reference.to_string(),
        context_root.to_string_lossy().to_string(),
    ]
}

/// One build, fully described.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageBuildRequest {
    pub vendor: String,
    pub operation_id: String,
    pub image_reference: String,
    /// Project-relative, exactly as the entry records it.
    pub dockerfile: String,
    /// The build context: the project root (PSS-FR-26).
    pub context_root: PathBuf,
}

/// Where a build reports what it is doing, and where it asks whether it has been
/// cancelled.
///
/// Reporting never blocks the build: an implementation of this trait publishes
/// and returns rather than waiting on a consumer (PSS-FR-27).
pub trait ImageBuildSink: Send + Sync {
    fn progress(&self, phase: &str, completed: Option<u64>, total: Option<u64>, message: &str);
    /// PSS-FR-28: a cancellation is explicit — it follows
    /// `cancel_project_vendor_image_build` and nothing else.
    fn cancelled(&self) -> bool;
}

/// PSS-FR-28: a build has exactly one of these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildTerminal {
    Succeeded,
    Failed(String),
    Cancelled,
}

/// Performing a build, behind a seam.
///
/// Production dispatches on the resolved backend ([`RealImageBuilder`]); a test
/// binds a scripted builder, which is what lets every decision here — the
/// refusals, the single-build rule, the events, and the terminal result — be
/// exercised on a machine with no Docker at all.
pub trait ImageBuilder: Send + Sync {
    fn build(
        &self,
        request: &ImageBuildRequest,
        backend: &ResolvedDockerBackend,
        sink: &dyn ImageBuildSink,
    ) -> BuildTerminal;
}

/// PSS-FR-29: the one build a project may have in flight.
#[derive(Default)]
pub struct ImageBuildRegistry {
    running: Mutex<Option<RunningBuild>>,
}

struct RunningBuild {
    operation_id: String,
    vendor: String,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl ImageBuildRegistry {
    /// Claim the project for a build, or refuse (PSS-FR-29).
    pub fn claim(
        &self,
        vendor: &str,
        operation_id: &str,
    ) -> Result<Arc<std::sync::atomic::AtomicBool>, String> {
        let mut running = self.running.lock().map_err(|_| "build registry poisoned".to_string())?;
        if running.is_some() {
            return Err(ERR_BUILD_ALREADY_RUNNING.to_string());
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        *running = Some(RunningBuild {
            operation_id: operation_id.to_string(),
            vendor: vendor.to_string(),
            cancel: cancel.clone(),
        });
        Ok(cancel)
    }

    /// PSS-FR-29: a terminal result of any kind releases the project at once,
    /// so a failed build neither blocks nor delays a later one.
    pub fn release(&self, operation_id: &str) {
        if let Ok(mut running) = self.running.lock() {
            if running.as_ref().is_some_and(|r| r.operation_id == operation_id) {
                *running = None;
            }
        }
    }

    /// Ask the named build to stop. Unknown ids are ignored: a cancellation of a
    /// build that has already reached its terminal result is not an error, and
    /// its result stands.
    pub fn cancel(&self, operation_id: &str) {
        if let Ok(running) = self.running.lock() {
            if let Some(build) = running.as_ref() {
                if build.operation_id == operation_id {
                    build.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
    }

    /// The build in flight, if any — the vendor and its operation id, which is
    /// what a surface re-entering the section renders against its own tab.
    pub fn in_flight(&self) -> Option<InFlightImageBuild> {
        self.running.lock().ok().and_then(|r| {
            r.as_ref().map(|b| InFlightImageBuild {
                vendor: b.vendor.clone(),
                operation_id: b.operation_id.clone(),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Reading a backend's output into progress (PSS-FR-27)
// ---------------------------------------------------------------------------

/// A line of Docker's own build output, read into the phase and message a
/// surface renders.
///
/// `Step 4/17 : RUN …` is the one shape both backends emit that carries a
/// position, and it is what turns an indeterminate build into a determinate one
/// (PSS-FR-27). Everything else is a message under the phase that last held.
pub fn read_build_line(line: &str) -> (Option<(u64, u64)>, String) {
    let trimmed = line.trim_end_matches(['\r', '\n']).trim();
    let position = trimmed
        .strip_prefix("Step ")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|token| token.split_once('/'))
        .and_then(|(done, total)| {
            let done = done.trim().parse::<u64>().ok()?;
            let total = total.trim().parse::<u64>().ok()?;
            (total > 0 && done <= total).then_some((done, total))
        });
    (position, trimmed.to_string())
}

/// The phase a build reports while it has not yet been told a position.
pub const PHASE_BUILDING: &str = "building";
/// The phase the build context is packed under, which the Docker Engine backend
/// does for itself where the CLI hands the daemon a directory.
pub const PHASE_CONTEXT: &str = "context";

/// Where a build's two events go.
///
/// Implemented by the `AppHandle` in production and by a collecting stub in the
/// tests, so the orchestration — the refusals, the single-build rule, and the
/// one terminal result — is exercised without a Tauri runtime.
pub trait BuildEvents: Send + Sync {
    fn progress(&self, payload: ImageBuildProgress);
    fn finished(&self, payload: ImageBuildFinished);
}

/// A build that has passed every check and holds the project's one build slot.
///
/// Constructing one is what claims the slot, so a caller cannot prepare a build
/// and then forget to run it without releasing what it took — the runner
/// releases on every path out (PSS-FR-29).
#[derive(Debug)]
pub struct PreparedBuild {
    pub request: ImageBuildRequest,
    pub backend: ResolvedDockerBackend,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

/// The sink a running build reports through: it publishes and returns rather
/// than waiting on a consumer, so reporting never blocks the build (PSS-FR-27).
struct EventSink<'a> {
    vendor: String,
    operation_id: String,
    events: &'a dyn BuildEvents,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl ImageBuildSink for EventSink<'_> {
    fn progress(&self, phase: &str, completed: Option<u64>, total: Option<u64>, message: &str) {
        self.events.progress(ImageBuildProgress {
            vendor: self.vendor.clone(),
            operation_id: self.operation_id.clone(),
            phase: phase.to_string(),
            completed,
            total,
            message: message.to_string(),
        });
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Run a prepared build to its **one** terminal result (PSS-FR-28), releasing
/// the project's build slot however it ends (PSS-FR-29).
pub fn run_prepared_build(
    prepared: PreparedBuild,
    builder: &dyn ImageBuilder,
    events: &dyn BuildEvents,
    registry: &ImageBuildRegistry,
) -> BuildTerminal {
    let sink = EventSink {
        vendor: prepared.request.vendor.clone(),
        operation_id: prepared.request.operation_id.clone(),
        events,
        cancel: prepared.cancel.clone(),
    };
    // The first update, before the backend has said anything: a build starts as
    // an indeterminate operation (PSS-FR-27).
    sink.progress(PHASE_BUILDING, None, None, "starting the build");

    let terminal = builder.build(&prepared.request, &prepared.backend, &sink);

    // Released before the terminal event, so a surface that starts another
    // build the moment it reads the result is not refused by a slot this build
    // still held.
    registry.release(&prepared.request.operation_id);

    let finished = match &terminal {
        BuildTerminal::Succeeded => ImageBuildFinished {
            vendor: prepared.request.vendor.clone(),
            operation_id: prepared.request.operation_id.clone(),
            outcome: ImageBuildOutcome::Succeeded,
            image_reference: Some(prepared.request.image_reference.clone()),
            diagnostic: None,
        },
        BuildTerminal::Failed(diagnostic) => ImageBuildFinished {
            vendor: prepared.request.vendor.clone(),
            operation_id: prepared.request.operation_id.clone(),
            outcome: ImageBuildOutcome::Failed,
            image_reference: None,
            diagnostic: Some(diagnostic.clone()),
        },
        BuildTerminal::Cancelled => ImageBuildFinished {
            vendor: prepared.request.vendor.clone(),
            operation_id: prepared.request.operation_id.clone(),
            outcome: ImageBuildOutcome::Cancelled,
            image_reference: None,
            diagnostic: None,
        },
    };
    events.finished(finished);
    terminal
}

/// Prepare a build: every refusal of PSS-FR-26 and PSS-FR-29, in the order the
/// author can act on them, and the project's build slot claimed last so a
/// refusal never takes it.
pub fn prepare_build(
    entry: Option<&ProjectVendorImage>,
    backend: Result<ResolvedDockerBackend, String>,
    context_root: PathBuf,
    vendor: &str,
    operation_id: &str,
    registry: &ImageBuildRegistry,
) -> Result<PreparedBuild, String> {
    if !is_known_vendor(vendor) {
        return Err(ERR_UNKNOWN_VENDOR.to_string());
    }
    let entry = entry.ok_or_else(|| ERR_IMAGE_NAME_EMPTY.to_string())?;
    let image_reference = entry
        .image_reference()
        .ok_or_else(|| ERR_IMAGE_NAME_EMPTY.to_string())?;
    let dockerfile = entry
        .dockerfile
        .clone()
        .ok_or_else(|| ERR_DOCKERFILE_UNSET.to_string())?;
    // GSS-FR-40: an unverified backend refuses and starts nothing. Read rather
    // than probed, so a daemon that is merely stopped is the build's own
    // failure and not this refusal.
    let backend = backend?;
    let cancel = registry.claim(vendor, operation_id)?;
    Ok(PreparedBuild {
        request: ImageBuildRequest {
            vendor: vendor.to_string(),
            operation_id: operation_id.to_string(),
            image_reference,
            dockerfile,
            context_root,
        },
        backend,
        cancel,
    })
}

#[cfg(test)]
mod tests;
