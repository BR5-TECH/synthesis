//! The rework run (`OCR-openspec-change-runs.md` OCR-FR-QUSP through
//! OCR-FR-AWDH; `../ai/GRL-graduation-loop.md` GRL-FR-CWSA, GRL-FR-HSNY,
//! GRL-FR-XNBQ; `GXD-graduation-execution.md` GXD-FR-KXXB, GXD-FR-GMDI;
//! `GOB-graduation-observability.md` GOB-FR-TEAY), driven with the agent
//! scripted and the OpenSpec CLI faked.

use super::change_runs::*;
use super::run_summary::ready_with;
use super::*;

/// A rework run that waits in its queue, with its request thread.
fn rework_run(fx: &Fixture, stream: &crate::streams::WorkStream) -> (GraduationRun, String) {
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let started = start_rework(fx, stream, "Drop the second task.").expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    (started.run, started.thread_id)
}

fn tasks_path() -> String {
    format!("openspec/changes/{CHANGE}/tasks.md")
}

/// The work turn of a rework run that does what the instruction asks.
fn revising() -> Turn {
    Turn::work().writing(&tasks_path(), "- [ ] Add the button\n")
}

fn proposals(fx: &Fixture, stream_id: &str) -> Vec<crate::change_proposals::ChangeEditProposal> {
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    crate::change_proposals::list_proposals_impl(&store, stream_id, CHANGE)
}

// OCR-FR-QUSP, OCR-FR-SSAI, GXD-FR-KXXB, GRD-FR-YBUM, OCR-FR-TQCW: the work
// turn stands in the run's rework worktree, at a seed commit that holds the
// change folder as the working copy has it, uncommitted edits included; the
// review stands in a checkout of it; the stream is not written.
#[test]
fn a_rework_run_works_in_its_own_worktree_at_its_seed() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    // CEP-FR-IYHP: an accepted hunk stands uncommitted in the working copy.
    let design = stream.worktree().join("openspec/changes").join(CHANGE).join("design.md");
    std::fs::write(&design, "# Design\n\nAccepted earlier.\n").unwrap();
    let stream_head = head_of(&stream.worktree());
    let (run, _) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);

    let work = &dispatch.of_part("work")[0];
    let base = crate::graduation::store_base(&fx.app).unwrap();
    assert_eq!(canonical(&work.directory), canonical(&base.rework_worktree(&run.id)));
    let seed = done.base_commit.clone().expect("the seed is the base commit");
    assert_eq!(work.head.as_deref(), Some(seed.as_str()));
    let repo = fx.repo();
    let seed_commit = repo.find_commit(git2::Oid::from_str(&seed).unwrap()).unwrap();
    assert_eq!(seed_commit.parent_id(0).unwrap().to_string(), stream_head);
    let in_seed = seed_commit
        .tree()
        .unwrap()
        .get_path(Path::new(&format!("openspec/changes/{CHANGE}/design.md")))
        .is_ok();
    assert!(in_seed, "the uncommitted file of the working copy is in the seed");
    assert_eq!(
        work.tree.get(&format!("openspec/changes/{CHANGE}/design.md")).map(String::as_str),
        Some("# Design\n\nAccepted earlier.\n")
    );

    let review = &dispatch.of_part("review")[0];
    assert_ne!(review.directory, work.directory, "the review stands in a checkout of its own");
    assert_eq!(review.head.as_deref(), Some(seed.as_str()));
    assert!(review.diff_paths.contains(&tasks_path()));

    // No turn wrote the stream.
    assert_eq!(head_of(&stream.worktree()), stream_head);
    assert!(done.commits.is_empty());
    let on_disk = std::fs::read_to_string(stream.worktree().join(tasks_path())).unwrap();
    assert_eq!(on_disk, "- [ ] Add the button\n- [ ] Wire the call\n");
}

// GRL-FR-CWSA, GRL-FR-MXXA, GRL-FR-GWDB, GRL-FR-HSNY, GRL-FR-GADT: the turns
// carry the rework blocks; the input names the phase, the change, and the
// instruction, and no thread or requester; the review reads the validation.
#[test]
fn the_turns_of_a_rework_run_carry_their_blocks_and_the_instruction() {
    let (fx, runner, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let work = &dispatch.of_part("work")[0];
    assert!(work
        .instruction
        .starts_with(crate::graduation::driver::prompts::WORK_PROMPT.trim_end()));
    assert!(work.instruction.contains("input.openspec.instruction"));
    assert!(work.instruction.contains("Change no file outside"));
    assert!(work.instruction.contains(&format!("openspec validate {CHANGE} --strict")));
    assert!(!work.instruction.contains("Drop the second task"), "no instruction text in it");
    let review = &dispatch.of_part("review")[0];
    assert!(review
        .instruction
        .starts_with(crate::graduation::driver::prompts::REVIEW_PROMPT.trim_end()));
    assert!(review.instruction.contains("Judge the revision against `input.openspec.instruction`"));
    for seen in dispatch.seen() {
        assert_eq!(
            seen.input["openspec"],
            serde_json::json!({
                "phase": "rework",
                "change": CHANGE,
                "instruction": "Drop the second task.",
            })
        );
    }
    assert_eq!(review.input["openspec_validation"]["valid"], true);
    assert!(review.input.get("openspec_apply_progress").is_none());
    assert_eq!(runner.calls_of("instructions"), 0, "a rework run reads no progress");
}

// OCR-FR-ZNJT, OCR-FR-UTFI, OCR-FR-WBZX, OCR-FR-GWGE, CEP-FR-QDJH, GOB-FR-TEAY:
// a ready review records one proposal of line hunks into the request thread as
// the requester, with the summary as its rationale, and the run completes with
// no commit and the proposal id.
#[test]
fn a_ready_rework_review_records_one_proposal_into_the_request_thread() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        revising().writing(&format!("openspec/changes/{CHANGE}/design.md"), "# Design\n"),
        ready_with(
            serde_json::json!({ "title": "Drop a task", "description": "One task is gone." }),
        ),
    ]);
    let done = fx.drive(&run, dispatch);
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(done.commits.is_empty(), "a rework run makes no commit");
    assert_eq!(done.observability.current_stage, crate::graduation::GraduationVisualStage::Done);

    let recorded = proposals(&fx, &stream.id);
    assert_eq!(recorded.len(), 1);
    let proposal = &recorded[0];
    assert_eq!(done.openspec.as_ref().unwrap().proposal_id.as_deref(), Some(proposal.id.as_str()));
    assert_eq!(proposal.thread_id, thread_id);
    assert_eq!(proposal.rationale, "One task is gone.");
    assert_eq!(proposal.agent, crate::comments::Participant::synthesis());
    let paths: Vec<&str> = proposal.ledger.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["design.md", "tasks.md"], "hunks in the order of their paths");

    let thread = thread(&fx, &stream.id, &thread_id);
    let last = thread.comments.last().unwrap();
    assert_eq!(last.author, crate::comments::Participant::synthesis());
    assert_eq!(last.body, "One task is gone.");
    // The stream's working copy is not written before the author decides.
    let on_disk = std::fs::read_to_string(stream.worktree().join(tasks_path())).unwrap();
    assert_eq!(on_disk, "- [ ] Add the button\n- [ ] Wire the call\n");
}

// OCR-FR-KXJX, OCR-FR-GWGE, OCR-FR-WBZX: a rework that changes nothing records
// no proposal; it appends the fixed comment into the request thread and
// completes. The verdict's rationale stands where no summary is given.
#[test]
fn a_rework_that_changes_nothing_comments_and_completes() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let done = fx.drive(&run, dispatch);
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(proposals(&fx, &stream.id).is_empty());
    assert!(done.openspec.as_ref().unwrap().proposal_id.is_none());
    let thread = thread(&fx, &stream.id, &thread_id);
    let last = thread.comments.last().unwrap();
    assert_eq!(last.body, format!("The rework pass found nothing to change in \"{CHANGE}\"."));
    assert_eq!(last.author, crate::comments::Participant::synthesis());
}

// OCR-FR-PBKH, GXD-FR-GMDI, GXD-FR-TJRV, GOB-FR-TEAY: a finish refused by a
// resolved request thread rests the run `blocked` at the review stage with the
// resume phase `record`; Continue retries the finish alone, with no turn and no
// pass.
#[test]
fn a_refused_finish_blocks_and_continue_retries_the_record_alone() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    let (app, stream_id, thread) = (fx.app.clone(), stream.id.clone(), thread_id.clone());
    let resolve = move || {
        let store = app.state::<crate::project::ProjectState>().require_store().unwrap();
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: stream_id.clone(),
            change: CHANGE.into(),
        };
        let found = crate::comments::locate_discussion_of(&store, &target, &thread).unwrap();
        crate::comments::set_resolution_to(
            &store,
            found.log_ref(),
            &thread,
            true,
            &crate::comments::Participant::local_human(),
            &crate::notes::now_rfc3339(),
        )
        .unwrap();
    };
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready().doing(resolve)]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "discussion_resolved");
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    assert_eq!(
        blocked.observability.current_stage,
        crate::graduation::GraduationVisualStage::Review
    );
    assert!(proposals(&fx, &stream.id).is_empty());
    let pass = blocked.pass();

    set_resolved(&fx, &stream.id, &thread_id, false);
    let continued = fx.continue_run(&blocked);
    let done = fx.drive(&continued, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert_eq!(dispatch.turns_taken(), 2, "no turn was dispatched by the retry");
    assert_eq!(done.pass(), pass, "no pass was spent");
    assert_eq!(proposals(&fx, &stream.id).len(), 1);
}

// OCR-FR-HFLI, GRD-FR-SWOJ: a turn of a rework run that stops without a review
// commits nothing; what it wrote stays in the rework worktree, and the run
// rests `interrupted`.
#[test]
fn a_stopped_rework_turn_commits_nothing() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let stream_head = head_of(&stream.worktree());
    let (run, _) = rework_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))
            .writing(&tasks_path(), "half\n")]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert!(run.commits.is_empty());
    assert_eq!(head_of(&stream.worktree()), stream_head);
    let base = crate::graduation::store_base(&fx.app).unwrap();
    let kept = std::fs::read_to_string(base.rework_worktree(&run.id).join(tasks_path())).unwrap();
    assert_eq!(kept, "half\n");
    assert_eq!(run.base_commit.as_deref().map(|s| s.len()), Some(40));
}

// GRL-FR-XNBQ: a rework review that answers ready on a write outside the
// change folder is replaced with a revise, and no proposal is recorded.
#[test]
fn a_ready_on_a_write_outside_the_change_folder_starts_another_pass() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        revising().writing("src/main.rs", "fn main() {}\n"),
        Turn::ready(),
        Turn::work(),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(proposals(&fx, &stream.id).is_empty());
    let instruction =
        dispatch.of_part("work")[1].instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains("src/main.rs"), "{instruction}");
}

// OCR-FR-FJFI, GRD-FR-GMTX: a completed rework run's worktree, registration,
// and scratch branch are gone; the stream is not touched.
#[test]
fn an_ended_rework_run_removes_what_it_owns() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let done = fx.drive(&run, ScriptedDispatch::new(vec![revising(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed);
    let base = crate::graduation::store_base(&fx.app).unwrap();
    assert!(!base.rework_worktree(&run.id).exists());
    let repo = fx.repo();
    let branch = crate::graduation::driver::rework_workspace::scratch_branch_for_test(&run.id);
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_err());
    let names: Vec<String> =
        repo.worktrees().unwrap().iter().flatten().flatten().map(str::to_string).collect();
    assert!(!names.contains(&format!("{}-cw", run.id)), "{names:?}");
    assert!(stream.worktree().exists(), "the stream stays");
}

// OCR-FR-FJFI, OCR-FR-AWDH: a rework run discarded while it rests has what it
// owns removed, and appends no comment; the first queue read after a launch
// removes what an ended run still left.
#[test]
fn a_discarded_rework_run_is_reclaimed_and_writes_nothing() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))
            .writing(&tasks_path(), "half\n")]);
    let run = fx.drive(&run, dispatch);
    let base = crate::graduation::store_base(&fx.app).unwrap();
    assert!(base.rework_worktree(&run.id).exists(), "a rest keeps the worktree");
    let comments_before = thread(&fx, &stream.id, &thread_id).comments.len();
    let head_before = head_of(&stream.worktree());
    let tasks = || std::fs::read_to_string(stream.worktree().join(tasks_path())).unwrap();
    let tasks_before = tasks();

    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).unwrap();
    assert!(!base.rework_worktree(&run.id).exists());
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments.len(), comments_before);
    assert!(proposals(&fx, &stream.id).is_empty());
    // OCR-FR-AWDH: the stream branch and its working copy are as they were.
    assert_eq!(head_of(&stream.worktree()), head_before);
    assert_eq!(tasks(), tasks_before);
    assert!(crate::streams::uncommitted_paths_of(&stream.worktree()).is_empty());
}

// OCR-FR-FJFI: on the first queue read, a rework run that has ended with its
// worktree still standing has it removed.
#[test]
fn the_queue_read_reclaims_what_an_ended_rework_run_left() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))]);
    let mut run = fx.drive(&run, dispatch);
    let base = crate::graduation::store_base(&fx.app).unwrap();
    assert!(base.rework_worktree(&run.id).exists());
    // A process that ended the run and stopped before it removed anything.
    run.state = GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    assert!(base.rework_worktree(&run.id).exists());

    crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert!(!base.rework_worktree(&run.id).exists());
}

// OCR-FR-GWGE: `revert_graduation_run` against a rework run is refused.
#[test]
fn a_rework_run_is_not_revertable() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let done = fx.drive(&run, ScriptedDispatch::new(vec![revising(), Turn::ready()]));
    let refused = crate::graduation::revert_graduation_run(fx.app.clone(), done.id.clone());
    assert_eq!(refused.unwrap_err(), "run_not_revertable");
}

// OCR-FR-OXVH, CEP-FR-OIZT, PCE-FR-MVKT: while the rework run is not terminal,
// every other recording on the change is refused with `rework_running`; the
// finish of the run is not.
#[test]
fn a_recording_is_refused_while_a_rework_run_stands() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    assert_eq!(
        crate::graduation::rework_run_on_change(&fx.app, &stream.id, CHANGE).as_deref(),
        Some(run.id.as_str())
    );
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let agent = crate::comments::Participant::synthesis();
    let hunk = crate::change_proposals::PathedHunk {
        path: "tasks.md".into(),
        hunk: crate::text_hunks::hunks::ProposedHunk::from_fields(
            Some("- [ ] Wire the call\n".into()),
            None,
            None,
            None,
            None,
            None,
        ),
    };
    let refused = crate::change_proposals::record_change_proposal(
        &fx.app,
        &store,
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id: &stream.id,
            change: CHANGE,
            hunks: std::slice::from_ref(&hunk),
            rationale: "Drop it.",
            agent: &agent,
            thread_id: &thread_id,
        },
    );
    assert_eq!(refused.unwrap_err(), crate::change_proposals::RecordRefusal::ReworkRunning);

    let done = fx.drive(&run, ScriptedDispatch::new(vec![revising(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed, "the finish is not refused");
    assert_eq!(proposals(&fx, &stream.id).len(), 1);
    assert!(crate::graduation::rework_run_on_change(&fx.app, &stream.id, CHANGE).is_none());
}

// GRL-FR-HSNY, GRL-FR-HSZD, GXD-FR-XQJR: a ready rework review on a change
// that does not validate is replaced with a revise, and no proposal is
// recorded; the turns are `work` and `review` turns.
#[test]
fn a_ready_on_an_invalid_rework_starts_another_pass() {
    let (fx, runner, _) = change_fixture(READY);
    runner.set(
        "validate",
        Ok(crate::openspec::ProcessOutput {
            exit_status: Some(1),
            stdout: br#"{ "items": [ { "valid": false, "issues": [ { "level": "ERROR", "path": "tasks.md", "message": "x" } ] } ] }"#.to_vec(),
        }),
    );
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![revising(), Turn::ready(), Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(proposals(&fx, &stream.id).is_empty());
    assert_eq!(dispatch.of_part("work")[0].turn_kind, TurnKind::Work);
    assert_eq!(dispatch.of_part("review")[0].turn_kind, TurnKind::Review);
}
