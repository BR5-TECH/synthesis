//! The loop driven end to end, with the execution agent removed
//! (`../ai/GRL-graduation-loop.md`).

use super::*;

/// GRL-FR-VYNX, GRL-FR-EKXT, GRD-FR-ARLT: one pass is a work turn followed by a
/// review turn, and a `ready` verdict commits the work onto the stream.
#[test]
fn one_pass_of_work_and_review_completes_the_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "export const panel = 1;\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.work_turns, 1);
    assert_eq!(run.review_turns, 1);
    assert_eq!(run.commits.len(), 1, "a completed run commits once");
    assert_eq!(dispatch.turns_taken(), 2);

    let parts: Vec<String> = dispatch.seen().into_iter().map(|s| s.part).collect();
    assert_eq!(parts, vec!["work".to_string(), "review".to_string()]);
}

/// GRL-FR-DXLU, GXD-FR-ODGX: the work turn stands in the stream's own working
/// copy and is told the captured prompt through the structured input alone.
#[test]
fn the_work_turn_stands_in_the_stream_and_carries_the_captured_prompt() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let work = dispatch.of_part("work");
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].directory, stream.worktree());
    assert_eq!(work[0].turn_kind, TurnKind::Work);
    assert_eq!(work[0].purpose, "generate");
    assert_eq!(work[0].pass, 1);
    assert_eq!(
        work[0].instruction_field("prompt").as_deref(),
        Some("Add the empty state to the panel.")
    );
}

/// GOB-FR-XYCY: a pass record holds the prompt that pass was asked to answer —
/// the captured prompt first, the review's own instruction after it — and never
/// the instruction every work turn carries.
#[test]
fn each_pass_records_the_prompt_it_was_asked_to_answer() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "The empty state is not there."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let passes = &run.observability.passes;
    assert_eq!(passes.len(), 2, "one revision, two passes");
    assert_eq!(
        passes[0].task, "Add the empty state to the panel.",
        "the first pass answers the prompt the run captured"
    );
    assert_eq!(
        Some(&passes[1].task),
        passes[0].next_instruction.as_ref(),
        "the second answers what the review told the next turn"
    );
    // The instruction every work turn carries is the same text for every run,
    // so a pass record holding it would say nothing about this pass.
    for pass in passes {
        assert_ne!(pass.task, *crate::graduation::driver::prompts::WORK_PROMPT);
    }
    // GRL-FR-DXLU: the turn is still handed that instruction, whole. The
    // record and the instruction are two different things, and this change
    // moves neither onto the other.
    for seen in dispatch.seen().iter().filter(|seen| seen.part == "work") {
        assert_eq!(seen.instruction, *crate::graduation::driver::prompts::WORK_PROMPT);
    }
}

/// GRL-FR-ARPX, GRL-FR-GQAB: a `revise` verdict starts one further pass, and
/// the findings travel whole into the work turn that answers them.
#[test]
fn a_revise_verdict_starts_one_further_pass_carrying_its_findings() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "export const panel = 1;\n"),
        Turn::revise(ReviewSeverity::Major, "The empty state is not there."),
        Turn::work().writing("src/panel.ts", "export const panel = 2;\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.work_turns, 2);
    assert_eq!(run.review_turns, 2);

    let work = dispatch.of_part("work");
    assert_eq!(work.len(), 2);
    assert_eq!(work[1].purpose, "revise");
    assert_eq!(work[1].pass, 2);
    let instruction = work[1]
        .instruction_field("loop_instruction")
        .expect("a revise turn carries the findings");
    assert!(
        instruction.contains("The empty state is not there."),
        "the finding travels whole: {instruction}"
    );
}

/// GRL-FR-ARPX: a second review that still asks for revision rests the run for
/// the author rather than starting a third pass.
#[test]
fn a_second_revise_verdict_rests_the_run_for_the_author() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Still not there."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::revise(ReviewSeverity::Major, "Still not there."),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(run.work_turns, 2);
    assert_eq!(run.review_turns, 2);
    assert_eq!(dispatch.turns_taken(), 4, "no third pass is started");
    assert!(run.commits.is_empty(), "nothing is committed unreviewed");
}

/// GRL-FR-UQNV: a `revise` verdict of two findings or fewer, all minor, ends
/// the loop as `ready` does.
#[test]
fn two_minor_findings_end_the_loop_as_ready_does() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Minor, "The name could be clearer."),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.work_turns, 1);
    assert_eq!(dispatch.turns_taken(), 2, "no further work turn is composed");
    assert_eq!(run.commits.len(), 1, "and the work is committed, as `ready` does");
    let pass = run.observability.passes.last().expect("one pass");
    assert_eq!(pass.findings.len(), 1, "the remark is recorded");
    assert!(
        pass.next_instruction.is_none(),
        "the findings are advisory remarks, not an instruction to a further turn"
    );
    assert!(run.checkpoint.loop_instruction.is_none());
}

/// GRD-FR-ARLT: the commit holds exactly the paths that differ from the base
/// commit, deletions included.
#[test]
fn the_commit_holds_what_the_work_turn_wrote_and_nothing_else() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("src/panel.ts", "export const panel = 1;\n")
            .writing("README.md", "seed\nand more\n")
            .deleting(".gitignore"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Completed);
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let base = repo
        .find_commit(git2::Oid::from_str(run.base_commit.as_deref().unwrap()).unwrap())
        .unwrap();
    let diff = repo
        .diff_tree_to_tree(Some(&base.tree().unwrap()), Some(&head.tree().unwrap()), None)
        .unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| {
            d.new_file()
                .path()
                .or_else(|| d.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
        })
        .collect();
    paths.sort();
    paths.dedup();
    assert_eq!(
        paths,
        vec![
            ".gitignore".to_string(),
            "README.md".to_string(),
            "src/panel.ts".to_string()
        ]
    );
}

/// GRL-FR-VBCL, GXD-FR-HGSU: an escalation ends the turn without a review and
/// rests the run for the author, carrying the ordered questions.
#[test]
fn an_escalation_ends_the_turn_without_a_review() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
        "What does the empty panel say?".to_string(),
    ]))]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(dispatch.turns_taken(), 1, "no review is dispatched");
    let escalation = run.escalation.expect("the questions");
    assert_eq!(escalation.questions.len(), 1);
    assert_eq!(escalation.questions[0].position, 1);
}

/// GXD-FR-XEUX: the seam ends no run. A pre-launch failure rests it instead.
#[test]
fn a_turn_that_could_not_be_launched_rests_the_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::answering(Answer::Unlaunchable(AgentExecutionError::RuntimeUnavailable))]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert!(!run.state.is_terminal(), "a run that could not start is not failed");
}

/// GRL-FR-DNKA: an agent-reported failure is material rather than an outcome.
/// The turn completed, so the run goes to review like any other.
#[test]
fn an_agent_reported_failure_still_reaches_a_review() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::answering(Answer::ReportedFailure(
            "The test runner is not installed here.".to_string(),
        ))
        .writing("src/panel.ts", "half\n"),
        Turn::revise(ReviewSeverity::Critical, "Only half is written."),
        Turn::work_saying("The second turn finished it.").writing("src/panel.ts", "whole\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 2);
    // GRL-FR-DNKA: what the agent said it could not do is context the review is
    // given, rather than something the loop keeps to itself.
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some("The test runner is not installed here.")
    );
    assert_eq!(run.state, GraduationRunState::Completed);
    // GRL-FR-DNKA: and the turn after it carries its **own** account rather
    // than the one before it, so a review never reads a stale account as if it
    // described the change set in front of it.
    assert_eq!(
        review[1].instruction_field("agent_account").as_deref(),
        Some("The second turn finished it."),
    );
}

/// GRD-FR-SWOJ: a turn that stopped without a review has whatever it wrote
/// committed as an abandoned turn, so the stream is left usable.
#[test]
fn a_stopped_turn_commits_what_it_wrote_as_an_abandoned_turn() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    // The turn writes, then the author stops the run while it is still running.
    let dispatch = ScriptedDispatch::new(vec![Turn::work()
        .writing("src/panel.ts", "half\n")
        .doing(move || {
            if let Some(state) = app.try_state::<GraduationState>() {
                state.cancel_run(&run_id, GraduationInterruptionReason::AuthorPause);
            }
        })]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(
        dispatch.turns_taken(),
        1,
        "nothing is judged: no review turn is dispatched after a stop"
    );
    assert_eq!(run.commits.len(), 1, "what the turn wrote is kept");
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(
        head.message().unwrap_or_default().contains("Abandoned"),
        "the commit says the turn was abandoned"
    );
    assert!(stream.worktree().join("src/panel.ts").is_file());
}

/// GRD-FR-HQPD, GRD-FR-YBUM: work the author left standing in the stream is
/// committed as the run's starting point, and that revision is the base.
#[test]
fn work_standing_in_the_stream_becomes_the_runs_base_commit() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]);
    let run = fx.drive(&run, dispatch);

    let base = run.base_commit.clone().expect("a base commit");
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    assert!(
        base_commit.tree().unwrap().get_path(Path::new("authored.md")).is_ok(),
        "the author's own work is inside the base commit"
    );
    assert!(
        !run.commits.contains(&base),
        "the base commit is not one the run made"
    );
}

/// GRL-FR-VIAT: a verdict the application cannot read is refused whole.
#[test]
fn a_verdict_with_no_finding_beside_revise_is_refused() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::answering(Answer::RawResult(serde_json::json!({
            "verdict": "revise",
            "rationale": "Something is wrong.",
            "findings": []
        }))),
        Turn::answering(Answer::RawResult(serde_json::json!({
            "verdict": "revise",
            "rationale": "Something is wrong.",
            "findings": []
        }))),
    ]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(
        run.blocker.as_ref().map(|b| b.code.as_str()),
        Some("review_verdict_invalid")
    );
}

/// GRL-FR-TVXI: a verdict the application cannot read is asked again as a
/// **review**, carrying the correction, rather than by starting a work turn.
#[test]
fn an_unreadable_verdict_asks_the_review_again() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        // A verdict of a shape the application refuses whole.
        Turn::answering(Answer::RawResult(serde_json::json!({
            "verdict": "maybe",
            "rationale": "I am not sure."
        }))),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.work_turns, 1, "no work turn is spent on a refused verdict");
    assert_eq!(run.review_turns, 2);
    assert_eq!(run.observability.passes.len(), 1, "and no pass is spent");

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 2);
    assert!(
        review[0].instruction_field("correction").is_none(),
        "the first review is not corrected"
    );
    let correction = review[1]
        .instruction_field("correction")
        .expect("the second review is told what to send instead");
    assert!(
        correction.contains("verdict") && correction.contains("findings"),
        "the correction names the shape: {correction}"
    );
    assert_eq!(
        run.checkpoint.verdict_refusals, 0,
        "a readable verdict resets the count"
    );
}

/// GRL-FR-TVXI: a second consecutive unreadable verdict blocks the run, and the
/// author's Continue gives it the whole bound again.
#[test]
fn two_unreadable_verdicts_block_the_run_and_continue_resets_the_count() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let unreadable = || {
        Turn::answering(Answer::RawResult(serde_json::json!({
            "verdict": "maybe",
            "rationale": "I am not sure."
        })))
    };
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        unreadable(),
        unreadable(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(
        run.blocker.as_ref().map(|b| b.code.as_str()),
        Some("review_verdict_invalid")
    );
    assert_eq!(run.review_turns, 2);
    assert_eq!(run.work_turns, 1);

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let continued =
        crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    assert_eq!(continued.checkpoint.verdict_refusals, 0);
}

/// GRD-FR-SWOJ, GRL-FR-REPL: a turn that ran out of time has what it wrote
/// committed as an abandoned turn, and the run rests rather than failing.
#[test]
fn a_turn_that_ran_out_of_time_keeps_what_it_wrote() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::answering(Answer::Process(
        ProcessOutcome::Timeout,
    ))
    .writing("src/panel.ts", "half\n")]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert!(!run.state.is_terminal());
    assert_eq!(run.commits.len(), 1, "what the turn wrote is kept");
    assert!(stream.worktree().join("src/panel.ts").is_file());
}

/// GXD-FR-XPUR: an escalation records the vendor session it was raised in, so
/// the author's answers are delivered into the continuation of the turn that
/// asked rather than into a fresh one.
#[test]
fn an_escalation_records_the_session_its_answers_continue() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let dispatch = ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec![
        "Which limit stands?".to_string(),
    ]))
    .with_session()]);
    let run = fx.drive(&run, dispatch);

    let escalation = run.escalation.expect("the questions");
    assert_eq!(
        escalation.resume.and_then(|r| r.session_id).as_deref(),
        Some("s1")
    );
}

/// GXD-FR-IOZU: every turn carries an activity sink bound to its run, so what
/// the agent does reaches the run it is doing it for.
#[test]
fn every_turn_reports_its_activity_against_its_own_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert!(
        dispatch.seen().iter().all(|seen| seen.had_activity),
        "a turn with no sink is a run the author watches an empty panel for"
    );
    assert!(
        dispatch.seen().iter().all(|seen| seen.had_durable_output),
        "a turn with no durable sink is a run whose record nobody keeps"
    );
    let store = fx
        .app
        .state::<std::sync::Arc<crate::agent_activity::ActivityStore>>()
        .inner()
        .clone();
    let page = store.read(&run.id, None, None);
    assert_eq!(
        page.records.len(),
        2 * SCRIPTED_ACTIVITY.len(),
        "both turns reported against this run"
    );
}

/// GXD-FR-LBYG: the response envelope is the agent's report of its turn and
/// never evidence about the filesystem.
#[test]
fn what_the_agent_claims_it_wrote_is_not_what_the_run_records() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // The turn writes one path and reports three, in its result and its summary.
    let dispatch = ScriptedDispatch::new(vec![
        Turn::answering(Answer::ClaimingPaths(vec![
            "src/invented.ts".to_string(),
            "src/also-invented.ts".to_string(),
        ]))
        .writing("src/real.ts", "1\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(
        run.checkpoint.changed_paths,
        vec!["src/real.ts".to_string()],
        "what changed is read from the working copy"
    );
    // And the review is given the same reading, not the agent's.
    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.path_list("changed_paths"), vec!["src/real.ts".to_string()]);
    assert_eq!(review.diff_paths, vec!["src/real.ts".to_string()]);

    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let tree = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert!(tree.get_path(Path::new("src/real.ts")).is_ok());
    assert!(tree.get_path(Path::new("src/invented.ts")).is_err());
}

/// GRD-FR-EWTN, GRD-FR-GMTX: a run the author discards while it is working ends
/// discarded, and stays discarded.
///
/// The turn is driven from a thread of its own, so the run the loop holds is
/// still `working` when the discard lands. A transition read from that stale
/// copy would write `interrupted` over the author's own decision.
#[test]
fn discarding_a_run_mid_turn_leaves_it_discarded() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    let dispatch = ScriptedDispatch::new(vec![Turn::work()
        .writing("src/panel.ts", "half\n")
        .doing(move || {
            app.state::<GraduationState>().set_loop_enabled(false);
            crate::graduation::discard_graduation_run(app.clone(), run_id.clone())
                .expect("the author discards it");
            app.state::<GraduationState>().set_loop_enabled(true);
        })]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::Discarded);
    // GRD-FR-GMTX: the stream outlives the run, and nothing of it is reclaimed.
    assert!(stream.worktree().is_dir());
    assert!(fx
        .repo()
        .find_branch(&stream.branch, git2::BranchType::Local)
        .is_ok());
    assert!(fx.app.state::<GraduationState>().holder_of(&stream.id).is_none());
}

/// GRL-FR-VBCL, GXD-FR-HGSU: a review turn may escalate too, and the run rests
/// for the author with the review named as the origin.
#[test]
fn a_review_that_cannot_decide_escalates_to_the_author() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::answering(Answer::Escalate(vec![
            "What should the empty panel say?".to_string(),
        ])),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(dispatch.turns_taken(), 2);
    let escalation = run.escalation.expect("the questions");
    assert_eq!(escalation.origin, GraduationEscalationOrigin::Review);
    assert!(run.commits.is_empty(), "nothing is committed unreviewed");
    // GRL-FR-YKRI: the checkout goes even when the turn asked instead of judging.
    let checkout = dispatch.of_part("review")[0].directory.clone();
    assert!(!checkout.exists());
}

/// GRD-FR-BLCR: reverting a run's commits makes new commits and rewrites no
/// history, and is refused where there is nothing to revert or a turn is running.
#[test]
fn reverting_a_run_makes_new_commits_and_rewrites_no_history() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // A run that made no commit is not revertable.
    assert!(
        crate::graduation::revert_graduation_run(fx.app.clone(), run.id.clone()).is_err(),
        "a run with no commit has nothing to revert"
    );

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let before: Vec<String> = {
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        walk.filter_map(|oid| oid.ok().map(|oid| oid.to_string())).collect()
    };

    crate::graduation::revert_graduation_run(fx.app.clone(), run.id.clone()).expect("reverted");

    let after: Vec<String> = {
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        walk.filter_map(|oid| oid.ok().map(|oid| oid.to_string())).collect()
    };
    assert_eq!(after.len(), before.len() + 1, "one new commit");
    assert!(
        before.iter().all(|revision| after.contains(revision)),
        "and nothing of the history was rewritten"
    );
    assert!(
        !stream.worktree().join("src/panel.ts").exists(),
        "the work the run made is undone in the tree"
    );
}

/// GRL-FR-ZDKP: cancellation reaches every turn, the review included. The
/// checkout goes, and what the work turn wrote is kept.
#[test]
fn a_review_turn_stopped_mid_way_keeps_the_work_and_reclaims_its_checkout() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");
    let app = fx.app.clone();
    let run_id = run.id.clone();

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::ready().doing(move || {
            app.state::<GraduationState>()
                .cancel_run(&run_id, GraduationInterruptionReason::AuthorPause);
        }),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(
        run.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::AuthorPause)
    );
    assert_eq!(run.commits.len(), 1, "what the work turn wrote is kept");
    let checkout = dispatch.of_part("review")[0].directory.clone();
    assert!(!checkout.exists(), "the checkout goes even on a stop");
    assert!(!crate::streams::registers_worktree(
        &fx.repo(),
        &format!("{}-rv", run.id)
    ));
}

/// GRD-FR-EGWS, GRL-FR-BRJO: a run writes into its stream's working copy and
/// into nothing of the project the author is standing in.
#[test]
fn a_run_writes_nothing_into_the_authors_own_checkout() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    let before = tree_under(&fx.root());
    fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );
    let after = tree_under(&fx.root());

    assert_eq!(
        before, after,
        "the author's own checkout is byte-identical after the run"
    );
}

/// GXD-FR-XEUX: the seam ends no run. Every process outcome and every pre-launch
/// error rests it instead.
#[test]
fn no_outcome_of_the_seam_ends_a_run() {
    for outcome in [
        ProcessOutcome::NonZeroExit,
        ProcessOutcome::InvalidStructuredOutput,
        ProcessOutcome::Timeout,
        ProcessOutcome::Cancelled,
        ProcessOutcome::Terminated,
    ] {
        let fx = Fixture::new();
        let stream = fx.stream("editor");
        let run = fx.enqueue(&stream, "Write the panel.");
        let run = fx.drive(
            &run,
            ScriptedDispatch::new(vec![Turn::answering(Answer::Process(outcome))]),
        );
        assert!(
            !run.state.is_terminal(),
            "{outcome:?} ended the run at {:?}",
            run.state
        );
        assert_eq!(run.state, GraduationRunState::Interrupted);
    }

    for error in [
        AgentExecutionError::RuntimeUnavailable,
        AgentExecutionError::IntegrationUnresolved("none".to_string()),
    ] {
        let fx = Fixture::new();
        let stream = fx.stream("editor");
        let run = fx.enqueue(&stream, "Write the panel.");
        let run = fx.drive(
            &run,
            ScriptedDispatch::new(vec![Turn::answering(Answer::Unlaunchable(error.clone()))]),
        );
        assert!(
            !run.state.is_terminal(),
            "{error:?} ended the run at {:?}",
            run.state
        );
    }
}

/// GRD-FR-QJHM: a run whose stream working copy is gone rests on a condition the
/// author clears, and the checkpoint it already held is not quietly emptied.
#[test]
fn a_run_whose_stream_working_copy_is_gone_rests_for_the_author() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Write the panel.");

    // The first pass runs, so the run holds a base commit and a change set.
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::revise(ReviewSeverity::Major, "Not yet."),
            Turn::work().writing("src/panel.ts", "2\n"),
            Turn::revise(ReviewSeverity::Major, "Still not."),
        ]),
    );
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(!run.checkpoint.changed_paths.is_empty());

    // The working copy goes, and the run is handed back.
    std::fs::remove_dir_all(stream.worktree()).expect("the working copy goes");
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    fx.app.state::<GraduationState>().set_loop_enabled(true);

    let run = fx.reload(&run.id);
    let dispatch = ScriptedDispatch::new(Vec::new());
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(
        run.blocker.as_ref().map(|b| b.code.as_str()),
        Some("stream_missing")
    );
    assert_eq!(dispatch.turns_taken(), 0, "no turn is dispatched into nothing");
    assert!(
        !run.checkpoint.changed_paths.is_empty(),
        "what the run already held is not emptied by a tree nothing can open"
    );
}
