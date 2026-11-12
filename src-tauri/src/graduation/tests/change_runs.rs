//! The starts and the restart of an OpenSpec change run
//! (`OCR-openspec-change-runs.md` OCR-FR-IDFC through OCR-FR-TWDK,
//! OCR-FR-SAVO, OCR-FR-NGGJ; `GSU-graduation-start.md` GSU-FR-UIDE,
//! GSU-FR-RNOM), with the OpenSpec CLI faked. The helpers here serve the
//! apply and rework scenarios too.
//!
//! OCR-FR-HNXC: every scenario of the change runs runs with no container
//! runtime, no OpenSpec CLI, and no network, through the scripted dispatch
//! seam and the fake process seam of OSM.

use std::sync::Arc;

use super::*;
use crate::openspec::tests::{env, ok, FakeProbe, FakeRunner, CLI};
use crate::openspec::{OpenSpec, ProcessOutput};

pub(super) const CHANGE: &str = "add-logout-3f2a";

pub(super) const VALID: &str = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": true, "issues": [] } ] }"#;

/// `instructions apply` of a change with every task done.
pub(super) const ALL_DONE: &str = r#"{ "changeName": "add-logout-3f2a", "changeDir": "/x", "schemaName": "spec-driven",
  "contextFiles": {}, "progress": { "total": 2, "complete": 2, "remaining": 0 },
  "tasks": [ { "id": "1", "description": "Add the button", "done": true, "sourcePath": "tasks.md", "line": 3 },
             { "id": "2", "description": "Wire the call", "done": true, "sourcePath": "tasks.md", "line": 4 } ],
  "taskTrackingConfigured": true, "state": "all_done", "instruction": "Apply the tasks." }"#;

/// The same change with one task left.
pub(super) const READY: &str = r#"{ "changeName": "add-logout-3f2a", "progress": { "total": 2, "complete": 1, "remaining": 1 },
  "tasks": [ { "id": "1", "description": "Add the button", "done": true },
             { "id": "2", "description": "Wire the call", "done": false } ],
  "state": "ready" }"#;

/// A change that cannot be applied yet.
pub(super) const BLOCKED: &str = r#"{ "changeName": "add-logout-3f2a", "progress": { "total": 0, "complete": 0, "remaining": 0 },
  "tasks": [], "state": "blocked", "missingArtifacts": ["design", "tasks"] }"#;

/// A fixture in OpenSpec mode whose fake CLI answers `instructions` with
/// `progress` and `validate` with a valid change.
pub(super) fn change_fixture(progress: &str) -> (Fixture, Arc<FakeRunner>, Arc<FakeProbe>) {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    let runner = Arc::new(
        FakeRunner::default()
            .json("--version", "1.17.0\n")
            .json("validate", VALID)
            .json("instructions", progress),
    );
    let probe = Arc::new(FakeProbe::default().with_executable(CLI));
    let seam = recorded_seam(&fx, runner.clone(), probe.clone());
    seam.cli_state(&crate::openspec::tests::Silent, crate::openspec::tests::buffer());
    fx.app.manage(seam);
    (fx, runner, probe)
}

/// OFP-FR-MJZU, OFP-FR-ITUD: a seam over `runner` whose wait is recorded and
/// managed, so a test reads the requested delays with [`waits`]. A kept
/// `failed` check stands for an hour, so a test that asserts a clear fails
/// without it, also on a slow runner.
pub(super) fn recorded_seam(
    fx: &Fixture,
    runner: Arc<dyn crate::openspec::OpenSpecRunner>,
    probe: Arc<FakeProbe>,
) -> OpenSpec {
    let wait = Arc::new(crate::openspec::RecordedWait::default());
    fx.app.manage(wait.clone());
    OpenSpec::new(runner, probe, env())
        .with_wait(wait)
        .with_failed_ttl(std::time::Duration::from_secs(3600))
}

/// OFP-FR-MJZU: the delays the seam of [`recorded_seam`] was asked to wait.
pub(super) fn waits(fx: &Fixture) -> Vec<u64> {
    fx.app.state::<Arc<crate::openspec::RecordedWait>>().delays()
}

/// Commit every file of a working copy onto its branch.
pub(super) fn commit_all(worktree: &Path, message: &str) {
    let repo = git2::Repository::open(worktree).unwrap();
    let mut index = repo.index().unwrap();
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let signature = git2::Signature::now("Test Author", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head]).unwrap();
}

/// The revision a working copy's branch stands at.
pub(super) fn head_of(worktree: &Path) -> String {
    git2::Repository::open(worktree)
        .unwrap()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

/// A stream whose branch holds the change, as a propose run leaves it.
pub(super) fn change_stream(fx: &Fixture, name: &str) -> crate::streams::WorkStream {
    let stream = fx.stream(name);
    let folder = stream.worktree().join("openspec/changes").join(CHANGE);
    std::fs::create_dir_all(folder.join("specs/auth")).unwrap();
    std::fs::write(folder.join("proposal.md"), "# Why\n\nLogout is missing.\n").unwrap();
    std::fs::write(folder.join("tasks.md"), "- [ ] Add the button\n- [ ] Wire the call\n").unwrap();
    std::fs::write(folder.join("specs/auth/spec.md"), "## ADDED Requirements\n").unwrap();
    commit_all(&stream.worktree(), "Propose the change");
    stream
}

/// The apply start, with the loop off so the run waits in its queue.
/// The refusal keeps the form the command rejects with (OFP-FR-QLQJ).
pub(super) fn start_apply(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
) -> Result<GraduationRun, crate::openspec::Refusal> {
    crate::graduation::start_change_apply(fx.app.clone(), stream.id.clone(), CHANGE.into())
}

/// The rework start. The refusal keeps the form the command rejects with
/// (OFP-FR-QLQJ).
pub(super) fn start_rework(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    instruction: &str,
) -> Result<crate::graduation::ChangeReworkStart, crate::openspec::Refusal> {
    crate::graduation::start_change_rework(
        fx.app.clone(),
        stream.id.clone(),
        CHANGE.into(),
        instruction.into(),
    )
}

/// OFP-FR-QLQJ: the refusal is one `OpenSpecFailure` object on the wire, with
/// `code`, and not a typed code.
pub(super) fn assert_object(refusal: &crate::openspec::Refusal, code: &str) {
    let failure = refusal.openspec().unwrap_or_else(|| panic!("not an object: {refusal}"));
    assert_eq!(failure.code, code);
    let wire = serde_json::to_value(refusal).unwrap();
    assert!(wire.is_object(), "{wire}");
    assert_eq!(wire["code"], code);
}

/// The request thread of a change, read from the machine store.
pub(super) fn thread(
    fx: &Fixture,
    stream_id: &str,
    thread_id: &str,
) -> crate::comments::Discussion {
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: CHANGE.to_string(),
    };
    crate::comments::locate_discussion_of(&store, &target, thread_id).expect("the thread")
}

/// Resolve or reopen a request thread, as the author does.
pub(super) fn set_resolved(fx: &Fixture, stream_id: &str, thread_id: &str, resolved: bool) {
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let discussion = thread(fx, stream_id, thread_id);
    crate::comments::set_resolution_to(
        &store,
        discussion.log_ref(),
        thread_id,
        resolved,
        &crate::comments::Participant::local_human(),
        &crate::notes::now_rfc3339(),
    )
    .unwrap();
}

fn queue_len(fx: &Fixture) -> usize {
    crate::graduation::queue::project_queue(&fx.app).map(|q| q.runs.len()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The apply start
// ---------------------------------------------------------------------------

// OCR-FR-VHQW, OCR-FR-IDFC, OCR-FR-NIOZ, OCR-FR-RYKA, OCR-FR-VUXZ, OCR-FR-FZVF,
// GSU-FR-RNOM, GRD-FR-KZBS: an apply start enqueues a run with the phase and the change, no
// draft, the fixed prompt and its checksum, `keep`, auto-start, and its title.
#[test]
fn an_apply_start_enqueues_a_draftless_run_of_the_change() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let head = head_of(&fx.root());

    let run = start_apply(&fx, &stream).expect("an apply start");
    let data = run.openspec.as_ref().expect("openspec data");
    assert_eq!((data.phase.as_str(), data.change.as_str()), ("apply", CHANGE));
    assert!(data.instruction.is_none() && data.thread_id.is_none() && data.requester.is_none());
    assert_eq!(run.input.draft_id, "");
    assert_eq!(run.input.draft_name, "");
    let prompt = format!("Apply the OpenSpec change \"{CHANGE}\": implement its tasks.");
    assert_eq!(run.input.prompt, prompt);
    assert_eq!(run.input.prompt_checksum, crate::fs::sha256_bytes(prompt.as_bytes()));
    assert_eq!(run.standing_work, StandingWork::Keep);
    assert!(run.standing_work_message.is_none());
    assert!(run.auto_start);
    assert!(run.direct_target.is_none());
    assert_eq!(run.stream_id, stream.id);
    assert_eq!(run.change_run_title().as_deref(), Some(format!("Apply {CHANGE}").as_str()));
    assert_eq!(
        crate::graduation::run_progress::label_of(&run),
        format!("Apply {CHANGE}…"),
        "GRD-FR-ZHNV: the label names the title"
    );
    let saved = crate::graduation::load_run(&fx.app, &run.id).expect("the saved run");
    assert_eq!(saved.openspec, run.openspec);
    // GSU-FR-RNOM: a run with no draft asks for no draft commit.
    assert_eq!(head_of(&fx.root()), head, "nothing was committed to the active worktree");
}

// OCR-FR-GTOM: an unknown stream, the mode off, a CLI that is not usable, an
// invalid name, and a missing change folder each refuse the start in that
// order, and leave no run.
#[test]
fn an_apply_start_is_refused_in_the_order_of_the_checks() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    let stream = change_stream(&fx, "editor");
    let refused = crate::graduation::start_change_apply(
        fx.app.clone(),
        "no-such-stream".into(),
        "Bad Name".into(),
    );
    assert_eq!(refused.unwrap_err(), "unknown_stream");
    let refused =
        crate::graduation::start_change_apply(fx.app.clone(), stream.id.clone(), "Bad".into());
    assert_eq!(refused.unwrap_err(), "invalid_change_name");
    let refused =
        crate::graduation::start_change_apply(fx.app.clone(), stream.id.clone(), "absent".into());
    assert_eq!(refused.unwrap_err(), "change_not_found");

    // A CLI that is not usable refuses before the name is read.
    let fresh = Fixture::new();
    std::fs::create_dir_all(fresh.root().join("openspec")).unwrap();
    std::fs::write(fresh.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    fresh.app.manage(OpenSpec::new(
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProbe::default()),
        env(),
    ));
    let fresh_stream = fresh.stream("editor");
    let refused = crate::graduation::start_change_apply(
        fresh.app.clone(),
        fresh_stream.id.clone(),
        "Bad".into(),
    );
    // OFP-FR-GPCO, OFP-FR-QLQJ: the one code, with the CLI state.
    let refusal = refused.unwrap_err();
    assert_eq!(refusal, "cli_unavailable: not_found");
    assert_eq!(refusal.openspec().unwrap().detail.as_deref(), Some("not_found"));

    std::fs::remove_file(fresh.root().join("openspec/config.yaml")).unwrap();
    let refused = crate::graduation::start_change_apply(
        fresh.app.clone(),
        fresh_stream.id.clone(),
        "Bad".into(),
    );
    // OFP-FR-QLQJ: an object, and not the typed code of the same text.
    assert_object(&refused.unwrap_err(), "openspec_mode_inactive");
    assert_eq!(queue_len(&fx), 0);
    assert_eq!(queue_len(&fresh), 0);
}

// OFP-FR-QLQJ, OFP-FR-GPCO, OCR-FR-GTOM: a rework start rejects
// `openspec_mode_inactive` and `cli_unavailable` with one `OpenSpecFailure`
// object each, and a refusal that is no OpenSpec failure keeps its typed code.
#[test]
fn a_rework_start_rejects_an_openspec_failure_as_an_object() {
    let (fx, _, probe) = change_fixture(READY);
    fx.allow_execution();
    let stream = change_stream(&fx, "editor");
    let blank = start_rework(&fx, &stream, " ").unwrap_err();
    assert!(blank.openspec().is_none(), "{blank}");
    assert_eq!(serde_json::to_value(&blank).unwrap(), serde_json::json!("blank_instruction"));

    probe.executables.lock().unwrap().clear();
    let refused = start_rework(&fx, &stream, "Drop it.").unwrap_err();
    assert_object(&refused, "cli_unavailable");
    assert_eq!(refused.openspec().unwrap().detail.as_deref(), Some("not_found"));

    std::fs::remove_file(fx.root().join("openspec/config.yaml")).unwrap();
    assert_object(&start_rework(&fx, &stream, "Drop it.").unwrap_err(), "openspec_mode_inactive");
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-LNKQ: a `blocked` change refuses with its missing artifacts, a read
// that fails refuses with its code, and `ready` and `all_done` admit.
#[test]
fn an_apply_start_reads_the_progress_of_the_change() {
    let (fx, runner, _) = change_fixture(BLOCKED);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let refused = start_apply(&fx, &stream).unwrap_err();
    assert_eq!(refused, "change_not_ready: design,tasks");
    assert_object(&refused, "change_not_ready");
    let read = runner.calls().into_iter().find(|c| c.args[0] == "instructions").unwrap();
    assert_eq!(read.args, ["instructions", "apply", "--change", CHANGE, "--json"]);
    assert_eq!(
        crate::changes::canonicalize_lenient(&read.cwd),
        crate::changes::canonicalize_lenient(&stream.worktree()),
        "read in the stream's working copy"
    );

    runner.set(
        "instructions",
        Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new(), stderr: Vec::new() }),
    );
    let refused = start_apply(&fx, &stream).unwrap_err();
    assert_eq!(refused, "apply_progress_unavailable: cli_failed");
    assert_object(&refused, "apply_progress_unavailable");
    assert_eq!(refused.openspec().unwrap().cause.as_deref(), Some("cli_failed"));
    assert_eq!(queue_len(&fx), 0, "a refusal leaves no run");

    runner.set("instructions", Ok(ok(ALL_DONE)));
    let first = start_apply(&fx, &stream).expect("all_done admits the start");
    crate::graduation::discard_graduation_run(fx.app.clone(), first.id).unwrap();
    runner.set("instructions", Ok(ok(READY)));
    start_apply(&fx, &stream).expect("ready admits the start");
}

// OCR-FR-MGPU, GSU-FR-UIDE: a non-terminal OpenSpec run of the change on the
// stream refuses an apply start, a rework start, and a propose start of the
// same name; another stream is not counted.
#[test]
fn an_active_openspec_run_of_the_change_refuses_every_start() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let first = start_apply(&fx, &stream).expect("the first start");
    assert_eq!(
        start_apply(&fx, &stream).unwrap_err(),
        format!("openspec_run_active: {}", first.id)
    );
    assert_eq!(
        start_rework(&fx, &stream, "Tighten the scope.").unwrap_err(),
        format!("openspec_run_active: {}", first.id)
    );
    let propose = crate::graduation::start_graduation(
        fx.app.clone(),
        fx.draft("logout"),
        stream.id.clone(),
        StandingWork::Keep,
        None,
        Some(CHANGE.into()),
    );
    assert_eq!(propose.unwrap_err(), "openspec_change_exists");
    let other = change_stream(&fx, "other");
    start_apply(&fx, &other).expect("another stream is not counted");
}

// OCR-FR-QBIF: a machine that cannot execute an agent refuses the start at the
// image preflight, after the checks of the change.
#[test]
fn an_apply_start_takes_the_image_preflight() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let refused = start_apply(&fx, &stream).unwrap_err();
    assert!(refused.to_string().starts_with("vendor_"), "an image preflight refusal: {refused}");
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-GTOM: with no project open, every start is refused with
// `no_project_open` before any other check, and writes nothing.
#[test]
fn a_start_with_no_project_open_is_refused_first() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    let stream = change_stream(&fx, "editor");
    let store_root = fx.app.state::<GraduationState>().root().expect("the store root");
    fx.app.state::<crate::project::ProjectState>().clear_root();
    let apply =
        crate::graduation::start_change_apply(fx.app.clone(), "no-such".into(), "Bad".into());
    assert_eq!(apply.unwrap_err(), "no_project_open");
    assert_eq!(start_apply(&fx, &stream).unwrap_err(), "no_project_open");
    assert_eq!(start_rework(&fx, &stream, "Drop it.").unwrap_err(), "no_project_open");
    let in_thread = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        "Drop it.",
        "no-such-thread",
        &crate::comments::Participant::synthesis(),
    );
    assert_eq!(in_thread.unwrap_err(), "no_project_open");
    let runs = std::fs::read_dir(store_root.join("g")).map(|d| d.count()).unwrap_or_default();
    assert_eq!(runs, 0, "no run record");
}

/// Point the stream's record at the project's own branch, so the write check
/// of its runs is the main branch check (GTC-FR-JWOL).
fn on_the_main_branch(fx: &Fixture, stream: &crate::streams::WorkStream) {
    let root = fx.app.state::<crate::streams::StreamState>().root().expect("the stream root");
    let record = crate::streams::StreamStore::new(root).record(&stream.id);
    let text = std::fs::read_to_string(&record).expect("the record");
    let mut value: toml::Value = toml::from_str(&text).expect("toml");
    value["branch"] = toml::Value::String(fx.branch());
    std::fs::write(&record, toml::to_string(&value).unwrap()).expect("the record");
    fx.app.manage(crate::git::write_guard::commands::WriteGuardState::offline());
}

// OCR-FR-QBIF / GRD-FR-WJLO: the write check of a change run is the main
// branch check of its stream branch: an unconfirmed main branch refuses the
// start with `main_branch_write_unconfirmed` and leaves no run.
#[test]
fn a_change_run_start_takes_the_write_check_of_its_branch() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    on_the_main_branch(&fx, &stream);
    let refused = start_apply(&fx, &stream).unwrap_err();
    assert!(refused.code() == crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED, "{refused}");
    let refused = start_rework(&fx, &stream, "Drop it.").unwrap_err();
    assert!(refused.code() == crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED, "{refused}");
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-GTOM / OCR-FR-QBIF: the image preflight comes before the write
// check, and the checks of the phase before the image preflight.
#[test]
fn the_checks_of_the_phase_and_the_image_come_before_the_write_check() {
    let (fx, runner, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    on_the_main_branch(&fx, &stream);
    // No agent can execute and the branch is not confirmed: the image wins.
    let refused = start_apply(&fx, &stream).unwrap_err();
    assert!(refused.to_string().starts_with("vendor_"), "the image preflight first: {refused}");
    // A change that is not ready wins over both.
    runner.set("instructions", Ok(ok(BLOCKED)));
    assert_eq!(start_apply(&fx, &stream).unwrap_err(), "change_not_ready: design,tasks");
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-MGPU: an active propose run and an active rework run of the change
// on the stream each refuse an apply start, with the id of that run.
#[test]
fn an_apply_start_is_refused_by_a_propose_run_or_a_rework_run_of_the_change() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let mut propose = fx.enqueue(&stream, "Add a logout button.");
    propose.openspec = Some(crate::graduation::openspec_start::propose_data(CHANGE));
    crate::graduation::save_run(&fx.app, &mut propose).unwrap();
    assert_eq!(
        start_apply(&fx, &stream).unwrap_err(),
        format!("openspec_run_active: {}", propose.id)
    );
    crate::graduation::discard_graduation_run(fx.app.clone(), propose.id.clone()).unwrap();

    let rework = start_rework(&fx, &stream, "Drop the second task.").expect("a rework start");
    assert_eq!(
        start_apply(&fx, &stream).unwrap_err(),
        format!("openspec_run_active: {}", rework.run.id)
    );
}

// ---------------------------------------------------------------------------
// The rework start
// ---------------------------------------------------------------------------

// OCR-FR-TBGA, OCR-FR-UAWO, OCR-FR-FZVF: a rework start opens a whole-change
// discussion whose first comment is the instruction in the author's name, and
// records the instruction, the thread, and the Synthesis participant.
#[test]
fn a_rework_start_opens_its_request_thread_and_records_the_requester() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");

    let started = start_rework(&fx, &stream, "Drop the second task.").expect("a rework start");
    let data = started.run.openspec.as_ref().expect("openspec data");
    assert_eq!(data.phase, "rework");
    assert_eq!(data.instruction.as_deref(), Some("Drop the second task."));
    assert_eq!(data.thread_id.as_deref(), Some(started.thread_id.as_str()));
    assert_eq!(data.requester, Some(crate::comments::Participant::synthesis()));
    assert_eq!(
        started.run.input.prompt,
        format!("Rework the OpenSpec change \"{CHANGE}\" on the instruction of the author.")
    );
    assert_eq!(started.run.change_run_title(), Some(format!("Rework {CHANGE}")));

    let discussion = thread(&fx, &stream.id, &started.thread_id);
    assert!(!discussion.is_fragment_targeted(), "a whole-change discussion");
    assert_eq!(discussion.comments.len(), 1);
    assert_eq!(discussion.comments[0].body, "Drop the second task.");
    assert!(matches!(discussion.comments[0].author, crate::comments::Participant::Human { .. }));

    // TOML keeps the requester whole.
    let saved = crate::graduation::load_run(&fx.app, &started.run.id).unwrap();
    assert_eq!(saved.openspec, started.run.openspec);
}

// OCR-FR-TCFQ, OCR-FR-SFDE: a blank instruction and a pending proposal of the
// change each refuse the start, and no discussion is opened for either.
#[test]
fn a_rework_start_is_refused_on_a_blank_instruction_or_a_pending_proposal() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let human = crate::comments::Participant::local_human();
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: CHANGE.into(),
    };
    assert_eq!(start_rework(&fx, &stream, "  \n ").unwrap_err(), "blank_instruction");
    assert_eq!(
        crate::comments::list_discussions_in(&store, &target).len(),
        0,
        "a blank instruction opens no discussion"
    );
    assert_eq!(queue_len(&fx), 0);

    let opened = crate::comments::open_discussion_unchecked(
        &store,
        &target,
        None,
        "Look at the tasks.".into(),
        Vec::new(),
        &human,
        &crate::notes::now_rfc3339(),
    )
    .unwrap();
    let agent = crate::comments::Participant::synthesis();
    crate::change_proposals::record_change_proposal(
        &fx.app,
        &store,
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id: &stream.id,
            change: CHANGE,
            hunks: &[crate::change_proposals::PathedHunk {
                path: "tasks.md".into(),
                hunk: crate::text_hunks::hunks::ProposedHunk::from_fields(
                    Some("- [ ] Wire the call\n".into()),
                    Some("- [ ] Wire the logout call\n".into()),
                    None,
                    None,
                    None,
                    None,
                ),
            }],
            rationale: "Name the call.",
            agent: &agent,
            thread_id: &opened.id,
        },
    )
    .expect("a pending proposal");
    let before = crate::comments::list_discussions_in(&store, &target).len();
    assert_eq!(
        start_rework(&fx, &stream, "Drop the second task.").unwrap_err(),
        "proposal_pending"
    );
    assert_eq!(crate::comments::list_discussions_in(&store, &target).len(), before);
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-TWDK, OCR-FR-TCFQ: a start in an existing thread opens no
// discussion, records the requester it is given, and is refused where the
// thread is resolved.
#[test]
fn a_rework_start_in_a_thread_opens_nothing_and_reads_the_thread() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let human = crate::comments::Participant::local_human();
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: CHANGE.into(),
    };
    let opened = crate::comments::open_discussion_unchecked(
        &store,
        &target,
        None,
        "Can you rework it?".into(),
        Vec::new(),
        &human,
        &crate::notes::now_rfc3339(),
    )
    .unwrap();
    let agent = crate::comments::Participant::Agent {
        agent_id: "a1".into(),
        handle: "Ada".into(),
        model: None,
        title: Some(String::new()),
    };

    set_resolved(&fx, &stream.id, &opened.id, true);
    let refused = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        "Drop the second task.",
        &opened.id,
        &agent,
    );
    assert_eq!(refused.unwrap_err(), "discussion_resolved");

    set_resolved(&fx, &stream.id, &opened.id, false);
    let run = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        "Drop the second task.",
        &opened.id,
        &agent,
    )
    .expect("a start in the thread");
    let data = run.openspec.as_ref().unwrap();
    assert_eq!(data.thread_id.as_deref(), Some(opened.id.as_str()));
    assert_eq!(data.requester.as_ref(), Some(&agent));
    assert_eq!(crate::comments::list_discussions_in(&store, &target).len(), 1);
    assert_eq!(thread(&fx, &stream.id, &opened.id).comments.len(), 1, "no comment was added");
}

// ---------------------------------------------------------------------------
// The restart
// ---------------------------------------------------------------------------

// OCR-FR-SAVO, OCR-FR-NGGJ, GRD-FR-ZAMI: a discarded change run restarts as a
// run of the same phase, change, and stream over its captured input; the
// stream and the standing-work arguments are not read; the discarded run is
// not counted.
#[test]
fn a_discarded_change_run_restarts_on_its_own_stream() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let other = fx.stream("other");
    let started = start_rework(&fx, &stream, "Drop the second task.").unwrap();
    crate::graduation::discard_graduation_run(fx.app.clone(), started.run.id.clone()).unwrap();

    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        started.run.id.clone(),
        other.id.clone(),
        StandingWork::CommitAndPush,
        Some("ignored".into()),
    )
    .expect("a restart");
    assert_eq!(restarted.stream_id, stream.id);
    assert_eq!(restarted.standing_work, StandingWork::Keep);
    assert!(restarted.standing_work_message.is_none());
    assert_eq!(restarted.input, started.run.input);
    assert_eq!(restarted.openspec, started.run.openspec);
    assert_eq!(restarted.restarted_from_run_id.as_deref(), Some(started.run.id.as_str()));
    assert_eq!(
        crate::comments::list_discussions_in(
            &fx.app.state::<crate::project::ProjectState>().require_store().unwrap(),
            &crate::comments::DiscussionTarget::Change {
                stream_id: stream.id.clone(),
                change: CHANGE.into()
            }
        )
        .len(),
        1,
        "a restart opens no discussion"
    );

    // OCR-FR-NGGJ: the thread is read afresh.
    crate::graduation::discard_graduation_run(fx.app.clone(), restarted.id.clone()).unwrap();
    set_resolved(&fx, &stream.id, &started.thread_id, true);
    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        restarted.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    );
    assert_eq!(refused.unwrap_err(), "discussion_resolved");
}

// OCR-FR-NGGJ: the checks are taken afresh at the restart.
#[test]
fn a_restart_takes_the_checks_again() {
    let (fx, runner, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let first = start_apply(&fx, &stream).unwrap();
    crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone()).unwrap();
    runner.set("instructions", Ok(ok(BLOCKED)));
    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        first.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    );
    assert_eq!(refused.unwrap_err(), "change_not_ready: design,tasks");
    runner.set("instructions", Ok(ok(READY)));
    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        first.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("a restart");
    assert!(restarted.is_change_apply());
}

// OCR-FR-QDOZ: a start is an `INFO` record with the run id, the stream id, the
// change, and the phase, and a refusal a `WARN` record with the code, both
// under the `backend` domain; no record holds the instruction.
#[test]
fn starts_and_refusals_log_ids_and_codes_alone() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let secret = "Drop the second task, the one about the zebra.";
    let started = start_rework(&fx, &stream, secret).expect("a start");
    let _ = start_rework(&fx, &stream, secret);
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
    let mine: Vec<_> = records
        .iter()
        .filter(|r| serde_json::to_string(&r.fields).unwrap_or_default().contains(&stream.id))
        .collect();
    let enqueued = mine
        .iter()
        .find(|r| r.message == "graduation enqueued an openspec change run")
        .expect("the start record");
    assert_eq!(enqueued.level, crate::logging::LogLevel::Info);
    assert!(enqueued.domains.contains(&crate::logging::Domain::Backend));
    let field = |r: &crate::logging::LogRecord, key: &str| r.fields.get(key).cloned();
    assert_eq!(field(enqueued, "run_id"), Some(serde_json::json!(started.run.id)));
    assert_eq!(field(enqueued, "stream_id"), Some(serde_json::json!(stream.id)));
    assert_eq!(field(enqueued, "change"), Some(serde_json::json!(CHANGE)));
    assert_eq!(field(enqueued, "phase"), Some(serde_json::json!("rework")));
    let refused = mine
        .iter()
        .find(|r| r.message == "graduation refused an openspec change run start")
        .expect("the refusal record");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert!(refused.domains.contains(&crate::logging::Domain::Backend));
    assert_eq!(field(refused, "code"), Some(serde_json::json!("openspec_run_active")));
    assert_eq!(field(refused, "phase"), Some(serde_json::json!("rework")));
    for record in &records {
        let text = format!("{} {}", record.message, serde_json::to_string(&record.fields).unwrap());
        assert!(!text.contains("zebra"), "a record holds the instruction: {text}");
    }
}

// OSM-FR-PEAK, OSM-FR-MFVN: `"read apply progress"` reads the change in the
// stream's working copy and returns the progress; an unknown stream fails
// with `unknown_stream` and starts no read.
#[test]
fn the_apply_progress_command_reads_a_stream_change() {
    let (fx, runner, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let read = |stream_id: &str| {
        tauri::async_runtime::block_on(crate::openspec::apply::read_apply_progress(
            fx.app.clone(),
            stream_id.to_string(),
            CHANGE.to_string(),
        ))
    };
    assert_eq!(read("no-such-stream").unwrap_err().code, "unknown_stream");
    assert_eq!(runner.calls_of("instructions"), 0);

    let progress = read(&stream.id).expect("the progress");
    assert_eq!(progress.state, "ready");
    assert_eq!((progress.total, progress.complete), (2, 1));
    assert_eq!(progress.next_task.as_deref(), Some("Wire the call"));
    let call = runner.calls().into_iter().find(|c| c.args[0] == "instructions").unwrap();
    assert_eq!(
        crate::changes::canonicalize_lenient(&call.cwd),
        crate::changes::canonicalize_lenient(&stream.worktree())
    );

    runner.set(
        "instructions",
        Ok(ok(r#"{ "state": "odd", "progress": { "total": 0, "complete": 0 } }"#)),
    );
    assert_eq!(read(&stream.id).unwrap_err().code, "cli_output_unreadable");
}
