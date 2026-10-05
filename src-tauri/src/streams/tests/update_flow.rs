//! Bringing one stream up to a pinned revision of its base branch
//! (GRB-FR-BQNF, GRB-FR-HJZC, GRB-FR-RMKD, GRB-FR-WGPS, GRB-FR-NFEB).

use std::path::Path;


use super::*;

// WKS-FR-RJPD / WKS-FR-UBGX: a listing says how far behind its base each stream
// stands, names the base branch's tip, and lists the commits the stream lacks.
#[test]
fn a_listing_reports_how_far_behind_a_stream_stands_and_what_it_is_missing() {
    let fx = Fixture::new();
    let stream = fx.create("behind", None).expect("created");
    let up_to_date = summary_of(&fx, &stream.id);
    assert_eq!(up_to_date.behind_base, 0, "a fresh stream is up to date");
    assert!(up_to_date.missing_commits.is_empty());
    assert_eq!(
        up_to_date.base_tip_revision,
        tip_of(&fx.repo(), &stream.base_branch).to_string(),
    );

    std::fs::write(fx.root().join("base-only.txt"), "one\n").unwrap();
    commit_all(&fx.repo(), "base moved on");
    let summary = summary_of(&fx, &stream.id);
    assert_eq!(summary.behind_base, 1);
    assert_eq!(summary.missing_commits.len(), 1);
    assert_eq!(summary.missing_commits[0].summary, "base moved on");
    assert_eq!(
        summary.missing_commits[0].revision,
        tip_of(&fx.repo(), &stream.base_branch).to_string(),
    );
    assert_eq!(
        summary.base_tip_revision,
        tip_of(&fx.repo(), &stream.base_branch).to_string(),
    );
}

// GRB-FR-NFEB: a stream that already holds the pinned revision rests at
// `nothing_to_update` and nothing is written.
#[test]
fn a_stream_already_holding_the_pinned_revision_updates_nothing() {
    let fx = Fixture::new();
    let stream = fx.create("current", None).expect("created");
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    let before = tip_of(&fx.repo(), &stream.branch);

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::NothingToUpdate);
    assert_eq!(tip_of(&fx.repo(), &stream.branch), before);
}

// GRB-FR-HJZC / WKS-FR-XDBM: `merge_source` puts the pinned revision into the
// stream branch as one merge commit, and the base branch does not move.
#[test]
fn merge_source_creates_a_merge_commit_on_the_stream_and_leaves_the_base_alone() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);
    let base_before = pinned;

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
    assert_eq!(record.semantic_turns, 0, "Git settled it alone");
    assert!(record.updated_paths.contains(&"base-only.txt".to_string()));

    let repo = fx.repo();
    assert_eq!(
        tip_of(&repo, &stream.base_branch),
        base_before,
        "the base branch does not move",
    );
    let tip = repo.find_commit(tip_of(&repo, &stream.branch)).unwrap();
    assert_eq!(tip.parent_count(), 2, "one merge commit");
    assert!(tip.parent_ids().any(|oid| oid == pinned));
    // The stream's working copy holds what the base brought.
    assert!(Path::new(&stream.worktree_path).join("base-only.txt").is_file());
}

// GRB-FR-RMKD: `rebase_source` replays the stream commits onto the pinned
// revision, so the stream branch holds it as an ancestor and no merge commit.
#[test]
fn rebase_source_replays_the_stream_commits_onto_the_pinned_revision() {
    let fx = Fixture::new();
    let stream = fx.create("replayed", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(
        Path::new(&stream.worktree_path).join("stream-only.txt"),
        "from the stream\n",
    )
    .unwrap();
    commit_all(&worktree, "stream work");
    std::fs::write(fx.root().join("base-only.txt"), "from the base\n").unwrap();
    commit_all(&fx.repo(), "base moved on");
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
    assert_eq!(tip_of(&repo, &stream.base_branch), pinned, "the base is untouched");
    let tip = repo.find_commit(tip_of(&repo, &stream.branch)).unwrap();
    assert_eq!(tip.parent_count(), 1, "a replay makes no merge commit");
    assert_eq!(tip.summary().ok().flatten(), Some("stream work"));
    assert_eq!(tip.parent_id(0).unwrap(), pinned, "replayed onto the pinned tip");
    // Both sides' files stand in the stream's working copy.
    let worktree_path = Path::new(&stream.worktree_path);
    assert!(worktree_path.join("stream-only.txt").is_file());
    assert!(worktree_path.join("base-only.txt").is_file());
}

// GRB-FR-WGPS: a rebase that conflicts on several replayed commits spends ONE
// semantic turn for the whole replay, not one for each commit.
#[test]
fn a_conflicting_rebase_spends_one_semantic_turn_for_the_whole_replay() {
    let fx = Fixture::new();
    let stream = fx.create("many commits", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    let worktree_path = Path::new(&stream.worktree_path);
    for step in 1..=3 {
        std::fs::write(
            worktree_path.join("README.md"),
            format!("{STREAM_SIDE}-{step}\n"),
        )
        .unwrap();
        commit_all(&worktree, &format!("stream edit {step}"));
    }
    std::fs::write(fx.root().join("README.md"), format!("{BASE_SIDE}\n")).unwrap();
    commit_all(&fx.repo(), "base edit");
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let dispatch = ScriptedDispatch::new(vec![Script::Settle(vec![(
        "README.md".to_string(),
        format!("{BASE_SIDE}-and-{STREAM_SIDE}\n"),
    )])]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned.to_string(),
        Vec::new(),
        dispatch.clone(),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");
    assert_eq!(
        dispatch.turns(),
        1,
        "three conflicting replay steps cost one turn, not three",
    );
    assert_eq!(record.semantic_turns, 1);
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), pinned, "the base is untouched");
    assert_eq!(
        std::fs::read_to_string(Path::new(&stream.worktree_path).join("README.md")).unwrap(),
        format!("{BASE_SIDE}-and-{STREAM_SIDE}\n"),
    );
}

// GRB-FR-EPYG / GRB-FR-WGPS: a replay step Git could not settle costs no turn
// where the two tips still merge cleanly. Git settles what Git can settle, and
// it settles the whole update at once rather than one replayed commit at a time.
#[test]
fn a_replay_step_that_conflicts_costs_no_turn_where_the_tips_merge_cleanly() {
    let fx = Fixture::new();
    let stream = fx.create("net zero", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    let worktree_path = Path::new(&stream.worktree_path);
    // The stream edits the very line the base branch also edits, then puts it
    // back. The tip of the stream therefore holds the seed content again, so
    // the two tips merge cleanly although the first replay step does not.
    std::fs::write(worktree_path.join("README.md"), format!("{STREAM_SIDE}\n")).unwrap();
    commit_all(&worktree, "stream edit");
    std::fs::write(worktree_path.join("README.md"), format!("{ANCESTOR_SIDE}\n")).unwrap();
    commit_all(&worktree, "stream edit undone");
    std::fs::write(fx.root().join("README.md"), format!("{BASE_SIDE}\n")).unwrap();
    commit_all(&fx.repo(), "base edit");
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let dispatch = ScriptedDispatch::new(Vec::new());
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned.to_string(),
        Vec::new(),
        dispatch.clone(),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");
    assert_eq!(dispatch.turns(), 0, "no agent turn was dispatched at all");
    assert_eq!(record.semantic_turns, 0);
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), pinned, "the base is untouched");
    assert_eq!(
        std::fs::read_to_string(Path::new(&stream.worktree_path).join("README.md")).unwrap(),
        format!("{BASE_SIDE}\n"),
        "the base branch's own edit stands, the stream having undone its own",
    );
}

// GRB-FR-TXVL / WKS-FR-ZHTC: an update whose turns settle nothing rests
// `conflicted` and leaves both branches and both working copies as they were.
#[test]
fn an_update_that_settles_nothing_changes_neither_branch() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let repo = fx.repo();
    let base_before = tip_of(&repo, &stream.base_branch);
    let stream_before = tip_of(&repo, &stream.branch);
    let pinned = base_before;

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(vec![
            Script::ClaimSuccessAndChangeNothing,
            Script::ClaimSuccessAndChangeNothing,
            Script::ClaimSuccessAndChangeNothing,
        ]),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Conflicted, "{record:?}");
    assert!(record.conflicts.iter().any(|c| c.path == "README.md"));
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), base_before);
    assert_eq!(tip_of(&repo, &stream.branch), stream_before);
    assert_eq!(
        std::fs::read_to_string(Path::new(&stream.worktree_path).join("README.md")).unwrap(),
        format!("{STREAM_SIDE}\n"),
    );
}

// GRB-FR-LADU / WKS-FR-CBXW: an update turn that cannot choose asks the author,
// and their answers reach the update that follows.
#[test]
fn an_escalated_update_is_answered_and_the_answers_reach_the_next_turn() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned.to_string(),
        Vec::new(),
        ScriptedDispatch::new(vec![Script::Escalate]),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Escalated, "{record:?}");
    let escalation = record.escalation.as_ref().expect("an escalation");
    assert_eq!(escalation.questions.len(), 1);
    assert_eq!(
        escalation.origin,
        crate::graduation::GraduationEscalationOrigin::SemanticMerge,
    );
    assert_eq!(record.base_revision, pinned.to_string(), "the pin is retained");

    // WKS-FR-DPNM: an escalated update is answered, never retried.
    let refusal = update_commands::retry_work_stream_update(fx.app.clone(), stream.id.clone())
        .expect_err("a retry of an escalated update is refused");
    assert_eq!(refusal, ERR_UPDATE_STATE_NOT_PERMITTED);

    let answers = vec![crate::graduation::GraduationEscalationAnswer {
        position: escalation.questions[0].position,
        answer: "the base limit stands".to_string(),
        summary: String::new(),
    }];
    let started = update_commands::answer_work_stream_update_escalation(
        fx.app.clone(),
        stream.id.clone(),
        answers,
    )
    .expect("accepted");
    assert_eq!(started.state, StreamUpdateState::Running);
    assert_eq!(started.decisions.len(), 1);
    assert_eq!(started.base_revision, pinned.to_string());
}

// WKS-FR-ZKUP / WKS-FR-AMWE: the update record stands apart from the merge
// record, and keeps the strategy and the pinned revision.
#[test]
fn the_update_record_stands_apart_from_the_merge_record() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.strategy, StreamUpdateStrategy::MergeSource);
    assert_eq!(record.base_revision, pinned);
    assert_eq!(record.base_branch, stream.base_branch);

    // WKS-FR-SGCM: and the listing carries it.
    let summary = summary_of(&fx, &stream.id);
    assert_eq!(
        summary.update.as_ref().map(|r| r.state),
        Some(StreamUpdateState::Updated),
    );
    assert!(summary.merge_run.is_none());
}

// WKS-FR-LRAV: clearing removes the update record and touches neither branch.
#[test]
fn clearing_an_update_removes_its_record_and_moves_no_branch() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let repo = fx.repo();
    let base_before = tip_of(&repo, &stream.base_branch);
    let stream_before = tip_of(&repo, &stream.branch);

    update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &base_before.to_string(),
        Vec::new(),
        ScriptedDispatch::new(vec![Script::Escalate]),
    )
    .expect("started");
    assert!(update_of(&fx, &stream.id).is_some());

    update_commands::clear_work_stream_update(fx.app.clone(), stream.id.clone()).expect("cleared");
    assert!(update_of(&fx, &stream.id).is_none());
    let repo = fx.repo();
    assert_eq!(tip_of(&repo, &stream.base_branch), base_before);
    assert_eq!(tip_of(&repo, &stream.branch), stream_before);
}

// WKS-FR-QFTH: an update record left `running` by a stopped application is
// recorded `failed` with `update_interrupted` at the next listing.
#[test]
fn an_update_a_stopped_application_left_running_is_recorded_interrupted() {
    let fx = Fixture::new();
    let stream = fx.create("stranded", None).expect("created");
    let fs = commands::store_fs(&fx.app).expect("fs");
    let store = commands::store(&fx.app).expect("store");
    let record = StreamUpdateRecord::starting(
        &stream,
        StreamUpdateStrategy::RebaseSource,
        "0000000000000000000000000000000000000000",
        Vec::new(),
        Vec::new(),
    );
    update_record::write_update(&fs, &store, &record).expect("written");

    let summary = summary_of(&fx, &stream.id);
    let update = summary.update.expect("an update record");
    assert_eq!(update.state, StreamUpdateState::Failed);
    assert_eq!(update.failure, ERR_UPDATE_INTERRUPTED);
}

// WKS-FR-EIBC: deleting a stream takes its update record with it.
#[test]
fn deleting_a_stream_removes_its_update_record() {
    let fx = Fixture::new();
    let stream = fx.create("short lived", None).expect("created");
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

    delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).expect("deleted");
    assert!(update_record::read_update(&fs, &store, &stream.id).is_none());
}

// WKS-FR-FQLS: a running update reports itself, and the row learns the strategy
// from the event.
#[test]
fn a_running_update_reports_which_turn_it_is_on() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    use tauri::Listener;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let sink = seen.clone();
    fx.app.listen(WORK_STREAM_UPDATE_PROGRESS, move |event| {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            sink.lock().unwrap().push(value);
        }
    });

    update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(vec![Script::Escalate]),
    )
    .expect("started");

    let reports = seen.lock().unwrap().clone();
    let turn = reports
        .iter()
        .find(|value| value["turn"].as_u64() == Some(1))
        .expect("a turn was reported");
    assert_eq!(turn["strategy"], "rebase_source");
    assert_eq!(turn["turnsMax"], UPDATE_ATTEMPTS_MAX as u64);
    assert!(
        turn["reconcilingPaths"]
            .as_array()
            .is_some_and(|paths| paths.iter().any(|p| p == "README.md")),
    );
    assert!(
        reports.iter().any(|value| value["turn"].as_u64() == Some(0)),
        "the update reports itself settled",
    );
}

// WKS-FR-UBGX: a listing names at most the first fifty missing commits and
// reports the whole number beside them.
#[test]
fn a_listing_cuts_the_missing_commits_to_fifty_and_still_counts_them_all() {
    let fx = Fixture::new();
    let stream = fx.create("far behind", None).expect("created");
    for step in 0..55 {
        std::fs::write(fx.root().join("base-only.txt"), format!("{step}\n")).unwrap();
        commit_all(&fx.repo(), &format!("base commit {step}"));
    }

    let summary = summary_of(&fx, &stream.id);
    assert_eq!(summary.behind_base, 55, "the count is the whole set");
    assert_eq!(summary.missing_commits.len(), 50, "the listing is cut");
    assert_eq!(
        summary.missing_commits[0].revision,
        tip_of(&fx.repo(), &stream.base_branch).to_string(),
        "newest first",
    );
}

// WKS-FR-XDBM: the recorded base branch is the source even where the project's
// active worktree stands on another branch, and the base worktree's own files
// are not touched.
#[test]
fn an_update_takes_its_source_from_the_recorded_base_branch_alone() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch);
    let tracked = |root: &Path| -> Vec<String> {
        files_under(root)
            .into_iter()
            .filter(|name| !name.starts_with(".git/"))
            .collect()
    };
    let base_files = tracked(&fx.root());

    // The author moves the active worktree onto a branch of their own, which
    // holds nothing the stream needs.
    let repo = fx.repo();
    let elsewhere = repo.find_commit(pinned).unwrap();
    repo.branch("author-branch", &elsewhere, true).unwrap();

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
    assert_eq!(record.base_branch, stream.base_branch);
    assert_eq!(
        tracked(&fx.root()),
        base_files,
        "the base branch's own worktree is not written",
    );
}

// GRB-FR-RMKD: a replay that needed a turn still lands every stream commit as
// its own single-parent commit, in the order the stream held them.
#[test]
fn a_reconciled_rebase_lands_every_stream_commit_in_order() {
    let fx = Fixture::new();
    let stream = fx.create("three of them", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    let worktree_path = Path::new(&stream.worktree_path);
    for step in 1..=3 {
        std::fs::write(
            worktree_path.join("README.md"),
            format!("{STREAM_SIDE}-{step}\n"),
        )
        .unwrap();
        commit_all(&worktree, &format!("stream edit {step}"));
    }
    std::fs::write(fx.root().join("README.md"), format!("{BASE_SIDE}\n")).unwrap();
    commit_all(&fx.repo(), "base edit");
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let settled = format!("{BASE_SIDE}-and-{STREAM_SIDE}\n");
    let dispatch = ScriptedDispatch::new(vec![Script::Settle(vec![(
        "README.md".to_string(),
        settled.clone(),
    )])]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned.to_string(),
        Vec::new(),
        dispatch,
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");

    let repo = fx.repo();
    let mut walk: Vec<(String, usize, String)> = Vec::new();
    let mut at = tip_of(&repo, &stream.branch);
    while at != pinned {
        let commit = repo.find_commit(at).unwrap();
        let blob = commit
            .tree()
            .unwrap()
            .get_path(Path::new("README.md"))
            .unwrap()
            .id();
        walk.push((
            commit.summary().ok().flatten().unwrap_or("").to_string(),
            commit.parent_count(),
            String::from_utf8_lossy(repo.find_blob(blob).unwrap().content()).into_owned(),
        ));
        at = commit.parent_id(0).unwrap();
    }
    walk.reverse();

    assert_eq!(
        walk.iter().map(|(s, _, _)| s.as_str()).collect::<Vec<_>>(),
        ["stream edit 1", "stream edit 2", "stream edit 3"],
        "every stream commit is replayed, in order",
    );
    assert!(walk.iter().all(|(_, parents, _)| *parents == 1), "no merge commit");
    // GRB-FR-WGPS: the one pass reached every step that asked about the path,
    // so no commit carries a side the application chose for the author.
    assert!(
        walk.iter().all(|(_, _, content)| content == &settled),
        "every replayed commit carries what the one pass settled: {walk:?}",
    );
}

// GRB-FR-CLRO: three semantic turns, and no more. A request whose turns ran and
// settled nothing rests `conflicted`; one where no turn ever ran rests `failed`.
#[test]
fn an_update_spends_three_turns_at_most_and_tells_the_two_endings_apart() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();

    let dispatch = ScriptedDispatch::new(vec![
        Script::ClaimSuccessAndChangeNothing,
        Script::ClaimSuccessAndChangeNothing,
        Script::ClaimSuccessAndChangeNothing,
        Script::ClaimSuccessAndChangeNothing,
    ]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        dispatch.clone(),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Conflicted);
    assert_eq!(dispatch.turns(), 3, "the bound is three turns for one request");

    // A request whose turns could never be launched is a different answer.
    let unlaunchable = ScriptedDispatch::new(vec![
        Script::Unlaunchable,
        Script::Unlaunchable,
        Script::Unlaunchable,
    ]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        unlaunchable,
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Failed, "{record:?}");
    assert_eq!(record.failure, ERR_UPDATE_ATTEMPTS_EXHAUSTED);
}

// WKS-FR-DPNM / WKS-FR-AMWE: a retry runs under the recorded strategy and the
// recorded pinned revision, and a settled update permits no retry at all.
#[test]
fn a_retry_runs_under_the_recorded_strategy_and_pinned_revision() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(vec![Script::ClaimSuccessAndChangeNothing]),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Conflicted);

    let started = update_commands::retry_work_stream_update(fx.app.clone(), stream.id.clone())
        .expect("a conflicted update runs again");
    assert_eq!(started.state, StreamUpdateState::Running);
    assert_eq!(started.strategy, StreamUpdateStrategy::RebaseSource);
    assert_eq!(started.base_revision, pinned);

}

// WKS-FR-VQRD / WKS-FR-DPNM: a settled update permits no retry at all.
#[test]
fn a_settled_update_permits_no_retry() {
    let fx = Fixture::new();
    let stream = fx.create("already current", None).expect("created");
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();

    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(Vec::new()),
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::NothingToUpdate, "{record:?}");

    assert_eq!(
        update_commands::retry_work_stream_update(fx.app.clone(), stream.id.clone())
            .expect_err("a settled update is not retryable"),
        ERR_UPDATE_STATE_NOT_PERMITTED,
    );
}

// WKS-FR-CBXW: an answer set that does not cover every recorded position, or
// that holds a blank answer, records nothing and starts nothing.
#[test]
fn an_answer_set_that_does_not_cover_every_question_starts_nothing() {
    let fx = Fixture::new();
    let stream = conflicting_stream(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();
    update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::MergeSource,
        &pinned,
        Vec::new(),
        ScriptedDispatch::new(vec![Script::Escalate]),
    )
    .expect("started");

    let refused = |answers: Vec<crate::graduation::GraduationEscalationAnswer>| {
        update_commands::answer_work_stream_update_escalation(
            fx.app.clone(),
            stream.id.clone(),
            answers,
        )
        .expect_err("refused")
    };
    let answer = |position, text: &str| crate::graduation::GraduationEscalationAnswer {
        position,
        answer: text.to_string(),
        summary: String::new(),
    };

    refused(Vec::new());
    refused(vec![answer(1, "  ")]);
    refused(vec![answer(1, "ok"), answer(2, "ok")]);
    refused(vec![answer(7, "ok")]);

    let still = update_of(&fx, &stream.id).expect("a record");
    assert_eq!(still.state, StreamUpdateState::Escalated, "nothing moved it");
    assert!(still.escalation.is_some(), "the question set stands");
    assert!(still.decisions.is_empty(), "and nothing was recorded");
}

// WKS-FR-QFTH: an update this process really has a job behind is the one update
// a sweep must leave alone.
#[test]
fn a_running_update_with_a_live_job_survives_a_listing() {
    let fx = Fixture::new();
    let stream = fx.create("live", None).expect("created");
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
    let state = fx.app.state::<StreamState>();
    assert!(state.claim_update_job(&stream.id), "this process holds the job");

    let summary = summary_of(&fx, &stream.id);
    assert_eq!(
        summary.update.map(|record| record.state),
        Some(StreamUpdateState::Running),
        "a live update is not called stranded",
    );
    state.release_update_job(&stream.id);
}

// WKS-FR-MJEB: the command answers at once with a `running` record.
#[test]
fn the_update_command_answers_at_once_with_a_running_record() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let pinned = tip_of(&fx.repo(), &stream.base_branch).to_string();

    let record = update_commands::update_work_stream(
        fx.app.clone(),
        stream.id.clone(),
        StreamUpdateStrategy::MergeSource,
        pinned.clone(),
    )
    .expect("started");

    assert_eq!(record.state, StreamUpdateState::Running);
    assert_eq!(record.base_revision, pinned);
    // WKS-FR-AMWE: and the commits the stream was missing are on the record.
    assert_eq!(record.missing_commits.len(), 1);
}

// GRB-FR-RMKD: a conflicting path that is executable stays executable through
// the replay. A rebase that rewrote a hook or a script as a plain file would
// break the tree it landed.
#[test]
fn a_settled_path_keeps_the_mode_it_was_settled_at() {
    let fx = Fixture::new();
    let stream = fx.create("executable", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    let worktree_path = Path::new(&stream.worktree_path);

    // The path is executable on both sides, and both sides change it.
    let script = |body: &str| format!("#!/bin/sh\n{body}\n");
    let write_executable = |root: &Path, body: &str| {
        let target = root.join("run.sh");
        std::fs::write(&target, script(body)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    };
    write_executable(worktree_path, "seed");
    commit_all(&worktree, "the script");
    // The base branch takes the same commit, so both sides start from it.
    write_executable(&fx.root(), "seed");
    commit_all(&fx.repo(), "the script on the base");
    write_executable(worktree_path, STREAM_SIDE);
    commit_all(&worktree, "stream edit");
    write_executable(&fx.root(), BASE_SIDE);
    commit_all(&fx.repo(), "base edit");
    let pinned = tip_of(&fx.repo(), &stream.base_branch);

    let dispatch = ScriptedDispatch::new(vec![Script::Settle(vec![(
        "run.sh".to_string(),
        script("settled"),
    )])]);
    let record = update_job_scripted(
        &fx,
        &stream,
        StreamUpdateStrategy::RebaseSource,
        &pinned.to_string(),
        Vec::new(),
        dispatch,
    )
    .expect("started");
    assert_eq!(record.state, StreamUpdateState::Updated, "{record:?}");

    let repo = fx.repo();
    let mode = repo
        .find_commit(tip_of(&repo, &stream.branch))
        .unwrap()
        .tree()
        .unwrap()
        .get_path(Path::new("run.sh"))
        .unwrap()
        .filemode();
    assert_eq!(record.semantic_turns, 1, "a turn settled the script: {record:?}");
    assert_eq!(mode, 0o100755, "the settled path is still executable");
}
