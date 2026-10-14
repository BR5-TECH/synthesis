//! The sandbox VM of a queue that becomes idle stops (GRD-FR-UESX,
//! `../tools/EAC-execute-agent-cli.md` EAC-FR-LQRI).

use super::*;

/// GRD-FR-UESX: whether a queue is idle. No run holds it (GRD-FR-BNTC), no
/// member stands in `queued`, and no loop of this process has claimed it.
pub(super) fn queue_is_idle(queue: &GraduationQueue, key: &str, claimed: bool) -> bool {
    !claimed
        && queue::holder_of_queue(queue, key).is_none()
        && queue::queue_of(queue, key).is_empty()
}

/// GRD-FR-UESX: whether a queue of a project is idle now, read from the store
/// and from the claims of this process. Where the store cannot be reached the
/// queue is not idle. A record that cannot be read is left out, as every
/// reader of the queue leaves it out (GRD-FR-LGDV). A wrong answer costs at
/// most one boot, because a stop never cuts a launch (EAC-FR-LQRI).
pub(super) fn idle_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    key: &str,
) -> bool {
    let claimed =
        app.try_state::<GraduationState>().is_some_and(|state| state.holder_of(key).is_some());
    let (Ok(fs), Ok(base)) = (store::store_fs(app), store::store_base(app)) else {
        return false;
    };
    queue_is_idle(&queue::load_queue(&fs, &base, project_key), key, claimed)
}

/// GRD-FR-UESX / EAC-FR-LQRI: after `run` ended, ask the executor to stop the
/// VM of its queue where that queue is now idle.
pub(super) fn stop_sandbox_if_idle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) {
    let key = run.queue_key();
    let project_key = run.project_key.clone();
    // The queue key can name a worktree path, so the records carry the VM name,
    // which is its digest, and the kind of scope, never the key itself.
    let vm = crate::tools::agent_exec::sandbox::vm_name(
        &project_key,
        &crate::tools::agent_exec::sandbox::SandboxScope { queue_key: key.clone() },
    );
    let scope_kind = if is_worktree_queue_key(&key) { "worktree" } else { "stream" };
    if !idle_now(app, &project_key, &key) {
        logging::log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation kept the sandbox VM of a queue that has work",
            log_fields! {
                "run_id" => run.id.clone(),
                "vm" => vm.as_str(),
                "scope_kind" => scope_kind,
            },
        );
        return;
    }
    logging::log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation found a queue idle and asked to stop its sandbox VM",
        log_fields! {
            "run_id" => run.id.clone(),
            "vm" => vm.as_str(),
            "scope_kind" => scope_kind,
        },
    );
    let still_idle = {
        let handle = app.clone();
        move || idle_now(&handle, &project_key, &key)
    };
    #[cfg(test)]
    if let Some(state) = app.try_state::<GraduationState>() {
        state.record_idle_stop(&run.queue_key(), &vm, Box::new(still_idle.clone()));
    }
    crate::tools::agent_exec::sandbox::stop_idle_scope_in_background(app, vm, still_idle);
}
