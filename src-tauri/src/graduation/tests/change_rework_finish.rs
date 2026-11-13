//! The seed, the gate, and the finish of a rework run
//! (`OCR-openspec-change-runs.md` OCR-FR-QUSP, OCR-FR-SSAI, OCR-FR-HFLI,
//! OCR-FR-UTFI, OCR-FR-WBZX, OCR-FR-TBGA, OCR-FR-PBKH, OCR-FR-FJFI;
//! `../ai/GRL-graduation-loop.md` GRL-FR-XNBQ), driven with the agent
//! scripted and the OpenSpec CLI faked. The cases of the plain run are in
//! `change_rework.rs`.

use std::sync::{Arc, Mutex};

use super::change_runs::*;
use super::*;

const INSTRUCTION: &str = "Drop the second task.";

fn tasks_path() -> String {
    format!("openspec/changes/{CHANGE}/tasks.md")
}

fn change_path(rel: &str) -> String {
    format!("openspec/changes/{CHANGE}/{rel}")
}

/// A rework run of the author that waits in its queue, with its thread.
fn rework_run(fx: &Fixture, stream: &crate::streams::WorkStream) -> (GraduationRun, String) {
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let started = start_rework(fx, stream, INSTRUCTION).expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    (started.run, started.thread_id)
}

/// The work turn that does what the instruction asks.
fn revising() -> Turn {
    Turn::work().writing(&tasks_path(), "- [ ] Add the button\n")
}

fn store(fx: &Fixture) -> crate::fs::RootFs {
    fx.app.state::<crate::project::ProjectState>().require_store().unwrap()
}

fn proposals(fx: &Fixture, stream_id: &str) -> Vec<crate::change_proposals::ChangeEditProposal> {
    crate::change_proposals::list_proposals_impl(&store(fx), stream_id, CHANGE)
}

fn hunks_of(fx: &Fixture, proposal_id: &str) -> Vec<crate::change_proposals::ChangeEditHunk> {
    crate::change_proposals::load_hunks_impl(
        &store(fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        proposal_id,
    )
    .expect("hunks")
    .hunks
}

fn agent() -> crate::comments::Participant {
    crate::comments::Participant::Agent {
        agent_id: "a1".into(),
        handle: "Ada".into(),
        model: None,
        title: Some(String::new()),
    }
}

/// The paths `git status` reports in a working copy, untracked ones included.
fn status_of(worktree: &Path) -> Vec<String> {
    let repo = git2::Repository::open(worktree).unwrap();
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    statuses.iter().filter_map(|entry| entry.path().ok().map(str::to_string)).collect()
}

// OCR-FR-QUSP: while the run works, its rework worktree stands at
// `g/<run-id>/cw` of the store, registered as `<run-id>-cw` on the scratch
// branch `synthesis/rework-run/<run-id>`; when the run completes, the three
// are gone (OCR-FR-FJFI).
#[test]
fn the_rework_worktree_stands_at_its_names_while_the_run_works() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let id = run.id.clone();
    let store_root = fx.app.state::<GraduationState>().root().expect("the store root");
    let folder = store_root.join("g").join(&id).join("cw");
    let branch = format!("synthesis/rework-run/{id}");
    let registration = format!("{id}-cw");
    let seen: Arc<Mutex<Option<(bool, bool, bool)>>> = Arc::default();
    let look = {
        let (seen, folder, branch, registration) =
            (Arc::clone(&seen), folder.clone(), branch.clone(), registration.clone());
        let root = fx.root();
        move || {
            let repo = git2::Repository::open(&root).unwrap();
            let names: Vec<String> =
                repo.worktrees().unwrap().iter().flatten().flatten().map(str::to_string).collect();
            *seen.lock().unwrap() = Some((
                folder.join(tasks_path()).is_file(),
                repo.find_branch(&branch, git2::BranchType::Local).is_ok(),
                names.contains(&registration),
            ));
        }
    };
    let dispatch = ScriptedDispatch::new(vec![revising().doing(look), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert_eq!(
        canonical(&crate::graduation::store_base(&fx.app).unwrap().rework_worktree(&id)),
        canonical(&folder)
    );
    assert_eq!(canonical(&dispatch.of_part("work")[0].directory), canonical(&folder));
    assert_eq!(*seen.lock().unwrap(), Some((true, true, true)), "folder, branch, registration");

    assert!(!folder.exists(), "the folder is gone");
    let repo = fx.repo();
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_err(), "the branch is gone");
    let names: Vec<String> =
        repo.worktrees().unwrap().iter().flatten().flatten().map(str::to_string).collect();
    assert!(!names.contains(&registration), "the registration is gone: {names:?}");
}

// OCR-FR-SSAI: a file that the stream branch holds and the working copy has
// deleted is not in the seed.
#[test]
fn a_file_deleted_in_the_working_copy_is_not_in_the_seed() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    std::fs::remove_file(stream.worktree().join(change_path("specs/auth/spec.md"))).unwrap();
    let (run, _) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    let seed = done.base_commit.clone().expect("the seed");
    let repo = fx.repo();
    let tree = repo.find_commit(git2::Oid::from_str(&seed).unwrap()).unwrap().tree().unwrap();
    assert!(tree.get_path(Path::new(&change_path("specs/auth/spec.md"))).is_err());
    assert!(tree.get_path(Path::new(&change_path("proposal.md"))).is_ok());
    assert!(!dispatch.of_part("work")[0].tree.contains_key(&change_path("specs/auth/spec.md")));
}

// OCR-FR-HFLI: a stopped rework turn leaves the stream's working copy as it
// was: the change's `tasks.md` holds its text, and the status is clean.
#[test]
fn a_stopped_rework_turn_leaves_the_working_copy_clean() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    assert!(status_of(&stream.worktree()).is_empty(), "clean before the run");
    let (run, _) = rework_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))
            .writing(&tasks_path(), "half\n")]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Interrupted);
    let on_disk = std::fs::read_to_string(stream.worktree().join(tasks_path())).unwrap();
    assert_eq!(on_disk, "- [ ] Add the button\n- [ ] Wire the call\n");
    assert_eq!(status_of(&stream.worktree()), Vec::<String>::new());
}

// GRL-FR-XNBQ: a ready rework review on a work that deletes a file of the
// change folder is a revise that names the file, and nothing is recorded.
#[test]
fn a_ready_on_a_deleted_change_file_starts_another_pass() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let spec = change_path("specs/auth/spec.md");
    let dispatch = ScriptedDispatch::new(vec![
        revising().deleting(&spec),
        Turn::ready(),
        Turn::work(),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(proposals(&fx, &stream.id).is_empty());
    let instruction =
        dispatch.of_part("work")[1].instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains("[critical]"), "{instruction}");
    assert!(instruction.contains("deletes files of the change folder"), "{instruction}");
    assert!(instruction.contains(&spec), "{instruction}");
}

// GRL-FR-XNBQ / GRL-FR-DCKG: a write into the check folder is not a path
// outside the change folder: the ready review is not refused for it.
#[test]
fn a_write_into_the_check_folder_is_not_refused() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    let report = crate::graduation::driver::checks_folder::REPORT_PATH;
    let dispatch =
        ScriptedDispatch::new(vec![revising().writing(report, "# Checks\n"), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert_eq!(dispatch.of_part("work").len(), 1, "no other pass");
    assert_eq!(proposals(&fx, &stream.id).len(), 1);
}

// OCR-FR-WBZX / OCR-FR-TBGA / OCR-FR-UTFI / CEP-FR-HRCX / OCR-FR-QDOZ: the
// finish of a run that a confirmed agent request started records the proposal
// and its comment as that agent, not as Synthesis. A new file is one `add`
// hunk of the whole text. The finish is logged with its hunk count.
#[test]
fn the_finish_of_an_agent_request_records_as_that_agent() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let thread_id = crate::comments::open_discussion_unchecked(
        &store(&fx),
        &crate::comments::DiscussionTarget::Change {
            stream_id: stream.id.clone(),
            change: CHANGE.into(),
        },
        None,
        "@ada drop a task".into(),
        Vec::new(),
        &crate::comments::Participant::local_human(),
        &crate::notes::now_rfc3339(),
    )
    .unwrap()
    .id;
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        INSTRUCTION,
        &thread_id,
        &agent(),
    )
    .expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let dispatch = ScriptedDispatch::new(vec![
        revising().writing(&change_path("design.md"), "# Design\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch);
    assert_eq!(done.state, GraduationRunState::Completed);
    let recorded = proposals(&fx, &stream.id);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].agent, agent());
    let thread = thread(&fx, &stream.id, &thread_id);
    assert_eq!(thread.comments.last().unwrap().author, agent());

    let hunks = hunks_of(&fx, &recorded[0].id);
    let design = hunks.iter().find(|h| h.path == "design.md").expect("the new file's hunk");
    assert!(design.new_file);
    assert_eq!(design.hunk.kind, crate::text_hunks::anchors::HunkKind::Add);
    assert_eq!(design.hunk.after.as_deref(), Some("# Design\n"));
    assert_eq!(design.hunk.before, None);

    // OCR-FR-QDOZ: the finish is an `INFO` record with the hunk count, and no
    // record holds the text of a file.
    let records = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..crate::logging::LogFilter::default()
            },
            None,
            5000,
        )
        .unwrap()
        .records;
    let finished = records
        .iter()
        .find(|r| {
            r.message == "graduation finished a rework run"
                && r.fields.get("run_id") == Some(&serde_json::json!(run.id))
        })
        .expect("the finish record");
    assert_eq!(finished.level, crate::logging::LogLevel::Info);
    assert!(finished.domains.contains(&crate::logging::Domain::Backend));
    assert_eq!(finished.fields.get("hunks"), Some(&serde_json::json!(hunks.len())));
    assert_eq!(finished.fields.get("phase"), Some(&serde_json::json!("rework")));
    assert_eq!(finished.fields.get("stream_id"), Some(&serde_json::json!(stream.id)));
    for record in &records {
        let text = format!("{} {}", record.message, serde_json::to_string(&record.fields).unwrap());
        assert!(!text.contains("# Design"), "a record holds a file's text: {text}");
    }
}

// OCR-FR-KXJX / OCR-FR-TBGA: the comment of a finish that found no difference
// is written as the agent of the request too.
#[test]
fn the_nothing_to_change_comment_of_an_agent_request_is_the_agents() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let thread_id = crate::comments::open_discussion_unchecked(
        &store(&fx),
        &crate::comments::DiscussionTarget::Change {
            stream_id: stream.id.clone(),
            change: CHANGE.into(),
        },
        None,
        "@ada look again".into(),
        Vec::new(),
        &crate::comments::Participant::local_human(),
        &crate::notes::now_rfc3339(),
    )
    .unwrap()
    .id;
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        INSTRUCTION,
        &thread_id,
        &agent(),
    )
    .expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let done = fx.drive(&run, ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed);
    let last = thread(&fx, &stream.id, &thread_id).comments.last().unwrap().clone();
    assert_eq!(last.body, format!("The rework pass found nothing to change in \"{CHANGE}\"."));
    assert_eq!(last.author, agent());
}

// OCR-FR-UTFI: a file that is not UTF-8 text in the run's tree, and a file
// that is not UTF-8 text in the working copy, become no hunk; the text files
// still do.
#[test]
fn a_file_that_is_not_text_becomes_no_hunk() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    // In the working copy alone: the run writes text over it.
    let logo = stream.worktree().join(change_path("logo.md"));
    std::fs::write(&logo, [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let (run, _) = rework_run(&fx, &stream);
    // In the run's tree alone: the working copy does not hold it.
    let worktree = crate::graduation::store_base(&fx.app).unwrap().rework_worktree(&run.id);
    let blob = worktree.join(change_path("diagram.bin"));
    let write_blob = move || std::fs::write(&blob, [0xc3, 0x28, 0xa0, 0xa1]).unwrap();
    let dispatch = ScriptedDispatch::new(vec![
        revising().writing(&change_path("logo.md"), "now text\n").doing(write_blob),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(dispatch.of_part("review")[0].diff_paths.contains(&change_path("diagram.bin")));
    let recorded = proposals(&fx, &stream.id);
    assert_eq!(recorded.len(), 1);
    let paths: Vec<&str> = recorded[0].ledger.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["tasks.md"], "the files that are not text give no hunk");
}

// OCR-FR-UTFI / OCR-FR-KXJX: a file whose bytes differ only in the ends of
// its lines gives no hunk, so the finish records no proposal and appends the
// nothing-to-change comment.
#[test]
fn a_file_that_differs_only_in_line_ends_is_nothing_to_change() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing(&tasks_path(), "- [ ] Add the button\r\n- [ ] Wire the call\r\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(dispatch.of_part("review")[0].diff_paths.contains(&tasks_path()), "the bytes differ");
    assert!(proposals(&fx, &stream.id).is_empty());
    assert!(done.openspec.as_ref().unwrap().proposal_id.is_none());
    let last = thread(&fx, &stream.id, &thread_id).comments.last().unwrap().clone();
    assert_eq!(last.body, format!("The rework pass found nothing to change in \"{CHANGE}\"."));
}

/// Continue a blocked finish, and assert that it records the proposal with no
/// turn and no pass (OCR-FR-PBKH).
fn assert_continued_alone(fx: &Fixture, blocked: &GraduationRun, dispatch: &Arc<ScriptedDispatch>) {
    let turns = dispatch.turns_taken();
    let pass = blocked.pass();
    let continued = fx.continue_run(blocked);
    let done = fx.drive(&continued, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert_eq!(dispatch.turns_taken(), turns, "no turn was dispatched by the retry");
    assert_eq!(done.pass(), pass, "no pass was spent");
    assert!(done.openspec.as_ref().unwrap().proposal_id.is_some());
}

// OCR-FR-PBKH / GXD-FR-GMDI: a finish refused by a pending proposal of the
// change rests the run `blocked` with `proposal_pending` at the resume phase
// `record`; once the author decides that proposal, Continue records the
// finish alone.
#[test]
fn a_finish_blocked_by_a_pending_proposal_is_continued_alone() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, thread_id) = rework_run(&fx, &stream);
    // A proposal that stands pending when the finish comes.
    let pend = {
        let (app, stream_id, run_id) = (fx.app.clone(), stream.id.clone(), run.id.clone());
        move || {
            let store = app.state::<crate::project::ProjectState>().require_store().unwrap();
            let hunk = crate::change_proposals::PathedHunk {
                path: "proposal.md".into(),
                hunk: crate::text_hunks::hunks::ProposedHunk::from_fields(
                    Some("Logout is missing.\n".into()),
                    Some("Logout is gone.\n".into()),
                    None,
                    None,
                    None,
                    None,
                ),
            };
            crate::change_proposals::record_rework_proposal(
                &app,
                &store,
                &crate::change_proposals::AppSites { app: &app },
                crate::change_proposals::NewChangeProposal {
                    stream_id: &stream_id,
                    change: CHANGE,
                    hunks: std::slice::from_ref(&hunk),
                    rationale: "Earlier.",
                    agent: &crate::comments::Participant::synthesis(),
                    thread_id: &thread_id,
                },
                &run_id,
            )
            .expect("a pending proposal");
        }
    };
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready().doing(pend)]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "proposal_pending");
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    let earlier = proposals(&fx, &stream.id);
    assert_eq!(earlier.len(), 1);

    crate::change_proposals::decline_change_edit_proposal(
        fx.app.clone(),
        earlier[0].id.clone(),
        None,
        fx.app.state(),
        fx.app.state(),
    )
    .expect("declined");
    assert_continued_alone(&fx, &blocked, &dispatch);
    assert_eq!(proposals(&fx, &stream.id).len(), 2);
}

// OCR-FR-PBKH / GXD-FR-GMDI: a finish that cannot read the working copy rests
// the run `blocked` with `write_failed` at the resume phase `record`; once the
// file reads again, Continue records the finish alone.
#[test]
fn a_finish_blocked_by_a_failed_read_is_continued_alone() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let (run, _) = rework_run(&fx, &stream);
    // The file of the working copy is a folder when the finish reads it.
    let tasks = stream.worktree().join(tasks_path());
    let block = {
        let tasks = tasks.clone();
        move || {
            std::fs::remove_file(&tasks).unwrap();
            std::fs::create_dir_all(tasks.join("inner")).unwrap();
        }
    };
    let dispatch = ScriptedDispatch::new(vec![revising(), Turn::ready().doing(block)]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "write_failed");
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    assert!(proposals(&fx, &stream.id).is_empty());

    std::fs::remove_dir_all(&tasks).unwrap();
    std::fs::write(&tasks, "- [ ] Add the button\n- [ ] Wire the call\n").unwrap();
    assert_continued_alone(&fx, &blocked, &dispatch);
    assert_eq!(proposals(&fx, &stream.id).len(), 1);
}
