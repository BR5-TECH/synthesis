//! The claim a loop takes on its run and its queue, and its release
//! (GRD-FR-BNTC, GRD-FR-KKKN, GRD-FR-QWLP).

use super::*;
use crate::graduation::is_streamless_queue_key;
use crate::project_scope::ProjectStores;

/// What a claim gives the loop it was taken for.
pub(super) struct LoopTicket {
    /// The one cancellation path of the loop's turns (GRL-FR-ZDKP).
    pub cancellation: CancellationToken,
    /// GRD-FR-QWLP: the loop's own id, so its end releases its own claim and
    /// its own loop entry and nothing of a later loop.
    pub loop_id: u64,
}

/// GRD-FR-BNTC: take the stream for this run, or answer that it is not free.
///
/// The claim is the check. A queue already holding a run, a project with no
/// free slot under its limit (GRD-FR-KKKN), and a loop that is still inside
/// the run or its queue (GRD-FR-QWLP) all refuse here.
pub(super) fn claim<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
) -> Option<LoopTicket> {
    let state = app.try_project_store::<GraduationState>()?;
    if !state.loop_enabled() {
        return None;
    }
    let limit = crate::graduation::scheduler::concurrency_limit(app);
    let direct =
        crate::graduation::load_run(app, run_id).map(|run| run.is_direct()).unwrap_or(false);
    // GRD-FR-BNTC, GRD-FR-KKKN: the stored runs say which run holds this queue
    // and which runs hold a slot. A run at rest holds both as a run at work does.
    let (holders, other_holder) = stored_holds(app, run_id, stream_id);
    if let Some(holder) = other_holder {
        logging::log_debug(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation refused a claim: another run holds the queue",
            log_fields! { "run_id" => run_id.to_string(), "holder" => holder },
        );
        return None;
    }
    let cancellation = CancellationToken::new();
    let Some(loop_id) =
        state.claim_loop(stream_id, run_id, direct, limit, &holders, cancellation.clone())
    else {
        // GRD-FR-QWLP / GRD-FR-TNVD: the run stays queued, and the loop that
        // stands in it offers the queue a dispatch again when it returns.
        if state.is_looping(run_id) || state.is_loop_in_queue(stream_id) {
            logging::log_debug(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation refused a claim: a stopped loop is still inside the run or its queue",
                log_fields! {
                    "run_id" => run_id.to_string(),
                    "stream_id" => stream_id.to_string(),
                },
            );
        }
        return None;
    };
    // GRD-FR-ZVNO: an ordinary worktree has no stream record to mark busy; its
    // lock is the claim above and the index entry of a dispatched direct run.
    // GRD-FR-KAKU: a PR summary run has no stream either.
    let marked = if is_streamless_queue_key(stream_id) {
        Ok(())
    } else {
        crate::streams::claim_stream(app, stream_id, run_id)
    };
    if let Err(reason) = marked {
        state.release_loop_claim(stream_id, run_id, loop_id);
        state.end_loop(run_id, loop_id);
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not mark a stream busy",
            log_fields! { "run_id" => run_id.to_string(), "reason" => reason },
        );
        return None;
    }
    logging::log_debug(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation loop claimed a run",
        log_fields! {
            "run_id" => run_id.to_string(),
            "stream_id" => stream_id.to_string(),
            "loop_id" => loop_id as i64,
        },
    );
    Some(LoopTicket { cancellation, loop_id })
}

/// GRD-FR-BNTC, GRD-FR-KKKN: from the run order's index, the runs that hold a
/// project slot and the run, other than this one, that holds this queue.
fn stored_holds<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    queue_key: &str,
) -> (std::collections::BTreeSet<String>, Option<String>) {
    let project_key = crate::graduation::queue::project_key(app);
    let (Ok(fs), Ok(base)) = (crate::graduation::store_fs(app), crate::graduation::store_base(app))
    else {
        return (std::collections::BTreeSet::new(), None);
    };
    let index = crate::graduation::read_queue_index(&fs, &base, &project_key);
    let holders = index
        .runs
        .iter()
        .filter(|entry| entry.holds_slot())
        .map(|entry| entry.run_id.clone())
        .collect();
    let other = index
        .runs
        .iter()
        .find(|entry| {
            entry.holds_queue() && entry.run_id != run_id && entry.queue_key() == queue_key
        })
        .map(|entry| entry.run_id.clone());
    (holders, other)
}

/// End the claim of this loop. The slots are offered again by the caller, once
/// the loop has also ended (GRD-FR-QWLP).
///
/// GRD-FR-BNTC: a run at rest still holds its queue and its project slot, in
/// `blocked`, `awaiting_author`, `interrupted` and `failed` alike. The work it
/// left standing in the working copy is what the author clears the stop against,
/// so the queue waits rather than starting the run behind it on top of that
/// work. Only a run that is `completed` or `discarded` gives the stream back.
pub(super) fn release_claim<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream_id: &str,
    loop_id: u64,
) {
    let holds =
        crate::graduation::load_run(app, run_id).map(|run| run.holds_queue()).unwrap_or(false);
    if let Some(state) = app.try_project_store::<GraduationState>() {
        state.release_loop_claim(stream_id, run_id, loop_id);
    }
    if holds {
        logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation left a queue held by a run at rest",
            log_fields! { "run_id" => run_id.to_string(), "stream_id" => stream_id.to_string() },
        );
    } else if !is_streamless_queue_key(stream_id) {
        let _ = crate::streams::release_stream(app, stream_id, run_id);
    }
}

/// GRD-FR-QWLP: the end of one loop, run when the loop returns or panics.
///
/// The claim is released, the progress operation is ended, and only then the
/// loop entry goes. A loop that claims the run after that point registers an
/// operation of its own, and this end must not take it (GRD-FR-ZHNV).
pub(super) struct LoopEnd<'a, R: tauri::Runtime> {
    pub app: &'a tauri::AppHandle<R>,
    pub run_id: &'a str,
    pub stream_id: &'a str,
    pub loop_id: u64,
}

impl<R: tauri::Runtime> Drop for LoopEnd<'_, R> {
    fn drop(&mut self) {
        release_claim(self.app, self.run_id, self.stream_id, self.loop_id);
        crate::graduation::end_run_progress(self.app, self.run_id);
        if let Some(state) = self.app.try_project_store::<GraduationState>() {
            state.end_loop(self.run_id, self.loop_id);
        }
        logging::log_debug(
            self.app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation loop ended",
            log_fields! {
                "run_id" => self.run_id.to_string(),
                "loop_id" => self.loop_id as i64,
                "panicking" => std::thread::panicking(),
            },
        );
    }
}
