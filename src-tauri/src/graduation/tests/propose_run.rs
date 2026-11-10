//! The propose run (`GSU-graduation-start.md` GSU-FR-MIVF through
//! GSU-FR-QSNE, `GRD-graduation.md` GRD-FR-LKGC, GRD-FR-SYHE, GRD-FR-UJII,
//! `../ai/GRL-graduation-loop.md` GRL-FR-FYKL through GRL-FR-MXXA), driven
//! with the agent scripted and the OpenSpec CLI faked.

use std::sync::Arc;

use super::*;
use crate::openspec::tests::{env, ok, FakeProbe, FakeRunner, CLI};
use crate::openspec::{OpenSpec, ProcessOutput};

const CHANGE: &str = "add-logout-3f2a";

const VALID: &str = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": true, "issues": [] } ] }"#;
const INVALID: &str = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": false,
  "issues": [ { "level": "ERROR", "path": "specs/auth/spec.md", "message": "No scenario" } ] } ] }"#;

fn invalid() -> Result<ProcessOutput, crate::openspec::ProcessError> {
    Ok(ProcessOutput { exit_status: Some(1), stdout: INVALID.as_bytes().to_vec() })
}

/// The fixture in OpenSpec mode, with a seam whose CLI is found and whose
/// check is kept, and `validate` answering `validation`.
fn openspec_fixture(
    validation: Result<ProcessOutput, crate::openspec::ProcessError>,
) -> (Fixture, Arc<FakeRunner>) {
    let (fx, runner, _) = openspec_fixture_probed(validation);
    (fx, runner)
}

/// The same, with the probe, so a test can take the CLI away.
fn openspec_fixture_probed(
    validation: Result<ProcessOutput, crate::openspec::ProcessError>,
) -> (Fixture, Arc<FakeRunner>, Arc<FakeProbe>) {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    let runner = Arc::new(
        FakeRunner::default().json("--version", "1.17.0\n").answering("validate", validation),
    );
    let probe = Arc::new(FakeProbe::default().with_executable(CLI));
    let seam = OpenSpec::new(runner.clone(), probe.clone(), env());
    seam.cli_state(&crate::openspec::tests::Silent, crate::openspec::tests::buffer());
    fx.app.manage(seam);
    (fx, runner, probe)
}

fn propose_run(fx: &Fixture, stream: &crate::streams::WorkStream) -> GraduationRun {
    let mut run = fx.enqueue(stream, "Add a logout button.");
    run.openspec = Some(crate::graduation::openspec_start::propose_data(CHANGE));
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    run
}

fn start(
    fx: &Fixture,
    draft: &str,
    stream: &crate::streams::WorkStream,
    change: Option<&str>,
) -> Result<GraduationRun, String> {
    crate::graduation::start_graduation(
        fx.app.clone(),
        draft.to_string(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
        change.map(str::to_string),
    )
}

// ---------------------------------------------------------------------------
// The start (GSU)
// ---------------------------------------------------------------------------

// GSU-FR-MIVF, GRD-FR-LKGC: a start that names a change makes a propose run
// that records its phase and change; one that names none makes a draft run.
#[test]
fn a_start_with_a_change_name_makes_a_propose_run() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    fx.allow_execution();
    let stream = fx.stream("editor");
    fx.hold_stream(&stream.id);
    let run = start(&fx, &fx.draft("logout"), &stream, Some(CHANGE)).expect("a propose start");
    let data = run.openspec.as_ref().expect("openspec data");
    assert_eq!((data.phase.as_str(), data.change.as_str()), ("propose", CHANGE));
    let saved = crate::graduation::load_run(&fx.app, &run.id).expect("the saved run");
    assert_eq!(saved.openspec, run.openspec);

    let plain = start(&fx, &fx.draft("other"), &stream, None).expect("a draft start");
    assert!(plain.openspec.is_none());
}

// GSU-FR-MTDA, GSU-FR-CZEP: a project not in OpenSpec mode, a CLI that is not
// usable, and an invalid name each refuse the start, before the image
// preflight, and enqueue nothing.
#[test]
fn a_propose_start_is_refused_without_the_mode_the_cli_or_a_valid_name() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let draft = fx.draft("logout");
    fx.app.manage(OpenSpec::new(
        Arc::new(FakeRunner::default().json("--version", "1.17.0")),
        Arc::new(FakeProbe::default().with_executable(CLI)),
        env(),
    ));
    // No OpenSpec in the project, and no image configured: the mode refuses first.
    assert_eq!(start(&fx, &draft, &stream, Some(CHANGE)).unwrap_err(), "openspec_mode_inactive");

    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let draft = fx.draft("logout");
    assert_eq!(start(&fx, &draft, &stream, Some("Add Logout")).unwrap_err(), "invalid_change_name");
    assert_eq!(start(&fx, &draft, &stream, Some("archive")).unwrap_err(), "invalid_change_name");

    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    fx.app.manage(OpenSpec::new(
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProbe::default()),
        env(),
    ));
    let stream = fx.stream("editor");
    let draft = fx.draft("logout");
    assert_eq!(start(&fx, &draft, &stream, Some(CHANGE)).unwrap_err(), "openspec_cli_unavailable");
    assert!(crate::graduation::queue::project_queue(&fx.app).unwrap().runs.is_empty());
}

// GSU-FR-UIDE: a change folder that stands in the stream, and a non-terminal
// propose run of the same stream and change, each refuse the start.
#[test]
fn a_propose_start_is_refused_for_a_change_that_exists() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    fx.allow_execution();
    let stream = fx.stream("editor");
    fx.hold_stream(&stream.id);
    std::fs::create_dir_all(stream.worktree().join("openspec/changes/taken")).unwrap();
    assert_eq!(
        start(&fx, &fx.draft("a"), &stream, Some("taken")).unwrap_err(),
        "openspec_change_exists"
    );

    fx.app.state::<GraduationState>().set_loop_enabled(false);
    start(&fx, &fx.draft("b"), &stream, Some(CHANGE)).expect("the first propose start");
    assert_eq!(
        start(&fx, &fx.draft("c"), &stream, Some(CHANGE)).unwrap_err(),
        "openspec_change_exists"
    );
    let other = fx.stream("other");
    start(&fx, &fx.draft("d"), &other, Some(CHANGE))
        .expect("another stream may propose the same name");
}

// GSU-FR-ZYXC: a direct start takes no change name and makes no propose run.
#[test]
fn a_direct_start_makes_no_propose_run() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let draft = fx.committed_draft("direct");
    let run = fx.start_direct(&draft).expect("a direct start");
    assert!(run.openspec.is_none());
}

// ---------------------------------------------------------------------------
// The loop (GRL)
// ---------------------------------------------------------------------------

// GRL-FR-DXLU, GRL-FR-FYKL, GRL-FR-GWDB, GRL-FR-APHV, GRL-FR-BFPT: the work
// turn carries `work.md` and then the work block with the change name; the
// review turn carries `review.md` and then the review block.
#[test]
fn the_turns_of_a_propose_run_carry_their_blocks() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let work = &dispatch.of_part("work")[0].instruction;
    let standing = crate::graduation::driver::prompts::WORK_PROMPT.trim_end();
    assert!(work.starts_with(standing), "the standing instruction comes first");
    assert!(work.contains(&format!("openspec/changes/{CHANGE}/")));
    assert!(work.contains(&format!("openspec instructions <artifact> --change {CHANGE} --json")));
    assert!(work.contains("Change no file outside"));
    assert!(work.contains(&format!("openspec validate {CHANGE} --strict")));
    assert!(!work.contains("{{ change-name }}"));
    assert!(!work.contains("judge the change"), "the review block is not the work block");

    let review = &dispatch.of_part("review")[0].instruction;
    assert!(review.starts_with(crate::graduation::driver::prompts::REVIEW_PROMPT.trim_end()));
    assert!(review.contains("Judge the change against the prompt"));
    assert!(review.contains("is a finding"));
    assert!(review.contains("input.openspec_validation"));
}

// GRL-FR-DXLU, GRL-FR-DAIB: a draft run's turns carry the standing
// instructions alone.
#[test]
fn a_draft_run_carries_no_propose_block() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add a logout button.");
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());
    assert_eq!(
        dispatch.of_part("work")[0].instruction,
        *crate::graduation::driver::prompts::WORK_PROMPT
    );
    assert!(dispatch.of_part("review")[0].input.get("openspec_validation").is_none());
}

// GRL-FR-MXXA: every turn of a propose run names its change, and the input
// version stays 1.
#[test]
fn every_turn_of_a_propose_run_names_its_change() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());
    for seen in dispatch.seen() {
        assert_eq!(
            seen.input["openspec"],
            serde_json::json!({ "phase": "propose", "change": CHANGE })
        );
        assert_eq!(seen.input["graduation_input_version"], 1);
    }
}

// GRL-FR-CDNI, GRD-FR-SYHE, DRS-FR-VHMB: the review reads the validation of
// its own checkout; a ready verdict on a valid change commits, the draft
// graduates, and its link names the change and the stream.
#[test]
fn a_valid_change_is_reviewed_with_its_validation_and_completes() {
    let (fx, runner) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing(&format!("openspec/changes/{CHANGE}/proposal.md"), "# Why\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);

    let review = &dispatch.of_part("review")[0];
    assert_eq!(
        review.input["openspec_validation"],
        serde_json::json!({ "valid": true, "issues": [] })
    );
    let validate: Vec<_> = runner.calls().into_iter().filter(|c| c.args[0] == "validate").collect();
    assert_eq!(validate.len(), 1);
    assert_eq!(validate[0].cwd, review.directory, "validated in the review checkout");
    assert_ne!(validate[0].cwd, stream.worktree());

    let queue = crate::graduation::queue::project_queue(&fx.app).unwrap();
    let link = crate::graduation::queue::draft_graduation(&queue, "d1").expect("a link");
    assert!(link.graduated);
    assert_eq!(
        link.openspec_change,
        Some(crate::graduation::DraftOpenSpecChange {
            name: CHANGE.into(),
            stream_id: stream.id.clone()
        })
    );
}

// GRL-FR-HSZD: a ready verdict on a change that does not validate becomes a
// revise with one critical finding, and the loop goes on as for any revise.
#[test]
fn a_ready_on_an_invalid_change_starts_another_pass() {
    let (fx, runner) = openspec_fixture(invalid());
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::work(), Turn::ready(), Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::AwaitingAuthor, "the pass budget of two is spent");
    assert!(run.commits.is_empty(), "nothing is committed from an invalid change");
    let parts: Vec<String> = dispatch.seen().into_iter().map(|s| s.part).collect();
    assert_eq!(parts, ["work", "review", "work", "review"]);
    // GRL-FR-CDNI: one validation before each review turn.
    assert_eq!(runner.calls_of("validate"), 2);
    let second_work = &dispatch.of_part("work")[1];
    let instruction = second_work.instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains(CHANGE), "the critical finding names the change: {instruction}");
}

// GRL-FR-KDYL, GRD-FR-UJII: a validation that cannot run dispatches no review
// turn and blocks the run with its code; Continue validates and reviews again.
#[test]
fn a_validation_that_cannot_run_blocks_the_run_until_continue() {
    let (fx, runner) =
        openspec_fixture(Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new() }));
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(run.blocker.as_ref().unwrap().code, "openspec_validation_unavailable");
    assert!(dispatch.of_part("review").is_empty(), "no review turn is dispatched");

    runner.set("validate", Ok(ok(VALID)));
    let resumed = fx.continue_run(&run);
    let resumed = fx.drive(&resumed, dispatch.clone());
    assert_eq!(resumed.state, GraduationRunState::Completed);
    assert_eq!(dispatch.of_part("review").len(), 1);
}

// GRL-FR-HSZD, GRL-FR-UQNV: a revise of minor findings, which would end the
// loop as `ready`, does not end it on a change that does not validate.
#[test]
fn an_advisory_revise_on_an_invalid_change_does_not_complete() {
    let (fx, _) = openspec_fixture(invalid());
    let stream = fx.stream("editor");
    let run = propose_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work(),
        Turn::revise(ReviewSeverity::Minor, "A small wording point."),
        Turn::work(),
        Turn::revise(ReviewSeverity::Minor, "A small wording point."),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(run.commits.is_empty());
    assert_eq!(dispatch.of_part("review").len(), 2);
}

// GRL-FR-KDYL, GRD-FR-UJII: OpenSpec mode that is no longer active, and a CLI
// that is no longer usable, each block the run before any review turn; the
// blocked run keeps its queue and its project slot.
#[test]
fn a_validation_without_the_mode_or_the_cli_blocks_and_keeps_the_slot() {
    for take_away in ["mode", "cli"] {
        let (fx, _, probe) = openspec_fixture_probed(Ok(ok(VALID)));
        let stream = fx.stream("editor");
        let run = propose_run(&fx, &stream);
        if take_away == "mode" {
            std::fs::remove_file(fx.root().join("openspec/config.yaml")).unwrap();
        } else {
            probe.executables.lock().unwrap().clear();
        }
        let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
        let run = fx.drive(&run, dispatch.clone());
        assert_eq!(run.state, GraduationRunState::Blocked, "{take_away}");
        assert_eq!(run.blocker.as_ref().unwrap().code, "openspec_validation_unavailable");
        assert!(dispatch.of_part("review").is_empty(), "{take_away}");

        let queue = crate::graduation::queue::project_queue(&fx.app).unwrap();
        assert!(queue.runs.iter().any(|r| r.id == run.id && !r.state.is_terminal()));
        let capacity = crate::graduation::get_graduation_capacity(fx.app.clone()).unwrap();
        assert_eq!(capacity.in_use, 1, "{take_away}: the blocked run keeps its project slot");
    }
}

// GSU-FR-QSNE: a restart of a propose run is a propose run of the same change,
// with its own folder standing on the stream; with the mode off it is refused.
#[test]
fn a_restart_of_a_propose_run_keeps_its_change() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = fx.stream("editor");
    let first = start(&fx, &fx.draft("logout"), &stream, Some(CHANGE)).expect("a propose start");
    crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone()).expect("a discard");
    std::fs::create_dir_all(stream.worktree().join("openspec/changes").join(CHANGE)).unwrap();

    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        first.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("the folder of the run's own work refuses no restart");
    assert_eq!(restarted.openspec, first.openspec);
    assert_eq!(restarted.restarted_from_run_id.as_deref(), Some(first.id.as_str()));

    crate::graduation::discard_graduation_run(fx.app.clone(), restarted.id.clone())
        .expect("a discard");
    std::fs::remove_file(fx.root().join("openspec/config.yaml")).unwrap();
    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        restarted.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    );
    assert_eq!(refused.unwrap_err(), "openspec_mode_inactive");
}

// GSU-FR-UIDE: a terminal propose run of the same name refuses no start.
#[test]
fn a_terminal_propose_run_of_the_same_name_refuses_no_start() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = fx.stream("editor");
    for state in [GraduationRunState::Completed, GraduationRunState::Discarded] {
        let mut old = propose_run(&fx, &stream);
        old.state = state;
        crate::graduation::save_run(&fx.app, &mut old).unwrap();
    }
    start(&fx, &fx.draft("logout"), &stream, Some(CHANGE))
        .expect("terminal runs hold no change name");
}

// GSU-FR-MTDA: the stream check comes first, so an unknown stream is refused as
// such while the mode is off.
#[test]
fn an_unknown_stream_is_refused_before_the_mode() {
    let fx = Fixture::new();
    let draft = fx.draft("logout");
    let refused = crate::graduation::start_graduation(
        fx.app.clone(),
        draft,
        "no-such-stream".into(),
        StandingWork::Keep,
        None,
        Some(CHANGE.into()),
    );
    assert_eq!(refused.unwrap_err(), crate::graduation::ERR_UNKNOWN_STREAM);
}

// DRS-FR-VHMB, GRD-FR-SYHE: the link from a draft to its change is read from
// the run, `get_draft_graduation` returns it, and `draft.toml` never holds it.
#[test]
fn the_draft_link_is_read_from_the_run_and_never_stored_in_the_draft() {
    let (fx, _) = openspec_fixture(Ok(ok(VALID)));
    let stream = fx.stream("editor");
    let draft = fx.draft("logout");
    let mut run = fx.enqueue_for(&stream, "Add a logout button.", &draft);
    run.openspec = Some(crate::graduation::openspec_start::propose_data(CHANGE));
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing(&format!("openspec/changes/{CHANGE}/proposal.md"), "# Why\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Completed);

    let read =
        crate::graduation::get_draft_graduation(fx.app.clone(), draft.clone()).unwrap().unwrap();
    assert_eq!(read.openspec, run.openspec);
    let mut checked = 0;
    let mut stack = vec![fx.root()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().is_some_and(|n| n != ".git") {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "draft.toml") {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(!text.contains("openspec"), "draft.toml holds the link: {text}");
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "the draft record was found and read");
}

// OSM-FR-MFVN, OSM-FR-MBTI: a stream that is not live fails with
// `unknown_stream` and starts no process; `read_openspec_change` reads a change
// in a stream's working copy.
#[test]
fn a_change_is_read_on_a_live_stream_and_an_unknown_stream_is_refused() {
    let (fx, runner) = openspec_fixture(Ok(ok(VALID)));
    let location = crate::openspec::ChangeLocation::Stream { stream_id: "no-such".into() };
    assert_eq!(
        crate::openspec::change::working_copy(&fx.app.clone(), &location).unwrap_err(),
        crate::openspec::OpenSpecReadError::UnknownStream
    );
    let refused = tauri::async_runtime::block_on(crate::openspec::change::read_openspec_change(
        fx.app.clone(),
        location,
        CHANGE.into(),
    ));
    assert_eq!(refused.unwrap_err(), "unknown_stream");
    assert!(runner.calls().iter().all(|c| c.args[0] == "--version"), "no read process for it");

    let stream = fx.stream("editor");
    std::fs::create_dir_all(stream.worktree().join("openspec/changes").join(CHANGE)).unwrap();
    runner.set(
        "status",
        Ok(ok(r#"{ "schemaName": "spec-driven", "isComplete": false, "artifacts": [] }"#)),
    );
    runner.set("show", Ok(ok(r#"{ "deltas": [] }"#)));
    runner.set("list", Ok(ok(r#"{ "changes": [] }"#)));
    let location = crate::openspec::ChangeLocation::Stream { stream_id: stream.id.clone() };
    let detail = tauri::async_runtime::block_on(crate::openspec::change::read_openspec_change(
        fx.app.clone(),
        location.clone(),
        CHANGE.into(),
    ))
    .expect("a change on a live stream");
    assert_eq!(detail.location, location);
    assert_eq!(detail.schema, "spec-driven");
    let status = runner.calls().into_iter().find(|c| c.args[0] == "status").unwrap();
    assert_eq!(
        crate::changes::canonicalize_lenient(&status.cwd),
        crate::changes::canonicalize_lenient(&stream.worktree())
    );
}
