//! What an update refuses, and what it refuses to touch (WKS-FR-KFVJ,
//! WKS-FR-OKVB, WKS-FR-HLGN, WKS-FR-TSOA, WKS-FR-WEJK).

use std::path::Path;

use tauri::Manager;

use super::*;

/// The typed code a refusal carries, without its detail.
fn code_of(reason: &str) -> String {
    reason.split(':').next().unwrap_or(reason).trim().to_string()
}

// WKS-FR-KFVJ / WKS-FR-ZHTC: a base branch that moved since the author read the
// revision refuses with `stale_base_revision` and leaves both branches and both
// working copies exactly as they were.
#[test]
fn a_base_branch_that_moved_since_the_preview_refuses_and_writes_nothing() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let stale = tip_of(&fx.repo(), &stream.base_branch);
    // The branch moves after the author read it.
    std::fs::write(fx.root().join("later.txt"), "later\n").unwrap();
    commit_all(&fx.repo(), "the base moved again");
    let moved_to = tip_of(&fx.repo(), &stream.base_branch);
    let stream_before = tip_of(&fx.repo(), &stream.branch);

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &stale.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Failed, "{record:?}");
    assert_eq!(record.failure, ERR_STALE_BASE_REVISION);
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), moved_to, "the base is untouched");
    assert_eq!(tip_of(&repo, &stream.branch), stream_before, "and so is the stream");
}

// WKS-FR-PWZC / GRB-FR-DYUA: the pinned revision is what every step reads, so
// the update lands the revision the author judged rather than a branch tip read
// a second time.
#[test]
fn the_pinned_revision_is_what_the_update_lands_and_not_the_branch_tip() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");

    let repo = fx.repo();
    let tip = repo.find_commit(tip_of(&repo, &stream.branch)).unwrap();
    assert!(
        tip.parent_ids().any(|oid| oid == pinned),
        "the merge commit's second parent is the pinned revision",
    );
    assert_eq!(record.base_revision, pinned.to_string());
}

// WKS-FR-OKVB: a dirty stream working copy refuses with `stream_dirty` carrying
// the complete path set, and writes nothing.
#[test]
fn a_dirty_stream_working_copy_refuses_with_every_path() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let worktree = Path::new(&stream.worktree_path);
    std::fs::write(worktree.join("scratch-a.txt"), "one\n").unwrap();
    std::fs::write(worktree.join("scratch-b.txt"), "two\n").unwrap();
    let pinned = tip_of(&fx.repo(), &stream.base_branch);
    let stream_before = tip_of(&fx.repo(), &stream.branch);

    let report = block_on(updating::run(
        &fx.app,
        &stream.id,
        StreamUpdateStrategy::MergeSource,
        pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    ));
    let reason = report.outcome.expect_err("refused");
    assert_eq!(code_of(&reason), ERR_STREAM_DIRTY);
    assert!(reason.contains("scratch-a.txt") && reason.contains("scratch-b.txt"));
    assert_eq!(tip_of(&fx.repo(), &stream.branch), stream_before);
}

// WKS-FR-OKVB: a dirty base worktree refuses with `base_dirty`, and an update
// whose base branch has no checkout refuses with `base_not_checked_out`.
#[test]
fn a_dirty_base_worktree_and_a_base_with_no_checkout_are_each_refused() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);
    std::fs::write(fx.root().join("uncommitted.txt"), "the author's own\n").unwrap();

    let report = block_on(updating::run(
        &fx.app,
        &stream.id,
        StreamUpdateStrategy::MergeSource,
        pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    ));
    let reason = report.outcome.expect_err("refused");
    assert_eq!(code_of(&reason), ERR_BASE_DIRTY);
    assert!(reason.contains("uncommitted.txt"));
    std::fs::remove_file(fx.root().join("uncommitted.txt")).unwrap();

    // A base branch no worktree holds: the primary checkout moves off it.
    let repo = fx.repo();
    let scratch = repo
        .find_commit(tip_of(&repo, &stream.base_branch))
        .unwrap();
    repo.branch("elsewhere", &scratch, true).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    let report = block_on(updating::run(
        &fx.app,
        &stream.id,
        StreamUpdateStrategy::MergeSource,
        pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    ));
    assert_eq!(
        code_of(&report.outcome.expect_err("refused")),
        ERR_BASE_NOT_CHECKED_OUT,
    );
}

// WKS-FR-NRQT: an update is refused while a run holds the stream.
#[test]
fn an_update_is_refused_while_a_run_holds_the_stream() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    claim_stream(&fx.app, &stream.id, "run-1").expect("claimed");

    let refusal = update_commands::update_work_stream(
        fx.app.clone(),
        stream.id.clone(),
        StreamUpdateStrategy::MergeSource,
        pinned,
    )
    .expect_err("refused");
    assert_eq!(refusal, ERR_STREAM_BUSY);
    assert!(update_of(&fx, &stream.id).is_none(), "nothing was recorded");
}

// WKS-FR-TSOA: a merge and an update of one repository do not run together.
// Each refuses while the other holds the repository update guard.
#[test]
fn a_merge_and_an_update_refuse_while_the_other_holds_the_guard() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let other = fx.create("second", None).expect("created");
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    let state = fx.app.state::<StreamState>();

    // A merge holds the guard, so an update of the same stream and of another
    // one are both refused.
    {
        let _held = state
            .begin_repository_update(&stream.id, Reconciliation::Merge)
            .expect("the merge hold");
        let report = block_on(updating::run(
            &fx.app,
            &stream.id,
            StreamUpdateStrategy::MergeSource,
            pinned.clone(),
            Vec::new(),
            ScriptedDispatch::new(Vec::new()),
        ));
        assert_eq!(report.outcome.expect_err("refused"), ERR_MERGE_IN_PROGRESS);
    }

    // And with an update holding it, a merge of either stream is refused.
    let _held = state
        .begin_repository_update(&stream.id, Reconciliation::Update)
        .expect("the update hold");
    assert_eq!(
        merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused"),
        ERR_UPDATE_IN_PROGRESS,
    );
    assert_eq!(
        merge(&fx, &other.id, StreamMergePublication::Uncommitted).expect_err("refused"),
        ERR_UPDATE_IN_PROGRESS,
    );
}

// WKS-FR-TSOA: an update refused by the repository update guard settles its own
// record rather than leaving it `running` for ever.
#[test]
fn a_reconciliation_refused_by_the_guard_settles_its_own_record() {
    let fx = Fixture::new();
    let first = fx.create("holds the guard", None).expect("created");
    let second = stream_behind_its_base(&fx);
    let state = fx.app.state::<StreamState>();
    let _held = state
        .begin_repository_update(&first.id, Reconciliation::Update)
        .expect("the hold");
    assert_eq!(
        merge(&fx, &second.id, StreamMergePublication::Uncommitted).expect_err("refused"),
        ERR_UPDATE_IN_PROGRESS,
    );

    // And the same for an update refused by a merge holding the guard.
    drop(_held);
    let _held = state
        .begin_repository_update(&first.id, Reconciliation::Merge)
        .expect("the hold");
    let pinned = tip_of(&fx.repo(), &second.base_branch).to_string();
    let record = update_job_scripted(
        &fx,
        &second,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Failed, "{record:?}");
    assert_eq!(record.failure, ERR_MERGE_IN_PROGRESS);
}

// WKS-FR-WEJK / GRB-FR-TXVL: `cancel_work_stream_update` reaches the update's
// own token, and a stream with no update running is answered without an error.
#[test]
fn cancelling_an_update_reaches_its_token_and_an_idle_stream_is_no_error() {
    let fx = Fixture::new();
    let stream = fx.create("idle", None).expect("created");

    update_commands::cancel_work_stream_update(fx.app.clone(), stream.id.clone())
        .expect("a stream with no update is answered, not refused");
    update_commands::cancel_work_stream_update(fx.app.clone(), "no-such-stream".to_string())
        .expect("and so is a stream that does not exist");

    let state = fx.app.state::<StreamState>();
    let hold = state
        .begin_repository_update(&stream.id, Reconciliation::Update)
        .expect("the hold");
    assert!(!hold.cancellation().is_cancelled());
    update_commands::cancel_work_stream_update(fx.app.clone(), stream.id.clone())
        .expect("answered");
    assert!(hold.cancellation().is_cancelled());
}

// GRB-FR-TXVL / WKS-FR-ZHTC: a cancelled update records `cancelled` and moves
// neither branch.
#[test]
fn a_cancelled_update_records_cancelled_and_moves_neither_branch() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let repo = fx.repo();
    let base_before = tip_of(&repo, &stream.base_branch);
    let stream_before = tip_of(&repo, &stream.branch);
    let app = fx.app.clone();
    let stream_id = stream.id.clone();

    // The turn cancels the update while it is running, exactly as the author's
    // own control does.
    let dispatch = ScriptedDispatch::new(vec![Script::SettleAnd(
        Vec::new(),
        Box::new(move || {
            let _ = update_commands::cancel_work_stream_update(app.clone(), stream_id.clone());
        }),
    )]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &base_before.to_string(),
        Vec::new(),
        dispatch,
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Cancelled, "{record:?}");
    assert_eq!(record.failure, ERR_UPDATE_CANCELLED);
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), base_before);
    assert_eq!(tip_of(&repo, &stream.branch), stream_before);
}

// WKS-FR-LRAV / WKS-FR-DPNM: clearing and retrying are refused while an update
// of that stream runs.
#[test]
fn clearing_and_retrying_are_refused_while_an_update_runs() {
    let fx = Fixture::new();
    let stream = fx.create("running", None).expect("created");
    let fs = commands::store_fs(&fx.app).expect("fs");
    let store = commands::store(&fx.app).expect("store");
    let record = StreamUpdateRecord::starting(
        &stream,
        StreamUpdateStrategy::MergeSource,
        "0000000000000000000000000000000000000000",
        Vec::new(),
        Vec::new(),
    );
    update_record::write_update(&fs, &store, &record).expect("written");

    assert_eq!(
        update_commands::clear_work_stream_update(fx.app.clone(), stream.id.clone())
            .expect_err("refused"),
        ERR_UPDATE_IN_PROGRESS,
    );
    assert_eq!(
        update_commands::retry_work_stream_update(fx.app.clone(), stream.id.clone())
            .expect_err("refused"),
        ERR_UPDATE_IN_PROGRESS,
    );
}

// WKS-FR-GYHF: every update command answers `unknown_stream` for a stream the
// project does not hold.
#[test]
fn an_unknown_stream_is_refused_by_every_update_command() {
    let fx = Fixture::new();
    let unknown = "no-such-stream".to_string();

    assert_eq!(
        update_commands::get_work_stream_update(fx.app.clone(), unknown.clone())
            .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::update_work_stream(
            fx.app.clone(),
            unknown.clone(),
            StreamUpdateStrategy::MergeSource,
            "0000000000000000000000000000000000000000".to_string(),
        )
        .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::clear_work_stream_update(fx.app.clone(), unknown)
            .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
}

// WKS-FR-NRQT: a stream whose working copy is gone is refused rather than
// updated from a tree nothing can open.
#[test]
fn a_stream_whose_working_copy_is_gone_is_refused() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    std::fs::remove_dir_all(&stream.worktree_path).expect("removed");

    let report = block_on(updating::run(
        &fx.app,
        &stream.id,
        StreamUpdateStrategy::MergeSource,
        pinned,
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    ));
    assert_eq!(
        code_of(&report.outcome.expect_err("refused")),
        ERR_STREAM_MISSING,
    );
}

// WKS-FR-GYHF: every one of the six update commands answers `unknown_stream`
// for a stream the project does not hold.
#[test]
fn every_update_command_refuses_a_stream_the_project_does_not_hold() {
    let fx = Fixture::new();
    let unknown = || "no-such-stream".to_string();

    assert_eq!(
        update_commands::get_work_stream_update(fx.app.clone(), unknown())
            .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::update_work_stream(
            fx.app.clone(),
            unknown(),
            StreamUpdateStrategy::MergeSource,
            "0000000000000000000000000000000000000000".to_string(),
        )
        .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::retry_work_stream_update(fx.app.clone(), unknown())
            .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::answer_work_stream_update_escalation(
            fx.app.clone(),
            unknown(),
            Vec::new(),
        )
        .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    assert_eq!(
        update_commands::clear_work_stream_update(fx.app.clone(), unknown())
            .expect_err("refused"),
        ERR_UNKNOWN_STREAM,
    );
    // WKS-FR-WEJK: cancellation alone is answered without an error, a stream
    // with no update running being no failure of the request.
    update_commands::cancel_work_stream_update(fx.app.clone(), unknown()).expect("answered");
}

// GRB-FR-TXVL / WKS-FR-ZHTC: a `rebase_source` update that settles nothing
// leaves the stream branch and its working copy byte-identical, index included.
#[test]
fn a_rebase_that_settles_nothing_leaves_the_stream_exactly_as_it_was() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let repo = fx.repo();
    let base_before = tip_of(&repo, &stream.base_branch);
    let stream_before = tip_of(&repo, &stream.branch);
    let worktree = Path::new(&stream.worktree_path);
    let content_before = std::fs::read_to_string(worktree.join("README.md")).unwrap();

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &base_before.to_string(),
        Vec::new(),
        ScriptedDispatch::new(vec![
            Script::ClaimSuccessAndChangeNothing,
            Script::ClaimSuccessAndChangeNothing,
            Script::ClaimSuccessAndChangeNothing,
        ]),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Conflicted, "{record:?}");
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), base_before);
    assert_eq!(
        tip_of(&repo, &stream.branch),
        stream_before,
        "a replay writes loose commits and moves no ref until it is settled",
    );
    assert_eq!(
        std::fs::read_to_string(worktree.join("README.md")).unwrap(),
        content_before,
    );
    assert!(
        git::uncommitted_paths(worktree, true).is_empty(),
        "the index and the working copy are as clean as they were",
    );
}

// GRB-FR-RMKD: a stream holding nothing of its own is fast-forwarded onto the
// pinned revision rather than given a copy of it.
#[test]
fn a_stream_with_no_commits_of_its_own_is_fast_forwarded() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");

    let repo = fx.repo();
    assert_eq!(
        tip_of(&repo, &stream.branch),
        pinned,
        "the stream stands on the pinned revision itself, not on a copy of it",
    );
    assert_eq!(
        summary_of(&fx, &stream.id).ahead_of_base,
        0,
        "so it is ahead of its base by nothing",
    );
}

// WKS-FR-JVLM / PST-FR-RONA: an update checks out the stream's working copy and
// only reads the base worktree. A draft edited in the base worktree neither
// refuses the update nor is saved there, so the pinned base does not move; a
// draft edited in the stream's working copy is saved before its checkout.
#[test]
fn an_update_saves_the_stream_side_and_leaves_the_base_side_alone() {
    let fx = Fixture::new();
    let home_rel = ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef";
    let base_home = fx.root().join(home_rel);
    std::fs::create_dir_all(base_home.join("files")).unwrap();
    std::fs::write(
        base_home.join("draft.toml"),
        "id = \"1a2b3c4d5e6-0001-deadbeef\"\nname = \"Push button\"\nstatus = \"active\"\n\
         promptPath = \"P.md\"\ncreatedAt = \"2026-01-01T00:00:00Z\"\n\
         updatedAt = \"2026-01-01T00:00:00Z\"\n",
    )
    .unwrap();
    std::fs::write(base_home.join("files/P.md"), "committed\n").unwrap();
    commit_all(&fx.repo(), "a committed draft");
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);
    std::fs::write(base_home.join("files/P.md"), "edited in the base\n").unwrap();
    let stream_home = Path::new(&stream.worktree_path).join(home_rel);
    std::fs::write(stream_home.join("files/P.md"), "edited in the stream\n").unwrap();

    let report = block_on(updating::run(
        &fx.app,
        &stream.id,
        StreamUpdateStrategy::MergeSource,
        pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    ));
    report.outcome.expect("the update lands");

    assert_eq!(tip_of(&fx.repo(), &stream.base_branch), pinned, "no save moved the base");
    assert_eq!(
        std::fs::read_to_string(base_home.join("files/P.md")).unwrap(),
        "edited in the base\n",
    );
    assert_eq!(
        std::fs::read_to_string(stream_home.join("files/P.md")).unwrap(),
        "edited in the stream\n",
        "the stream's checkout kept the saved prompt",
    );
}

