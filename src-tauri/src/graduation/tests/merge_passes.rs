//! One merge pass, the pass budget of a merge run, and what Continue and an
//! answer do to a run whose pinned tips moved (`GRB-graduation-rebase.md`
//! GRB-FR-SRNW, GRB-FR-QPLA, GRB-FR-TGAZ, GRB-FR-JQFO, GRB-FR-VNKX,
//! `GRL-graduation-loop.md` GRL-FR-MWPQ, GRL-FR-CZBT, GRL-FR-VBCL,
//! `GXD-graduation-execution.md` GXD-FR-MKTZ, GXD-FR-BQLN, GXD-FR-HSQV).

use super::merge_handoff::*;
use super::*;
use crate::graduation::logs::GraduationLogStream;
use crate::streams::StreamMergePublication;

fn settling() -> Turn {
    Turn::work()
        .writing("README.md", "settled readme\n")
        .writing("both-new.txt", "settled new\n")
        .writing("gone.txt", "kept\n")
}

fn queued_merge_run(fx: &Fixture) -> (crate::streams::WorkStream, GraduationRun) {
    fx.allow_execution();
    let stream = conflicted_stream(fx);
    let (run_id, _) = handed_off(fx, &stream, StreamMergePublication::Uncommitted);
    (stream, fx.reload(&run_id))
}

// GRB-FR-QPLA, GRB-FR-SRNW, GRL-FR-MWPQ, GXD-FR-MKTZ, GRL-FR-MRVK: a pass is one
// `merge_work` turn then one fresh `merge_review` turn. The work turn stands in
// the merge worktree, the review in a throwaway checkout of its own on the
// snapshot commit, neither carries a mount, and the review's findings go whole
// into the next work turn, a single minor finding included.
#[test]
fn a_merge_pass_is_a_work_turn_then_a_fresh_review_in_its_own_checkout() {
    let fx = Fixture::new();
    let (stream, run) = queued_merge_run(&fx);
    let data = run.merge.clone().unwrap();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "one\n"),
        Turn::revise(ReviewSeverity::Minor, "Settle the other two paths."),
        settling(),
        Turn::ready(),
    ]);

    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let seen = dispatch.seen();
    let parts: Vec<&str> = seen.iter().map(|s| s.part.as_str()).collect();
    assert_eq!(parts, vec!["merge_work", "merge_review", "merge_work", "merge_review"]);
    let kinds: Vec<TurnKind> = seen.iter().map(|s| s.turn_kind).collect();
    assert_eq!(
        kinds,
        vec![TurnKind::MergeWork, TurnKind::MergeReview, TurnKind::MergeWork, TurnKind::MergeReview]
    );
    let passes: Vec<u32> = seen.iter().map(|s| s.pass).collect();
    assert_eq!(passes, vec![1, 1, 2, 2]);
    assert!(seen.iter().all(|s| !s.semantic_mount), "no turn carries the /rebase mount");

    // The instruction of each turn is its own merge instruction.
    assert_eq!(seen[0].instruction, driver::MERGE_WORK_PROMPT.as_str());
    assert_eq!(seen[1].instruction, driver::MERGE_REVIEW_PROMPT.as_str());

    // The work turn stands in the merge worktree, seeded from the snapshot.
    let worktree = crate::graduation::store_base(&fx.app).unwrap().merge_worktree(&run.id);
    assert_eq!(seen[0].directory, worktree);
    assert_eq!(seen[0].head.as_deref(), Some(data.snapshot_commit.as_str()));
    assert_ne!(seen[0].directory, stream.worktree());
    assert_ne!(seen[0].directory, fx.root());

    // The review stands in a checkout of its own, on the snapshot commit, over
    // exactly what the work turn changed against it.
    assert_ne!(seen[1].directory, worktree);
    assert_ne!(seen[1].directory, stream.worktree());
    assert_eq!(seen[1].head.as_deref(), Some(data.snapshot_commit.as_str()));
    assert_eq!(seen[1].diff_paths, vec!["README.md".to_string()]);
    assert_eq!(
        seen[1].tree.get("README.md").map(String::as_str),
        Some("one\n"),
        "the review reads the work as it stands"
    );
    assert_eq!(seen[1].resume_session, None, "a merge review is a fresh session every time");
    // The second review stands on the same snapshot over the later work, so it
    // is a checkout made for it and not the first one left standing.
    assert_eq!(seen[3].head.as_deref(), Some(data.snapshot_commit.as_str()));
    assert_eq!(seen[3].tree.get("README.md").map(String::as_str), Some("settled readme\n"));
    assert_eq!(seen[3].tree.get("gone.txt").map(String::as_str), Some("kept\n"));

    // The findings go whole into the next work turn.
    assert_eq!(seen[2].purpose, "revise");
    let instruction = seen[2].instruction_field("loop_instruction").expect("the findings");
    assert!(instruction.contains("Settle the other two paths."), "{instruction}");
    // Only the review is told what was reconciled beyond the unresolved paths.
    let merge_of = |s: &Seen| s.input.get("merge").cloned().unwrap();
    assert!(merge_of(&seen[0]).get("reconciled_paths").is_none());
    assert_eq!(merge_of(&seen[1])["reconciled_paths"], serde_json::json!([]));
    assert_eq!(merge_of(&seen[0])["unresolved_paths"], serde_json::json!(data.unresolved_paths));
}

// GRB-FR-TGAZ, GRL-FR-CZBT, GXD-FR-BQLN, GRB-FR-JQFO: a merge run takes two passes.
// (That the project's own budget does not reach it is pinned in `merge_input`.) When the budget is spent and the review still
// says `revise` the run rests in `awaiting_author` with the reason
// `pass_budget_exhausted`, releases its stream, and keeps its worktree and its
// snapshot ref. Continue adds two passes and resumes in the same worktree.
#[test]
fn a_spent_budget_rests_the_run_and_continue_adds_two_passes() {
    let fx = Fixture::new();
    let (stream, run) = queued_merge_run(&fx);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "one\n"),
        Turn::revise(ReviewSeverity::Major, "First."),
        Turn::work().writing("README.md", "two\n"),
        Turn::revise(ReviewSeverity::Major, "Second."),
    ]);

    let spent = fx.drive(&run, dispatch.clone());

    assert_eq!(spent.state, GraduationRunState::AwaitingAuthor, "{spent:?}");
    assert_eq!(dispatch.turns_taken(), 4, "two passes and no third");
    assert_eq!(spent.checkpoint.pass_floor, 1);
    assert_eq!(spent.checkpoint.pass_limit, 2);
    let rest = lines_of(&fx, &run.id, GraduationLogStream::Structured)
        .into_iter()
        .find(|r| r["event"] == "run rested for the author")
        .expect("the rest is recorded");
    assert_eq!(rest["fields"]["reason"], "pass_budget_exhausted");
    assert_eq!(rest["fields"]["pass_budget"], 2);
    assert_eq!(rest["fields"]["passes_made"], 2);
    assert!(
        crate::streams::get_work_stream(fx.app.clone(), stream.id.clone())
            .unwrap()
            .busy_run_id
            .is_none(),
        "the stream is released"
    );
    let worktree = merge_worktree_of(&fx, &run.id);
    assert!(worktree.exists() && snapshot_ref_stands(&fx, &run.id), "worktree and snapshot are kept");
    let first_files = tree_under(&worktree);

    let continued = fx.continue_run(&spent);
    assert_eq!(continued.state, GraduationRunState::Queued);
    assert_eq!(continued.checkpoint.pass_floor, 3);
    assert_eq!(continued.checkpoint.pass_limit, 4, "two more passes");
    let dispatch = ScriptedDispatch::new(vec![settling(), Turn::ready()]);
    let done = fx.drive(&continued, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let work = dispatch.of_part("merge_work");
    assert_eq!(work[0].pass, 3, "the next number, not a restart");
    assert_eq!(work[0].directory, worktree, "the same merge worktree resumes");
    assert_eq!(work[0].tree.get("README.md"), first_files.get("README.md"), "the saved work stands");
}

// GRB-FR-VNKX, GRD-FR-XHSE, GXD-FR-BJYT: Continue against a merge run whose pinned
// tip moved is refused with `merge_branch_moved`, changes nothing, and writes to
// no branch and no worktree.
#[test]
fn continue_is_refused_when_a_pinned_tip_moved_and_changes_nothing() {
    let fx = Fixture::new();
    let (stream, run) = queued_merge_run(&fx);
    let spent = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("README.md", "one\n"),
            Turn::revise(ReviewSeverity::Major, "First."),
            Turn::work().writing("README.md", "two\n"),
            Turn::revise(ReviewSeverity::Major, "Second."),
        ]),
    );
    assert_eq!(spent.state, GraduationRunState::AwaitingAuthor);
    commit_file(&fx.repo(), "later.txt", "later\n", "the base moves");
    let live_before = live(&fx, &stream);
    let worktree_before = tree_under(&merge_worktree_of(&fx, &run.id));

    let refusal = crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone())
        .expect_err("refused");

    assert_eq!(refusal, crate::streams::ERR_MERGE_BRANCH_MOVED);
    assert_eq!(live(&fx, &stream), live_before);
    assert_eq!(tree_under(&merge_worktree_of(&fx, &run.id)), worktree_before);
    let after = fx.reload(&run.id);
    assert_eq!(after.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(after.checkpoint.pass_limit, spent.checkpoint.pass_limit, "the window did not move");
    assert!(after.failure.is_none(), "the request refuses; it does not fail the run");
}

// GRB-FR-VNKX, GRL-FR-VBCL, GXD-FR-HSQV, GXD-FR-BJYT: a merge turn that escalates
// pauses the run in `awaiting_author` without a review; the answer goes into the
// merge turn that asked and the pass does not advance; and an answer given after a
// pinned tip moved is refused with `merge_branch_moved` and records nothing.
#[test]
fn an_escalating_merge_turn_pauses_the_run_and_an_answer_over_a_moved_tip_is_refused() {
    let fx = Fixture::new();
    let (stream, run) = queued_merge_run(&fx);
    let paused = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
            "Which limit stands?".to_string(),
        ]))]),
    );
    assert_eq!(paused.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(paused.pass(), 1);
    assert!(paused.escalation.is_some());
    let answers = vec![GraduationEscalationAnswer {
        position: 1,
        answer: "The first one.".to_string(),
        summary: "The first one".to_string(),
    }];

    // A tip moved: the answer is refused, and the escalation still stands.
    commit_file(&fx.repo(), "later.txt", "later\n", "the base moves");
    let live_before = live(&fx, &stream);
    let refusal = crate::graduation::answer_graduation_escalation(
        fx.app.clone(),
        run.id.clone(),
        answers.clone(),
    )
    .expect_err("refused");
    assert_eq!(refusal, crate::streams::ERR_MERGE_BRANCH_MOVED);
    let after = fx.reload(&run.id);
    assert_eq!(after.state, GraduationRunState::AwaitingAuthor);
    assert!(after.escalation.is_some(), "nothing was recorded");
    assert!(after.checkpoint.pending_escalation_answers.is_empty());
    assert_eq!(live(&fx, &stream), live_before);
}

// GXD-FR-HSQV, GRL-FR-VBCL: where no tip moved the answer goes into the phase
// that asked: the next merge work turn, in the same pass.
#[test]
fn an_answer_goes_into_the_merge_work_turn_that_asked_and_the_pass_does_not_advance() {
    let fx = Fixture::new();
    let (_, run) = queued_merge_run(&fx);
    let paused = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
            "Which limit stands?".to_string(),
        ]))]),
    );
    assert_eq!(paused.state, GraduationRunState::AwaitingAuthor);

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    crate::graduation::answer_graduation_escalation(
        fx.app.clone(),
        run.id.clone(),
        vec![GraduationEscalationAnswer {
            position: 1,
            answer: "The first one.".to_string(),
            summary: "The first one".to_string(),
        }],
    )
    .expect("the answers");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let dispatch = ScriptedDispatch::new(vec![settling(), Turn::ready()]);

    let done = fx.drive(&fx.reload(&run.id), dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let work = dispatch.of_part("merge_work");
    assert_eq!(work[0].purpose, "escalation_answer");
    assert_eq!(work[0].pass, 1, "the answer continues the pass that asked");
    assert_eq!(work[0].list_len("escalation_answers"), 1);
    assert_eq!(dispatch.of_part("merge_review").len(), 1);
}
