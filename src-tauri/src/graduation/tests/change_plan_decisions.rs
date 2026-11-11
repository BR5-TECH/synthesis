//! The decisions of a plan proposal and the plan bases Continue makes of them
//! (`OPU-openspec-plan-updates.md` OPU-FR-THOZ, OPU-FR-PXDY, OPU-FR-ZNRR,
//! OPU-FR-SKTV, OPU-FR-EVBZ, OPU-FR-DEEB, OPU-FR-YNXV, OPU-FR-FSCV,
//! OPU-FR-YTKN; `CEP-change-edit-proposals.md` CEP-FR-KGOL, CEP-FR-YYZZ),
//! driven with the agent scripted and the OpenSpec CLI faked.

use super::change_plan_continue::*;
use super::change_plan_updates::*;
use super::change_runs::*;
use super::*;
use crate::change_proposals::RecordRefusal;
use crate::graduation::plan_updates::{self as plan};

/// The run's whole record, to show that a refusal changed nothing.
fn snapshot(fx: &Fixture, run_id: &str) -> serde_json::Value {
    serde_json::to_value(fx.reload(run_id)).unwrap()
}

/// An apply run at rest on a plan update with edits alone, which proposes
/// `design.md`.
fn rested_on_edits(fx: &Fixture, stream: &crate::streams::WorkStream) -> GraduationRun {
    let run = apply_run(fx, stream);
    let update = plan_update(Vec::new())
        .writing("src/logout.ts", "export const logout = () => {};\n")
        .writing(&plan_file("design.md"), "# Design\n");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![update]));
    assert_eq!(rested.rest.as_ref().map(|r| r.reason.as_str()), Some(plan::REST_PLAN_UPDATE));
    rested
}

fn text_at(repo: &git2::Repository, tree: &git2::Tree<'_>, path: &str) -> Option<String> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    let blob = entry.to_object(repo).unwrap().peel_to_blob().unwrap();
    Some(String::from_utf8(blob.content().to_vec()).unwrap())
}

fn commit_paths(repo: &git2::Repository, commit: &git2::Commit<'_>) -> Vec<String> {
    let parent = commit.parent(0).unwrap().tree().unwrap();
    let diff = repo.diff_tree_to_tree(Some(&parent), Some(&commit.tree().unwrap()), None).unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
        .collect();
    paths.sort();
    paths
}

/// A recording of a `design.md` proposal on the change, outside any test
/// fixture, so that a turn can make it while it runs.
fn record_design<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    thread_id: &str,
) -> Result<crate::change_proposals::ChangeEditProposal, RecordRefusal> {
    let store = app.state::<crate::project::ProjectState>().require_store().unwrap();
    let hunk = crate::change_proposals::PathedHunk {
        path: "design.md".into(),
        hunk: crate::graduation::driver::hunks_of(None, "# Design\n").remove(0),
    };
    crate::change_proposals::record_change_proposal(
        app,
        &store,
        &crate::change_proposals::AppSites { app },
        crate::change_proposals::NewChangeProposal {
            stream_id,
            change: CHANGE,
            hunks: std::slice::from_ref(&hunk),
            rationale: "The design.",
            agent: &crate::comments::Participant::synthesis(),
            thread_id,
        },
    )
}

// OPU-FR-YNXV, OPU-FR-FSCV, OPU-FR-NQBK: a second plan update of the same
// run moves the private ref to a newer plan base. Its one parent is the first
// plan base, its tree holds the accepted files of both plan proposals and no
// work of the run, and the change set and the run's `ready` commit hold the
// run's own files alone.
#[test]
fn a_second_plan_update_moves_the_ref_to_a_newer_plan_base() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let base = head_of(&stream.worktree());
    let rested = rested_on_edits(&fx, &stream);
    let first = rested.rest.clone().unwrap().proposal_id.unwrap();
    accept(&fx, &first, &hunk_id(&fx, &first, "design.md"), None).expect("accepted");
    let continued = continue_of(&fx, &rested).expect("continued");
    let first_base = continued.base_commit.clone().unwrap();

    let reason = "# Why\n\nLogout and the table are missing.\n";
    let again = plan_update(Vec::new())
        .writing("src/wire.ts", "export const wire = 1;\n")
        .writing(&plan_file("proposal.md"), reason);
    let rested = fx.drive(&continued, ScriptedDispatch::new(vec![again]));
    let rest = rested.rest.clone().expect("the second rest");
    let second = rest.proposal_id.clone().unwrap();
    assert_ne!(second, first);
    accept(&fx, &second, &hunk_id(&fx, &second, "proposal.md"), None).expect("accepted");
    let close = plan_proposal(&fx, &rest).commit.expect("the second close commit");
    let continued = continue_of(&fx, &rested).expect("continued again");
    let newer = continued.base_commit.clone().unwrap();
    assert_ne!(newer, first_base);

    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let made = repo.find_commit(git2::Oid::from_str(&newer).unwrap()).unwrap();
    assert_eq!(made.parent_count(), 1);
    assert_eq!(made.parent_id(0).unwrap().to_string(), first_base);
    let first_commit = repo.find_commit(git2::Oid::from_str(&first_base).unwrap()).unwrap();
    assert_eq!(first_commit.parent_id(0).unwrap().to_string(), base);
    assert_eq!(repo.refname_to_id(&plan::plan_base::ref_name(&rested.id)).unwrap(), made.id());
    let tree = made.tree().unwrap();
    assert_eq!(text_at(&repo, &tree, &change_path("design.md")).as_deref(), Some("# Design\n"));
    assert_eq!(text_at(&repo, &tree, &change_path("proposal.md")).as_deref(), Some(reason));
    assert_eq!(
        text_at(&repo, &tree, &change_path("tasks.md")).as_deref(),
        Some("- [ ] Add the button\n- [ ] Wire the call\n"),
        "the tasks as the base holds them"
    );
    assert_eq!(text_at(&repo, &tree, "src/logout.ts"), None, "no run work");
    assert_eq!(text_at(&repo, &tree, "src/wire.ts"), None, "no run work");

    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    let done = fx.drive(&continued, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.blocker);
    let mut changed = done.checkpoint.changed_paths.clone();
    changed.sort();
    let own = vec![change_path("tasks.md"), "src/logout.ts".to_string(), "src/wire.ts".to_string()];
    assert_eq!(changed, own, "the change set measures from the plan base");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_id(0).unwrap().to_string(), close);
    assert_eq!(commit_paths(&repo, &head), own, "the run's commit holds its own work");
    assert!(repo.find_reference(&plan::plan_base::ref_name(&rested.id)).is_err());
}

// OPU-FR-YTKN, OPU-FR-NQBK: Continue makes the plan base in a repository
// that holds no Git identity.
#[test]
fn a_plan_base_is_made_where_the_repository_has_no_git_identity() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_edits(&fx, &stream);
    let base = rested.base_commit.clone();
    let proposal_id = rested.rest.clone().unwrap().proposal_id.unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None).expect("accepted");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    {
        let mut config = repo.config().unwrap().open_level(git2::ConfigLevel::Local).unwrap();
        config.set_str("user.name", "").unwrap();
        config.set_str("user.email", "").unwrap();
    }
    assert!(repo.signature().is_err(), "the repository holds no Git identity");
    let continued = continue_of(&fx, &rested).expect("continued");
    assert_ne!(continued.base_commit, base);
    let made = continued.base_commit.unwrap();
    assert_eq!(
        repo.refname_to_id(&plan::plan_base::ref_name(&rested.id)).unwrap().to_string(),
        made
    );
}

// OPU-FR-THOZ, OPU-FR-PXDY: Continue is refused, and the run's record does
// not change, with `proposal_pending` while the plan proposal is pending (also
// where the validation cannot run, because the proposal is checked first) and
// while its close commit is owed, and with `openspec_validation_unavailable`
// and the read's code where `apply_instructions` cannot run.
#[test]
fn every_refused_continue_changes_nothing() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_edits(&fx, &stream);
    let proposal_id = rested.rest.clone().unwrap().proposal_id.unwrap();
    let before = snapshot(&fx, &rested.id);

    runner.set("validate", Err(crate::openspec::ProcessError::TimedOut));
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "proposal_pending");
    assert_eq!(snapshot(&fx, &rested.id), before);
    runner.set("validate", Ok(crate::openspec::tests::ok(VALID)));

    let gitdir = git2::Repository::open(stream.worktree()).unwrap().path().to_path_buf();
    let lock = gitdir.join("index.lock");
    std::fs::write(&lock, "").unwrap();
    let decided = accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None);
    std::fs::remove_file(&lock).unwrap();
    assert!(decided.is_ok(), "{decided:?}");
    let owed = plan_proposal(&fx, rested.rest.as_ref().unwrap());
    assert!(owed.commit_owed && owed.commit.is_none(), "the close commit is owed");
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "proposal_pending");
    assert_eq!(snapshot(&fx, &rested.id), before);
    crate::change_proposals::complete_change_edit_decision(
        fx.app.clone(),
        proposal_id.clone(),
        fx.app.state(),
    )
    .expect("the commit is made");
    assert!(plan_proposal(&fx, rested.rest.as_ref().unwrap()).commit.is_some());

    runner.set("instructions", Err(crate::openspec::ProcessError::TimedOut));
    let refused = continue_of(&fx, &rested).unwrap_err();
    assert!(refused.starts_with("openspec_validation_unavailable:"), "{refused}");
    assert_eq!(snapshot(&fx, &rested.id), before);
    runner.set("instructions", Ok(crate::openspec::tests::ok(ALL_DONE)));
    continue_of(&fx, &rested).expect("continued");
}

// OPU-FR-ZNRR, OPU-FR-SKTV, OPU-FR-DEEB, CEP-FR-YYZZ: Continue enters one
// ledger entry for each hunk of the plan proposal that is not withdrawn, in
// proposal order: two hunks of one path are `edit 1` and `edit 2`, an edited
// acceptance is `edited`, and the hunk that the plan proposal's update
// withdrew is skipped. That update is not refused for the apply run. The turn
// carries the same hunks by path, position, and outcome.
#[test]
fn the_ledger_numbers_the_hunks_of_a_path_and_skips_a_withdrawn_one() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let long = "# Why\n\nLine one.\nLine two.\nLine three.\nLine four.\nLine five.\n";
    std::fs::write(stream.worktree().join(change_path("proposal.md")), long).unwrap();
    commit_all(&stream.worktree(), "A longer proposal");
    let run = apply_run(&fx, &stream);
    let proposed = "# Why\n\nLine 1.\nLine two.\nLine three.\nLine four.\nLine 5.\n";
    let update = plan_update(Vec::new())
        .writing(&plan_file("proposal.md"), proposed)
        .writing(&plan_file("design.md"), "# Design\n");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![update]));
    let rest = rested.rest.clone().unwrap();
    let proposal_id = rest.proposal_id.clone().unwrap();
    let all = hunks(&fx, &proposal_id);
    let paths: Vec<&str> = all.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["design.md", "proposal.md", "proposal.md"]);

    let withdrawn = crate::change_proposals::update_change_proposal(
        &fx.app,
        &store(&fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::ChangeProposalUpdate {
            stream_id: &stream.id,
            change: CHANGE,
            updates: &[crate::change_proposals::ChangeUpdateEntry {
                entry: crate::text_hunks::updates::UpdateEntry::Withdraw {
                    id: all[0].hunk.id.clone(),
                },
                path: None,
            }],
            rationale: "No design.",
            agent: &crate::comments::Participant::synthesis(),
            thread_id: rest.thread_id.as_deref().unwrap(),
        },
    );
    assert!(withdrawn.is_ok(), "{withdrawn:?}");

    let loaded = crate::change_proposals::load_hunks_impl(
        &store(&fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        &proposal_id,
    )
    .unwrap();
    crate::change_proposals::edit_change_edit_hunk(
        fx.app.clone(),
        proposal_id.clone(),
        all[1].hunk.id.clone(),
        "Line I.\n".into(),
        loaded.checksum.clone(),
        fx.app.state(),
    )
    .expect("edited");
    accept(&fx, &proposal_id, &all[1].hunk.id, None).expect("accepted");
    reject(&fx, &proposal_id, &all[2].hunk.id, Some("Keep the old reason.")).expect("rejected");

    let continued = continue_of(&fx, &rested).expect("continued");
    let entries: Vec<(&str, &str, Option<&str>)> = continued
        .checkpoint
        .decisions
        .iter()
        .map(|d| (d.question.as_str(), d.answer.as_str(), d.note.as_deref()))
        .collect();
    assert_eq!(
        entries,
        vec![
            ("proposal.md edit 1", "edited", None),
            ("proposal.md edit 2", "rejected", Some("Keep the old reason.")),
        ]
    );
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    fx.drive(&continued, dispatch.clone());
    assert_eq!(
        dispatch.of_part("work")[0].input["plan_update"]["hunks"],
        serde_json::json!([
            { "path": "proposal.md", "position": 1, "outcome": "edited" },
            { "path": "proposal.md", "position": 2, "outcome": "rejected" },
        ])
    );
}

// OPU-FR-EVBZ, CEP-FR-KGOL: while the question set of the plan update
// stands, an edit of a hunk of the plan proposal and a decline of the whole
// proposal are refused with `plan_questions_pending`, and the proposal stays
// pending.
#[test]
fn an_edit_and_a_decline_wait_on_the_questions() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
    let proposal_id = rested.rest.clone().unwrap().proposal_id.unwrap();
    let design = hunk_id(&fx, &proposal_id, "design.md");
    let loaded = crate::change_proposals::load_hunks_impl(
        &store(&fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        &proposal_id,
    )
    .unwrap();
    let edited = crate::change_proposals::edit_change_edit_hunk(
        fx.app.clone(),
        proposal_id.clone(),
        design,
        "# Other\n".into(),
        loaded.checksum,
        fx.app.state(),
    );
    assert_eq!(edited.map(|_| ()).unwrap_err(), "plan_questions_pending");
    let declined = crate::change_proposals::decline_change_edit_proposal(
        fx.app.clone(),
        proposal_id.clone(),
        None,
        fx.app.state(),
        fx.app.state(),
    );
    assert_eq!(declined.map(|_| ()).unwrap_err(), "plan_questions_pending");
    let held = plan_proposal(&fx, rested.rest.as_ref().unwrap());
    assert_eq!(held.state, crate::change_proposals::ChangeProposalState::Pending);
}

// OPU-FR-DEEB, OPU-FR-WOKJ, CEP-FR-YYZZ: a recording on the change is refused
// with `apply_running` while the apply run waits in its queue, while its turn
// runs, and at a `plan_update` rest that recorded no plan proposal, also into
// the plan thread: only a `plan_findings` rest admits one.
#[test]
fn a_recording_is_refused_while_the_run_is_queued_working_or_at_a_plan_update() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let other = other_thread(&fx, &stream.id);
    assert_eq!(fx.reload(&run.id).state, GraduationRunState::Queued);
    assert_eq!(record_into(&fx, &stream.id, &other).unwrap_err(), RecordRefusal::ApplyRunning);

    let during: Arc<Mutex<Option<Result<(), RecordRefusal>>>> = Arc::new(Mutex::new(None));
    let record = {
        let (app, stream_id, other, during) =
            (fx.app.clone(), stream.id.clone(), other.clone(), during.clone());
        move || {
            let outcome = record_design(&app, &stream_id, &other).map(|_| ());
            *during.lock().unwrap() = Some(outcome);
        }
    };
    let questions_alone = plan_update(questions()).doing(record);
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![questions_alone]));
    assert_eq!(*during.lock().unwrap(), Some(Err(RecordRefusal::ApplyRunning)), "while working");

    let rest = rested.rest.clone().expect("the rest");
    assert_eq!((rest.reason.as_str(), rest.proposal_id.as_deref()), ("plan_update", None));
    let plan_thread = rest.thread_id.clone().unwrap();
    assert_eq!(
        record_into(&fx, &stream.id, &plan_thread).unwrap_err(),
        RecordRefusal::ApplyRunning
    );
    assert_eq!(record_into(&fx, &stream.id, &other).unwrap_err(), RecordRefusal::ApplyRunning);
    assert_eq!(fx.reload(&rested.id).rest.unwrap().proposal_id, None);
}

// OPU-FR-ZBXF, OPU-FR-FSCV: a discard after a Continue that made a plan base
// removes the private ref of the plan base, and the close commit of the plan
// proposal stays on the stream branch.
#[test]
fn a_discard_after_continue_removes_the_plan_base_and_keeps_the_close_commit() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_edits(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let proposal_id = rest.proposal_id.clone().unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None).expect("accepted");
    let close = plan_proposal(&fx, &rest).commit.expect("the close commit");
    continue_of(&fx, &rested).expect("continued");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    assert!(repo.find_reference(&plan::plan_base::ref_name(&rested.id)).is_ok());

    crate::graduation::discard_graduation_run(fx.app.clone(), rested.id.clone())
        .expect("discarded");
    assert!(repo.find_reference(&plan::plan_base::ref_name(&rested.id)).is_err());
    assert_eq!(head_of(&stream.worktree()), close, "the close commit stays on the branch");
}
