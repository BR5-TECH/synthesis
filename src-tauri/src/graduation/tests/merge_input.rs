//! What a turn of a merge run is told and bounded by: the task input, the
//! instruction, the turn kind and the pass window (`GRL-graduation-loop.md`
//! GRL-FR-TXEB, GRL-FR-GADT, GRL-FR-SLQF, GRL-FR-MRVK, GRL-FR-CZBT,
//! `GXD-graduation-execution.md` GXD-FR-XQJR, GXD-FR-MKTZ, GXD-FR-BQLN,
//! GXD-FR-PWYD).

use super::*;
use crate::streams::{StreamMergeConflict, StreamMergePublication};

/// A merge run record, on the production shape, with no store behind it.
pub(super) fn bare_merge_run() -> GraduationRun {
    let prompt = "Reconcile the merge of the work stream \"feature\" into the branch main.";
    GraduationRun {
        id: "gmerge1".into(),
        stream_id: "w1".into(),
        stream_name: "feature".into(),
        direct_target: None,
        target_hold: None,
        project_key: "p".into(),
        state: GraduationRunState::Queued,
        standing_work: StandingWork::Keep,
        standing_work_message: None,
        standing_work_outcome: None,
        input: CapturedGraduationInput {
            draft_id: String::new(),
            draft_name: String::new(),
            prompt: prompt.into(),
            prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
            captured_at: "t0".into(),
        },
        base_commit: None,
        commits: Vec::new(),
        auto_start: true,
        archived: false,
        archived_at: None,
        work_turns: 0,
        review_turns: 0,
        logs: crate::graduation::logs::GraduationLogIndexes::default(),
        checkpoint: GraduationCheckpoint::default(),
        observability: GraduationObservability::default(),
        escalation: None,
        blocker: None,
        interruption: None,
        restarted_from_run_id: None,
        failure: None,
        merge: Some(GraduationMergeData {
            name: "Merge feature".into(),
            stream_branch: "synthesis/stream/feature".into(),
            base_branch: "main".into(),
            base_tip: "a".repeat(40),
            stream_tip: "b".repeat(40),
            merge_base: "c".repeat(40),
            snapshot_commit: "d".repeat(40),
            publication: StreamMergePublication::Commit {
                message: "Land feature".into(),
            },
            changed_paths: vec!["README.md".into(), "src/call.rs".into(), "src/queue.rs".into()],
            unresolved_paths: vec!["src/queue.rs".into()],
            conflicts: vec![StreamMergeConflict {
                path: "src/queue.rs".into(),
                base_change: "updated".into(),
                stream_change: "deleted".into(),
            }],
            result: None,
        }),
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

/// The same record without `merge` data: an ordinary draft run.
pub(super) fn bare_draft_run() -> GraduationRun {
    let mut run = bare_merge_run();
    run.merge = None;
    run.input.draft_id = "d1".into();
    run.input.draft_name = "a draft".into();
    run
}

// GRL-FR-TXEB, GXD-FR-MKTZ: a work turn of a merge run is `merge_work` and
// carries the merge context of the run's data, with no `reconciled_paths`.
#[test]
fn a_merge_work_input_carries_the_merge_context() {
    let run = bare_merge_run();

    let input = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_GENERATE);

    assert_eq!(input.part, "merge_work");
    assert_eq!(input.graduation_input_version, 1);
    assert_eq!(input.draft_name, "", "a merge run has no draft name");
    assert_eq!(input.prompt, run.input.prompt);
    input.validate(2).expect("the input is valid");
    let merge = input.merge.as_ref().expect("the merge context");
    let data = run.merge.as_ref().unwrap();
    assert_eq!(merge.stream_branch, data.stream_branch);
    assert_eq!(merge.base_branch, data.base_branch);
    assert_eq!(merge.base_tip, data.base_tip);
    assert_eq!(merge.stream_tip, data.stream_tip);
    assert_eq!(merge.merge_base, data.merge_base);
    assert_eq!(merge.snapshot_commit, data.snapshot_commit);
    assert_eq!(merge.changed_paths, data.changed_paths);
    assert_eq!(merge.unresolved_paths, data.unresolved_paths);
    assert_eq!(merge.conflicts.len(), 1);
    assert_eq!(merge.conflicts[0].path, "src/queue.rs");
    assert_eq!(merge.conflicts[0].base_change, "updated");
    assert_eq!(merge.conflicts[0].stream_change, "deleted");
    assert!(merge.reconciled_paths.is_none(), "only a review is told what was reconciled");
}

// GRL-FR-TXEB: a review turn of a merge run is `merge_review` and its context
// also holds `reconciled_paths`: the paths the work changed beyond the
// unresolved ones.
#[test]
fn a_merge_review_input_names_the_paths_reconciled_beyond_the_unresolved_ones() {
    let mut run = bare_merge_run();
    run.checkpoint.changed_paths = vec!["src/call.rs".into(), "src/queue.rs".into(), "tests/q.rs".into()];

    let input = driver::compose_input(&run, driver::PART_REVIEW, driver::PURPOSE_GENERATE);

    assert_eq!(input.part, "merge_review");
    input.validate(2).expect("the input is valid");
    assert_eq!(
        input.merge.as_ref().unwrap().reconciled_paths,
        Some(vec!["src/call.rs".to_string(), "tests/q.rs".to_string()])
    );
    // The reconciled set is empty where the work changed only unresolved paths.
    run.checkpoint.changed_paths = vec!["src/queue.rs".into()];
    let input = driver::compose_input(&run, driver::PART_REVIEW, driver::PURPOSE_GENERATE);
    assert_eq!(input.merge.unwrap().reconciled_paths, Some(Vec::new()));
}

// GRL-FR-TXEB: the context is snake_case on the wire, `reconciled_paths` is on a
// review alone, and an ordinary run's input carries no `merge` key at all.
#[test]
fn the_merge_context_is_snake_case_and_absent_from_an_ordinary_input() {
    let run = bare_merge_run();
    let work = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_GENERATE).to_map();
    let review = driver::compose_input(&run, driver::PART_REVIEW, driver::PURPOSE_GENERATE).to_map();

    let context = work["merge"].as_object().expect("an object");
    for key in [
        "stream_branch",
        "base_branch",
        "base_tip",
        "stream_tip",
        "merge_base",
        "snapshot_commit",
        "changed_paths",
        "unresolved_paths",
        "conflicts",
    ] {
        assert!(context.contains_key(key), "{key} is in {context:?}");
    }
    assert!(!context.contains_key("reconciled_paths"));
    assert!(context["conflicts"][0].get("base_change").is_some());
    assert!(context["conflicts"][0].get("stream_change").is_some());
    assert!(review["merge"].as_object().unwrap().contains_key("reconciled_paths"));

    let draft = bare_draft_run();
    for part in [driver::PART_WORK, driver::PART_REVIEW] {
        let map = driver::compose_input(&draft, part, driver::PURPOSE_GENERATE).to_map();
        assert!(!map.contains_key("merge"), "{part}");
        assert_eq!(map["part"], part);
    }
}

// GRL-FR-TXEB: the merge parts are valid with a context alone, and the context
// is valid with a merge part alone.
#[test]
fn the_merge_parts_are_valid_only_with_a_context() {
    let run = bare_merge_run();
    let valid = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_GENERATE);

    let mut without_context = valid.clone();
    without_context.merge = None;
    assert!(without_context.validate(2).is_err(), "merge_work without a context");

    let mut ordinary_part = valid.clone();
    ordinary_part.part = driver::PART_WORK.to_string();
    assert!(ordinary_part.validate(2).is_err(), "a context beside the work part");

    let mut review_part = valid.clone();
    review_part.part = driver::PART_MERGE_REVIEW.to_string();
    review_part.validate(2).expect("merge_review with a context");

    let mut unknown = valid;
    unknown.part = "merge".to_string();
    assert!(unknown.validate(2).is_err());

    // An ordinary part with no context stays valid.
    driver::compose_input(&bare_draft_run(), driver::PART_WORK, driver::PURPOSE_GENERATE)
        .validate(2)
        .expect("an ordinary input");
}

// GRL-FR-GADT, GRL-FR-SLQF, GRL-FR-MRVK: a merge run never compiles work.md,
// review.md or rebase.md, and its two instructions are distinct; an ordinary
// run never gets a merge instruction.
#[test]
fn a_merge_run_carries_its_own_two_instructions() {
    let merge = bare_merge_run();
    let draft = bare_draft_run();

    for part in [driver::PART_WORK, driver::PART_MERGE_WORK] {
        assert_eq!(driver::instruction_for_run(&merge, part), driver::MERGE_WORK_PROMPT.as_str());
    }
    for part in [driver::PART_REVIEW, driver::PART_MERGE_REVIEW] {
        assert_eq!(driver::instruction_for_run(&merge, part), driver::MERGE_REVIEW_PROMPT.as_str());
    }
    assert_ne!(driver::MERGE_WORK_PROMPT.as_str(), driver::MERGE_REVIEW_PROMPT.as_str());
    for part in [
        driver::PART_WORK,
        driver::PART_REVIEW,
        driver::PART_MERGE_WORK,
        driver::PART_MERGE_REVIEW,
        driver::PART_SEMANTIC_MERGE,
    ] {
        let text = driver::instruction_for_run(&merge, part);
        assert_ne!(text, driver::WORK_PROMPT.as_str(), "{part}");
        assert_ne!(text, driver::REVIEW_PROMPT.as_str(), "{part}");
        assert_ne!(text, driver::REBASE_PROMPT.as_str(), "{part}");
    }

    assert_eq!(driver::instruction_for_run(&draft, driver::PART_WORK), driver::WORK_PROMPT.as_str());
    assert_eq!(driver::instruction_for_run(&draft, driver::PART_REVIEW), driver::REVIEW_PROMPT.as_str());
    assert_eq!(
        driver::instruction_for_run(&draft, driver::PART_SEMANTIC_MERGE),
        driver::REBASE_PROMPT.as_str()
    );
}

// GXD-FR-XQJR, GXD-FR-MKTZ, GRL-FR-MWPQ: a merge run dispatches `merge_work` and
// `merge_review` turns and no other kind; an ordinary run is unchanged.
#[test]
fn a_merge_run_dispatches_the_two_merge_turn_kinds() {
    let merge = bare_merge_run();
    let draft = bare_draft_run();

    assert_eq!(driver::turn_kind_for_run(&merge, driver::PART_WORK), TurnKind::MergeWork);
    assert_eq!(driver::turn_kind_for_run(&merge, driver::PART_MERGE_WORK), TurnKind::MergeWork);
    assert_eq!(driver::turn_kind_for_run(&merge, driver::PART_REVIEW), TurnKind::MergeReview);
    assert_eq!(driver::turn_kind_for_run(&merge, driver::PART_MERGE_REVIEW), TurnKind::MergeReview);

    assert_eq!(driver::turn_kind_for_run(&draft, driver::PART_WORK), TurnKind::Work);
    assert_eq!(driver::turn_kind_for_run(&draft, driver::PART_REVIEW), TurnKind::Review);
    assert_eq!(
        driver::turn_kind_for_run(&draft, driver::PART_SEMANTIC_MERGE),
        TurnKind::SemanticRebase
    );
}

// GRL-FR-CZBT, GXD-FR-BQLN, GXD-FR-PWYD, GRL-FR-REPL: a merge run's window is two
// passes whatever the project's budget, and a Continue after exhaustion moves
// the floor so that the window again holds two passes.
#[test]
fn a_merge_runs_window_is_two_passes_whatever_the_projects_budget() {
    let mut run = bare_merge_run();
    assert_eq!(crate::graduation::MERGE_PASS_BUDGET, 2);

    for budget in [1u32, 2, 5, 40] {
        let settings = driver::LoopSettings {
            pass_budget: budget,
            ..driver::LoopSettings::default()
        };
        assert_eq!(settings.pass_limit_for(&run), 2, "budget {budget}, first window");
        assert_eq!(settings.pass_limit_from(&run, 3), 4, "budget {budget}, after a floor move");
        assert_eq!(settings.pass_limit_from(&run, 5), 6, "budget {budget}, after two moves");
    }

    run.checkpoint.pass_floor = 3;
    let settings = driver::LoopSettings::default();
    assert_eq!(settings.pass_limit_for(&run), 4, "the window follows the run's floor");

    // The budget the project sets still governs a run that is not a merge run.
    let draft = bare_draft_run();
    let wide = driver::LoopSettings {
        pass_budget: 5,
        ..driver::LoopSettings::default()
    };
    assert_eq!(wide.pass_limit_for(&draft), 5);
    assert_eq!(wide.pass_limit_from(&draft, 3), 7);
}

// GRL-FR-KWNP: the turns of a merge run read the execution timeout alone: a
// settings value for the pass budget changes nothing of a merge run's window,
// while the timeout stays the project's own.
#[test]
fn a_merge_run_still_reads_the_execution_timeout() {
    let settings = driver::LoopSettings {
        execution_timeout_ms: 123_000,
        pass_budget: 9,
    };
    let merge = bare_merge_run();
    assert_eq!(settings.execution_timeout_ms, 123_000);
    assert_eq!(settings.pass_limit_for(&merge), 2);
}
