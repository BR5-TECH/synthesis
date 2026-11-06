//! Starting a PR summary run, and what the run order says about it
//! (`GRD-graduation.md` GRD-FR-KAKU, GRD-FR-HMDW, GRD-FR-JAUJ, GRD-FR-MACY,
//! GRD-FR-CQGB, GRD-FR-ZHNV).

use super::pr_summary_turn::commit_on_branch;
use super::*;

/// A fixture with a `main` branch and a head branch one commit ahead of it,
/// whose queue never spawns a loop on its own.
fn with_branches() -> Fixture {
    let fx = Fixture::new();
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("main", &head, true).unwrap();
    commit_on_branch(&repo, "feature/panel", "panel.ts", "export {};\n");
    fx
}

fn start(
    fx: &Fixture,
    head: &str,
    base: &str,
    mode: PrSummaryMode,
) -> Result<GraduationRun, String> {
    crate::graduation::start_pull_request_summary(
        fx.app.clone(),
        head.to_string(),
        base.to_string(),
        mode,
    )
}

fn run_count(fx: &Fixture) -> usize {
    project_queue(&fx.app).map(|queue| queue.runs.len()).unwrap_or(0)
}

// GRD-FR-HMDW, GRD-FR-JAUJ, GRD-FR-KAKU: the start enqueues a `queued` run in
// the queue `pr:<head>`, titled `Summarize <head>`, with no draft and no
// stream, and the run order's index says what it is.
#[test]
fn a_start_enqueues_a_pr_summary_run_in_the_queue_of_its_head() {
    let fx = with_branches();
    let run = start(&fx, "feature/panel", "main", PrSummaryMode::Diff).expect("started");

    assert_eq!(run.state, GraduationRunState::Queued);
    assert_eq!(run.queue_key(), "pr:feature/panel");
    let data = run.pr_summary.as_ref().expect("PR summary data");
    assert_eq!(data.name, "Summarize feature/panel");
    assert_eq!((data.head.as_str(), data.base.as_str()), ("feature/panel", "main"));
    assert_eq!(data.mode, PrSummaryMode::Diff);
    assert!(data.head_tip.is_none() && data.merge_base.is_none(), "pinned at the dispatch");
    assert!(data.inputs.is_empty() && data.result.is_none());
    assert!(run.is_pr_summary() && !run.is_merge() && !run.is_direct());
    assert!(run.input.draft_id.is_empty() && run.stream_id.is_empty());
    // GRD-FR-HMDW: the start offers the project a dispatch, and the queue it
    // offers is free: no stream is marked for a PR summary queue.
    let attempts = fx.app.state::<crate::graduation::driver::drive::SpawnTripwire>().take();
    assert_eq!(attempts, vec![(run.id.clone(), true)]);

    let index = read_queue_index(
        &store_fs(&fx.app).unwrap(),
        &store_base(&fx.app).unwrap(),
        &run.project_key,
    );
    let entry = index.runs.iter().find(|entry| entry.run_id == run.id).expect("indexed");
    assert!(entry.pr_summary && !entry.merge);
    assert_eq!(entry.queue_key(), "pr:feature/panel");
    // The record reads back on the wire shape, camelCase.
    let wire = serde_json::to_value(&run).unwrap();
    assert_eq!(wire["prSummary"]["name"], "Summarize feature/panel");
    assert_eq!(wire["prSummary"]["mode"], "diff");
}

// GRD-FR-HMDW: a head or a base that is not a branch returns `unknown branch`,
// and nothing is enqueued.
#[test]
fn a_name_that_is_not_a_branch_enqueues_nothing() {
    let fx = with_branches();
    for (head, base) in [("feature/none", "main"), ("feature/panel", "release")] {
        assert_eq!(
            start(&fx, head, base, PrSummaryMode::Diff).unwrap_err(),
            crate::changes::ERR_UNKNOWN_BRANCH,
        );
    }
    assert_eq!(run_count(&fx), 0);
}

// GRD-FR-JAUJ: a second start for a head whose queue holds a run that is not
// terminal is refused with `pr_summary_active` and the run id. Another head
// has a queue of its own.
#[test]
fn a_second_start_for_the_same_head_is_refused_while_the_first_stands() {
    let fx = with_branches();
    let first = start(&fx, "feature/panel", "main", PrSummaryMode::Diff).expect("started");
    let refused = start(&fx, "feature/panel", "main", PrSummaryMode::Diff).unwrap_err();
    assert_eq!(refused, format!("{ERR_PR_SUMMARY_ACTIVE}: {}", first.id));
    assert_eq!(run_count(&fx), 1);

    commit_on_branch(&fx.repo(), "feature/other", "other.ts", "x\n");
    let other = start(&fx, "feature/other", "main", PrSummaryMode::Diff).expect("own queue");
    assert_eq!(other.queue_key(), "pr:feature/other");

    // A discarded run is terminal, so the head may start again.
    crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone()).expect("gone");
    start(&fx, "feature/panel", "main", PrSummaryMode::Diff).expect("started again");
}

// GRD-FR-MACY: a `combine` request with no run summary is refused with
// `no_run_summaries` and enqueues nothing. No run has landed on the head, so
// the production source holds none.
#[test]
fn a_combine_request_with_no_run_summary_is_refused() {
    let fx = with_branches();
    assert!(completed_run_summaries(&fx.app, "feature/panel").is_empty());
    assert_eq!(
        start(&fx, "feature/panel", "main", PrSummaryMode::Combine).unwrap_err(),
        ERR_NO_RUN_SUMMARIES,
    );
    assert_eq!(run_count(&fx), 0);
}

// GRD-FR-MACY: a `combine` run captures the run summaries at enqueue, in the
// order its source gives them.
#[test]
fn a_combine_run_captures_its_summaries_at_enqueue() {
    let fx = with_branches();
    let summaries = vec![
        PrSummaryInput { run_id: "g1".into(), title: "One".into(), description: "1".into() },
        PrSummaryInput { run_id: "g2".into(), title: "Two".into(), description: "2".into() },
    ];
    let given = summaries.clone();
    let run = enqueue_pull_request_summary(
        &fx.app,
        "feature/panel",
        "main",
        PrSummaryMode::Combine,
        move |_, head| {
            assert_eq!(head, "feature/panel");
            given
        },
    )
    .expect("enqueued");
    assert_eq!(run.pr_summary.as_ref().unwrap().inputs, summaries);
    assert_eq!(fx.reload(&run.id).pr_summary.unwrap().inputs, summaries, "durably");
}

// GRD-FR-CQGB: auto-start, archive and discard apply as to every run; restart
// is refused with `run_state_not_permitted` and revert with
// `run_not_revertable`.
#[test]
fn a_pr_summary_run_takes_the_common_controls_and_refuses_restart_and_revert() {
    let fx = with_branches();
    let run = start(&fx, "feature/panel", "main", PrSummaryMode::Diff).expect("started");

    let off = crate::graduation::set_graduation_auto_start(fx.app.clone(), run.id.clone(), false)
        .expect("auto-start");
    assert!(!off.auto_start);
    let archived =
        crate::graduation::archive_graduation_run(fx.app.clone(), run.id.clone()).unwrap();
    assert!(archived.archived);
    crate::graduation::unarchive_graduation_run(fx.app.clone(), run.id.clone()).unwrap();

    assert_eq!(
        crate::graduation::revert_graduation_run(fx.app.clone(), run.id.clone()).unwrap_err(),
        ERR_RUN_NOT_REVERTABLE,
    );
    let discarded =
        crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).unwrap();
    assert_eq!(discarded.state, GraduationRunState::Discarded);
    let restart = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        run.id.clone(),
        String::new(),
        StandingWork::Keep,
        None,
    )
    .unwrap_err();
    assert_eq!(
        restart,
        format!("{ERR_RUN_STATE_NOT_PERMITTED}: a PR summary run is not restarted")
    );
}

// GRD-FR-CQGB, GRD-FR-JAUJ: a reorder applies to a queued PR summary run as to
// every run, within its own queue `pr:<head>`. The queue of another head is
// another queue, so a move never crosses into it.
#[test]
fn a_pr_summary_run_is_reordered_within_its_own_queue() {
    let fx = with_branches();
    let run = start(&fx, "feature/panel", "main", PrSummaryMode::Diff).expect("started");
    commit_on_branch(&fx.repo(), "feature/other", "other.ts", "x\n");
    let other = start(&fx, "feature/other", "main", PrSummaryMode::Diff).expect("own queue");
    let order = |queue: &GraduationQueue| -> Vec<String> {
        queue.runs.iter().map(|run| run.id.clone()).collect()
    };
    let before = order(&project_queue(&fx.app).unwrap());

    let reorder = |id: &str, expected: usize, new: usize| {
        crate::graduation::reorder_graduation_run(fx.app.clone(), id.to_string(), expected, new)
    };
    let same = reorder(&run.id, 0, 0).expect("a move to its own place");
    assert_eq!(order(&same), before);
    assert_eq!(
        reorder(&other.id, 0, 1).unwrap_err(),
        format!("{ERR_QUEUE_POSITION_OUT_OF_RANGE}: 1"),
        "the run of another head is not in this queue",
    );
    assert_eq!(reorder(&run.id, 1, 0).unwrap_err(), format!("{ERR_QUEUE_POSITION_STALE}: 0"));
    assert_eq!(order(&project_queue(&fx.app).unwrap()), before, "a refusal moves nothing");
    assert_eq!(fx.reload(&run.id).state, GraduationRunState::Queued);
}

// GRD-FR-JAUJ, GRD-FR-CCHK, GRD-FR-RKWQ: a PR summary run holds its queue and a
// slot on the terms of every run, except that a `failed` one holds neither. A
// failed run of a draft still holds its queue.
#[test]
fn a_failed_pr_summary_run_alone_gives_its_queue_back() {
    use GraduationRunState as S;
    for state in [S::Working, S::Interrupted, S::Blocked, S::AwaitingAuthor] {
        assert!(holds_queue_in(state, true, true), "{state:?} holds the queue");
        assert!(holds_slot_in(state, true, true), "{state:?} holds a slot");
    }
    assert!(holds_queue_in(S::Queued, true, true) && holds_slot_in(S::Queued, true, true));
    assert!(!holds_queue_in(S::Queued, false, true), "never dispatched, holds nothing");
    assert!(!holds_queue_in(S::Failed, true, true) && !holds_slot_in(S::Failed, true, true));
    assert!(holds_queue_in(S::Failed, true, false), "a failed draft run still holds its queue");
    assert!(!holds_slot_in(S::Failed, true, false));
}

// GRD-FR-ZHNV: the operation of an executing PR summary run is labelled with
// its title.
#[test]
fn the_progress_label_of_a_pr_summary_run_names_its_title() {
    let run = super::pr_summary_turn::pr_summary_run("feature/panel", PrSummaryMode::Diff);
    assert_eq!(crate::graduation::run_progress::label_of(&run), "Summarize feature/panel…");
}
