//! The interactive launch (EAC-FR-CKGX .. EAC-FR-HCIK): the author's own
//! terminal session in the environment a turn would use.
//!
//! It is the one launch path beside `execute_agent_cli`, and only
//! `TRS-terminal-sessions.md` calls it. It carries no task, no turn kind, no
//! response contract, no sink, and no time limit, and it parses nothing the
//! process writes (EAC-FR-LUZG). The headless path is unchanged.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::oneshot;

use super::sandbox::SandboxScope;
use super::*;
use crate::terminals::model::CliVendor;
use crate::terminals::process::ProcessHandle;

mod compose;
mod devcontainer_place;
mod docker_place;
pub mod removals;
mod vm_place;

pub use compose::{
    interactive_exec_args, interactive_run_args, program_argv, SHELL_PROGRAM, SHELL_SCRIPT,
};

/// What runs in the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractiveProgram {
    Shell,
    AgentCli(CliVendor),
}

/// Where the session runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractivePlace {
    Docker,
    Devcontainer,
    Microsandbox,
}

/// Bytes of start output for the session history (DVC-FR-KTBH).
pub type ProgressSink = Arc<dyn Fn(&[u8]) + Send + Sync>;

pub struct InteractiveLaunchRequest {
    /// The target checkout. EAC-FR-06 validates it.
    pub execution_directory: PathBuf,
    pub program: InteractiveProgram,
    pub place: InteractivePlace,
    /// The VM to attach to: for `Microsandbox` alone.
    pub sandbox_scope: Option<SandboxScope>,
    /// Names the dev container scope key (`terminal:<id>`) and the session
    /// state directory.
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
    /// Start output, such as each line of `devcontainer up` (DVC-FR-KTBH).
    pub progress: Option<ProgressSink>,
}

impl InteractiveProgram {
    fn kind(&self) -> &'static str {
        match self {
            InteractiveProgram::Shell => "shell",
            InteractiveProgram::AgentCli(_) => "agent_cli",
        }
    }

    fn vendor(&self) -> &'static str {
        match self {
            InteractiveProgram::Shell => "",
            InteractiveProgram::AgentCli(vendor) => vendor.id(),
        }
    }
}

impl InteractivePlace {
    fn as_str(&self) -> &'static str {
        match self {
            InteractivePlace::Docker => "docker",
            InteractivePlace::Devcontainer => "devcontainer",
            InteractivePlace::Microsandbox => "microsandbox",
        }
    }
}

/// The application state an interactive launch reads.
pub(super) struct Parts<'a> {
    pub store: tauri::State<'a, GlobalSettingsStore>,
    pub integrations: tauri::State<'a, AgenticIntegrations>,
    pub project_key: String,
    pub project_root: crate::fs::RootFs,
}

impl<'a> Parts<'a> {
    fn resolve<R: tauri::Runtime>(app: &'a tauri::AppHandle<R>) -> Result<Self, String> {
        use crate::project::OpenProject as _;
        use tauri::Manager as _;
        let store = app
            .try_state::<GlobalSettingsStore>()
            .ok_or_else(|| "the settings store is not available".to_string())?;
        let integrations = app
            .try_state::<AgenticIntegrations>()
            .ok_or_else(|| "the agentic integrations are not available".to_string())?;
        let state = app.open_project_state().ok_or_else(|| "no project is open".to_string())?;
        let project_key = state.slot_key();
        let project_root = state.require_root()?;
        Ok(Parts { store, integrations, project_key, project_root })
    }
}

/// The guarded instance an interactive launch reads and writes through: the
/// target checkout, and the two roots this application owns.
fn target_fs(directory: &Path) -> Result<FsAccess, String> {
    let app_data = crate::fs::app_data_dir().map_err(|e| e.to_string())?;
    let short_data = crate::fs::short_data_dir().map_err(|e| e.to_string())?;
    FsAccess::builder()
        .required_root(directory)
        .owned_root(app_data)
        .owned_root(short_data)
        .build()
        .map_err(|e| format!("the checkout is not accessible: {e}"))
}

/// The root the session-state directories of interactive launches live under.
fn session_root() -> Result<PathBuf, String> {
    crate::fs::app_data_dir()
        .map(|dir| dir.join(SESSION_STATE_DIRNAME).join("terminals"))
        .map_err(|e| e.to_string())
}

/// EAC-FR-CKGX: starts one process with a terminal in a Docker container, a
/// dev container, or a running VM. The error is one line for the author.
pub async fn launch_interactive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: InteractiveLaunchRequest,
) -> Result<ProcessHandle, String> {
    let program = request.program;
    let place = request.place;
    let result = launch_inner(app, request).await;
    match &result {
        Ok(handle) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "interactive launch started",
            log_fields! {
                "program" => program.kind(),
                "vendor" => program.vendor(),
                "place" => place.as_str(),
                "environment" => handle.environment_name.clone().unwrap_or_default(),
            },
        ),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "interactive launch failed",
            log_fields! {
                "program" => program.kind(),
                "vendor" => program.vendor(),
                "place" => place.as_str(),
                // The line can echo the output of a lifecycle command, so the
                // record holds its shape alone. The line goes to the terminal.
                "reason_chars" => reason.chars().count(),
                "no_vm_running" => reason == super::sandbox::NO_VM_RUNNING,
            },
        ),
    }
    result
}

async fn launch_inner<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: InteractiveLaunchRequest,
) -> Result<ProcessHandle, String> {
    if !cfg!(unix) {
        return Err("platform_unsupported".to_string());
    }
    let parts = Parts::resolve(app)?;
    let fs = Arc::new(target_fs(&request.execution_directory)?);
    let executor = AgentCliExecutor::default();
    let directory = executor
        .validate_directory(&fs, &request.execution_directory)
        .map_err(|e| e.kind().to_string())?;
    let handle = match request.place {
        InteractivePlace::Docker => {
            executor.launch_docker_interactive(&parts, &fs, &directory, &request).await?
        }
        InteractivePlace::Devcontainer => {
            executor.launch_devcontainer_interactive(&parts, &fs, &directory, &request).await?
        }
        InteractivePlace::Microsandbox => {
            vm_place::launch(
                super::sandbox::production_pool(),
                &parts.project_key,
                &directory,
                &request,
            )
            .await?
        }
    };
    let sink = app.clone();
    let program = request.program;
    let place = request.place;
    Ok(on_end(handle, move |code| {
        logging::log_info(
            &sink,
            &BUFFER,
            &[Domain::Backend],
            "interactive launch ended",
            log_fields! {
                "program" => program.kind(),
                "vendor" => program.vendor(),
                "place" => place.as_str(),
                "exit_code" => code.map(i64::from).unwrap_or(-1),
            },
        );
    }))
}

/// TRS-FR-KJYH: whether a VM of `scope` of the open project runs now. It
/// never boots and never waits for a launch.
pub fn vm_running<R: tauri::Runtime>(app: &tauri::AppHandle<R>, scope: &SandboxScope) -> bool {
    use crate::project::OpenProject as _;
    let Some(state) = app.open_project_state() else {
        return false;
    };
    vm_place::running(super::sandbox::production_pool(), &state.slot_key(), scope)
}

/// Runs `end` once the process of `handle` has ended, before the end reaches
/// the caller. A cleanup and the end record of EAC-FR-HCIK go here.
pub fn on_end(
    mut handle: ProcessHandle,
    end: impl FnOnce(Option<i32>) + Send + 'static,
) -> ProcessHandle {
    let (tx, rx) = oneshot::channel();
    let original = std::mem::replace(&mut handle.exit, rx);
    tokio::spawn(async move {
        let code = original.await.ok().flatten();
        end(code);
        let _ = tx.send(code);
    });
    handle
}

/// Runs one cleanup at most once, whichever of the terminate and the end
/// comes first.
pub(super) struct CleanupOnce(std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>);

impl CleanupOnce {
    pub fn new(cleanup: impl FnOnce() + Send + 'static) -> Arc<Self> {
        Arc::new(CleanupOnce(std::sync::Mutex::new(Some(Box::new(cleanup)))))
    }

    pub fn run(&self) {
        let cleanup = self.0.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(cleanup) = cleanup {
            cleanup();
        }
    }
}

#[cfg(test)]
#[path = "interactive/tests.rs"]
mod tests;
