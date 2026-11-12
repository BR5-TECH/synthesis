//! OpenSpec mode (`specifications/core/OSM-openspec-mode.md`).
//!
//! Decides whether the open project is in OpenSpec mode, finds the OpenSpec
//! CLI on this machine, and gives the rest of the backend one typed seam that
//! reads capabilities, requirements, and changes through that CLI. The module
//! parses no OpenSpec Markdown: the CLI of the project does, so the OpenSpec
//! version and the schemas of the project decide what a change contains.
//!
//! The seam runs only `--version`, `list`, `show`, `status`, `validate`, and
//! `instructions` with the artifact `apply` alone, and never a subcommand that
//! writes (OSM-FR-VALR). The one write is `archive`, which only
//! `crate::openspec_archive` runs, on the process terms of this module
//! (OSM-FR-XKQB). Each call to the CLI is blocking and bounded; a caller on an
//! async path runs the seam on a blocking thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use serde::Serialize;

use crate::fs::RootFs;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

pub mod apply;
pub mod archive_cli;
pub mod change;
pub mod change_image;
pub mod cli;
pub mod detection;
pub mod failure;
mod process;
pub mod project_validation;
pub mod reads;
pub mod spec_index;

#[cfg(test)]
pub(crate) mod tests;

pub use apply::{ApplyInstructions, ApplyProgress, ApplyTask};
pub use archive_cli::{ArchiveAnswer, ArchiveCliFailure};
pub use change::{
    ChangeArtifact, ChangeDelta, ChangeDetail, ChangeFile, ChangeLocation, ChangeValidation,
    ValidationIssue,
};
pub use cli::{
    CliEnvironment, CliProbe, OpenSpecRunner, ProcessError, ProcessOutput, RecordedWait, RetryWait,
};
pub use failure::{
    classify_openspec_failure, CliDiagnostic, FailureClass, OpenSpecFailure, Refusal,
};
pub(crate) use process::{OnExitOne, RunFailure};
pub use project_validation::{ProjectValidation, ProjectValidationItem, ValidationScope};
pub use reads::{ArtifactState, Capability, CapabilitySpec, ChangeSummary, Requirement, Scenario};

// ---------------------------------------------------------------------------
// The contract surface (OSM contract surface)
// ---------------------------------------------------------------------------

/// OSM-FR-SKUP: the state of the OpenSpec CLI on this machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OpenSpecCliState {
    Available { path: String, version: String },
    NotFound,
    Unsupported { path: String, version: String },
    Failed { path: String, reason: CliFailure },
}

impl OpenSpecCliState {
    /// The stable code of the state, for a log record.
    pub fn code(&self) -> &'static str {
        match self {
            OpenSpecCliState::Available { .. } => "available",
            OpenSpecCliState::NotFound => "not_found",
            OpenSpecCliState::Unsupported { .. } => "unsupported",
            OpenSpecCliState::Failed { .. } => "failed",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, OpenSpecCliState::Available { .. })
    }
}

/// OSM-FR-SKUP: why a found CLI is in the `failed` state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliFailure {
    NotExecutable,
    TimedOut,
    NoVersion,
}

/// OSM-FR-YXUC: what `get_openspec_status` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecStatus {
    pub setting: String,
    pub uses_openspec: bool,
    pub active: bool,
    pub cli: OpenSpecCliState,
}

/// Why a read of the seam failed (OSM contract surface `OpenSpecReadError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenSpecReadError {
    NoOpenProject,
    OpenSpecModeInactive,
    CliUnavailable(OpenSpecCliState),
    InvalidCapabilityId,
    CapabilityNotFound,
    /// OSM-FR-ZIMO, OFP-FR-WNVO.
    CliTimedOut {
        diagnostics: Vec<CliDiagnostic>,
    },
    /// OSM-FR-WWWK: `None` for an exit by a signal, and for a process that
    /// could not start.
    CliFailed {
        exit_status: Option<i32>,
        diagnostics: Vec<CliDiagnostic>,
    },
    /// OSM-FR-WWWK, OFP-FR-NJSO: exit 1 with CLI JSON.
    CliRejected {
        diagnostics: Vec<CliDiagnostic>,
    },
    CliOutputUnreadable,
    /// OSM-FR-MFVN.
    UnknownStream,
    /// OSM-FR-IHUO.
    InvalidChangeName,
    /// OSM-FR-BCJO.
    ChangeNotFound,
    /// OSM-FR-KWWM: the change folder moved to `openspec/changes/archive/`.
    ChangeArchived {
        archived_as: String,
    },
}

impl OpenSpecReadError {
    /// The stable code of the error (OSM-FR-IFWM).
    pub fn code(&self) -> &'static str {
        match self {
            OpenSpecReadError::NoOpenProject => "no_open_project",
            OpenSpecReadError::OpenSpecModeInactive => failure::CODE_MODE_INACTIVE,
            OpenSpecReadError::CliUnavailable(_) => failure::CODE_CLI_UNAVAILABLE,
            OpenSpecReadError::InvalidCapabilityId => "invalid_capability_id",
            OpenSpecReadError::CapabilityNotFound => "capability_not_found",
            OpenSpecReadError::CliTimedOut { .. } => failure::CODE_TIMED_OUT,
            OpenSpecReadError::CliFailed { .. } => failure::CODE_FAILED,
            OpenSpecReadError::CliRejected { .. } => failure::CODE_REJECTED,
            OpenSpecReadError::CliOutputUnreadable => failure::CODE_OUTPUT_UNREADABLE,
            OpenSpecReadError::UnknownStream => "unknown_stream",
            OpenSpecReadError::InvalidChangeName => "invalid_change_name",
            OpenSpecReadError::ChangeNotFound => "change_not_found",
            OpenSpecReadError::ChangeArchived { .. } => "change_archived",
        }
    }

    /// OFP-FR-EOAD: the CLI state of `cli_unavailable`, the exit status of
    /// `cli_failed` (empty for a signal), and the archive folder of
    /// `change_archived`.
    pub fn detail(&self) -> Option<String> {
        match self {
            OpenSpecReadError::CliUnavailable(state) => Some(state.code().to_string()),
            OpenSpecReadError::CliFailed { exit_status, .. } => {
                Some(exit_status.map(|status| status.to_string()).unwrap_or_default())
            }
            OpenSpecReadError::ChangeArchived { archived_as } => Some(archived_as.clone()),
            _ => None,
        }
    }

    /// OFP-FR-OFLU: the diagnostics of a transient or a content failure, and
    /// none for every other.
    pub fn diagnostics(&self) -> &[CliDiagnostic] {
        match self {
            OpenSpecReadError::CliTimedOut { diagnostics }
            | OpenSpecReadError::CliFailed { diagnostics, .. }
            | OpenSpecReadError::CliRejected { diagnostics } => diagnostics,
            _ => &[],
        }
    }

    /// OFP-FR-QLQJ: the failure as a command rejects it.
    pub fn to_failure(&self) -> OpenSpecFailure {
        OpenSpecFailure::of_read(self)
    }
}

/// OFP-FR-MJZU: production waits for real; a build under test records the
/// delay and waits for nothing, so no test of the crate depends on time.
fn default_wait() -> Arc<dyn RetryWait> {
    #[cfg(test)]
    {
        Arc::new(cli::RecordedWait::default())
    }
    #[cfg(not(test))]
    {
        Arc::new(cli::RealWait)
    }
}

/// One kept version check (OSM-FR-ZNVZ).
#[derive(Clone, Debug)]
struct KeptCheck {
    modified: Option<SystemTime>,
    state: OpenSpecCliState,
    at: Instant,
}

// ---------------------------------------------------------------------------
// The managed state
// ---------------------------------------------------------------------------

/// The one OpenSpec seam of the application, managed by Tauri.
pub struct OpenSpec {
    runner: Arc<dyn OpenSpecRunner>,
    probe: Arc<dyn CliProbe>,
    env: CliEnvironment,
    /// OSM-FR-ZNVZ: the version check, kept per executable path and per
    /// modification time of that file, with the moment it ran.
    checks: Mutex<HashMap<PathBuf, KeptCheck>>,
    /// OSM-FR-ZNVZ: how long a `failed` check is kept.
    failed_ttl: std::time::Duration,
    slots: Arc<cli::Slots>,
    /// OFP-FR-MJZU: the wait before a transient process runs once more.
    wait: Arc<dyn RetryWait>,
}

impl Default for OpenSpec {
    fn default() -> Self {
        OpenSpec::new(
            Arc::new(cli::RealRunner),
            Arc::new(cli::RealCliProbe),
            CliEnvironment::from_process(),
        )
    }
}

impl OpenSpec {
    pub fn new(
        runner: Arc<dyn OpenSpecRunner>,
        probe: Arc<dyn CliProbe>,
        env: CliEnvironment,
    ) -> Self {
        OpenSpec {
            runner,
            probe,
            env,
            checks: Mutex::new(HashMap::new()),
            failed_ttl: cli::FAILED_CHECK_TTL,
            slots: Arc::new(cli::Slots::new(cli::MAX_CONCURRENT)),
            wait: default_wait(),
        }
    }

    /// The same seam with another wait seam (OFP-FR-MJZU).
    pub fn with_wait(mut self, wait: Arc<dyn RetryWait>) -> Self {
        self.wait = wait;
        self
    }

    /// The slot gate of the seam, so a test can look at it during a wait.
    #[cfg(test)]
    pub(crate) fn slots(&self) -> Arc<cli::Slots> {
        self.slots.clone()
    }

    /// OFP-FR-ITUD, OSM-FR-NQBE: an explicit act of the author clears every
    /// kept `failed` check, so the next check runs at once. A kept
    /// `available` or `unsupported` result stays.
    pub fn clear_failed_checks<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
    ) {
        let cleared = {
            let mut checks = self.lock_checks();
            let before = checks.len();
            checks.retain(|_, kept| !matches!(kept.state, OpenSpecCliState::Failed { .. }));
            before - checks.len()
        };
        logging::log_info(
            sink,
            buffer,
            &[Domain::Backend],
            "openspec cli check cleared",
            log_fields! { "cleared" => cleared },
        );
    }

    // -- Mode ------------------------------------------------------------

    /// PSS-FR-IMDN: the stored setting, `auto` where it is unset or where the
    /// store cannot be read.
    pub fn setting(root: &RootFs) -> String {
        crate::project_settings::load_project_config_from(root)
            .ok()
            .and_then(|config| config.openspec_mode)
            .unwrap_or_else(|| crate::project_settings::openspec_mode::AUTO.to_string())
    }

    /// OSM-FR-UILN: the setting is `auto` and the project uses OpenSpec.
    /// Detection runs again at each call (OSM-FR-JGYD).
    pub fn mode_active(root: Option<&RootFs>) -> bool {
        root.is_some_and(|root| {
            Self::setting(root) == crate::project_settings::openspec_mode::AUTO
                && detection::uses_openspec(root)
        })
    }

    /// CVL-FR-DPWP: whether a conversational turn gets the three OpenSpec read
    /// tools: the mode is active and the CLI is `available`.
    pub fn read_tools_attached<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> bool {
        Self::mode_active(root) && self.cli_state_in(sink, buffer, root).is_available()
    }

    /// CVL-FR-DPWP on the path of a turn: the mode is active and the kept CLI
    /// check says `available`. It starts no process: where no check is kept
    /// for the resolved CLI, the turn gets no read tools and `on_missing`
    /// runs, so the caller can start a check off the turn's thread.
    pub fn read_tools_attached_now(
        &self,
        root: Option<&RootFs>,
        on_missing: impl FnOnce(),
    ) -> bool {
        if !Self::mode_active(root) {
            return false;
        }
        match self.kept_cli_state() {
            Some(state) => state.is_available(),
            None => {
                on_missing();
                false
            }
        }
    }

    /// The CLI state that a kept check gives for the CLI as it resolves now,
    /// or `None` where no check is kept for that path and modification time.
    /// A CLI that resolves to nothing is `not_found`, which needs no check.
    pub fn kept_cli_state(&self) -> Option<OpenSpecCliState> {
        let Some(path) = cli::resolve(&self.env, self.probe.as_ref()) else {
            return Some(OpenSpecCliState::NotFound);
        };
        let modified = self.probe.modified(&path);
        self.lock_checks()
            .get(&path)
            .filter(|kept| self.still_kept(kept, modified))
            .map(|kept| kept.state.clone())
    }

    /// OSM-FR-ZNVZ: a check stands for its path and modification time, and a
    /// `failed` one for `failed_ttl` at most.
    fn still_kept(&self, kept: &KeptCheck, modified: Option<SystemTime>) -> bool {
        kept.modified == modified
            && !(matches!(kept.state, OpenSpecCliState::Failed { .. })
                && kept.at.elapsed() >= self.failed_ttl)
    }

    /// The same seam with another lifetime for a `failed` check.
    #[cfg(test)]
    pub fn with_failed_ttl(mut self, ttl: std::time::Duration) -> Self {
        self.failed_ttl = ttl;
        self
    }

    /// OSM-FR-YXUC: the setting, the detection, the mode, and the CLI state.
    /// The CLI is checked also while the mode is inactive.
    pub fn status<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> OpenSpecStatus {
        self.status_checked(sink, buffer, root, false)
    }

    /// OSM-FR-YXUC, OFP-FR-ITUD: the same, where `recheck` first clears a
    /// kept `failed` check.
    pub fn status_checked<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
        recheck: bool,
    ) -> OpenSpecStatus {
        if recheck {
            self.clear_failed_checks(sink, buffer);
        }
        let setting = root
            .map(Self::setting)
            .unwrap_or_else(|| crate::project_settings::openspec_mode::AUTO.to_string());
        let uses_openspec = root.is_some_and(detection::uses_openspec);
        let active = uses_openspec && setting == crate::project_settings::openspec_mode::AUTO;
        OpenSpecStatus {
            setting,
            uses_openspec,
            active,
            cli: self.cli_state_in(sink, buffer, root),
        }
    }

    // -- The CLI ---------------------------------------------------------

    /// OSM-FR-EZKW, OSM-FR-ZNVZ, OSM-FR-SKUP: resolve the CLI again, and check
    /// its version only where the path or its modification time is new. With
    /// no project open, a check runs in the temporary folder.
    pub fn cli_state<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
    ) -> OpenSpecCliState {
        self.cli_state_in(sink, buffer, None)
    }

    /// The same, with a check that runs in the root of the active worktree
    /// (OSM-FR-HHUD).
    pub fn cli_state_in<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> OpenSpecCliState {
        let Some(path) = cli::resolve(&self.env, self.probe.as_ref()) else {
            return OpenSpecCliState::NotFound;
        };
        let modified = self.probe.modified(&path);
        if let Some(kept) = self.lock_checks().get(&path) {
            if self.still_kept(kept, modified) {
                return kept.state.clone();
            }
        }
        let cwd = root.map(|r| r.path().to_path_buf()).unwrap_or_else(std::env::temp_dir);
        let state = self.check_version(sink, buffer, &path, &cwd);
        self.lock_checks()
            .insert(path, KeptCheck { modified, state: state.clone(), at: Instant::now() });
        state
    }

    fn lock_checks(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, KeptCheck>> {
        self.checks.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// OSM-FR-ZNVZ: `openspec --version` exits zero and prints a version whose
    /// major number is `MIN_MAJOR` or more. OFP-FR-FETF: a process that times
    /// out, ends by a signal, or exits with a status other than zero runs once
    /// more before the check records `failed`.
    fn check_version<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        path: &Path,
        cwd: &Path,
    ) -> OpenSpecCliState {
        let shown = path.to_string_lossy().to_string();
        let run = || {
            self.run_once(sink, buffer, path, "--version", &["--version"], cwd, OnExitOne::Failed)
        };
        let outcome = match run() {
            Err(first) if !matches!(first, RunFailure::Spawn) => {
                self.pause_before_retry(sink, buffer, "--version", first.code());
                run()
            }
            other => other,
        };
        let state = match outcome {
            Err(RunFailure::TimedOut { .. }) => {
                OpenSpecCliState::Failed { path: shown, reason: CliFailure::TimedOut }
            }
            Err(RunFailure::Exit { .. } | RunFailure::Rejected { .. }) => {
                OpenSpecCliState::Failed { path: shown, reason: CliFailure::NoVersion }
            }
            Err(RunFailure::Spawn) => {
                OpenSpecCliState::Failed { path: shown, reason: CliFailure::NotExecutable }
            }
            Ok((_, stdout)) => match cli::parse_version(&String::from_utf8_lossy(&stdout)) {
                None => OpenSpecCliState::Failed { path: shown, reason: CliFailure::NoVersion },
                Some((version, major)) if major >= cli::MIN_MAJOR => {
                    OpenSpecCliState::Available { path: shown, version }
                }
                Some((version, _)) => OpenSpecCliState::Unsupported { path: shown, version },
            },
        };
        logging::log_info(
            sink,
            buffer,
            &[Domain::Backend],
            "openspec cli checked",
            log_fields! { "state" => state.code() },
        );
        state
    }

    // -- The reads -------------------------------------------------------

    /// OSM-FR-XSOC: a read needs an open project, an active mode, and a
    /// usable CLI, in that order. Nothing runs when one is missing.
    fn ready<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> Result<PathBuf, OpenSpecReadError> {
        let root = root.ok_or(OpenSpecReadError::NoOpenProject)?;
        if !Self::mode_active(Some(root)) {
            return Err(OpenSpecReadError::OpenSpecModeInactive);
        }
        match self.cli_state_in(sink, buffer, Some(root)) {
            OpenSpecCliState::Available { path, .. } => Ok(PathBuf::from(path)),
            other => Err(OpenSpecReadError::CliUnavailable(other)),
        }
    }

    fn logged<T, S: LogSink + Clone + Send + 'static>(
        sink: &S,
        buffer: &'static LogBuffer,
        read: &'static str,
        result: Result<T, OpenSpecReadError>,
    ) -> Result<T, OpenSpecReadError> {
        if let Err(error) = &result {
            logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec read failed",
                log_fields! { "read" => read, "code" => error.code() },
            );
        }
        result
    }

    /// OSM-FR-JTOQ: `openspec list --specs --json`.
    pub fn list_capabilities<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> Result<Vec<Capability>, OpenSpecReadError> {
        let result = self.ready(sink, buffer, root).and_then(|exe| {
            let root = root.ok_or(OpenSpecReadError::NoOpenProject)?;
            let args = ["list", "--specs", "--json"];
            let out = self.run_logged(sink, buffer, &exe, "list", &args, root.path())?;
            reads::parse_capabilities(&out)
        });
        Self::logged(sink, buffer, "list_capabilities", result)
    }

    /// OSM-FR-JAIC, OSM-FR-SIBT: `openspec show <id> --type spec --json
    /// --no-interactive`, after the id and the file are checked.
    pub fn read_capability<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
        id: &str,
    ) -> Result<CapabilitySpec, OpenSpecReadError> {
        let result = self.ready(sink, buffer, root).and_then(|exe| {
            let root = root.ok_or(OpenSpecReadError::NoOpenProject)?;
            if !reads::valid_capability_id(id) {
                return Err(OpenSpecReadError::InvalidCapabilityId);
            }
            let rel = format!("{}/{id}/{}", detection::SPECS_DIR, detection::SPEC_FILE);
            if !detection::is_regular_file(root, &rel) {
                return Err(OpenSpecReadError::CapabilityNotFound);
            }
            let args = ["show", id, "--type", "spec", "--json", "--no-interactive"];
            let out = self.run_logged(sink, buffer, &exe, "show", &args, root.path())?;
            reads::parse_capability(&out)
        });
        Self::logged(sink, buffer, "read_capability", result)
    }

    /// OSM-FR-OPZF, OSM-FR-FCMY, LOC-FR-DSMK: `list_changes(location)` in the
    /// active worktree location.
    pub fn list_changes<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        root: Option<&RootFs>,
    ) -> Result<Vec<ChangeSummary>, OpenSpecReadError> {
        match root {
            Some(at) => self.list_changes_at(sink, buffer, root, at),
            None => {
                Self::logged(sink, buffer, "list_changes", Err(OpenSpecReadError::NoOpenProject))
            }
        }
    }

    /// OSM-FR-OPZF, OSM-FR-FCMY, OSM-FR-DDHL: `list --changes --json` and
    /// `status --all --json` once each in the working copy `at` of a location,
    /// and one directory listing for each change. The mode is decided on the
    /// active worktree.
    pub fn list_changes_at<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
    ) -> Result<Vec<ChangeSummary>, OpenSpecReadError> {
        let result = self.ready_in(sink, buffer, active, at).and_then(|exe| {
            let list_args = ["list", "--changes", "--json"];
            let list = self.run_logged(sink, buffer, &exe, "list", &list_args, at.path())?;
            let status_args = ["status", "--all", "--json"];
            let status = self.run_logged(sink, buffer, &exe, "status", &status_args, at.path())?;
            let mut changes = reads::parse_changes(&list, &status)?;
            for change in &mut changes {
                change.capabilities = reads::delta_capabilities(at, &change.name);
            }
            Ok(changes)
        });
        Self::logged(sink, buffer, "list_changes", result)
    }

    /// OSM-FR-XSOC, OSM-FR-DDHL: an open project, an active mode on the active
    /// worktree, and a usable CLI checked in the working copy `at`.
    pub(crate) fn ready_in<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
    ) -> Result<PathBuf, OpenSpecReadError> {
        let active = active.ok_or(OpenSpecReadError::NoOpenProject)?;
        if !Self::mode_active(Some(active)) {
            return Err(OpenSpecReadError::OpenSpecModeInactive);
        }
        match self.cli_state_in(sink, buffer, Some(at)) {
            OpenSpecCliState::Available { path, .. } => Ok(PathBuf::from(path)),
            other => Err(OpenSpecReadError::CliUnavailable(other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri command and the open hook
// ---------------------------------------------------------------------------

/// OSM-FR-YXUC: `"get openspec status"`. Read-only. It runs on a blocking
/// thread, because the first CLI check of a session starts a Node process.
/// `recheck`, false where absent, first clears a kept `failed` check
/// (OFP-FR-ITUD).
#[tauri::command]
pub async fn get_openspec_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    recheck: Option<bool>,
) -> Result<OpenSpecStatus, String> {
    let recheck = recheck.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || status_now(&app, recheck))
        .await
        .map_err(|e| format!("openspec status failed: {e}"))
}

pub(crate) fn status_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    recheck: bool,
) -> OpenSpecStatus {
    use tauri::Manager;
    let root = app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
    let status = match app.try_state::<OpenSpec>() {
        Some(seam) => seam.status_checked(app, &logging::BUFFER, root.as_ref(), recheck),
        None => OpenSpec::default().status(app, &logging::BUFFER, root.as_ref()),
    };
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        "openspec status read",
        log_fields! {
            "setting" => status.setting.clone(),
            "usesOpenspec" => status.uses_openspec,
            "active" => status.active,
            "cli" => status.cli.code(),
            "recheck" => recheck,
        },
    );
    status
}

/// OSM-FR-ZNVZ: check the CLI in the background when a project opens, so that
/// the first conversational turn finds the result kept and starts no process
/// on its own path. A turn that finds no kept check starts one the same way.
pub fn warm_on_project_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    warm_cli_check(app);
}

/// Check the CLI on a blocking thread and keep the result (OSM-FR-ZNVZ).
pub fn warm_cli_check<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if app.try_state::<OpenSpec>().is_none() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(seam) = app.try_state::<OpenSpec>() {
            let root =
                app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
            let _ = seam.cli_state_in(&app, &logging::BUFFER, root.as_ref());
        }
    });
}
