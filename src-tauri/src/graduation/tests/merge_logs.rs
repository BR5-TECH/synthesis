//! What a merge run writes to its two durable log streams, to its observability
//! record and to the application's diagnostic buffer (`GRS-graduation-run-log-
//! storage.md` GRS-FR-XUOA, GRS-FR-ZQEM, GRS-FR-HBQT, GRS-FR-WNRC,
//! `GLG-graduation-loop-logging.md` GLG-FR-QKZV, GLG-FR-MMVN, GLG-FR-FBKP,
//! GLG-FR-HRTD, GLG-FR-QNLC, GLG-FR-WYSP, `GOB-graduation-observability.md`
//! GOB-FR-KRWE, GOB-FR-DNHS, GOB-FR-VCXQ, GOB-FR-MZFA).

use std::collections::BTreeSet;

use super::merge_handoff::*;
use super::*;
use crate::graduation::logs::{GraduationLogProducer, GraduationLogStream};
use crate::streams::StreamMergePublication;

/// A work turn that settles all three unresolved paths.
fn settling() -> Turn {
    Turn::work()
        .writing("README.md", "settled readme\n")
        .writing("both-new.txt", "settled new\n")
        .writing("gone.txt", "kept\n")
}

fn field<'a>(record: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    record.get(key).unwrap_or(&serde_json::Value::Null)
}

/// A merge run handed off and driven with the scripted turns.
fn driven(fx: &Fixture, turns: Vec<Turn>) -> (GraduationRun, Arc<ScriptedDispatch>) {
    fx.allow_execution();
    let stream = conflicted_stream(fx);
    let (run_id, _) = handed_off(fx, &stream, StreamMergePublication::Uncommitted);
    let run = fx.reload(&run_id);
    let dispatch = ScriptedDispatch::new(turns);
    (fx.drive(&run, dispatch.clone()), dispatch)
}

/// The records the application's diagnostic buffer holds for one run.
fn diagnostics(run_id: &str) -> Vec<crate::logging::LogRecord> {
    let filter: crate::logging::LogFilter =
        serde_json::from_value(serde_json::json!({ "query": run_id })).unwrap();
    crate::logging::BUFFER
        .query(&filter, None, 5000)
        .expect("a page")
        .records
}

fn outcome_of(record: &crate::logging::LogRecord) -> Option<&str> {
    record.fields.get("outcome").and_then(|v| v.as_str())
}

// GRS-FR-XUOA, GRS-FR-ZQEM, GRS-FR-HBQT: the producers are the seven of the
// table. No producer is named for a merge, and the phase each carries is the
// table's.
#[test]
fn the_producer_table_has_no_stream_merge_producer() {
    use crate::graduation::GraduationVisualStage as Stage;
    let all = [
        (GraduationLogProducer::QueueWait, "queue_wait", Stage::Queued),
        (GraduationLogProducer::WorkTurn, "work_turn", Stage::Working),
        (GraduationLogProducer::ClarificationJudgement, "clarification_judgement", Stage::Working),
        (GraduationLogProducer::ReviewTurn, "review_turn", Stage::Review),
        (GraduationLogProducer::Commit, "commit", Stage::Done),
        (GraduationLogProducer::SemanticMergeTurn, "semantic_merge_turn", Stage::Done),
    ];
    for (producer, name, phase) in all {
        assert_eq!(producer.as_str(), name);
        assert_eq!(producer.phase_id(None), phase, "{name}");
    }
    assert_eq!(
        GraduationLogProducer::StageTransition.phase_id(Some(Stage::Review)),
        Stage::Review,
        "a transition carries the phase being left"
    );
    assert!(
        serde_json::from_value::<GraduationLogProducer>(serde_json::json!("stream_merge")).is_err(),
        "the producer `stream_merge` does not exist"
    );
    assert_eq!(
        serde_json::to_value(GraduationLogProducer::SemanticMergeTurn).unwrap(),
        "semantic_merge_turn",
        "the update's reconciliation turn keeps its producer"
    );
}

// GRS-FR-HBQT, GRS-FR-WNRC, GRS-FR-XUOA, GLG-FR-FBKP, GLG-FR-QKZV: a merge run's
// turns are attributed as work and review turns of their pass, the apply step is
// one `commit` record with the event `merge_applied`, phase `done`, no pass and
// origin `application`, and no record is attributed to a merge producer.
#[test]
fn a_merge_runs_turns_and_apply_step_are_attributed_by_the_producer_table() {
    let fx = Fixture::new();
    let (run, _) = driven(
        &fx,
        vec![
            Turn::work().writing("README.md", "one\n"),
            Turn::revise(ReviewSeverity::Major, "Not yet."),
            settling(),
            Turn::ready(),
        ],
    );
    assert_eq!(run.state, GraduationRunState::Completed, "{run:?}");

    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    let producers: BTreeSet<&str> = records
        .iter()
        .filter_map(|r| field(r, "producer").as_str())
        .collect();
    assert_eq!(
        producers,
        BTreeSet::from(["queue_wait", "work_turn", "review_turn", "commit"])
    );

    let of = |producer: &str| -> Vec<&serde_json::Value> {
        records
            .iter()
            .filter(|r| field(r, "producer").as_str() == Some(producer))
            .collect()
    };
    let work = of("work_turn");
    assert_eq!(work.len(), 4, "two merge_work turns, each started and ended");
    assert!(work.iter().all(|r| field(r, "phase_id") == "working"));
    let passes: Vec<u64> = work.iter().map(|r| field(r, "pass").as_u64().unwrap()).collect();
    assert_eq!(passes, vec![1, 1, 2, 2]);
    let review = of("review_turn");
    assert!(review.iter().all(|r| field(r, "phase_id") == "review"));
    let passes: Vec<u64> = review.iter().map(|r| field(r, "pass").as_u64().unwrap()).collect();
    assert_eq!(passes, vec![1, 1, 2, 2]);

    // GLG-FR-FBKP: a record outside any pass carries none.
    let queued = of("queue_wait");
    assert_eq!(queued.len(), 1);
    assert_eq!(field(queued[0], "pass"), &serde_json::Value::Null);

    let commit = of("commit");
    assert_eq!(commit.len(), 1, "one apply record");
    assert_eq!(field(commit[0], "event"), "merge_applied");
    assert_eq!(field(commit[0], "phase_id"), "done");
    assert_eq!(field(commit[0], "pass"), &serde_json::Value::Null);
    assert_eq!(field(commit[0], "origin"), "application");
    // The record names counts and no content.
    assert_eq!(field(commit[0], "fields")["paths"].as_u64(), Some(4));
    let text = serde_json::to_string(commit[0]).unwrap();
    assert!(!text.contains("settled"), "no file content: {text}");
}

// GRS-FR-WNRC: a refused or failed apply writes no `merge_applied` record.
#[test]
fn a_blocked_apply_writes_no_merge_applied_record() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    // The stream is dirty when the review lets the merge through.
    let work_tree = stream.worktree();
    let run = fx.reload(&run_id);
    let dispatch = ScriptedDispatch::new(vec![
        settling().doing(move || {
            std::fs::write(work_tree.join("scratch.txt"), "dirty\n").unwrap();
        }),
        Turn::ready(),
    ]);

    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Blocked);
    let records = lines_of(&fx, &run.id, GraduationLogStream::Structured);
    assert!(
        !records.iter().any(|r| field(r, "event") == "merge_applied"),
        "no record of a merge that did not land"
    );
    assert_eq!(run.merge.as_ref().unwrap().result, None);
}

// GLG-FR-HRTD, GLG-FR-QKZV, GLG-FR-MMVN, GXD-FR-CYIW, GRD-FR-NHRY: a merge run has
// no source draft, so its turns write no statistics line at all.
#[test]
fn a_merge_run_writes_no_statistics_line() {
    let fx = Fixture::new();
    let (run, _) = driven(&fx, vec![settling(), Turn::ready()]);
    assert_eq!(run.state, GraduationRunState::Completed, "{run:?}");

    crate::statistics::wait_for_writer();

    let directory = fx.root().join("statistics");
    let files: Vec<_> = std::fs::read_dir(&directory)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(files.is_empty(), "no statistics file: {files:?}");
    assert_eq!(run.input.draft_id, "");
}

// GOB-FR-KRWE, GOB-FR-PLTB, GOB-FR-DNHS, GOB-FR-VCXQ, GOB-FR-MZFA, GOB-FR-BTXN: a merge run
// carries the ordinary observability record: the same four stages, the same
// transition reasons (so no stage, condition or reason has a name of its own,
// and the merge labels are a surface's choice by `merge` data), a pass record per pass whose first task is the merge
// statement, and no field that marks it as a merge or holds its result.
#[test]
fn a_merge_run_carries_the_ordinary_observability_record() {
    let fx = Fixture::new();
    let (run, _) = driven(
        &fx,
        vec![
            Turn::work().writing("README.md", "one\n"),
            // One minor finding still starts another pass: the advisory-minor
            // rule does not apply to a merge review.
            Turn::revise(ReviewSeverity::Minor, "A small thing."),
            settling(),
            Turn::ready(),
        ],
    );
    assert_eq!(run.state, GraduationRunState::Completed, "{run:?}");
    let observed = serde_json::to_value(&run.observability).unwrap();

    assert_eq!(observed["observabilityVersion"], 1);
    assert_eq!(observed["currentStage"], "done");
    assert_eq!(observed["stageCondition"], "complete");
    let keys: BTreeSet<&str> = observed.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from(["observabilityVersion", "currentStage", "stageCondition", "stageHistory", "passes"]),
        "no field marks a merge run or holds its result"
    );

    // GOB-FR-DNHS: working at the merge_work dispatch, review at the merge_review
    // dispatch, done with the reason `finished` when the apply succeeds.
    let history: Vec<(String, String, String)> = observed["stageHistory"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["from"].as_str().unwrap().to_string(),
                e["to"].as_str().unwrap().to_string(),
                e["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let expected: Vec<(String, String, String)> = [
        ("queued", "queued", "enqueued"),
        ("queued", "working", "work_started"),
        ("working", "review", "review_started"),
        ("review", "working", "review_revision"),
        ("working", "review", "review_started"),
        ("review", "done", "finished"),
    ]
    .iter()
    .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    .collect();
    assert_eq!(history, expected);

    // GOB-FR-VCXQ: the first task is the merge statement, a later task is the
    // review's instruction, and a `revise` with only a minor finding fails the pass.
    let passes = observed["passes"].as_array().unwrap();
    assert_eq!(passes.len(), 2);
    assert_eq!(passes[0]["task"], run.input.prompt.as_str());
    assert_eq!(passes[0]["status"], "failed");
    assert_eq!(passes[0]["findings"].as_array().unwrap().len(), 1);
    assert!(passes[1]["task"].as_str().unwrap().starts_with("The review asked for these changes."));
    assert_eq!(passes[1]["status"], "passed");

    // GOB-FR-MZFA: the result is a field of the run's merge data, which stands
    // from the moment the run is completed.
    let result = run.merge.as_ref().unwrap().result.as_ref().expect("a result");
    assert_eq!(result.published, MergePublished::Uncommitted);
    assert_eq!(result.commit, None);
    assert!(result.merged_paths.contains(&"README.md".to_string()));
}

// GOB-FR-VCXQ, GOB-FR-BTXN, GRS-FR-HBQT, GXD-FR-BQLN: pass numbers ascend through
// the whole life of a merge run: a Continue after the budget is spent starts the
// next number, in the pass records and in the log records alike, and a pass
// number is never reused.
#[test]
fn pass_numbers_keep_ascending_across_a_continue() {
    let fx = Fixture::new();
    let (spent, _) = driven(
        &fx,
        vec![
            Turn::work().writing("README.md", "one\n"),
            Turn::revise(ReviewSeverity::Major, "First."),
            Turn::work().writing("README.md", "two\n"),
            Turn::revise(ReviewSeverity::Major, "Second."),
        ],
    );
    assert_eq!(spent.state, GraduationRunState::AwaitingAuthor, "{spent:?}");
    assert_eq!(spent.checkpoint.pass_limit, 2);

    let continued = fx.continue_run(&spent);
    assert_eq!(continued.checkpoint.pass_floor, 3, "the window moves past the spent passes");
    assert_eq!(continued.checkpoint.pass_limit, 4, "and again holds two passes");
    let done = fx.drive(&continued, ScriptedDispatch::new(vec![settling(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");

    let passes: Vec<u64> = serde_json::to_value(&done.observability).unwrap()["passes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pass"].as_u64().unwrap())
        .collect();
    assert_eq!(passes, vec![1, 2, 3]);
    let records = lines_of(&fx, &done.id, GraduationLogStream::Structured);
    let work_passes: BTreeSet<u64> = records
        .iter()
        .filter(|r| field(r, "producer") == "work_turn")
        .filter_map(|r| field(r, "pass").as_u64())
        .collect();
    assert_eq!(work_passes, BTreeSet::from([1, 2, 3]));
    let review_passes: BTreeSet<u64> = records
        .iter()
        .filter(|r| field(r, "producer") == "review_turn")
        .filter_map(|r| field(r, "pass").as_u64())
        .collect();
    assert_eq!(review_passes, BTreeSet::from([1, 2, 3]));
}

// GLG-FR-QNLC, GLG-FR-WYSP, GRB-FR-OHWT: the loop writes one backend record at the
// boundaries of a merge run, correlated by the run id, with the outcome, the
// domains the spec sets, and no content.
#[test]
fn the_boundaries_of_a_merge_run_are_recorded_without_content() {
    use crate::logging::{Domain, LogLevel};
    let fx = Fixture::new();
    let (run, _) = driven(&fx, vec![settling(), Turn::ready()]);
    assert_eq!(run.state, GraduationRunState::Completed, "{run:?}");
    let records = diagnostics(&run.id);
    let boundary = |name: &str| -> Vec<&crate::logging::LogRecord> {
        records
            .iter()
            .filter(|r| r.fields.get("boundary").and_then(|v| v.as_str()) == Some(name))
            .collect()
    };

    // One dispatch record for each of the two turns, naming the turn kind.
    let dispatches = boundary("dispatch");
    let kinds: Vec<&str> = dispatches
        .iter()
        .filter_map(|r| r.fields.get("turn_kind").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(kinds, vec!["merge_work", "merge_review"]);
    for record in &dispatches {
        assert_eq!(record.fields["pass"].as_u64(), Some(1));
    }

    // The tip check before the dispatches found both tips standing: at least
    // one check stands before each of the two turns.
    let checks = boundary("tip_check");
    assert!(checks.len() >= 2, "{} checks", checks.len());
    for record in &checks {
        assert_eq!(outcome_of(record), Some("ok"));
        assert_eq!(record.level, LogLevel::Info);
    }

    // The apply step, with the counts and the pinned tips.
    let applies = boundary("apply");
    assert_eq!(applies.len(), 1);
    assert_eq!(outcome_of(applies[0]), Some("applied"));
    assert_eq!(applies[0].fields["paths"].as_u64(), Some(4));
    let data = run.merge.as_ref().unwrap();
    assert_eq!(applies[0].fields["base_tip"], data.base_tip.as_str());
    assert_eq!(applies[0].fields["stream_tip"], data.stream_tip.as_str());

    // The cleanup record carries the domain `backend` alone.
    let cleanups = boundary("cleanup");
    assert_eq!(cleanups.len(), 1, "one cleanup, one record");
    assert_eq!(outcome_of(cleanups[0]), Some("reclaimed"));
    assert_eq!(cleanups[0].domains, vec![Domain::Backend]);

    // Every other boundary carries `ai` and `backend` together.
    for record in dispatches.iter().chain(&checks).chain(&applies) {
        assert!(record.domains.contains(&Domain::Ai) && record.domains.contains(&Domain::Backend));
    }
    // Every record of the run names the run and no content: no marker text and
    // no file body appear in any field.
    for record in &records {
        let text = serde_json::to_string(&record.fields).unwrap();
        for forbidden in ["<<<<<<<", "settled readme", "settled new", "base side", "stream side"] {
            assert!(!text.contains(forbidden), "{} carries `{forbidden}`", record.message);
        }
    }
}

// GLG-FR-QNLC, GRB-FR-BXEJ, GRD-FR-XHSE: a refused tip check is a record of level
// `WARN` with the outcome `merge_branch_moved`.
#[test]
fn a_refused_tip_check_is_a_warning_with_the_typed_outcome() {
    use crate::logging::{Domain, LogLevel};
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    commit_file(&fx.repo(), "later.txt", "later\n", "the base moves");
    let run = fx.reload(&run_id);

    let run = fx.drive(&run, ScriptedDispatch::new(vec![settling(), Turn::ready()]));

    assert_eq!(run.state, GraduationRunState::Failed);
    let refused: Vec<crate::logging::LogRecord> = diagnostics(&run_id)
        .into_iter()
        .filter(|r| outcome_of(r) == Some("merge_branch_moved"))
        .collect();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].level, LogLevel::Warn);
    assert!(refused[0].domains.contains(&Domain::Ai) && refused[0].domains.contains(&Domain::Backend));
    assert_eq!(refused[0].fields["boundary"], "tip_check");
}

// GLG-FR-QNLC: an apply that cannot proceed is recorded with its blocker code as
// the outcome; one the guard refuses or a dirty side refuses is a warning, and one
// that failed to write is an error.
#[test]
fn a_blocked_apply_is_recorded_with_its_blocker_code() {
    use crate::logging::LogLevel;
    // A dirty stream.
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let work_tree = stream.worktree();
    let dispatch = ScriptedDispatch::new(vec![
        settling().doing(move || std::fs::write(work_tree.join("scratch.txt"), "x\n").unwrap()),
        Turn::ready(),
    ]);
    fx.drive(&fx.reload(&run_id), dispatch);
    let record = diagnostics(&run_id)
        .into_iter()
        .find(|r| r.fields.get("boundary").and_then(|v| v.as_str()) == Some("apply"))
        .expect("an apply record");
    assert_eq!(outcome_of(&record), Some("merge_dirty_side"));
    assert_eq!(record.level, LogLevel::Warn);

    // The guard held.
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let state = fx.app.state::<crate::streams::StreamState>();
    let hold = state
        .begin_repository_update("another", crate::streams::Reconciliation::Update)
        .expect("a free guard");
    fx.drive(&fx.reload(&run_id), ScriptedDispatch::new(vec![settling(), Turn::ready()]));
    drop(hold);
    let record = diagnostics(&run_id)
        .into_iter()
        .find(|r| r.fields.get("boundary").and_then(|v| v.as_str()) == Some("apply"))
        .expect("an apply record");
    assert_eq!(outcome_of(&record), Some("merge_guard_held"));
    assert_eq!(record.level, LogLevel::Warn);

    // A write that fails: the base branch is held by no worktree.
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    fx.drive(&fx.reload(&run_id), ScriptedDispatch::new(vec![settling(), Turn::ready()]));
    let record = diagnostics(&run_id)
        .into_iter()
        .find(|r| r.fields.get("boundary").and_then(|v| v.as_str()) == Some("apply"))
        .expect("an apply record");
    assert_eq!(outcome_of(&record), Some("merge_apply_failed"));
    assert_eq!(record.level, LogLevel::Error);
}
