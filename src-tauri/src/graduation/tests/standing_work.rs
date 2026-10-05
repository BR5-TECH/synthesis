//! What a run does with work standing in its stream
//! (`GRD-graduation.md` GRD-FR-HQPD, GRD-FR-PXVJ, GRD-FR-KDWA).
//!
//! The author takes the decision when the run is enqueued and the run applies
//! it when its turn comes, so every scenario here puts work in the stream and
//! then drives the production loop over it.

use super::*;

/// The revision the stream branch holds now.
fn head_of(stream: &crate::streams::WorkStream) -> String {
    git2::Repository::open(stream.worktree())
        .expect("stream repo")
        .head()
        .and_then(|h| h.peel_to_commit())
        .expect("a head commit")
        .id()
        .to_string()
}

/// Whether a commit's tree holds a path.
fn tree_holds(stream: &crate::streams::WorkStream, revision: &str, path: &str) -> bool {
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let commit = repo
        .find_commit(git2::Oid::from_str(revision).expect("a revision"))
        .expect("the commit");
    let tree = commit.tree().expect("its tree");
    tree.get_path(Path::new(path)).is_ok()
}

/// GRD-FR-HQPD, GRD-FR-KDWA: `keep` commits nothing of its own, and the run is
/// measured from the revision the branch already holds.
#[test]
fn a_run_that_keeps_the_standing_work_starts_from_the_branch_as_it_stands() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let before = head_of(&stream);
    let run = fx.enqueue_with(&stream, "Add the empty state.", "d1", StandingWork::Keep);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(
        run.base_commit.as_deref(),
        Some(before.as_str()),
        "the base is the revision that stood before the run"
    );
    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    assert_eq!(outcome.commit, None, "the choice committed nothing");
    assert_eq!(outcome.pushed, None, "the choice asked for no push");
    assert!(
        !tree_holds(&stream, &before, "authored.md"),
        "the author's own work was not committed before the run"
    );
    // The run works on top of it, so what the run commits carries it.
    let made = run.commits.first().expect("the run's own commit");
    assert!(
        tree_holds(&stream, made, "authored.md"),
        "the standing work is part of what the run commits"
    );
}

/// GRD-FR-KDWA: `commit` records the revision the standing work became, and
/// that revision is the run's base.
#[test]
fn a_run_that_commits_the_standing_work_records_the_revision_it_made() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let before = head_of(&stream);
    let run = fx.enqueue_with(&stream, "Add the empty state.", "d1", StandingWork::Commit);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    let committed = outcome.commit.clone().expect("a commit");
    assert_ne!(committed, before, "the standing work made a new revision");
    assert_eq!(
        run.base_commit.as_deref(),
        Some(committed.as_str()),
        "the run is measured from what it committed first"
    );
    assert!(tree_holds(&stream, &committed, "authored.md"));
    assert_eq!(outcome.pushed, None, "this choice asks for no push");
    assert!(
        !run.commits.contains(&committed),
        "the standing work is not one of the run's own commits"
    );
}

/// The message of the commit a revision names.
fn message_of(stream: &crate::streams::WorkStream, revision: &str) -> String {
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let commit = repo
        .find_commit(git2::Oid::from_str(revision).expect("a revision"))
        .expect("the commit");
    commit.message().unwrap_or_default().to_string()
}

/// GRD-FR-RJFC: the commit takes the message the run carries.
#[test]
fn the_standing_commit_takes_the_message_the_run_carries() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_under(
        &stream,
        "Add it.",
        "d1",
        StandingWork::Commit,
        Some("Notes from the review meeting".to_string()),
    );

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );

    let committed = run
        .standing_work_outcome
        .as_ref()
        .and_then(|outcome| outcome.commit.clone())
        .expect("a standing commit");
    assert_eq!(
        message_of(&stream, &committed),
        "Notes from the review meeting",
        "the message is committed as it was written"
    );
    // GRD-FR-HQPD: the author's own commit. The repository's own signature is
    // what says so — a run's work is committed under the same one, and an
    // agent identity here would be a claim about who wrote the standing work.
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let signature = repo.signature().expect("a signature");
    let commit = repo
        .find_commit(git2::Oid::from_str(&committed).unwrap())
        .expect("the commit");
    assert_eq!(commit.author().name(), signature.name());
    assert_eq!(commit.committer().name(), signature.name());
}

/// GRD-FR-RJFC: a run carrying no message commits under its own name, which is
/// the name of the draft its captured input holds.
#[test]
fn a_run_with_no_message_commits_under_its_own_name() {
    for carried in [None, Some(String::new()), Some("   ".to_string())] {
        let fx = Fixture::new();
        let stream = fx.stream("editor");
        std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
        let run = fx.enqueue_under(&stream, "Add it.", "d1", StandingWork::Commit, carried);
        // The fixture names every captured draft the same way, and that name is
        // what the run is called.
        assert_eq!(run.input.draft_name, "editor draft");

        let run = fx.drive(
            &run,
            ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
        );

        let committed = run
            .standing_work_outcome
            .as_ref()
            .and_then(|outcome| outcome.commit.clone())
            .expect("a standing commit");
        assert_eq!(message_of(&stream, &committed).trim(), "editor draft");
    }
}

/// GRD-FR-KDWA: a stream that holds nothing uncommitted records no commit, and
/// the run still has a base.
#[test]
fn a_clean_stream_records_no_standing_commit() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let before = head_of(&stream);
    let run = fx.enqueue_with(&stream, "Add the empty state.", "d1", StandingWork::Commit);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(run.base_commit.as_deref(), Some(before.as_str()));
    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    assert_eq!(outcome.commit, None, "there was nothing to commit");
    assert_eq!(outcome.pushed, None, "and this choice asks for no push");
    assert_eq!(outcome.push_failure, None);
}

/// GRD-FR-PXVJ, GTC-FR-YHDU: the run driver composes the one push primitive
/// against the stream's own checkout, and a push that does not land stops
/// nothing. The run completes, and the outcome carries the reason.
#[test]
fn a_push_that_does_not_land_does_not_stop_the_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    // The fixture's project has no remote, and the machine has no credential
    // store, so the push cannot land. Neither is a reason to stop the run.
    let run = fx.enqueue_with(
        &stream,
        "Add the empty state.",
        "d1",
        StandingWork::CommitAndPush,
    );

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(
        run.state,
        GraduationRunState::Completed,
        "the run finished its work"
    );
    assert_eq!(run.blocker, None, "a refused push blocks nothing");
    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    assert!(outcome.commit.is_some(), "the commit half still happened");
    assert_eq!(outcome.pushed, Some(false));
    let failure = outcome.push_failure.as_ref().expect("a typed reason");
    // The transfer itself refused, and named why: the project has no remote to
    // push to. A machine that could not attempt the push at all reports
    // `push_unavailable` instead, and the two must not read alike.
    assert_eq!(failure.code, crate::git::ERR_NO_REMOTE_CONFIGURED);
}

/// GRD-FR-PXVJ, GTC-FR-YHDU: the push is made against the **stream's** checkout,
/// and it lands on the project's remote.
#[test]
fn a_push_lands_the_stream_branch_on_the_projects_remote() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    // A bare repository beside the project, so the transfer is real and no
    // network is involved.
    let remote = TempDir::new().expect("a remote dir");
    git2::Repository::init_bare(remote.path()).expect("a bare repo");
    fx.repo()
        .remote("origin", remote.path().to_str().expect("a path"))
        .expect("a remote");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::CommitAndPush);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    assert_eq!(outcome.pushed, Some(true), "the remote took it");
    assert_eq!(outcome.push_failure, None);

    // The branch the remote now holds is the stream's, at the revision the
    // standing work was committed as — which is what proves the push was made
    // from the stream's own checkout rather than from the author's.
    let published = git2::Repository::open(remote.path()).expect("the remote repo");
    let landed = published
        .find_branch(&stream.branch, git2::BranchType::Local)
        .expect("the stream branch on the remote")
        .get()
        .peel_to_commit()
        .expect("its commit");
    assert_eq!(
        landed.id().to_string(),
        outcome.commit.clone().expect("a standing commit"),
        "the remote holds exactly what the standing work committed"
    );
}

/// GRD-FR-PXVJ: a stream holding nothing uncommitted still pushes, because the
/// branch may carry commits the remote has not seen.
#[test]
fn a_clean_stream_is_pushed_even_though_it_committed_nothing() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let remote = TempDir::new().expect("a remote dir");
    git2::Repository::init_bare(remote.path()).expect("a bare repo");
    fx.repo()
        .remote("origin", remote.path().to_str().expect("a path"))
        .expect("a remote");
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::CommitAndPush);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready(),
        ]),
    );

    let outcome = run.standing_work_outcome.as_ref().expect("an outcome");
    assert_eq!(outcome.commit, None, "there was nothing standing to commit");
    assert_eq!(outcome.pushed, Some(true), "and the branch went up anyway");
}

/// Every structured record of one run, in the order it was written.
fn structured(fx: &Fixture, run_id: &str) -> Vec<serde_json::Value> {
    lines_of(fx, run_id, crate::graduation::logs::GraduationLogStream::Structured)
}

/// The records of one event of a run's structured log.
fn records_of<'a>(
    written: &'a [serde_json::Value],
    event: &str,
) -> Vec<&'a serde_json::Value> {
    written
        .iter()
        .filter(|record| record.get("event").and_then(|e| e.as_str()) == Some(event))
        .collect()
}

/// GRD-FR-KDWA: what the standing-work step did is readable from the run's own
/// log, with the choice and both halves of what it did.
#[test]
fn the_standing_work_step_reports_what_it_did() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let remote = TempDir::new().expect("a remote dir");
    git2::Repository::init_bare(remote.path()).expect("a bare repo");
    fx.repo()
        .remote("origin", remote.path().to_str().expect("a path"))
        .expect("a remote");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::CommitAndPush);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );

    let written = structured(&fx, &run.id);
    let settled = records_of(&written, "standing work settled");
    assert_eq!(settled.len(), 1, "one step, one record");
    let fields = settled[0].get("fields").expect("its fields");
    assert_eq!(fields["choice"], serde_json::json!("commit_and_push"));
    assert_eq!(fields["committed"], serde_json::json!(true));
    assert_eq!(fields["pushed"], serde_json::json!(true));
    assert!(fields.get("push_failure").is_none(), "nothing refused it");
    // It belongs to the wait the run was still in: no turn had started.
    assert_eq!(settled[0]["phase_id"], serde_json::json!("queued"));
}

/// GRD-FR-KDWA: a choice that acts on nothing reports nothing, so a record in
/// the log means the stream was changed.
#[test]
fn a_step_that_acted_on_nothing_reports_nothing() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    // Standing work is there — `keep` is what decides to leave it, so this is
    // the choice and not the empty stream.
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::Keep);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );

    assert!(records_of(&structured(&fx, &run.id), "standing work settled").is_empty());
}

/// GRD-FR-KDWA: the record is written before the first turn starts.
#[test]
fn the_step_is_settled_before_the_run_is_dispatched() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::Commit);
    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );

    let written = structured(&fx, &run.id);
    let position = |event: &str| {
        written
            .iter()
            .position(|record| record.get("event").and_then(|e| e.as_str()) == Some(event))
            .unwrap_or_else(|| panic!("no record of {event}"))
    };
    assert!(
        position("standing work settled") < position("run dispatched"),
        "the stream was settled before the run was dispatched"
    );
    let first_turn = written
        .iter()
        .position(|record| {
            record.get("producer").and_then(|p| p.as_str()) == Some("work_turn")
        })
        .expect("a work turn");
    assert!(position("standing work settled") < first_turn, "and before the first turn");
}

/// GRD-FR-HQPD: a commit the step cannot make **does** stop the run, because a
/// run with no base has nothing to measure its work from.
#[test]
fn a_standing_commit_that_fails_blocks_the_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::Commit);

    // The index is held by another write, which is what the commit takes
    // before it stages anything (GTC-FR-19).
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let held = crate::git::index_lock::IndexLock::try_acquire(&repo)
        .expect("the lock is readable")
        .expect("nothing else holds it");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );
    drop(held);

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(
        run.blocker.as_ref().map(|b| b.code.as_str()),
        Some("base_commit_failed")
    );
    assert_eq!(run.base_commit, None, "no base was established");
    assert_eq!(run.standing_work_outcome, None, "and nothing is reported as done");
}

/// GRD-FR-HQPD, GSU-FR-MLEJ: the choice acts on what stands in the stream when
/// the run is dispatched, and not on what stood there when it was enqueued.
///
/// This is the whole reason the decision is a choice rather than an
/// acknowledgement of a path set: a run can wait behind several others, and
/// what it will find is not what the author was looking at.
#[test]
fn the_choice_acts_on_what_stands_at_the_dispatch() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("at-enqueue.md"), "then\n").unwrap();

    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::Commit);

    // The stream moves on while the run waits its turn.
    std::fs::remove_file(stream.worktree().join("at-enqueue.md")).unwrap();
    std::fs::write(stream.worktree().join("at-dispatch.md"), "now\n").unwrap();

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );

    let committed = run
        .standing_work_outcome
        .as_ref()
        .and_then(|outcome| outcome.commit.clone())
        .expect("a standing commit");
    assert!(
        tree_holds(&stream, &committed, "at-dispatch.md"),
        "what stood at the dispatch was committed"
    );
    assert!(
        !tree_holds(&stream, &committed, "at-enqueue.md"),
        "what stood at the enqueue and was gone by the dispatch was not"
    );
}

/// GSU-FR-MLEJ, GSU-FR-ELZO: a stream holding uncommitted work refuses no
/// start, the start touches no working copy, and what the author answered is
/// what the run carries.
#[test]
fn the_start_records_the_answers_and_touches_no_working_copy() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let draft = fx.draft("editor scroll");
    // The stream is held, so the run this start makes waits in its queue.
    fx.hold_stream(&stream.id);
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let before = head_of(&stream);

    let run = crate::graduation::start_graduation(
        fx.app.clone(),
        draft,
        stream.id.clone(),
        StandingWork::CommitAndPush,
        Some("Notes from the review meeting".to_string()),
    )
    .expect("work standing in the stream refuses no start");

    assert_eq!(run.state, GraduationRunState::Queued);
    assert_eq!(run.standing_work, StandingWork::CommitAndPush);
    assert_eq!(
        run.standing_work_message.as_deref(),
        Some("Notes from the review meeting"),
        "the message the author wrote is the one the run carries"
    );
    assert_eq!(run.standing_work_outcome, None, "nothing has run yet");
    assert_eq!(head_of(&stream), before, "the start committed nothing");
    assert!(stream.worktree().join("authored.md").is_file());
}

/// GSU-FR-ELZO, GRD-FR-FTBQ: the start captures the prompt whole and enqueues
/// one run, under the answers it was given.
#[test]
fn the_start_captures_the_prompt_and_enqueues_one_run() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let draft = fx.draft("editor scroll");
    // The stream is held, so the run this start makes waits in its queue.
    fx.hold_stream(&stream.id);

    let run = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("a run");

    assert_eq!(run.input.draft_id, draft);
    assert_eq!(
        run.input.prompt_checksum,
        crate::fs::sha256_bytes(run.input.prompt.as_bytes()),
        "the checksum is of the bytes that were captured"
    );
    assert_eq!(run.standing_work, StandingWork::Keep);
    assert_eq!(run.standing_work_message, None);
    // GSU-FR-ELZO: one run, and it is the one the queue holds.
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert_eq!(queue.runs.len(), 1);
    assert_eq!(queue.runs[0].id, run.id);
}

/// GSU-FR-IRAC, GRD-FR-ZAMI: a restart records its own answers, and reads the
/// discarded run's captured input rather than its choice.
#[test]
fn a_restart_records_its_own_answers() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let draft = fx.draft("editor scroll");
    // The stream is held, so the run this start makes waits in its queue.
    fx.hold_stream(&stream.id);

    let first = crate::graduation::start_graduation(
        fx.app.clone(),
        draft,
        stream.id.clone(),
        StandingWork::Keep,
        Some("the first run's message".to_string()),
    )
    .expect("a run");
    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone())
        .expect("a discarded run");
    assert_eq!(discarded.state, GraduationRunState::Discarded);

    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        discarded.id.clone(),
        stream.id.clone(),
        StandingWork::CommitAndPush,
        Some("the restart's own message".to_string()),
    )
    .expect("a new run");

    assert_eq!(restarted.standing_work, StandingWork::CommitAndPush);
    assert_eq!(
        restarted.standing_work_message.as_deref(),
        Some("the restart's own message"),
        "the restart's answers are its own, not the discarded run's"
    );
    assert_eq!(restarted.restarted_from_run_id.as_deref(), Some(discarded.id.as_str()));
    // The prompt travels whole; the answers about the stream do not.
    assert_eq!(restarted.input.prompt, discarded.input.prompt);
    assert_eq!(fx.reload(&discarded.id).standing_work, StandingWork::Keep);
}

// GSU-FR-RNOM / PST-FR-YWXF: a restart asks for a graduation-start commit named
// for the draft as it is now called, not as the discarded run captured it.
#[test]
fn a_restart_asks_for_a_commit_under_the_drafts_current_name() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let draft = fx.draft("editor scroll");
    fx.hold_stream(&stream.id);
    let first = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("a run");
    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone())
        .expect("a discarded run");
    let access = fx
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("an instance");
    let root = crate::fs::RootFs::new(fx.root(), access);
    crate::drafts::rename_draft_impl(&root, &draft, "editor scrolling").expect("a rename");

    crate::graduation::restart_graduation_run(
        fx.app.clone(),
        discarded.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("a new run");

    let messages: Vec<String> = crate::storage_floor::commit::pending_events(&root)
        .into_iter()
        .filter(|(id, event, _)| {
            id == &draft && *event == crate::storage_floor::commit::DraftEvent::GraduationStarted
        })
        .map(|(_, _, message)| message)
        .collect();
    assert_eq!(
        messages,
        vec![
            "draft: graduate \"editor scroll\"".to_string(),
            "draft: graduate \"editor scrolling\"".to_string(),
        ],
    );
}

/// GRD-FR-KDWA, GRD-FR-HQPD: the shape both processes read the record by.
#[test]
fn the_record_carries_the_choice_and_the_outcome_in_the_shape_the_window_reads() {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    run.standing_work = StandingWork::CommitAndPush;
    run.standing_work_message = Some("Notes from the meeting".into());
    run.standing_work_outcome = Some(StandingWorkOutcome {
        commit: Some("a91bc04".into()),
        pushed: Some(false),
        push_failure: Some(StandingWorkPushFailure {
            code: "github_unreachable".into(),
        }),
    });

    let wire = serde_json::to_value(&run).expect("a serialized run");
    assert_eq!(wire["standingWork"], serde_json::json!("commit_and_push"));
    assert_eq!(
        wire["standingWorkMessage"],
        serde_json::json!("Notes from the meeting")
    );
    let outcome = &wire["standingWorkOutcome"];
    assert_eq!(outcome["commit"], serde_json::json!("a91bc04"));
    assert_eq!(outcome["pushed"], serde_json::json!(false));
    assert_eq!(
        outcome["pushFailure"]["code"],
        serde_json::json!("github_unreachable")
    );

    // And each stored value reads back as the choice it names.
    for (stored, choice) in [
        ("keep", StandingWork::Keep),
        ("commit", StandingWork::Commit),
        ("commit_and_push", StandingWork::CommitAndPush),
    ] {
        assert_eq!(
            serde_json::from_value::<StandingWork>(serde_json::json!(stored)).expect(stored),
            choice
        );
    }
}

/// GSU-FR-MLEJ: the choice travels with the run, and enqueuing acts on no
/// working copy.
///
/// `start_graduation` itself is not the route: its image preflight refuses on a
/// machine with no container runtime (GSU-FR-GLVQ). What is exercised here is
/// the record it composes and what that record leaves behind it.
#[test]
fn enqueuing_records_the_choice_and_touches_no_working_copy() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::write(stream.worktree().join("authored.md"), "mine\n").unwrap();
    let before = head_of(&stream);

    let run = fx.enqueue_with(&stream, "Add it.", "d1", StandingWork::CommitAndPush);

    assert_eq!(run.standing_work, StandingWork::CommitAndPush);
    assert_eq!(run.standing_work_outcome, None, "nothing has run yet");
    assert_eq!(run.state, GraduationRunState::Queued);
    assert_eq!(
        head_of(&stream),
        before,
        "the enqueue committed nothing in the stream"
    );
    assert!(
        stream.worktree().join("authored.md").is_file(),
        "the standing work is untouched"
    );
    // GSU-FR-MLEJ: the choice and the message are durable, and both are read
    // again when the run is dispatched.
    let stored = fx.reload(&run.id);
    assert_eq!(stored.standing_work, StandingWork::CommitAndPush);
    assert_eq!(stored.standing_work_message, None);

    let named = fx.enqueue_under(
        &stream,
        "Add it.",
        "d2",
        StandingWork::Commit,
        Some("Notes from the meeting".to_string()),
    );
    assert_eq!(
        fx.reload(&named.id).standing_work_message.as_deref(),
        Some("Notes from the meeting")
    );
}

/// GRD-FR-HQPD: a record written before the choice existed reads as `commit`,
/// which is what such a run was enqueued under.
#[test]
fn a_record_that_names_no_choice_reads_as_commit() {
    let stored = serde_json::json!({
        "id": "g1",
        "streamId": "w1",
        "streamName": "editor",
        "state": "queued",
        "input": {
            "draftId": "d1",
            "draftName": "d",
            "prompt": "p",
            "promptChecksum": "0",
            "capturedAt": "2026-01-01T00:00:00Z"
        },
        "commits": [],
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z"
    });
    let run: GraduationRun = serde_json::from_value(stored).expect("an older record");
    assert_eq!(run.standing_work, StandingWork::Commit);
    assert_eq!(run.standing_work_outcome, None);
}
