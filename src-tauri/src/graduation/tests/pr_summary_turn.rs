//! The one turn of a PR summary run, driven through the production loop with
//! the agent scripted (`../ai/GRL-graduation-loop.md` GRL-FR-UNKD, GRL-FR-ZZGO,
//! GRL-FR-SNHR; `GRD-graduation.md` GRD-FR-TYVT, GRD-FR-SWHW, GRD-FR-CCHK).

use super::*;

/// A PR summary run as a bare record, for what composes its input.
pub(super) fn pr_summary_run(head: &str, mode: PrSummaryMode) -> GraduationRun {
    let mut run = GraduationRun::new_for_test("g-ps", "", "2026-11-06T09:00:00Z");
    run.stream_id = String::new();
    run.stream_name = String::new();
    run.input.prompt = String::new();
    run.input.draft_name = String::new();
    run.pr_summary = Some(PrSummaryData {
        name: pr_summary_title(head),
        head: head.to_string(),
        base: "main".to_string(),
        mode,
        head_tip: Some("a".repeat(40)),
        merge_base: Some("b".repeat(40)),
        inputs: vec![
            PrSummaryInput {
                run_id: "g1".into(),
                title: "First".into(),
                description: "The first part.".into(),
            },
            PrSummaryInput {
                run_id: "g2".into(),
                title: "Second".into(),
                description: "The second part.".into(),
            },
        ],
        result: None,
    });
    run
}

/// A commit on `branch`, made without touching any worktree, that writes
/// `path`, a top-level file name. The branch is created at the current head where it does not exist.
pub(super) fn commit_on_branch(
    repo: &git2::Repository,
    branch: &str,
    path: &str,
    content: &str,
) -> git2::Oid {
    let parent = match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(found) => found.get().peel_to_commit().expect("a tip"),
        Err(_) => {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch(branch, &head, false).expect("a branch");
            head
        }
    };
    let blob = repo.blob(content.as_bytes()).expect("a blob");
    let mut builder = repo.treebuilder(Some(&parent.tree().unwrap())).expect("a builder");
    builder.insert(path, blob, 0o100644).expect("an entry");
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &signature,
        &signature,
        "a change on the branch",
        &tree,
        &[&parent],
    )
    .expect("a commit")
}

/// A fixture with a head branch one commit ahead of `main`, and a PR summary
/// run for it, enqueued through the command.
fn summary_run(fx: &Fixture, mode: PrSummaryMode) -> (GraduationRun, git2::Oid, git2::Oid) {
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let repo = fx.repo();
    let base = repo.head().unwrap().peel_to_commit().unwrap().id();
    let current = repo.head().unwrap().shorthand().unwrap().to_string();
    if current != "main" {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("main", &head, true).unwrap();
    }
    let tip = commit_on_branch(&repo, "feature/panel", "panel.ts", "export {};\n");
    let run = crate::graduation::enqueue_pull_request_summary(
        &fx.app,
        "feature/panel",
        "main",
        mode,
        |_, _| {
            vec![PrSummaryInput {
                run_id: "g1".into(),
                title: "Add the panel".into(),
                description: "The panel.".into(),
            }]
        },
    )
    .expect("enqueued");
    (run, tip, base)
}

fn summary_answer(title: &str, description: &str) -> Turn {
    Turn::answering(Answer::RawResult(serde_json::json!({
        "title": title,
        "description": description,
    })))
}

// GRL-FR-UNKD, GXD-FR-XQJR, EAC-FR-43, GRL-FR-GADT, GRL-FR-SNHR, GRD-FR-TYVT,
// GRD-FR-SWHW, GRD-FR-KAKU: a PR summary run dispatches one `pr_summary` turn of
// kind `pr_maintenance` under `pr-summary.md` and the `pull_request_summary`
// contract, in a checkout detached at the pinned head tip. A valid answer
// completes the run and is recorded, the checkout is gone afterwards, and no
// branch, worktree or commit of the author changes.
#[test]
fn a_pr_summary_run_is_one_turn_in_a_disposable_checkout_of_the_head_tip() {
    let fx = Fixture::new();
    let (run, tip, base) = summary_run(&fx, PrSummaryMode::Diff);
    let readme_before = std::fs::read_to_string(fx.root().join("README.md")).unwrap();
    let main_before = fx.repo().head().unwrap().peel_to_commit().unwrap().id();
    let dispatch = ScriptedDispatch::new(vec![summary_answer(
        "  Add the queue panel  ",
        "## Summary\n\nAdds the panel.\n",
    )]);

    let done = fx.drive(&run, dispatch.clone());

    let seen = dispatch.seen();
    assert_eq!(seen.len(), 1, "one turn and no review");
    let turn = &seen[0];
    assert_eq!(turn.part, "pr_summary");
    assert_eq!(turn.turn_kind, TurnKind::PrMaintenance);
    assert_eq!(
        turn.result_contract,
        Some(crate::tools::agent_exec::protocol::ResultContract::PullRequestSummary)
    );
    assert_eq!(turn.instruction, driver::PR_SUMMARY_PROMPT.as_str());
    assert_eq!(turn.sandbox_scope.as_deref(), Some("pr:feature/panel"));
    // GRL-FR-SNHR: the context, and empty draft, prompt and stream fields.
    let context = turn.input.get("pr_summary").expect("the PR summary context");
    assert_eq!(context["mode"], "diff");
    assert_eq!(context["head"], "feature/panel");
    assert_eq!(context["base"], "main");
    assert_eq!(context["head_tip"], tip.to_string());
    assert_eq!(context["merge_base"], base.to_string());
    assert_eq!(context["inputs"], serde_json::json!([]), "diff mode carries no input");
    for empty in ["draft_name", "prompt", "stream_name"] {
        assert_eq!(turn.instruction_field(empty).as_deref(), Some(""), "{empty} is empty");
    }
    // GRD-FR-TYVT: the turn stood in the run's own checkout, detached at the
    // pinned head tip, holding the head branch's tree.
    let checkout = turn.directory.clone();
    assert!(checkout_paths::is_nonced_checkout_of(&fx.app, &run.id, "ps", &checkout));
    assert_eq!(turn.head.as_deref(), Some(tip.to_string().as_str()));
    assert!(turn.tree.contains_key("panel.ts"), "the head branch's tree: {:?}", turn.tree);

    // GRD-FR-SWHW: the answer completes the run and is recorded.
    assert_eq!(done.state, GraduationRunState::Completed);
    let data = done.pr_summary.as_ref().expect("the data");
    assert_eq!(data.head_tip.as_deref(), Some(tip.to_string().as_str()));
    assert_eq!(data.merge_base.as_deref(), Some(base.to_string().as_str()));
    let result = data.result.as_ref().expect("a result");
    assert_eq!(result.title, "Add the queue panel");
    assert_eq!(result.description, "## Summary\n\nAdds the panel.");
    // GRD-FR-KAKU: no commit, no draft, no stream, no run summary.
    assert!(done.commits.is_empty());
    assert!(done.input.draft_id.is_empty() && done.stream_id.is_empty());
    assert!(done.run_summary.is_none());
    // GRD-FR-TYVT: the checkout and its registration are gone, and nothing of
    // the author's moved.
    assert!(!checkout.exists(), "the checkout is removed");
    let repo = fx.repo();
    assert!(checkout_paths::registrations_left(&repo, &run.id, "ps").is_empty(), "no registration");
    assert!(
        repo.find_branch(&format!("synthesis/pr-summary/{}", run.id), git2::BranchType::Local)
            .is_err(),
        "no scratch branch",
    );
    assert_eq!(
        repo.find_branch("feature/panel", git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap()
            .id(),
        tip,
    );
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), main_before);
    assert_eq!(std::fs::read_to_string(fx.root().join("README.md")).unwrap(), readme_before);
    // GRD-FR-CCHK: a completed run holds nothing.
    assert!(!done.holds_queue() && !done.holds_slot());
}

// GRL-FR-SNHR: in `combine` mode the turn carries the captured run summaries
// in landing order.
#[test]
fn a_combine_turn_carries_the_captured_run_summaries() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Combine);
    let dispatch = ScriptedDispatch::new(vec![summary_answer("Combined", "Both parts.")]);

    let done = fx.drive(&run, dispatch.clone());

    let context = dispatch.seen()[0].input["pr_summary"].clone();
    assert_eq!(context["mode"], "combine");
    assert_eq!(
        context["inputs"],
        serde_json::json!([{ "title": "Add the panel", "description": "The panel." }]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);

    // The bare composition keeps the order the run captured.
    let bare = pr_summary_run("feature/panel", PrSummaryMode::Combine);
    let input = driver::compose_pr_summary_input(&bare).expect("an input");
    let titles: Vec<_> =
        input.pr_summary.unwrap().inputs.into_iter().map(|input| input.title).collect();
    assert_eq!(titles, vec!["First", "Second"]);
}

// GRL-FR-ZZGO, GRD-FR-SWHW, GRD-FR-CCHK, GRD-FR-RKWQ: an answer that is not a
// title of one line and a description fails the run with its typed cause and
// records no result. The failed run holds neither its queue nor a slot.
#[test]
fn an_unreadable_answer_fails_the_run_and_frees_its_queue() {
    for (case, answer) in [
        ("a blank title", serde_json::json!({ "title": " ", "description": "d" })),
        ("a title of two lines", serde_json::json!({ "title": "a\nb", "description": "d" })),
        ("no description", serde_json::json!({ "title": "t" })),
        ("an extra field", serde_json::json!({ "title": "t", "description": "d", "x": 1 })),
    ] {
        let fx = Fixture::new();
        let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
        let dispatch = ScriptedDispatch::new(vec![Turn::answering(Answer::RawResult(answer))]);

        let done = fx.drive(&run, dispatch);

        assert_eq!(done.state, GraduationRunState::Failed, "{case}");
        assert_eq!(done.failure.as_ref().unwrap().code, "unreadable_answer", "{case}");
        assert!(done.pr_summary.as_ref().unwrap().result.is_none(), "{case}");
        assert!(!done.holds_queue() && !done.holds_slot(), "{case}");
        let index = read_queue_index(
            &store_fs(&fx.app).unwrap(),
            &store_base(&fx.app).unwrap(),
            &done.project_key,
        );
        let entry = index.runs.iter().find(|entry| entry.run_id == done.id).unwrap();
        assert!(entry.pr_summary && entry.pr_head == "feature/panel");
        assert!(!entry.holds_queue() && !entry.holds_slot(), "{case}: the index agrees");
        // GRD-FR-CCHK: a new run for the same head starts at once.
        crate::graduation::enqueue_pull_request_summary(
            &fx.app,
            "feature/panel",
            "main",
            PrSummaryMode::Diff,
            |_, _| Vec::new(),
        )
        .unwrap_or_else(|error| panic!("{case}: a new run is refused: {error}"));
    }
}

// GRL-FR-ZZGO, GRD-FR-SWHW, GRL-FR-UNKD: an escalation, an agent-reported
// failure, a process that did not complete and a launch that did not happen
// each fail the run with their typed cause.
#[test]
fn every_other_end_of_the_turn_fails_the_run_with_its_cause() {
    for (answer, code) in [
        (Answer::Escalate(vec!["Which base?".into()]), "pr_summary_escalated"),
        (Answer::ReportedFailure("could not".into()), "agent_reported_failure"),
        (Answer::Process(ProcessOutcome::Timeout), "execution_timeout"),
        (Answer::Process(ProcessOutcome::NonZeroExit), "agent_exited"),
        (
            Answer::Unlaunchable(AgentExecutionError::IntegrationUnresolved("none".into())),
            "launch_failed",
        ),
    ] {
        let fx = Fixture::new();
        let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
        let done = fx.drive(&run, ScriptedDispatch::new(vec![Turn::answering(answer)]));
        assert_eq!(done.state, GraduationRunState::Failed, "{code}");
        assert_eq!(done.failure.as_ref().map(|f| f.code.as_str()), Some(code));
        assert!(done.pr_summary.as_ref().unwrap().result.is_none());
        assert!(done.escalation.is_none(), "no escalation is recorded");
        assert!(
            checkout_paths::checkouts_left(&fx.app, &run.id, "ps").is_empty(),
            "{code}: the checkout is removed"
        );
    }
}

// GRL-FR-QKDC, GRL-FR-ZZGO: a refused answer is asked once more in the same
// session, and a readable second answer completes the run.
#[test]
fn a_refused_answer_is_corrected_once_in_its_session() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::answering(Answer::Refused(
            crate::tools::agent_exec::protocol::EnvelopeInvalid::Malformed,
        ))
        .with_session("s1"),
        summary_answer("Add the panel", "Adds it."),
    ]);

    let done = fx.drive(&run, dispatch.clone());

    let seen = dispatch.seen();
    assert_eq!(seen.len(), 2, "the turn and its one correction");
    assert_eq!(seen[1].part, "pr_summary");
    assert_eq!(seen[1].resume_session.as_deref(), Some("s1"));
    assert!(seen[1].instruction_field("correction").is_some(), "the correction is carried");
    assert_eq!(seen[1].directory, seen[0].directory, "the same checkout");
    assert_eq!(done.state, GraduationRunState::Completed);

    // A second refusal fails the run.
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let refused = || {
        Turn::answering(Answer::Refused(
            crate::tools::agent_exec::protocol::EnvelopeInvalid::Malformed,
        ))
        .with_session("s1")
    };
    let done = fx.drive(&run, ScriptedDispatch::new(vec![refused(), refused()]));
    assert_eq!(done.state, GraduationRunState::Failed);
    assert_eq!(done.failure.as_ref().unwrap().code, "unreadable_answer");
}

// GRD-FR-CCHK, GRD-FR-CQGB: a turn the author paused rests the run
// `interrupted`, and Continue dispatches the turn again from the start.
#[test]
fn a_paused_turn_rests_the_run_and_continue_starts_the_turn_again() {
    let fx = Fixture::new();
    let (run, tip, _) = summary_run(&fx, PrSummaryMode::Diff);
    let app = fx.app.clone();
    let id = run.id.clone();
    let paused = Turn::answering(Answer::Success).with_session("s1").doing(move || {
        crate::graduation::pause_graduation_run(app.clone(), id.clone()).expect("paused");
    });

    let first = ScriptedDispatch::new(vec![paused]);
    let rested = fx.drive(&run, first.clone());
    assert_eq!(rested.state, GraduationRunState::Interrupted);
    assert_eq!(
        rested.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::AuthorPause)
    );
    assert!(rested.holds_queue() && rested.holds_slot(), "an interrupted run holds both");
    assert!(
        checkout_paths::checkouts_left(&fx.app, &run.id, "ps").is_empty(),
        "the checkout is removed when the turn ends"
    );
    let paused_in = first.seen()[0].directory.clone();

    let continued = fx.continue_run(&rested);
    assert_eq!(continued.state, GraduationRunState::Queued);
    let dispatch = ScriptedDispatch::new(vec![summary_answer("Add the panel", "Adds it.")]);
    let done = fx.drive(&continued, dispatch.clone());

    let again = &dispatch.seen()[0];
    // GRD-FR-XKGC: the turn after Continue stands at a path of its own.
    assert!(checkout_paths::is_nonced_checkout_of(&fx.app, &run.id, "ps", &again.directory));
    assert!(checkout_paths::is_nonced_checkout_of(&fx.app, &run.id, "ps", &paused_in));
    assert_ne!(again.directory, paused_in, "a new checkout after Continue");
    assert_eq!(again.purpose, "generate", "the turn starts again");
    assert_eq!(again.resume_session, None, "no session of the paused turn is resumed");
    assert_eq!(again.head.as_deref(), Some(tip.to_string().as_str()), "the pinned tip");
    assert_eq!(done.state, GraduationRunState::Completed);
}

// GRD-FR-TYVT: the head tip and the merge base are pinned at the first
// dispatch, so a head branch that moves after it does not move the run.
#[test]
fn the_head_tip_is_pinned_at_the_first_dispatch() {
    let fx = Fixture::new();
    let (run, tip, _) = summary_run(&fx, PrSummaryMode::Diff);
    let app = fx.app.clone();
    let id = run.id.clone();
    let paused = Turn::answering(Answer::Success).doing(move || {
        crate::graduation::pause_graduation_run(app.clone(), id.clone()).expect("paused");
    });
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![paused]));
    commit_on_branch(&fx.repo(), "feature/panel", "later.ts", "later\n");

    let continued = fx.continue_run(&rested);
    let dispatch = ScriptedDispatch::new(vec![summary_answer("t", "d")]);
    let done = fx.drive(&continued, dispatch.clone());
    assert_eq!(dispatch.seen()[0].head.as_deref(), Some(tip.to_string().as_str()));
    assert!(!dispatch.seen()[0].tree.contains_key("later.ts"));
    assert_eq!(
        done.pr_summary.unwrap().head_tip.as_deref(),
        Some(tip.to_string().as_str()),
        "the pin is kept",
    );
}

// GRD-FR-TYVT: a head branch that is gone at the first dispatch fails the run
// before any turn.
#[test]
fn a_head_branch_gone_before_the_first_dispatch_fails_the_run() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    fx.repo().find_branch("feature/panel", git2::BranchType::Local).unwrap().delete().unwrap();
    let dispatch = ScriptedDispatch::new(Vec::new());
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.turns_taken(), 0);
    assert_eq!(done.state, GraduationRunState::Failed);
    assert_eq!(done.failure.unwrap().code, "unknown branch");
}

// GRL-FR-ZZGO: an answer the executor took, but that is not a summary this
// application can read, is asked for once more in its session with the
// `pr_summary_result` correction. A second such answer fails the run.
#[test]
fn an_unreadable_summary_is_asked_for_again_once_in_its_session() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let two_lines = || {
        Turn::answering(Answer::RawResult(serde_json::json!({
            "title": "a\nb",
            "description": "d",
        })))
        .with_session("s1")
    };
    let dispatch =
        ScriptedDispatch::new(vec![two_lines(), summary_answer("Add the panel", "Adds it.")]);

    let done = fx.drive(&run, dispatch.clone());

    let seen = dispatch.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].resume_session.as_deref(), Some("s1"));
    assert_eq!(seen[1].instruction, driver::PR_SUMMARY_PROMPT.as_str());
    assert_eq!(
        seen[1].instruction_field("correction"),
        driver::prompt_section(&driver::REFUSALS, "pr_summary_result"),
    );
    assert_eq!(seen[1].directory, seen[0].directory, "the same checkout");
    assert_eq!(done.state, GraduationRunState::Completed);
    assert_eq!(done.work_turns, 2);

    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let dispatch = ScriptedDispatch::new(vec![two_lines(), two_lines()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.turns_taken(), 2, "corrected once and no more");
    assert_eq!(done.state, GraduationRunState::Failed);
    assert_eq!(done.failure.as_ref().unwrap().code, "unreadable_answer");
}

// GRD-FR-CCHK: a correction saved by a turn the author paused is not carried
// into the turn Continue dispatches, which starts again from the start.
#[test]
fn continue_drops_a_correction_the_paused_turn_left() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let app = fx.app.clone();
    let id = run.id.clone();
    let refused_and_paused = Turn::answering(Answer::Refused(
        crate::tools::agent_exec::protocol::EnvelopeInvalid::Malformed,
    ))
    .with_session("s1")
    .ignoring_cancellation()
    .doing(move || {
        crate::graduation::pause_graduation_run(app.clone(), id.clone()).expect("paused");
    });
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![refused_and_paused]));
    assert_eq!(rested.state, GraduationRunState::Interrupted);
    assert!(rested.checkpoint.answer_correction.is_some(), "the pause saved a correction");

    let continued = fx.continue_run(&rested);
    let dispatch = ScriptedDispatch::new(vec![summary_answer("Add the panel", "Adds it.")]);
    let done = fx.drive(&continued, dispatch.clone());
    let again = &dispatch.seen()[0];
    assert_eq!(again.instruction_field("correction"), None);
    assert_eq!(again.resume_session, None);
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(done.checkpoint.answer_correction.is_none());
}

// GRD-FR-CCHK: a turn cancelled with no recorded cause rests the run
// `interrupted`, holding its queue and its slot, and leaves no checkout.
#[test]
fn a_cancelled_turn_rests_the_run_interrupted() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Cancelled))]),
    );
    assert_eq!(done.state, GraduationRunState::Interrupted);
    assert_eq!(
        done.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::RetryableFailure)
    );
    assert!(done.holds_queue() && done.holds_slot());
    assert!(checkout_paths::checkouts_left(&fx.app, &run.id, "ps").is_empty());
}

// GRD-FR-TYVT: a checkout a stopped run left standing is removed with its
// registration and scratch branch when the run is discarded, and the removal
// is idempotent. What the turn wrote there reaches no worktree of the author.
#[test]
fn the_checkout_goes_when_the_run_is_terminal() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let app = fx.app.clone();
    let id = run.id.clone();
    let paused = Turn::answering(Answer::Success).writing("scratch.txt", "x\n").doing(move || {
        crate::graduation::pause_graduation_run(app.clone(), id.clone()).expect("paused");
    });
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![paused]));
    assert!(!fx.root().join("scratch.txt").exists(), "the author's worktree is untouched");
    // A checkout that stands, as a process killed in the turn leaves it.
    let checkout = driver::pr_summary_checkout::create(&fx.app, &rested).expect("a checkout");
    assert!(checkout.is_dir());
    let head = git2::Repository::open(&checkout).unwrap();
    assert!(head.head_detached().unwrap(), "detached at the pinned tip");
    drop(head);

    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("gone");

    assert!(!checkout.exists(), "the checkout is removed");
    let repo = fx.repo();
    assert!(checkout_paths::registrations_left(&repo, &run.id, "ps").is_empty());
    assert!(repo
        .find_branch(&format!("synthesis/pr-summary/{}", run.id), git2::BranchType::Local)
        .is_err());
    assert!(fx.reload(&run.id).commits.is_empty());
    // Idempotent: a second reclaim finds nothing and changes nothing.
    crate::graduation::reclaim_ended_pr_summary_run(&fx.app, &fx.reload(&run.id));
    assert!(!checkout.exists());
}

// GRD-FR-SWHW, GRL-FR-UNKD: a head that shares no commit with the base, a run
// with no total time left, and a turn whose activity could not be kept each
// fail the run with their typed cause, which is not retryable.
#[test]
fn the_other_typed_causes_fail_the_run() {
    // No merge base: an orphan head branch.
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    {
        let repo = fx.repo();
        let blob = repo.blob(b"alone\n").unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        builder.insert("alone.txt", blob, 0o100644).unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let signature = repo.signature().unwrap();
        let orphan = repo.commit(None, &signature, &signature, "orphan", &tree, &[]).unwrap();
        repo.reference("refs/heads/feature/panel", orphan, true, "orphan").unwrap();
    }
    let dispatch = ScriptedDispatch::new(Vec::new());
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.turns_taken(), 0);
    let failure = done.failure.unwrap();
    assert_eq!(failure.code, "no_merge_base");
    assert!(!failure.retryable);

    // No total time left: no turn and no checkout.
    let fx = Fixture::new();
    let (mut run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    run.turn_time_used_ms = u64::MAX / 2;
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    let dispatch = ScriptedDispatch::new(Vec::new());
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.turns_taken(), 0);
    assert_eq!(done.failure.as_ref().unwrap().code, "total_time_limit");
    assert!(checkout_paths::checkouts_left(&fx.app, &run.id, "ps").is_empty());

    // The activity of the turn could not be kept.
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let failure = crate::tools::agent_exec::DurableOutputFailure {
        code: "disk_full".into(),
        message: "no space".into(),
    };
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Unlaunchable(
            AgentExecutionError::DurableOutputFailed(failure),
        ))]),
    );
    assert_eq!(done.state, GraduationRunState::Failed);
    assert_eq!(done.failure.as_ref().unwrap().code, "log_persistence_failed");
    assert!(!done.failure.unwrap().retryable);
}

// GRD-FR-WJLO, GRD-FR-KAKU: a PR summary run commits nothing, so neither its
// start nor its turn runs the main branch write check. A base that is an
// unconfirmed main branch is no refusal, and the run completes.
#[test]
fn a_pr_summary_run_into_an_unconfirmed_main_branch_is_not_refused() {
    let fx = Fixture::new();
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("main", &head, true).unwrap();
    commit_on_branch(&repo, "feature/panel", "panel.ts", "export {};\n");
    fx.app.manage(crate::git::write_guard::commands::WriteGuardState::offline());
    let guard = crate::git::write_guard::commands::require_branch_write(
        &fx.app,
        &fx.root(),
        Some("main"),
        "commit_paths",
    );
    assert!(guard.is_err(), "main is an unconfirmed main branch");

    let run = crate::graduation::start_pull_request_summary(
        fx.app.clone(),
        "feature/panel".to_string(),
        "main".to_string(),
        PrSummaryMode::Diff,
    )
    .expect("the start runs no check");
    let done = fx.drive(&run, ScriptedDispatch::new(vec![summary_answer("Panel", "Adds it.")]));

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    assert_eq!(done.pr_summary.unwrap().result.unwrap().title, "Panel");
}

// GRD-FR-XKGC, GRL-FR-LYXG: an earlier checkout that cannot be removed is
// reported, and the PR summary turn still runs, in a new checkout.
#[cfg(unix)]
#[test]
fn a_summary_runs_when_an_earlier_checkout_cannot_be_removed() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_summary_runs_when_an_earlier_checkout_cannot_be_removed",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    let home = store_base(&fx.app).unwrap().home(&run.id);
    let stuck = home.join("ps-0123abcd");
    std::fs::create_dir_all(stuck.join("locked")).unwrap();
    std::fs::write(stuck.join("locked/f.txt"), "x\n").unwrap();
    let guard = checkout_paths::ReadOnly::new(&stuck.join("locked"));

    let dispatch = ScriptedDispatch::new(vec![summary_answer("Add the panel", "Adds it.")]);
    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "failure: {:?}", done.failure);
    let seen = &dispatch.seen()[0];
    assert!(checkout_paths::is_nonced_checkout_of(&fx.app, &run.id, "ps", &seen.directory));
    assert_ne!(canonical(&seen.directory), canonical(&stuck));
    assert!(stuck.join("locked/f.txt").is_file(), "the stuck checkout stays");
    let warned = checkout_paths::warnings(
        &run.id,
        "graduation could not remove an earlier checkout of a PR summary run",
    );
    assert!(!warned.is_empty(), "the stuck checkout is reported");
    drop(guard);
}

// GRD-FR-TYVT: a checkout that cannot be made fails the run with
// `pr_summary_checkout_failed`, before any turn.
#[test]
fn a_checkout_that_cannot_be_made_fails_the_run() {
    let fx = Fixture::new();
    let (run, _, _) = summary_run(&fx, PrSummaryMode::Diff);
    // A branch `synthesis/pr-summary` makes the scratch branch
    // `synthesis/pr-summary/<run-id>` impossible to create.
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("synthesis/pr-summary", &head, true).unwrap();

    let dispatch = ScriptedDispatch::new(Vec::new());
    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(dispatch.turns_taken(), 0, "no turn is dispatched");
    assert_eq!(done.state, GraduationRunState::Failed);
    assert_eq!(done.failure.as_ref().map(|f| f.code.as_str()), Some("pr_summary_checkout_failed"));
    assert!(checkout_paths::checkouts_left(&fx.app, &run.id, "ps").is_empty());
    assert!(checkout_paths::registrations_left(&fx.repo(), &run.id, "ps").is_empty());
}

// GRD-FR-XKGC, GRD-FR-TYVT: a checkout of the name without a nonce, as an
// earlier version of the application left it, goes at the queue read once the
// run is terminal, with its registration and scratch branch.
#[test]
fn a_checkout_of_the_earlier_name_goes_when_the_run_is_terminal() {
    let fx = Fixture::new();
    let (run, tip, _) = summary_run(&fx, PrSummaryMode::Diff);
    let done = fx.drive(&run, ScriptedDispatch::new(vec![summary_answer("Add", "Adds it.")]));
    assert_eq!(done.state, GraduationRunState::Completed);
    let repo = fx.repo();
    let home = store_base(&fx.app).unwrap().home(&run.id);
    let branch = format!("synthesis/pr-summary/{}", run.id);
    repo.branch(&branch, &repo.find_commit(tip).unwrap(), true).unwrap();
    let reference = repo.find_branch(&branch, git2::BranchType::Local).unwrap().into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    repo.worktree(&format!("{}-ps", run.id), &home.join("ps"), Some(&opts)).unwrap();
    git2::Repository::open(home.join("ps")).unwrap().set_head_detached(tip).unwrap();

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    assert!(!home.join("ps").exists());
    let repo = fx.repo();
    assert!(checkout_paths::registrations_left(&repo, &run.id, "ps").is_empty());
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_err());
}
