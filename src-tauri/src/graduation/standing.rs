//! What a run does with work standing in its stream
//! (`GRD-graduation.md` GRD-FR-HQPD, GRD-FR-PXVJ, GRD-FR-KDWA).
//!
//! The author takes this decision when the run is enqueued, and the run applies
//! it when its turn comes. The two moments are far apart — a run can wait
//! behind several others — so nothing here reads a path set the author was
//! shown: it acts on what stands in the stream at this moment.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::tools::agent_exec::runtime::CancellationToken;

use super::*;

/// The reason a push could not be attempted at all.
///
/// The state a push needs is managed by the application, so a handle that holds
/// none is a machine the push cannot run on rather than a remote that refused.
const ERR_PUSH_UNAVAILABLE: &str = "push_unavailable";

/// GRD-FR-PXVJ: the push did not answer inside the bound, so the run went on.
const ERR_PUSH_DID_NOT_FINISH: &str = "push_did_not_finish";

/// GRD-FR-PXVJ: the run stopped while the push was still running.
const ERR_PUSH_ABANDONED: &str = "push_abandoned";

/// How long a push may hold the dispatch before the run goes on without it.
///
/// A remote that accepts the connection and then never answers would otherwise
/// hold the stream, and the whole queue behind it, for as long as the machine
/// stays up. The choice exists to keep a run from being blocked later, so the
/// step that serves it must not become the block itself.
const PUSH_BOUND: Duration = Duration::from_secs(300);

/// How often the wait looks at the run's own stop.
const PUSH_POLL: Duration = Duration::from_millis(250);

/// GRD-FR-HQPD / GRD-FR-KDWA: apply the run's choice, and report what it did.
///
/// The commit is the one thing that can fail the step: a run whose base cannot
/// be established has nothing to measure its work from. A push that the remote
/// refuses is recorded and nothing more (GRD-FR-PXVJ), because a run stopped by
/// an unreachable remote is exactly the block the choice exists to avoid.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
    cancellation: &CancellationToken,
) -> Result<StandingWorkOutcome, String> {
    let mut outcome = StandingWorkOutcome::default();
    if run.standing_work.commits() {
        outcome.commit = commit::commit_author_work(worktree, &commit_message(run))?;
    }
    if run.standing_work.pushes() {
        match push_stream_branch(app, worktree, &run.project_key, cancellation) {
            Ok(()) => {
                outcome.pushed = Some(true);
                crate::logging::log_info(
                    app,
                    &crate::logging::BUFFER,
                    &[crate::logging::Domain::Backend, crate::logging::Domain::Remote],
                    "the stream branch was pushed for a run",
                    crate::log_fields! { "run" => &run.id, "stream" => &run.stream_name },
                );
            }
            Err(code) => {
                // The typed cause alone. A transport failure can echo the
                // remote's URL back, and nothing downstream redacts what
                // reaches the Logs panel.
                let code = crate::git::redact_credentials(&code);
                crate::logging::log_warn(
                    app,
                    &crate::logging::BUFFER,
                    &[crate::logging::Domain::Backend, crate::logging::Domain::Remote],
                    "the stream branch was not pushed for a run",
                    crate::log_fields! {
                        "run" => &run.id,
                        "stream" => &run.stream_name,
                        "cause" => &code,
                    },
                );
                outcome.pushed = Some(false);
                outcome.push_failure = Some(StandingWorkPushFailure { code });
            }
        }
    }
    Ok(outcome)
}

/// GRD-FR-RJFC: the message the standing-work commit takes.
///
/// A run that carries none commits under its own name, which is the name of the
/// draft its captured input holds. A message of spaces alone is none: it says
/// no more than an empty one does.
fn commit_message(run: &GraduationRun) -> String {
    run.standing_work_message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| run.input.draft_name.clone())
}

/// GRD-FR-PXVJ / GTC-FR-YHDU: push the branch the stream's checkout holds.
///
/// It composes the one push primitive, against the stream's working copy rather
/// than against the author's active worktree. The Git panel's own channels take
/// nothing from it: a push the author did not start must not report itself as
/// one they did.
///
/// The transfer runs on a thread of its own and the dispatch waits on it under
/// a bound, because libgit2 offers no way to stop one that has stopped
/// answering. A transfer the wait gives up on keeps running and may still land;
/// what it may not do is hold the stream.
fn push_stream_branch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    worktree: &Path,
    project_key: &str,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let (report, answered) = mpsc::channel();
    let handle = app.clone();
    let root = worktree.to_path_buf();
    // The run's own project and not whichever one is open now: the author may
    // switch projects while this run works, and the credential a push presents
    // belongs to the project the stream is part of.
    let key = project_key.to_string();
    std::thread::spawn(move || {
        let _ = report.send(push_now(&handle, &root, &key));
    });

    await_push(&answered, cancellation, Instant::now() + PUSH_BOUND)
}

/// GRD-FR-PXVJ: wait for the transfer, but not past the run's own stop or the
/// bound.
///
/// Every way out of it is a report the run goes on from: the step records what
/// the push did and stops nothing (GRD-FR-KDWA).
fn await_push(
    answered: &mpsc::Receiver<Result<(), String>>,
    cancellation: &CancellationToken,
    deadline: Instant,
) -> Result<(), String> {
    loop {
        match answered.recv_timeout(PUSH_POLL) {
            Ok(outcome) => return outcome,
            // The thread ended without reporting, which only a panic inside it
            // does. The run goes on, as it does for every other push failure.
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ERR_PUSH_UNAVAILABLE.to_string())
            }
            Err(RecvTimeoutError::Timeout) => {
                if cancellation.is_cancelled() {
                    return Err(ERR_PUSH_ABANDONED.to_string());
                }
                if Instant::now() >= deadline {
                    return Err(ERR_PUSH_DID_NOT_FINISH.to_string());
                }
            }
        }
    }
}

/// The transfer itself, on the thread that waits for no run.
fn push_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    worktree: &PathBuf,
    project_key: &str,
) -> Result<(), String> {
    let (Some(store), Some(tokens), Some(registry)) = (
        app.try_state::<crate::global_settings::GlobalSettingsStore>(),
        app.try_state::<crate::github_tokens::GithubTokens>(),
        app.try_state::<crate::progress::ProgressRegistry>(),
    ) else {
        return Err(ERR_PUSH_UNAVAILABLE.to_string());
    };
    let emit = |_name: &str, _payload: serde_json::Value| {};
    crate::git::push_branch_at(
        app,
        &crate::logging::BUFFER,
        &emit,
        &registry,
        worktree,
        &store,
        &tokens,
        project_key,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GRD-FR-PXVJ: a push that answers is what the step reports.
    #[test]
    fn a_push_that_answers_is_reported_as_it_answered() {
        let (report, answered) = mpsc::channel();
        report.send(Ok(())).unwrap();
        assert_eq!(
            await_push(&answered, &CancellationToken::new(), Instant::now() + PUSH_BOUND),
            Ok(())
        );

        let (report, answered) = mpsc::channel();
        report.send(Err("no remote configured".to_string())).unwrap();
        assert_eq!(
            await_push(&answered, &CancellationToken::new(), Instant::now() + PUSH_BOUND),
            Err("no remote configured".to_string())
        );
    }

    /// GRD-FR-PXVJ: a transfer that stops answering does not hold the stream.
    /// The wait ends at the bound and the run goes on without it.
    #[test]
    fn a_push_that_never_answers_ends_at_the_bound() {
        // Held, so the channel is open and the wait is the only thing that
        // ends it — exactly the remote that accepts and then says nothing.
        let (_report, answered) = mpsc::channel::<Result<(), String>>();
        let started = Instant::now();
        assert_eq!(
            await_push(&answered, &CancellationToken::new(), started),
            Err(ERR_PUSH_DID_NOT_FINISH.to_string())
        );
        assert!(started.elapsed() < PUSH_BOUND, "it did not wait the whole bound out");
    }

    /// GRD-FR-PXVJ: a run the author stopped is not kept waiting for a push.
    #[test]
    fn a_run_that_was_stopped_does_not_wait_for_its_push() {
        let (_report, answered) = mpsc::channel::<Result<(), String>>();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            await_push(&answered, &cancellation, Instant::now() + PUSH_BOUND),
            Err(ERR_PUSH_ABANDONED.to_string())
        );
    }

    /// GRD-FR-PXVJ: a transfer thread that ends without reporting is a failed
    /// push and not a run that waits for a report nobody will send.
    #[test]
    fn a_transfer_that_reports_nothing_is_a_failed_push() {
        let (report, answered) = mpsc::channel::<Result<(), String>>();
        drop(report);
        assert_eq!(
            await_push(&answered, &CancellationToken::new(), Instant::now() + PUSH_BOUND),
            Err(ERR_PUSH_UNAVAILABLE.to_string())
        );
    }
}
