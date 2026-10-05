//! Journey 37 of the coverage matrix: what a merge run writes to its logs
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! The turns of a merge run are attributed as work and review turns, its apply
//! step is a `commit` record, and the loop writes no statistic for it.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::{GraduationLogStream, GraduationRunState, ReviewSeverity};

/// Every record of one log stream of the run in focus, read from the run store.
fn records(outcome: &super::outcome::Outcome, stream: GraduationLogStream) -> Vec<serde_json::Value> {
    super::super::lines_of(outcome.fixture(), outcome.run_id(), stream)
}

fn text<'a>(record: &'a serde_json::Value, key: &str) -> &'a str {
    record.get(key).and_then(|v| v.as_str()).unwrap_or_default()
}

fn passes(records: &[serde_json::Value], producer: &str) -> Vec<u64> {
    let mut found: Vec<u64> = records
        .iter()
        .filter(|record| text(record, "producer") == producer)
        .filter_map(|record| record.get("pass").and_then(|v| v.as_u64()))
        .collect();
    found.sort();
    found.dedup();
    found
}

// GTE-FR-VRHN, GTE-FR-RDPE / GRS-FR-HBQT, GRS-FR-WNRC, GRS-FR-XUOA, GRS-FR-KJVN,
// GLG-FR-HRTD, GLG-FR-FBKP: a `merge_work` turn is a `work_turn` producer in the
// phase `working` and a `merge_review` turn a `review_turn` producer in the phase
// `review`, each with the number of its pass, and the numbers ascend across a
// Continue. No producer is named for a merge, a record outside a pass carries no
// pass, and the apply is one `commit` record, `merge_applied`, in the phase
// `done` with no pass and no file content.
#[test]
fn the_turns_of_a_merge_run_are_attributed_as_work_and_review_turns() {
    let outcome = conflicted_merge("merge attribution")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "one\n"))
        .merge_review_turn(Script::answers_revise(ReviewSeverity::Major, "First."))
        .merge_work_turn(Script::reports_success().writes("README.md", "two\n"))
        .merge_review_turn(Script::answers_revise(ReviewSeverity::Major, "Second."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .continued(vec![
            Script::reports_success().writes("README.md", "both sides secret\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed);

    for stream in [GraduationLogStream::Structured, GraduationLogStream::Source] {
        let records = records(&outcome, stream);
        assert!(!records.is_empty(), "the run's {stream:?} stream holds records");
        for record in &records {
            let producer = text(record, "producer");
            assert!(
                [
                    "queue_wait",
                    "work_turn",
                    "clarification_judgement",
                    "review_turn",
                    "commit",
                    "stage_transition"
                ]
                .contains(&producer),
                "no producer is named for a merge: {producer}"
            );
            match producer {
                "work_turn" => assert_eq!(text(record, "phase_id"), "working", "a merge_work turn works"),
                "review_turn" => assert_eq!(text(record, "phase_id"), "review", "a merge_review turn reviews"),
                "queue_wait" => assert!(record["pass"].is_null(), "a queue wait carries no pass"),
                _ => {}
            }
        }
        assert_eq!(passes(&records, "work_turn"), vec![1, 2, 3], "work turn passes ascend across a Continue");
        assert_eq!(passes(&records, "review_turn"), vec![1, 2, 3], "review turn passes ascend across a Continue");
    }

    let structured = records(&outcome, GraduationLogStream::Structured);
    let applied: Vec<&serde_json::Value> = structured
        .iter()
        .filter(|record| text(record, "event") == "merge_applied")
        .collect();
    assert_eq!(applied.len(), 1, "the apply writes one record");
    let record = applied[0];
    assert_eq!(text(record, "producer"), "commit");
    assert_eq!(text(record, "phase_id"), "done");
    assert!(record["pass"].is_null(), "the apply carries no pass");
    assert_eq!(text(record, "origin"), "application");
    assert!(
        !record.to_string().contains("secret"),
        "the apply record holds no file content"
    );
    outcome.expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-VRHN, GTE-FR-CZPM / GRS-FR-WNRC, GRD-FR-JSBE: an apply that is refused
// writes no `merge_applied` record. The retry that lands the merge writes exactly
// one.
#[test]
fn a_blocked_apply_writes_no_apply_record_and_the_retry_writes_one() {
    let applied = |outcome: &super::outcome::Outcome| {
        records(outcome, GraduationLogStream::Structured)
            .iter()
            .filter(|record| text(record, "event") == "merge_applied")
            .count()
    };
    let outcome = conflicted_merge("merge apply record")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(
            Script::answers_ready("Both intents stand.").dirtying_the_base_meanwhile("scratch.txt", "mine\n"),
        )
        .run()
        .expect_state(GraduationRunState::Blocked);
    assert_eq!(applied(&outcome), 0, "a refused apply writes no record");
    let outcome = outcome
        .cleaning_the_base("scratch.txt")
        .continued(Vec::new())
        .expect_state(GraduationRunState::Completed);
    assert_eq!(applied(&outcome), 1, "the apply that landed writes one record");
}

// GTE-FR-VRHN / GLG-FR-HRTD, GRD-FR-NHRY, GRD-FR-VCTH, GXD-FR-CYIW: a merge run has
// no source draft, so the loop writes no statistic for any of its turns: no
// `agent_operation` line and no `token_usage` line stands in the project's
// statistics store once the writer has settled.
#[test]
fn a_merge_run_writes_no_statistic_line() {
    let outcome = conflicted_merge("merge statistics")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_merge_run_named("Merge feature");
    crate::statistics::wait_for_writer();
    let store = outcome.fixture().root().join("statistics");
    let root = outcome.fixture().root();
    let access = super::settings::guard(&root);
    let lines: Vec<String> = access
        .list_dir(&store)
        .map(|entries| {
            entries
                .into_iter()
                .filter_map(|entry| access.read_text(store.join(entry.name)).ok())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        lines.iter().all(|text| !text.contains("agent_operation") && !text.contains("token_usage")),
        "no statistic line was written for the merge run: {lines:?}"
    );
}
