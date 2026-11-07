//! The decision ledger and the review history of a run
//! (`GXD-graduation-execution.md` GXD-FR-NWSL, GXD-FR-HRDM, GXD-FR-BTIN,
//! GXD-FR-WRVH, GXD-FR-QPFT).

use super::*;

fn finding(description: &str) -> ReviewFinding {
    ReviewFinding {
        severity: ReviewSeverity::Major,
        description: description.into(),
        affected_files: vec!["src/panel.ts".into()],
        correction: "Fix it.".into(),
    }
}

fn decision(question: &str, turn: &str, pass: u32) -> GraduationDecision {
    GraduationDecision {
        question: question.into(),
        answer: "An answer.".into(),
        summary: "Summary".into(),
        note: Some("A note.".into()),
        turn: turn.into(),
        pass,
    }
}

// GXD-FR-NWSL, GXD-FR-HSQV: the turn a ledger entry names follows the origin of
// the escalation and the kind of the run.
#[test]
fn the_turn_of_an_entry_follows_the_origin_and_the_run_kind() {
    use GraduationEscalationOrigin::*;
    assert_eq!(Work.decision_turn(false), "work");
    assert_eq!(Review.decision_turn(false), "review");
    assert_eq!(Work.decision_turn(true), "merge_work");
    assert_eq!(Review.decision_turn(true), "merge_review");
    assert_eq!(SemanticMerge.decision_turn(false), "semantic_rebase");
}

// GXD-FR-BTIN: every turn kind carries the whole ledger in order, and the
// ledger is present and empty before the first answer.
#[test]
fn every_turn_kind_carries_the_whole_ledger() {
    let ledger = vec![decision("First?", "work", 1), decision("Second?", "review", 2)];
    let mut draft = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    draft.input.prompt = "Do it.".into();
    let mut merge = super::merge_input::bare_merge_run();

    for run in [&mut draft, &mut merge] {
        for part in [driver::PART_WORK, driver::PART_REVIEW] {
            let empty = driver::compose_input(run, part, driver::PURPOSE_GENERATE).to_map();
            assert_eq!(empty.get("decisions"), Some(&serde_json::json!([])));
        }
        run.checkpoint.decisions = ledger.clone();
        for part in [driver::PART_WORK, driver::PART_REVIEW] {
            let input = driver::compose_input(run, part, driver::PURPOSE_RESUME);
            assert_eq!(input.decisions, ledger, "the whole ledger, in order, for {part}");
        }
    }
}

// GXD-FR-QPFT, GXD-FR-WRVH: a work turn is given the findings of the passes
// before the latest review. A review turn is given none.
#[test]
fn only_a_work_turn_carries_the_earlier_findings() {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    run.input.prompt = "Do it.".into();
    run.checkpoint.review_history = vec![
        ReviewHistoryEntry { pass: 1, finding: finding("First.") },
        ReviewHistoryEntry { pass: 1, finding: finding("Second.") },
        ReviewHistoryEntry { pass: 2, finding: finding("Latest.") },
    ];
    let work = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_REVISE);
    let described: Vec<(u32, &str)> =
        work.review_history.iter().map(|e| (e.pass, e.description.as_str())).collect();
    assert_eq!(described, vec![(1, "First."), (1, "Second.")]);
    assert_eq!(work.review_history[0].severity, "major");
    assert_eq!(work.review_history[0].affected_files, vec!["src/panel.ts".to_string()]);

    let review = driver::compose_input(&run, driver::PART_REVIEW, driver::PURPOSE_GENERATE);
    assert!(review.review_history.is_empty());
    assert!(review.to_map().get("review_history").is_none());
}

// GXD-FR-QPFT: the first revise pass has the latest findings in its
// instruction and nothing before them.
#[test]
fn the_latest_findings_are_not_repeated_in_the_history() {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    run.input.prompt = "Do it.".into();
    run.checkpoint.review_history = vec![ReviewHistoryEntry { pass: 1, finding: finding("One.") }];
    let work = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_REVISE);
    assert!(work.review_history.is_empty());
}

// GXD-FR-NWSL: a merge run's work turn gets the history and its review turn
// does not; the merge context stays.
#[test]
fn a_merge_work_turn_carries_the_history_and_a_merge_review_turn_does_not() {
    let mut run = super::merge_input::bare_merge_run();
    run.checkpoint.review_history = vec![
        ReviewHistoryEntry { pass: 1, finding: finding("Old.") },
        ReviewHistoryEntry { pass: 2, finding: finding("Latest.") },
    ];
    let work = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_REVISE);
    assert_eq!(work.part, "merge_work");
    assert_eq!(work.review_history.len(), 1);
    let review = driver::compose_input(&run, driver::PART_REVIEW, driver::PURPOSE_GENERATE);
    assert_eq!(review.part, "merge_review");
    assert!(review.review_history.is_empty());
}

// GXD-FR-GMDI, GXD-FR-QGYA, GXD-FR-ZLBK, GRD-FR-KQLD: the ledger and the history
// are stored with the checkpoint in the run record, and a record written before
// they existed reads as empty.
#[test]
fn the_checkpoint_stores_both_and_an_old_record_reads_as_empty() {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    run.checkpoint.decisions = vec![decision("First?", "work", 1)];
    run.checkpoint.review_history = vec![ReviewHistoryEntry { pass: 1, finding: finding("One.") }];
    run.checkpoint.verdict_refusals = 1;
    run.checkpoint.pass_limit = 2;
    let text = toml::to_string(&run).expect("the record serializes");
    let read: GraduationRun = toml::from_str(&text).expect("the record reads back");
    assert_eq!(read.checkpoint.decisions, run.checkpoint.decisions);
    assert_eq!(read.checkpoint.review_history, run.checkpoint.review_history);
    assert_eq!(read.checkpoint.verdict_refusals, 1);

    let bare = GraduationCheckpoint::default();
    let text = toml::to_string(&bare).expect("serializes");
    let stripped: String = text
        .lines()
        .filter(|line| !line.starts_with("decisions") && !line.starts_with("reviewHistory"))
        .collect::<Vec<_>>()
        .join("\n");
    let old: GraduationCheckpoint = toml::from_str(&stripped).expect("an old record reads");
    assert!(old.decisions.is_empty() && old.review_history.is_empty());
}

// GRL-FR-BNDD, GRL-FR-VHIS, GRL-FR-RTNC: the instructions say what the spec
// asks of each turn kind.
#[test]
fn the_instructions_state_the_binding_decisions_and_the_review_rules() {
    let work = include_str!("../../../../resources/prompts/graduation/work.md");
    let merge_work = include_str!("../../../../resources/prompts/graduation/merge-work.md");
    let review = include_str!("../../../../resources/prompts/graduation/review.md");
    let merge_review = include_str!("../../../../resources/prompts/graduation/merge-review.md");
    let rebase = include_str!("../../../../resources/prompts/graduation/rebase.md");

    for (name, text) in [("work", work), ("merge-work", merge_work)] {
        assert!(text.contains("`input.decisions`") && text.contains("binding"), "{name}");
        assert!(text.contains("`input.review_history`"), "{name}");
        assert!(text.contains("probably already fixed"), "{name}");
    }
    for (name, text) in [("review", review), ("merge-review", merge_review)] {
        assert!(text.contains("`input.decisions`") && text.contains("binding"), "{name}");
        assert!(!text.contains("input.review_history"), "{name} names no review history");
    }
    assert!(rebase.contains("binding"));
    assert!(review.contains("You run no test command") || review.contains("run no test command"));
    assert!(!review.contains("definition of done"));
    assert!(
        merge_review.contains("test commands") || merge_review.contains("`task tests`"),
        "the merge review keeps its commands"
    );
}
