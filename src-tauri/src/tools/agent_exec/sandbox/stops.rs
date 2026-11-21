//! Where a VM stops outside a launch (EAC-FR-ABHG, EAC-FR-LQRI): when its
//! stream is deleted, when its queue becomes idle, when the project closes, and
//! when the application exits.

use super::*;
use crate::tools::agent_exec::devcontainer_pool;

/// DVC-FR-DTWX: a dev container that could not be removed stays tracked, and
/// the record names the scope and the step that failed.
fn log_removal_failures<S>(sink: &S, failures: &[devcontainer_pool::RemovalFailure])
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    for (scope, reason) in failures {
        logging::log_warn(
            sink,
            &BUFFER,
            &DOMAINS,
            "dev container could not be removed",
            log_fields! { "scope" => scope.as_str(), "reason" => reason.as_str() },
        );
    }
}

/// EAC-FR-ABHG: stop the VM of one stream of one project. Called when the
/// stream is deleted.
pub async fn stop_scope<S>(sink: &S, project_key: &str, queue_key: &str)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let vm = vm_name(project_key, &SandboxScope { queue_key: queue_key.to_string() });
    // DVC-FR-DTWX: the dev container of the scope goes at the same point.
    let (_, removed) =
        tokio::join!(production_pool().stop(&vm), devcontainer_pool::production_pool().stop(&vm));
    if let Err(reason) = removed {
        log_removal_failures(sink, &[(vm.clone(), reason)]);
    }
    logging::log_info(
        sink,
        &BUFFER,
        &DOMAINS,
        "sandbox VM of a stream stopped",
        log_fields! {
            "vm" => vm.as_str()
        },
    );
}

/// EAC-FR-ABHG: stop every VM. Called when the application exits.
pub async fn stop_all<S>(sink: &S)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    // DVC-FR-DTWX: and every dev container, at the same time, so neither
    // uses the exit bound of the other.
    let (_, failures) =
        tokio::join!(production_pool().stop_all(), devcontainer_pool::production_pool().stop_all());
    log_removal_failures(sink, &failures);
    logging::log_info(sink, &BUFFER, &DOMAINS, "every sandbox VM stopped", log_fields! {});
}

/// EAC-FR-ABHG: how long an exit waits for the VMs to stop. A VM that does not
/// stop in that time still ends with the application process.
const EXIT_STOP_BOUND: Duration = Duration::from_secs(10);

/// EAC-FR-ABHG: stop the VM of a deleted stream, without holding the command
/// that deleted it.
pub fn stop_scope_in_background<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: String,
    queue_key: String,
) {
    let app = app.clone();
    crate::project_scope::context::spawn(async move {
        stop_scope(&app, &project_key, &queue_key).await;
    });
}

/// EAC-FR-LQRI: stop the VM `vm` of a queue that became idle, without holding
/// the caller. `still_idle` is asked again once the stop holds the VM, so a queue
/// that has work again by then keeps its VM.
pub fn stop_idle_scope_in_background<R, F>(app: &tauri::AppHandle<R>, vm: String, still_idle: F)
where
    R: tauri::Runtime,
    F: FnOnce() -> bool + Send + 'static,
{
    let app = app.clone();
    crate::project_scope::context::spawn(async move {
        // DVC-FR-DTWX: a scope runs either a dev container or a VM.
        let devcontainers = devcontainer_pool::production_pool();
        if devcontainers.holds(&vm) {
            match devcontainers.stop_when(&vm, still_idle).await {
                Ok(true) => logging::log_info(
                    &app,
                    &BUFFER,
                    &DOMAINS,
                    "dev container of an idle queue removed",
                    log_fields! { "scope" => vm.as_str() },
                ),
                Ok(false) => {}
                Err(reason) => log_removal_failures(&app, &[(vm.clone(), reason)]),
            }
            return;
        }
        if production_pool().stop_when(&vm, still_idle).await {
            logging::log_info(
                &app,
                &BUFFER,
                &DOMAINS,
                "sandbox VM of an idle queue stopped",
                log_fields! { "vm" => vm.as_str() },
            );
        } else {
            logging::log_debug(
                &app,
                &BUFFER,
                &DOMAINS,
                "sandbox VM of a queue not stopped: no VM runs, or the queue has work again",
                log_fields! { "vm" => vm.as_str() },
            );
        }
    });
}

/// EAC-FR-ABHG: stop the VMs of one project and no other. Called when that
/// project closes: the other open projects keep their VMs.
pub async fn stop_project<S>(sink: &S, project_key: &str)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    // DVC-FR-DTWX: and the dev containers of that project, at the same time.
    let (_, failures) = tokio::join!(
        production_pool().stop_project(project_key),
        devcontainer_pool::production_pool().stop_project(project_key)
    );
    log_removal_failures(sink, &failures);
    logging::log_info(sink, &BUFFER, &DOMAINS, "sandbox VMs of a project stopped", log_fields! {});
}

/// EAC-FR-ABHG: stop the VMs of the closing project, without holding the close.
pub fn stop_project_in_background<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: String,
) {
    let app = app.clone();
    crate::project_scope::context::spawn(async move {
        stop_project(&app, &project_key).await;
    });
}

/// EAC-FR-ABHG: stop every VM before the application exits, bounded so a VM
/// that does not answer cannot hold the exit. Runs on a thread of its own, so
/// it never blocks inside an asynchronous runtime.
pub fn stop_all_before_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    let stopped = crate::project_scope::context::thread_spawn(move || {
        tauri::async_runtime::block_on(async move {
            let _ = tokio::time::timeout(EXIT_STOP_BOUND, stop_all(&app)).await;
        });
    })
    .join();
    if stopped.is_err() {
        // The thread panicked. The VMs still end with the process.
        eprintln!("synthesis: the sandbox VMs could not be stopped before exit");
    }
}
