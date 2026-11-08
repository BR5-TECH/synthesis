//! What the instructions and the task input say about the check gate
//! (`../ai/GRL-graduation-loop.md` GRL-FR-JGBX, GRL-FR-WRHT, GRL-FR-XSFA,
//! GRL-FR-IPNO, GRL-FR-NTFD, GRL-FR-PNLQ).

use super::*;

fn work_instructions() -> [(&'static str, String); 2] {
    [
        (
            "work.md",
            crate::prompts::instruction_text(include_str!(
                "../../../../resources/prompts/graduation/work.md"
            )),
        ),
        (
            "merge-work.md",
            crate::prompts::instruction_text(include_str!(
                "../../../../resources/prompts/graduation/merge-work.md"
            )),
        ),
    ]
}

fn review_instructions() -> [(&'static str, String); 2] {
    [
        (
            "review.md",
            crate::prompts::instruction_text(include_str!(
                "../../../../resources/prompts/graduation/review.md"
            )),
        ),
        (
            "merge-review.md",
            crate::prompts::instruction_text(include_str!(
                "../../../../resources/prompts/graduation/merge-review.md"
            )),
        ),
    ]
}

// GRL-FR-JGBX, GRL-FR-IJTT: both work instructions say that a turn ends with
// every required check passed or with an escalation, that the environment is
// no exception, and that a success lists each full check in `result.checks`.
// The old way out — a failure the review reads — is gone.
#[test]
fn the_work_instructions_end_a_turn_green_or_with_an_escalation() {
    for (name, text) in work_instructions() {
        for needed in [
            "**Your turn ends with every check the project requires passed, or with an escalation to the author.**",
            "such as the environment",
            "is no exception",
            "answer `escalation_required` and ask the author",
            "A success carries `result`, with `checks`: one entry for each full check, as `.synthesis-checks/report.md` records it.",
            "`name`, `command`, `finished`",
            "`exit_status`",
            "`no_checks_reason`",
            "Report success only where every entry finished with exit status 0 or carries `waived_by`.",
            "do not report `outcome: \"failure\"`",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
        for gone in [
            "or say plainly in your summary that you could not and why",
            "A review will read the working copy either way",
        ] {
            assert!(!text.contains(gone), "{name} still says `{gone}`");
        }
    }
}

// GRL-FR-WRHT, GRL-FR-HLNB: `waived_by` cites a decision of `input.decisions`
// in which the author lets that check fail, and the report names the same
// position.
#[test]
fn the_work_instructions_allow_a_waiver_only_from_an_author_decision() {
    for (name, text) in work_instructions() {
        for needed in [
            "An author's decision in `input.decisions` can let one check fail.",
            "set its `waived_by` to the 1-based position of that decision in `input.decisions`",
            "name the same position in the entry of that check in `.synthesis-checks/report.md`",
            "Never set `waived_by` for any other reason.",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
    }
}

// GRL-FR-XSFA, GRL-FR-KYYW: both work instructions name `check_correction`
// and say that it lets the turn change files, and that the folder is kept for
// a turn that carries it.
#[test]
fn the_work_instructions_act_on_a_check_correction() {
    for (name, text) in work_instructions() {
        for needed in [
            "`input.check_correction`",
            "You may change files to do it.",
            "It keeps the folder for a turn that carries `input.correction` or `input.check_correction`",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
    }
}

// GRL-FR-XSFA: an answer correction keeps the work and changes no file, a
// check correction lets the turn change files, and the escalate correction
// changes no file. None names a value of the answer.
#[test]
fn each_correction_states_whether_the_turn_may_change_files() {
    let refusals = &driver::REFUSALS;
    for section in driver::corrections::correction_sections() {
        let text = driver::prompt_section(refusals, section).expect("an answer section");
        assert!(text.contains("change no file"), "{section}");
        assert!(!text.contains("You may change files"), "{section}");
    }
    for section in ["checks_not_passing", "checks_record_missing", "work_failed"] {
        let text = driver::prompt_section(refusals, section).expect("a check section");
        assert!(text.contains("You may change files."), "{section}");
        assert!(text.contains("`escalation_required`"), "{section}");
        assert!(!text.contains("change no file"), "{section}");
    }
    let escalate = driver::prompt_section(refusals, "checks_escalate").expect("the section");
    assert!(escalate.contains("Change no file."));
    assert!(escalate.contains("You may not report success, and you may not report failure."));
    assert!(escalate.contains("between one and eight `questions`"));
}

// GRL-FR-NTFD, GRL-FR-PNLQ, GRL-FR-HLNB: both review instructions judge a
// failed check that names a decision against that decision.
#[test]
fn the_review_instructions_judge_a_waiver_against_its_decision() {
    for (name, text) in review_instructions() {
        for needed in [
            "**A failed check that names an author decision is judged against that decision.**",
            "Where the author's answer in that decision lets that check fail, the failure is not a finding.",
            "Where the decision does not exist or does not let that check fail, the failure is a finding.",
            "or failed under an author decision that lets it fail",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
    }
}

// GRL-FR-IPNO: `check_correction` travels with `work` and `merge_work` alone,
// and never beside `correction`.
#[test]
fn a_check_correction_is_valid_on_a_work_part_alone_and_never_beside_a_correction() {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-08T09:00:00Z");
    run.input.prompt = "Add the empty state.".into();
    let base = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_GENERATE);

    let mut work = base.clone();
    work.check_correction = Some("fix the checks".into());
    assert_eq!(work.validate(0), Ok(()));
    let mut merge = work.clone();
    merge.part = driver::PART_MERGE_WORK.into();
    merge.merge = Some(Default::default());
    assert_eq!(merge.validate(0), Ok(()), "a check correction on merge_work");

    let mut both = work.clone();
    both.correction = Some("send this instead".into());
    assert!(both.validate(0).is_err(), "a check correction beside a correction");

    for part in [driver::PART_REVIEW, driver::PART_SEMANTIC_MERGE] {
        let mut other = work.clone();
        other.part = part.into();
        assert!(other.validate(0).is_err(), "a check correction on {part}");
    }
    let mut merge_review = work.clone();
    merge_review.part = driver::PART_MERGE_REVIEW.into();
    merge_review.merge = Some(Default::default());
    assert!(merge_review.validate(0).is_err(), "a check correction on merge_review");

    // An answer correction alone stays valid on every part it serves.
    let mut answer = base;
    answer.correction = Some("send this instead".into());
    assert_eq!(answer.validate(0), Ok(()));
}

/// GRL-FR-IJTT: a success whose one check finished red.
fn red_turn() -> Turn {
    Turn::answering(Answer::RawResult(serde_json::json!({ "checks": [{
        "name": "tests",
        "command": "run the tests",
        "finished": true,
        "exit_status": 1,
    }] })))
    .with_session()
}

/// The result contract each dispatch named, in dispatch order, by part.
fn contracts(
    dispatch: &ScriptedDispatch,
) -> Vec<(String, Option<crate::tools::agent_exec::protocol::ResultContract>)> {
    dispatch.seen().into_iter().map(|seen| (seen.part, seen.result_contract)).collect()
}

// GRL-FR-IJTT, EAC-FR-43: every work turn names the `work_result` contract, the
// check correction of the gate included.
#[test]
fn every_work_turn_names_the_work_result_contract() {
    use crate::tools::agent_exec::protocol::ResultContract;
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let dispatch = ScriptedDispatch::new(vec![red_turn(), Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed, "{run:?}");
    assert_eq!(
        contracts(&dispatch),
        vec![
            ("work".to_string(), Some(ResultContract::WorkResult)),
            ("work".to_string(), Some(ResultContract::WorkResult)),
            ("review".to_string(), Some(ResultContract::ReviewVerdict)),
        ]
    );
}

// GRL-FR-IJTT, GRL-FR-XWNQ, EAC-FR-43: every `merge_work` turn of a merge run
// names the `work_result` contract, the check correction and the escalate
// correction of the gate included, and its `merge_review` turn the
// `review_verdict` contract.
#[test]
fn every_merge_work_turn_names_the_work_result_contract() {
    use crate::streams::StreamMergePublication;
    use crate::tools::agent_exec::protocol::ResultContract;
    let work = Some(ResultContract::WorkResult);
    let settled = || {
        red_turn()
            .writing("README.md", "settled readme\n")
            .writing("both-new.txt", "settled new\n")
            .writing("gone.txt", "kept\n")
    };
    let cases = [
        (
            "corrected to green",
            vec![settled(), Turn::work(), Turn::ready()],
            GraduationRunState::Completed,
            vec![
                ("merge_work".to_string(), work),
                ("merge_work".to_string(), work),
                ("merge_review".to_string(), Some(ResultContract::ReviewVerdict)),
            ],
        ),
        (
            "escalated",
            vec![
                settled(),
                red_turn(),
                Turn::answering(Answer::Escalate(vec!["May the tests fail?".into()])),
            ],
            GraduationRunState::AwaitingAuthor,
            vec![
                ("merge_work".to_string(), work),
                ("merge_work".to_string(), work),
                ("merge_work".to_string(), work),
            ],
        ),
    ];
    for (case, turns, state, expected) in cases {
        let fx = Fixture::new();
        fx.allow_execution();
        let stream = super::merge_handoff::conflicted_stream(&fx);
        let (run_id, _) =
            super::merge_handoff::handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
        let dispatch = ScriptedDispatch::new(turns);
        let run = fx.drive(&fx.reload(&run_id), dispatch.clone());
        assert_eq!(run.state, state, "{case}: {run:?}");
        assert_eq!(contracts(&dispatch), expected, "{case}");
    }
}
