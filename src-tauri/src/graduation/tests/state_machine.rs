//! State-machine, queue and observability tests for the graduation run
//! (`GRD-graduation.md`, `GRL-graduation-loop.md`).

use crate::graduation::*;

fn run_at(state: GraduationRunState) -> GraduationRun {
    GraduationRun {
        id: "g1".into(),
        stream_id: "w1".into(),
        stream_name: "editor work".into(),
        direct_target: None,
        target_hold: None,
        project_key: "p".into(),
        state,
        standing_work: StandingWork::default(),
        standing_work_message: None,
        standing_work_outcome: None,
        input: CapturedGraduationInput {
            draft_id: "d1".into(),
            draft_name: "Editor scroll".into(),
            prompt: "Fix the editor scroll.".into(),
            prompt_checksum: "abc".into(),
            captured_at: "2026-01-01T00:00:00Z".into(),
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
        merge: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

// GRD-FR-QJHM: the three terminal states, and no others.
#[test]
fn only_the_three_terminal_states_are_terminal() {
    use GraduationRunState as S;
    for state in [S::Completed, S::Discarded, S::Failed] {
        assert!(state.is_terminal(), "{} is terminal", state.as_str());
    }
    for state in [
        S::Queued,
        S::Working,
        S::Reviewing,
        S::AwaitingAuthor,
        S::Blocked,
        S::Interrupted,
    ] {
        assert!(!state.is_terminal(), "{} is not terminal", state.as_str());
    }
}

// GRD-FR-BNTC: a run holds its stream while it is doing agent work or resting
// on a condition inside one. Every other state holds nothing.
#[test]
fn a_run_holds_its_stream_only_while_it_is_working_reviewing_or_blocked() {
    use GraduationRunState as S;
    for state in [S::Working, S::Reviewing, S::Blocked] {
        assert!(state.holds_stream(), "{} holds", state.as_str());
    }
    // A paused, interrupted, waiting, queued or ended run releases it — a
    // stream a paused run kept would be stuck behind an author who has stopped.
    for state in [
        S::Queued,
        S::AwaitingAuthor,
        S::Interrupted,
        S::Completed,
        S::Discarded,
        S::Failed,
    ] {
        assert!(!state.holds_stream(), "{} releases", state.as_str());
    }
}

// GRD-FR-DXWL: a draft a non-terminal run holds is locked, and a terminal run
// releases it.
#[test]
fn a_non_terminal_run_locks_its_draft() {
    assert!(run_at(GraduationRunState::Queued).locks_draft());
    assert!(run_at(GraduationRunState::Working).locks_draft());
    assert!(run_at(GraduationRunState::AwaitingAuthor).locks_draft());
    assert!(!run_at(GraduationRunState::Completed).locks_draft());
    assert!(!run_at(GraduationRunState::Discarded).locks_draft());
}

// GRD-FR-EGWS: a stream starts the earliest eligible member of its queue, and
// a run whose auto-start is off is passed over rather than waited for.
#[test]
fn a_queue_starts_only_an_eligible_run() {
    let mut run = run_at(GraduationRunState::Queued);
    assert!(run.is_eligible());
    run.auto_start = false;
    assert!(!run.is_eligible(), "auto-start off is not eligible");
    run.auto_start = true;
    run.state = GraduationRunState::Working;
    assert!(!run.is_eligible(), "a working run is not started again");
}

// GRD-FR-EGWS: a stream already holding a working run starts nothing, and a
// stream's queue is read from the project's one run order.
#[test]
fn a_busy_stream_starts_nothing_and_a_free_one_starts_its_earliest_member() {
    let mut first = run_at(GraduationRunState::Queued);
    first.id = "g1".into();
    let mut second = run_at(GraduationRunState::Queued);
    second.id = "g2".into();
    let mut other = run_at(GraduationRunState::Queued);
    other.id = "g3".into();
    other.stream_id = "w2".into();

    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: vec![first, second, other],
    };
    assert_eq!(queue::next_eligible(&queue, "w1").map(|r| r.id.as_str()), Some("g1"));
    assert_eq!(queue::next_eligible(&queue, "w2").map(|r| r.id.as_str()), Some("g3"));

    // With the first run working, its stream starts nothing more.
    let mut busy = queue.runs.clone();
    busy[0].state = GraduationRunState::Working;
    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: busy,
    };
    assert!(queue::next_eligible(&queue, "w1").is_none());
    // The other stream is unaffected: streams work independently.
    assert_eq!(queue::next_eligible(&queue, "w2").map(|r| r.id.as_str()), Some("g3"));
}

// GRD-FR-VLFO: a stream's queue is the subsequence of the project's run order
// holding that stream's waiting members.
#[test]
fn a_streams_queue_is_the_subsequence_of_the_projects_run_order() {
    let mut a = run_at(GraduationRunState::Queued);
    a.id = "g1".into();
    let mut b = run_at(GraduationRunState::Queued);
    b.id = "g2".into();
    b.stream_id = "w2".into();
    let mut c = run_at(GraduationRunState::Queued);
    c.id = "g3".into();
    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: vec![a, b, c],
    };
    let ours: Vec<&str> = queue::queue_of(&queue, "w1")
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ours, vec!["g1", "g3"]);
    assert_eq!(queue::position_in_queue(&queue, "g3"), Some(1));
}

// GOB-FR-JAJU / GOB-FR-HXUZ: each state resolves its own stage and condition,
// and the three that keep the stage they stood at keep it.
#[test]
fn each_state_resolves_its_stage_and_its_condition() {
    use GraduationRunState as S;
    use GraduationStageCondition as C;
    use GraduationVisualStage as V;

    let mut o = GraduationObservability::default();
    o.apply_state(S::Working, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Working, C::Active));

    o.apply_state(S::Reviewing, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Review, C::Active));

    // Waiting, blocked and interrupted keep the stage the run stood at.
    o.apply_state(S::AwaitingAuthor, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Review, C::Waiting));
    o.apply_state(S::Blocked, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Review, C::Blocked));
    o.apply_state(S::Interrupted, true);
    assert_eq!((o.current_stage, o.stage_condition), (V::Review, C::Paused));
    o.apply_state(S::Interrupted, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Review, C::Stopped));

    o.apply_state(S::Completed, false);
    assert_eq!((o.current_stage, o.stage_condition), (V::Done, C::Complete));
}

// GOB-FR-VVNI: a transition is appended only where the stage actually changes.
#[test]
fn a_change_of_condition_alone_appends_no_transition() {
    let mut o = GraduationObservability::default();
    o.note_stage(
        GraduationVisualStage::Working,
        1,
        "t".into(),
        StageReason::WorkStarted,
    );
    assert_eq!(o.stage_history.len(), 1);
    // The same stage again appends nothing.
    o.note_stage(
        GraduationVisualStage::Working,
        1,
        "t".into(),
        StageReason::WorkStarted,
    );
    assert_eq!(o.stage_history.len(), 1);
    // Coming to rest changes the condition and appends no entry.
    o.apply_state(GraduationRunState::Blocked, false);
    assert_eq!(o.stage_history.len(), 1);
}

// GOB-FR-BTXN: `review_revision` is the one backward move.
#[test]
fn the_only_backward_move_is_a_review_asking_for_revision() {
    let mut o = GraduationObservability::default();
    o.note_stage(GraduationVisualStage::Working, 1, "t".into(), StageReason::WorkStarted);
    o.note_stage(GraduationVisualStage::Review, 1, "t".into(), StageReason::ReviewStarted);
    o.note_stage(
        GraduationVisualStage::Working,
        2,
        "t".into(),
        StageReason::ReviewRevision,
    );
    let backward: Vec<&StageTransition> = o
        .stage_history
        .iter()
        .filter(|t| t.from == GraduationVisualStage::Review && t.to == GraduationVisualStage::Working)
        .collect();
    assert_eq!(backward.len(), 1);
    assert_eq!(backward[0].reason, StageReason::ReviewRevision);
}

// GOB-FR-XYCY / GOB-FR-DKCJ: a pass carries what it was asked to do and what
// the review decided, findings whole.
#[test]
fn a_pass_records_its_task_its_verdict_and_its_findings_whole() {
    let mut o = GraduationObservability::default();
    o.open_pass(1, "do the work".into(), "t0".into());
    assert_eq!(o.passes[0].status, PassStatus::Working);

    let verdict = ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: "one thing is missing".into(),
        findings: vec![ReviewFinding {
            severity: ReviewSeverity::Major,
            description: "the empty state is not implemented".into(),
            affected_files: vec!["src/panel.tsx".into()],
            correction: "render the empty state".into(),
        }],
    };
    o.settle_pass(&verdict, Some("do this next".into()), "t1".into());

    let record = &o.passes[0];
    assert_eq!(record.status, PassStatus::Failed);
    assert_eq!(record.task, "do the work");
    assert_eq!(record.rationale.as_deref(), Some("one thing is missing"));
    assert_eq!(record.findings.len(), 1);
    assert_eq!(record.findings[0].correction, "render the empty state");
    assert_eq!(record.next_instruction.as_deref(), Some("do this next"));
}

// DRS-FR-KQTW: a draft is graduated while a run of it has committed and is not
// discarded, and the status is released when every such run is discarded.
#[test]
fn a_draft_is_graduated_while_a_committing_run_of_it_stands() {
    let mut run = run_at(GraduationRunState::Completed);
    run.commits = vec!["abc".into()];
    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: vec![run.clone()],
    };
    assert!(queue::draft_graduation_stands(&queue, "d1"));

    // Discarded releases it.
    let mut discarded = run.clone();
    discarded.state = GraduationRunState::Discarded;
    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: vec![discarded],
    };
    assert!(!queue::draft_graduation_stands(&queue, "d1"));

    // A run that committed nothing never granted it.
    let queue = GraduationQueue {
        project_key: "p".into(),
        runs: vec![run_at(GraduationRunState::Completed)],
    };
    assert!(!queue::draft_graduation_stands(&queue, "d1"));
}

// GRL-FR-ARPX / GRL-FR-REPL: two passes is the bound a project that configures
// no budget of its own runs under.
#[test]
fn the_default_pass_bound_is_two() {
    assert_eq!(driver::DEFAULT_PASS_BUDGET, 2);
    assert_eq!(driver::REVIEW_TURNS_PER_PASS, 1);
}

// GXD-FR-ODGX: a task input outside the contract is refused before any
// container exists.
#[test]
fn a_task_input_is_refused_before_a_container_is_created() {
    let run = run_at(GraduationRunState::Queued);
    let good = driver::compose_input(&run, driver::PART_WORK, driver::PURPOSE_GENERATE);
    assert!(good.validate(driver::DEFAULT_PASS_BUDGET).is_ok());

    let mut wrong_version = good.clone();
    wrong_version.graduation_input_version = 2;
    assert!(wrong_version.validate(driver::DEFAULT_PASS_BUDGET).is_err());

    let mut unknown_part = good.clone();
    unknown_part.part = "authoring".into();
    assert!(unknown_part.validate(driver::DEFAULT_PASS_BUDGET).is_err(), "a retired part is refused");

    let mut unknown_purpose = good.clone();
    unknown_purpose.purpose = "publish".into();
    assert!(unknown_purpose.validate(driver::DEFAULT_PASS_BUDGET).is_err());

    let mut past_the_bound = good.clone();
    past_the_bound.pass = driver::DEFAULT_PASS_BUDGET + 1;
    assert!(past_the_bound.validate(driver::DEFAULT_PASS_BUDGET).is_err());

    let mut no_prompt = good;
    no_prompt.prompt = "   ".into();
    assert!(no_prompt.validate(driver::DEFAULT_PASS_BUDGET).is_err());
}

// GXD-FR-XQJR / EAC-FR-IRRD: the masked kind is reached by one part alone.
#[test]
fn only_the_semantic_merge_part_reaches_the_masked_turn_kind() {
    use crate::tools::agent_exec::TurnKind;
    assert_eq!(driver::turn_kind_for(driver::PART_WORK), TurnKind::Work);
    assert_eq!(driver::turn_kind_for(driver::PART_REVIEW), TurnKind::Review);
    assert_eq!(
        driver::turn_kind_for(driver::PART_SEMANTIC_MERGE),
        TurnKind::SemanticRebase
    );
    for part in ["", "unknown", "authoring", "implementation"] {
        assert_ne!(
            driver::turn_kind_for(part),
            TurnKind::SemanticRebase,
            "part {part:?} must not reach the masked kind"
        );
    }
}

// ESU-FR-19: an escalation request outside the tool form records nothing.
#[test]
fn an_escalation_request_is_validated_on_the_tool_form() {
    let question = |text: &str| GraduationEscalationQuestion {
        position: 1,
        question: text.into(),
        options: Vec::new(),
    };
    assert!(validate_escalation_request("why", &[question("what?")]).is_ok());
    assert!(validate_escalation_request("  ", &[question("what?")]).is_err());
    assert!(validate_escalation_request("why", &[]).is_err());

    let too_many: Vec<GraduationEscalationQuestion> =
        (0..9).map(|_| question("what?")).collect();
    assert!(validate_escalation_request("why", &too_many).is_err());

    let mut too_many_options = question("what?");
    too_many_options.options = (0..4)
        .map(|_| GraduationProposedResponse {
            answer: "a".into(),
            summary: "s".into(),
            description: "d".into(),
        })
        .collect();
    assert!(validate_escalation_request("why", &[too_many_options]).is_err());
}

// GRL-FR-GADT: the loop's instructions are the compiled-in files, and each part
// carries its own.
#[test]
fn each_part_carries_its_own_compiled_in_instruction() {
    let work = driver::instruction_for(driver::PART_WORK);
    let review = driver::instruction_for(driver::PART_REVIEW);
    assert!(!work.is_empty() && !review.is_empty());
    assert_ne!(work, review);
    // GRL-FR-YIJG: the authoring comments never reach an agent.
    for text in [work, review] {
        assert!(!text.contains("<!--"), "an authoring comment reached the agent");
    }
}

// GRL-FR-DAIB: the work instruction names no deliverable of its own. What a
// change must contain is the project's rule rather than this loop's.
#[test]
fn the_work_instruction_imposes_no_shape_on_the_work() {
    let work = driver::instruction_for(driver::PART_WORK);
    assert!(
        work.contains("CLAUDE.md") && work.contains("AGENTS.md"),
        "the work turn is directed at the project's own instructions",
    );
    for imposed in [
        "write the specification",
        "specifications/",
        "the specification it describes",
    ] {
        assert!(
            !work.to_lowercase().contains(&imposed.to_lowercase()),
            "the work instruction must not require {imposed:?}",
        );
    }
}

// GRD-FR-BNTC: one run of a stream works at a time, and streams work at the
// same time up to the project's limit.
#[test]
fn a_stream_takes_one_run_and_a_project_takes_its_limit_of_streams() {
    use crate::tools::agent_exec::runtime::CancellationToken;
    let state = GraduationState::default();
    let token = CancellationToken::new;

    // One run of a stream at a time.
    assert!(state.claim("w1", "g1", false, crate::project_settings::GraduationConcurrency::limited(2), token()));
    assert!(!state.claim("w1", "g2", false, crate::project_settings::GraduationConcurrency::limited(2), token()), "the stream is held");
    assert_eq!(state.holder_of("w1").as_deref(), Some("g1"));

    // A second stream works beside it, up to the limit.
    assert!(state.claim("w2", "g2", false, crate::project_settings::GraduationConcurrency::limited(2), token()));
    assert_eq!(state.working_count(), 2);
    assert!(!state.claim("w3", "g3", false, crate::project_settings::GraduationConcurrency::limited(2), token()), "the limit is reached");

    // One run holds one stream, whatever else is free.
    assert!(!state.claim("w3", "g1", false, crate::project_settings::GraduationConcurrency::limited(4), token()));

    // Releasing one frees a slot.
    state.release("w1");
    assert_eq!(state.working_count(), 1);
    assert!(state.claim("w3", "g3", false, crate::project_settings::GraduationConcurrency::limited(2), token()));

    // A limit of zero is read as one: a project that may dispatch nothing at
    // all is a queue that never moves.
    let single = GraduationState::default();
    assert!(single.claim("w1", "g1", false, crate::project_settings::GraduationConcurrency::limited(0), token()));
    assert!(!single.claim("w2", "g2", false, crate::project_settings::GraduationConcurrency::limited(0), token()));
}

/// GXD-FR-GMDI: a record written before the checkpoint carried a resume phase
/// or a blocker count reads back, and its defaults are the behaviour it was
/// written under.
///
/// Every run on an author's disk predates these fields, so a record that failed
/// to deserialise would lose the run rather than continue it.
#[test]
fn a_record_written_before_these_fields_still_reads() {
    let stored = serde_json::json!({
        "pass": 2,
        "changedPaths": ["src/panel.ts"],
        "hiddenPaths": [],
        "hiddenPathsOmitted": 0,
        "pendingEscalationAnswers": [],
        "verdictRefusals": 1,
        // GRL-FR-DNKA: a checkpoint written before the account was renamed
        // carries the old key. It is not a field of this record any more, and a
        // record that refused to load over it would lose a run in flight on an
        // author's disk.
        "agentFailure": "what the work turn said under the old name"
    });

    let checkpoint: GraduationCheckpoint =
        serde_json::from_value(stored).expect("an older checkpoint still reads");

    assert_eq!(checkpoint.pass, 2);
    assert_eq!(checkpoint.verdict_refusals, 1);
    // GRL-FR-DNKA: the old key is dropped and the account reads as absent, which
    // is the safe reading — a run resumed across the rename is reviewed on its
    // change set alone, exactly as it would have been.
    assert!(
        checkpoint.agent_account.is_none(),
        "the old key was read as an account",
    );
    assert!(
        checkpoint.resume_part.is_none(),
        "no pointer means the work turn, which is what it was written under"
    );
    assert!(checkpoint.blocked_code.is_none());
    assert_eq!(checkpoint.block_attempts, 0);
}

/// GRU-FR-LBPR: a blocker written before it carried an attempt reads back, and
/// the zero it reads is what a surface renders as no attempt at all.
#[test]
fn a_blocker_written_before_it_carried_an_attempt_still_reads() {
    let stored = serde_json::json!({
        "code": "review_checkout_failed",
        "message": "symlink refused",
        "clearsBy": "Continue the run to try the review again."
    });

    let blocker: GraduationBlocker =
        serde_json::from_value(stored).expect("an older blocker still reads");

    assert_eq!(blocker.code, "review_checkout_failed");
    assert_eq!(blocker.attempt, 0);
}

/// GXD-FR-GMDI / GOB-FR-BTXN: the names these fields cross the seam under.
///
/// The frontend mirrors them by hand in `src/types/graduation.ts` and
/// `src/types/graduationObservability.ts`. A rename on either side is a runtime
/// failure that no typecheck on either side would catch, so the wire names are
/// pinned here rather than left to the derive.
#[test]
fn the_new_fields_cross_the_seam_under_the_names_the_frontend_reads() {
    let mut checkpoint = GraduationCheckpoint {
        pass: 2,
        block_attempts: 3,
        ..Default::default()
    };
    let value = serde_json::to_value(&checkpoint).expect("serialized");

    assert_eq!(value.get("blockAttempts").and_then(|v| v.as_u64()), Some(3));
    assert!(
        value.get("resumePart").is_none(),
        "an absent phase is omitted rather than sent as null"
    );
    assert!(value.get("blockedCode").is_none());

    checkpoint.resume_part = Some("review".to_string());
    checkpoint.blocked_code = Some("review_checkout_failed".to_string());
    let value = serde_json::to_value(&checkpoint).expect("serialized");
    assert_eq!(value.get("resumePart").and_then(|v| v.as_str()), Some("review"));
    assert_eq!(
        value.get("blockedCode").and_then(|v| v.as_str()),
        Some("review_checkout_failed")
    );

    let blocker = GraduationBlocker {
        code: "review_checkout_failed".to_string(),
        message: "symlink refused".to_string(),
        clears_by: "Continue the run.".to_string(),
        attempt: 2,
    };
    let value = serde_json::to_value(&blocker).expect("serialized");
    assert_eq!(value.get("attempt").and_then(|v| v.as_u64()), Some(2));

    assert_eq!(
        serde_json::to_value(crate::graduation::observability::StageReason::BlockedRetry)
            .expect("serialized"),
        serde_json::json!("blocked_retry")
    );
}
