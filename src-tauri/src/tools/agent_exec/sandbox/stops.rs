//! Where a VM stops outside a launch (EAC-FR-ABHG, EAC-FR-LQRI): when its
//! stream is deleted, when its queue becomes idle, when the project closes, and
//! when the application exits.

use super::*;

/// EAC-FR-ABHG: stop the VM of one stream of one project. Called when the
/// stream is deleted.
pub async fn stop_scope<S>(sink: &S, project_key: &str, queue_key: &str)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let vm = vm_name(project_key, &SandboxScope { queue_key: queue_key.to_string() });
    production_pool().stop(&vm).await;
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

/// EAC-FR-ABHG: stop every VM. Called when the project closes and when the
/// application exits.
pub async fn stop_all<S>(sink: &S)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    production_pool().stop_all().await;
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
    tauri::async_runtime::spawn(async move {
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
    tauri::async_runtime::spawn(async move {
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

/// EAC-FR-ABHG: stop every VM when the project closes, without holding the
/// close.
pub fn stop_all_in_background<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        stop_all(&app).await;
    });
}

/// EAC-FR-ABHG: stop every VM before the application exits, bounded so a VM
/// that does not answer cannot hold the exit. Runs on a thread of its own, so
/// it never blocks inside an asynchronous runtime.
pub fn stop_all_before_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    let stopped = std::thread::spawn(move || {
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
