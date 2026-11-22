//! EAC-FR-QQMA / EAC-FR-INWE: a shell in the running VM of a graduation
//! queue. It never boots a VM and never keeps one.

use std::path::Path;

use super::super::descriptor;
use super::super::descriptor::WORKSPACE_TARGET;
use super::super::sandbox::{self, InteractiveVmProcess, SandboxPool, SandboxScope};
use super::compose;
use super::{InteractiveLaunchRequest, InteractiveProgram};
use crate::terminals::process::ProcessHandle;

/// TRS-FR-KJYH: whether the VM of `scope` of the project runs now.
pub fn running(pool: &SandboxPool, project_key: &str, scope: &SandboxScope) -> bool {
    pool.is_booted(&sandbox::vm_name(project_key, scope))
}

/// EAC-FR-QQMA: the process of a shell in a VM: a login shell, as the host
/// user, in the directory of EAC-FR-ZKMR, with no credential.
pub fn vm_process(directory: &Path, cols: u16, rows: u16) -> InteractiveVmProcess {
    let (uid, gid) = descriptor::host_ids();
    InteractiveVmProcess {
        argv: compose::program_argv(&InteractiveProgram::Shell),
        workdir: descriptor::container_path(directory)
            .unwrap_or_else(|| WORKSPACE_TARGET.to_string()),
        user: format!("{uid}:{gid}"),
        env: compose::terminal_literals(),
        cols,
        rows,
    }
}

pub(super) async fn launch(
    pool: &SandboxPool,
    project_key: &str,
    directory: &Path,
    request: &InteractiveLaunchRequest,
) -> Result<ProcessHandle, String> {
    if request.program != InteractiveProgram::Shell {
        return Err("only a shell runs in a VM".to_string());
    }
    let scope = request.sandbox_scope.as_ref().ok_or_else(|| sandbox::NO_VM_RUNNING.to_string())?;
    let vm = sandbox::vm_name(project_key, scope);
    pool.attach_interactive(&vm, vm_process(directory, request.cols, request.rows)).await
}
