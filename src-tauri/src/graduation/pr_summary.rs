//! A PR summary run, and the run summary a draft run keeps
//! (`GRD-graduation.md` GRD-FR-KAKU, GRD-FR-HMDW, GRD-FR-JAUJ, GRD-FR-TYVT,
//! GRD-FR-MACY, GRD-FR-SWHW, GRD-FR-CCHK, GRD-FR-CQGB, GRD-FR-UUGP).
//!
//! A PR summary run is one agent turn that reads the change of a head branch
//! and returns a title and a description for its pull request. It has no draft
//! and no stream, it changes no file, and it makes no commit. It waits in a
//! queue of its own, `pr:<head>`, and holds that queue and one project slot on
//! the terms of every run.

use super::*;

/// GRD-FR-HMDW: the prefix of the queue key of a head branch's PR summary
/// queue.
pub const PR_QUEUE_PREFIX: &str = "pr:";

/// GRD-FR-HMDW: the queue key of the PR summary queue of one head branch.
pub fn pr_queue_key(head: &str) -> String {
    format!("{PR_QUEUE_PREFIX}{head}")
}

/// Whether a queue key names a PR summary queue.
pub fn is_pr_queue_key(key: &str) -> bool {
    key.starts_with(PR_QUEUE_PREFIX)
}

/// GRD-FR-ZVNO / GRD-FR-HMDW: whether a queue key names no work stream, so
/// that no stream record is marked busy or released for it.
pub fn is_streamless_queue_key(key: &str) -> bool {
    is_worktree_queue_key(key) || is_pr_queue_key(key)
}

/// GRD-FR-UUGP / GRL-FR-FRNJ: a title and a description that a pull request can
/// reuse.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// One line, never blank.
    pub title: String,
    /// Markdown, never blank.
    pub description: String,
}

impl RunSummary {
    /// GRL-FR-ECES / GRL-FR-ZZGO: read a summary from an answer, on its own.
    ///
    /// An object of exactly a `title` of one non-blank line and a non-blank
    /// `description`. Anything else is no summary. The title is trimmed, and the
    /// description loses its outer blank space.
    pub fn read(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        if object.len() != 2 {
            return None;
        }
        let title = object.get("title")?.as_str()?.trim();
        let description = object.get("description")?.as_str()?.trim();
        if title.is_empty() || title.contains(['\n', '\r']) || description.is_empty() {
            return None;
        }
        Some(Self { title: title.to_string(), description: description.to_string() })
    }
}

/// GRD-FR-HMDW: what a PR summary run is asked to describe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrSummaryMode {
    /// The change from the merge base to the head tip.
    #[default]
    Diff,
    /// The run summaries of the runs whose work is on the head branch.
    Combine,
}

impl PrSummaryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Combine => "combine",
        }
    }
}

/// GRD-FR-MACY: one run summary a `combine` run captured at enqueue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSummaryInput {
    pub run_id: String,
    pub title: String,
    pub description: String,
}

/// GRD-FR-KAKU: what makes a run a PR summary run, and what it records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSummaryData {
    /// `Summarize <head>`: the run's title (GRD-FR-JAUJ).
    pub name: String,
    pub head: String,
    pub base: String,
    pub mode: PrSummaryMode,
    /// GRD-FR-TYVT: the full object id of the head tip, pinned at the first
    /// dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_tip: Option<String>,
    /// GRD-FR-TYVT: the merge base of the head and the base, pinned with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<String>,
    /// GRD-FR-MACY: the captured run summaries, in landing order. Empty in
    /// `diff` mode.
    #[serde(default)]
    pub inputs: Vec<PrSummaryInput>,
    /// GRD-FR-SWHW: written when the run completes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunSummary>,
}

/// GRD-FR-JAUJ: the title of a PR summary run.
pub fn pr_summary_title(head: &str) -> String {
    format!("Summarize {head}")
}

impl GraduationRun {
    /// GRD-FR-KAKU: whether this run is a PR summary run.
    pub fn is_pr_summary(&self) -> bool {
        self.pr_summary.is_some()
    }
}

/// GRD-FR-MACY: the run summaries a `combine` run of `head` captures, in
/// landing order, each run with no summary skipped.
///
/// It reads `list_branch_completed_runs(head)` (GRD-FR-BHFP). A read that
/// fails captures nothing, so the request is refused with `no_run_summaries`.
pub(crate) fn completed_run_summaries<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    head: &str,
) -> Vec<PrSummaryInput> {
    let listed = match landings::branch_completed_runs_blocking(app, head) {
        Ok(listed) => listed,
        Err(reason) => {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation could not read the completed runs of a head branch",
                log_fields! { "head" => head.to_string(), "reason" => reason },
            );
            return Vec::new();
        }
    };
    listed
        .into_iter()
        .filter_map(|run| {
            let summary = run.run_summary?;
            Some(PrSummaryInput {
                run_id: run.run_id,
                title: summary.title,
                description: summary.description,
            })
        })
        .collect()
}

/// GRD-FR-HMDW: enqueue a PR summary run for the pull request from `head` into
/// `base`, and offer the project a dispatch.
#[tauri::command]
pub fn start_pull_request_summary<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    head: String,
    base: String,
    mode: PrSummaryMode,
) -> Result<GraduationRun, String> {
    let run = {
        let state = app.try_state::<GraduationState>();
        let _write = state.as_ref().map(|state| state.write_guard());
        enqueue_pull_request_summary(&app, &head, &base, mode, completed_run_summaries)?
    };
    scheduler::dispatch_pending(&app);
    Ok(run)
}

/// GRD-FR-HMDW, GRD-FR-JAUJ, GRD-FR-MACY: the checks and the durable write of
/// a start, with the source of a `combine` run's summaries supplied.
///
/// The caller holds the store hold (GRD-FR-WSXA). A refusal writes nothing.
pub(crate) fn enqueue_pull_request_summary<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    head: &str,
    base: &str,
    mode: PrSummaryMode,
    summaries: impl FnOnce(&tauri::AppHandle<R>, &str) -> Vec<PrSummaryInput>,
) -> Result<GraduationRun, String> {
    let project_key = queue::project_key(app);
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    let refuse = |code: String| {
        logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation refused a PR summary run",
            log_fields! {
                "head" => head.to_string(),
                "mode" => mode.as_str(),
                "code" => code.split(':').next().unwrap_or_default().to_string(),
            },
        );
        Err(code)
    };
    // GRD-FR-HMDW: both names must be branches of the project's repository.
    let repo = project_repo(app)?;
    for name in [head, base] {
        if let Err(reason) = crate::changes::resolve_branch_commit(&repo, name) {
            return refuse(reason);
        }
    }
    // GRD-FR-JAUJ: one PR summary run at a time for one head branch.
    let key = pr_queue_key(head);
    if let Some(queue) = queue::project_queue(app) {
        if let Some(active) =
            queue.runs.iter().find(|run| run.queue_key() == key && !run.state.is_terminal())
        {
            return refuse(format!("{ERR_PR_SUMMARY_ACTIVE}: {}", active.id));
        }
    }
    // GRD-FR-MACY: a `combine` run captures its input at enqueue.
    let inputs = match mode {
        PrSummaryMode::Diff => Vec::new(),
        PrSummaryMode::Combine => summaries(app, head),
    };
    if mode == PrSummaryMode::Combine && inputs.is_empty() {
        return refuse(ERR_NO_RUN_SUMMARIES.to_string());
    }

    // GRD-FR-KAKU: no draft and no stream. The captured input stays empty.
    let input = CapturedGraduationInput {
        draft_id: String::new(),
        draft_name: String::new(),
        prompt: String::new(),
        prompt_checksum: String::new(),
        captured_at: crate::notes::now_rfc3339(),
    };
    let mut run = commands::build_run(
        &project_key,
        String::new(),
        String::new(),
        None,
        input,
        StandingWork::Keep,
        None,
    );
    run.pr_summary = Some(PrSummaryData {
        name: pr_summary_title(head),
        head: head.to_string(),
        base: base.to_string(),
        mode,
        head_tip: None,
        merge_base: None,
        inputs,
        result: None,
    });
    // GRS-FR-KDOY: both log streams are created in the write that creates the
    // run.
    crate::graduation::logs::initialize(app, &mut run);
    runs::save_run(app, &mut run)?;
    events::announce_queue(app, &run);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation enqueued a PR summary run",
        log_fields! {
            "run_id" => run.id.clone(),
            "head" => head.to_string(),
            "mode" => mode.as_str(),
            "inputs" => run.pr_summary.as_ref().map(|data| data.inputs.len()).unwrap_or(0) as i64,
        },
    );
    Ok(run)
}

/// The project's repository, opened at its primary worktree.
pub(crate) fn project_repo<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<git2::Repository, String> {
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.root())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    crate::streams::primary_repo_of(&root)
}

/// GRD-FR-TYVT: remove the disposable checkout of a PR summary run that has
/// ended.
///
/// Idempotent. A run a loop of this process is still inside is left alone: its
/// own thread removes the checkout when the turn has returned.
pub fn reclaim_ended_pr_summary_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) {
    if !run.is_pr_summary() || !run.state.is_terminal() {
        return;
    }
    if app.try_state::<GraduationState>().is_some_and(|state| state.is_looping(&run.id)) {
        return;
    }
    if !driver::pr_summary_checkout::stands(app, run) {
        return;
    }
    match driver::pr_summary_checkout::release(app, run) {
        Ok(()) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation reclaimed the checkout of a PR summary run",
            log_fields! { "run_id" => run.id.clone() },
        ),
        Err(reason) => logging::log_error(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not reclaim the checkout of a PR summary run",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        ),
    }
}
