//! Landings, and the completed runs of a branch (`GRD-graduation.md`
//! GRD-FR-PXQN, GRD-FR-UXXF, GRD-FR-BHFP, GRD-FR-RVXK, GRD-FR-MACY, GRD-FR-MOWZ;
//! `WKS-work-streams.md` WKS-FR-JXPV, WKS-FR-WUUH).

use super::merge_handoff::{commit_file, tip};
use super::pr_summary_turn::commit_on_branch;
use super::run_summary::ready_with;
use super::*;
use crate::streams::{StreamMergePublication, StreamMergeResult};

/// A completed stream run that committed `path`, with a run summary titled
/// `title` where one is given.
fn completed_stream_run(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    path: &str,
    title: Option<&str>,
) -> GraduationRun {
    let run = fx.enqueue(stream, &format!("Write {path}."));
    let review = match title {
        Some(title) => ready_with(serde_json::json!({ "title": title, "description": "d" })),
        None => Turn::ready(),
    };
    let done =
        fx.drive(&run, ScriptedDispatch::new(vec![Turn::work().writing(path, "1\n"), review]));
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    done
}

/// Merge a stream into its base branch, with no queue start behind it.
fn merge(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    publication: StreamMergePublication,
) -> StreamMergeResult {
    super::merge_handoff::request(fx, stream, publication).expect("the check settled")
}

fn commit_publication() -> StreamMergePublication {
    StreamMergePublication::Commit { message: "Merge the stream".to_string() }
}

/// Every file the run store holds, with its bytes.
fn store_files(fx: &Fixture) -> BTreeMap<String, String> {
    tree_under(fx.store.path())
}

/// A completed run record with one landing, for the pure listing.
fn landed(id: &str, branch: &str, commit: git2::Oid, at: &str) -> GraduationRun {
    let mut run = GraduationRun::new_for_test(id, &format!("draft-{id}"), at);
    run.state = GraduationRunState::Completed;
    run.commits = vec![commit.to_string()];
    run.landings = vec![Landing {
        branch: branch.to_string(),
        commit: commit.to_string(),
        at: at.to_string(),
    }];
    run
}

// GRD-FR-PXQN: a direct run records one landing when it completes: its pinned
// branch, its last commit, and the time. The wire shape is camelCase.
#[test]
fn a_completed_direct_run_records_a_landing_on_its_pinned_branch() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );

    assert_eq!(done.state, GraduationRunState::Completed);
    let branch = fx.branch();
    assert_eq!(done.landings.len(), 1, "{:?}", done.landings);
    let landing = &done.landings[0];
    assert_eq!(landing.branch, branch);
    assert_eq!(Some(&landing.commit), done.commits.last());
    assert_eq!(landing.commit, tip(&fx.repo(), &branch).to_string());
    assert!(!landing.at.is_empty());
    let wire = serde_json::to_value(&done).unwrap();
    assert_eq!(wire["landings"][0]["branch"], branch);
    assert_eq!(wire["landings"][0]["commit"], landing.commit);
}

// GRD-FR-PXQN: a direct run that made no commit records no landing, and a stream
// run records none when it completes: its landing comes with the merge.
#[test]
fn a_run_with_no_commit_and_a_stream_run_record_no_landing_at_completion() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let draft = fx.committed_draft("direct");
    // On the stream's working copy, where the draft's own statistics are not
    // written, a turn that writes nothing makes no commit.
    let run =
        fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "Change nothing.", &draft);
    let done = fx.drive(&run, ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(done.commits.is_empty(), "{:?}", done.checkpoint.changed_paths);
    assert!(done.landings.is_empty());

    let done = completed_stream_run(&fx, &stream, "src/a.ts", None);
    assert_eq!(done.commits.len(), 1);
    assert!(done.landings.is_empty());
}

// GRD-FR-UXXF, WKS-FR-JXPV: a `merged` result with a commit makes each completed
// draft run of the stream record a landing on the base branch at the merge
// commit, a direct run on the stream's working copy included. A run that made
// no commit, and a run of another stream, record none.
#[test]
fn a_committed_git_merge_lands_the_streams_completed_runs_on_the_base_branch() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let other = fx.stream("other");
    let first = completed_stream_run(&fx, &stream, "src/a.ts", None);
    let draft = fx.committed_draft("direct");
    let direct =
        fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "Write b.", &draft);
    let direct = fx.drive(
        &direct,
        ScriptedDispatch::new(vec![Turn::work().writing("src/b.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(direct.state, GraduationRunState::Completed, "{direct:?}");
    let empty = fx.drive(
        &fx.enqueue(&stream, "Change nothing."),
        ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]),
    );
    let elsewhere = completed_stream_run(&fx, &other, "src/c.ts", None);
    // The direct run counted its draft's statistics in the project's worktree,
    // which a merge refuses to write over.
    commit_file(&fx.repo(), "README.md", "seed\n", "draft statistics");

    let commit = match merge(&fx, &stream, commit_publication()) {
        StreamMergeResult::Merged { commit: Some(commit), .. } => commit,
        other => panic!("expected a committed merge, got {other:?}"),
    };

    let landed = fx.reload(&first.id);
    assert_eq!(landed.landings.len(), 1);
    assert_eq!(landed.landing_on(&stream.base_branch).expect("a landing").commit, commit);
    // The direct run keeps its landing on the stream branch, and adds one on the
    // base branch.
    let direct = fx.reload(&direct.id);
    assert_eq!(direct.landings.len(), 2, "{:?}", direct.landings);
    assert_eq!(direct.landings[0].branch, stream.branch);
    assert_eq!(direct.landing_on(&stream.base_branch).expect("a landing").commit, commit);
    assert!(fx.reload(&empty.id).landings.is_empty(), "no commit, no landing");
    assert!(fx.reload(&elsewhere.id).landings.is_empty(), "another stream");
}

// GRD-FR-UXXF: an `uncommitted` merge records the base tip at the merge. A second
// merge keeps the landing a run already has on the base branch, and lands the
// run that completed after the first merge.
#[test]
fn an_uncommitted_merge_records_the_base_tip_and_a_second_merge_keeps_earlier_landings() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let first = completed_stream_run(&fx, &stream, "src/a.ts", None);
    let base_tip = tip(&fx.repo(), &stream.base_branch).to_string();

    let result = merge(&fx, &stream, StreamMergePublication::Uncommitted);
    assert!(matches!(result, StreamMergeResult::Merged { commit: None, .. }), "{result:?}");
    let landing = fx.reload(&first.id).landing_on(&stream.base_branch).cloned().expect("landed");
    assert_eq!(landing.commit, base_tip);

    // The author commits the unstaged result, and the stream goes on.
    commit_file(&fx.repo(), "README.md", "seed\n", "take the merge");
    let later = completed_stream_run(&fx, &stream, "src/b.ts", None);
    let commit = match merge(&fx, &stream, commit_publication()) {
        StreamMergeResult::Merged { commit: Some(commit), .. } => commit,
        other => panic!("expected a committed merge, got {other:?}"),
    };

    let first = fx.reload(&first.id);
    assert_eq!(first.landings, vec![landing], "the earlier landing is kept, and only it");
    assert_eq!(fx.reload(&later.id).landing_on(&stream.base_branch).unwrap().commit, commit);
}

// GRD-FR-UXXF: a stream merge that a merge run applies lands the stream's
// completed draft runs on the base branch at the merge commit. The merge run
// itself records no landing, and the landings are recorded once only.
#[test]
fn a_merge_run_apply_lands_the_streams_completed_runs() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("feature");
    let work = completed_stream_run(&fx, &stream, "src/a.ts", None);
    let stream_repo = git2::Repository::open(stream.worktree()).expect("the stream's repo");
    commit_file(&stream_repo, "README.md", "stream side\n", "stream edit");
    commit_file(&fx.repo(), "README.md", "base side\n", "base edit");
    let run_id = match merge(&fx, &stream, commit_publication()) {
        StreamMergeResult::Conflicted { run_id, .. } => run_id,
        other => panic!("expected a handoff, got {other:?}"),
    };
    assert!(fx.reload(&work.id).landings.is_empty(), "nothing has landed yet");

    let merge_run = fx.drive(
        &fx.reload(&run_id),
        ScriptedDispatch::new(vec![
            Turn::work().writing("README.md", "both sides\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(merge_run.state, GraduationRunState::Completed, "{merge_run:?}");
    assert!(merge_run.landings.is_empty());
    let merge_commit = merge_run.merge.as_ref().unwrap().result.as_ref().unwrap().commit.clone();
    let landing = fx.reload(&work.id).landing_on(&stream.base_branch).cloned().expect("landed");
    assert_eq!(Some(landing.commit.clone()), merge_commit);
    // A resumed apply that records the landings again changes nothing.
    assert_eq!(
        record_stream_landings(&fx.app, &stream.id, &stream.base_branch, "0".repeat(40).as_str()),
        0
    );
    assert_eq!(fx.reload(&work.id).landings, vec![landing]);
}

// GRD-FR-UXXF: an `uncommitted` merge run apply lands the runs at the pinned base
// tip.
#[test]
fn an_uncommitted_merge_run_apply_lands_the_runs_at_the_base_tip() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("feature");
    let work = completed_stream_run(&fx, &stream, "src/a.ts", None);
    let stream_repo = git2::Repository::open(stream.worktree()).expect("the stream's repo");
    commit_file(&stream_repo, "README.md", "stream side\n", "stream edit");
    commit_file(&fx.repo(), "README.md", "base side\n", "base edit");
    let run_id = match merge(&fx, &stream, StreamMergePublication::Uncommitted) {
        StreamMergeResult::Conflicted { run_id, .. } => run_id,
        other => panic!("expected a handoff, got {other:?}"),
    };
    let pinned = fx.reload(&run_id).merge.unwrap().base_tip;

    let merge_run = fx.drive(
        &fx.reload(&run_id),
        ScriptedDispatch::new(vec![
            Turn::work().writing("README.md", "both sides\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(merge_run.state, GraduationRunState::Completed, "{merge_run:?}");
    let landing = fx.reload(&work.id).landing_on(&stream.base_branch).cloned().expect("landed");
    assert_eq!(landing.commit, pinned);
}

/// A stream with one completed draft run, whose merge Git cannot settle,
/// handed to a merge run on `publication`.
fn conflicted_handoff(
    fx: &Fixture,
    publication: StreamMergePublication,
) -> (crate::streams::WorkStream, GraduationRun, String) {
    fx.allow_execution();
    let stream = fx.stream("feature");
    let work = completed_stream_run(fx, &stream, "src/a.ts", None);
    let stream_repo = git2::Repository::open(stream.worktree()).expect("the stream's repo");
    commit_file(&stream_repo, "README.md", "stream side\n", "stream edit");
    commit_file(&fx.repo(), "README.md", "base side\n", "base edit");
    let run_id = match merge(fx, &stream, publication) {
        StreamMergeResult::Conflicted { run_id, .. } => run_id,
        other => panic!("expected a handoff, got {other:?}"),
    };
    (stream, work, run_id)
}

/// Record on a merge run that its merge was applied, as a loop that stopped
/// between the apply and the completion leaves it.
fn applied_before_a_stop(fx: &Fixture, run_id: &str, pull: Option<PullMergeSubject>) {
    let mut run = fx.reload(run_id);
    let data = run.merge.as_mut().expect("merge data");
    data.result = Some(GraduationMergeResult {
        published: MergePublished::Uncommitted,
        commit: None,
        merged_paths: Vec::new(),
    });
    data.pull = pull;
    save_run(&fx.app, &mut run).expect("saved");
}

// GRD-FR-UXXF: a merge run whose loop stopped after the apply was recorded, and
// before the landings, records them when it is resumed: at the pinned base tip
// for an `uncommitted` publication.
#[test]
fn a_resumed_merge_run_that_was_applied_records_the_landings() {
    let fx = Fixture::new();
    let (stream, work, run_id) = conflicted_handoff(&fx, StreamMergePublication::Uncommitted);
    applied_before_a_stop(&fx, &run_id, None);
    let pinned = fx.reload(&run_id).merge.unwrap().base_tip;

    let merge_run = fx.drive(&fx.reload(&run_id), ScriptedDispatch::new(Vec::new()));

    assert_eq!(merge_run.state, GraduationRunState::Completed, "{merge_run:?}");
    let landing = fx.reload(&work.id).landing_on(&stream.base_branch).cloned().expect("landed");
    assert_eq!(landing.commit, pinned);
}

// GRD-FR-UXXF: a pull run is not the merge of a stream, so its apply lands no
// run of the stream.
#[test]
fn a_pull_run_records_no_landing() {
    let fx = Fixture::new();
    let (stream, work, run_id) = conflicted_handoff(&fx, StreamMergePublication::Uncommitted);
    let pull = PullMergeSubject {
        branch: stream.base_branch.clone(),
        upstream: "origin/main".to_string(),
        worktree_path: fx.root().to_string_lossy().into_owned(),
        strategy: crate::git::PullStrategy::Merge,
    };
    applied_before_a_stop(&fx, &run_id, Some(pull));

    let pull_run = fx.drive(&fx.reload(&run_id), ScriptedDispatch::new(Vec::new()));

    assert_eq!(pull_run.state, GraduationRunState::Completed, "{pull_run:?}");
    assert!(fx.reload(&work.id).landings.is_empty());
    assert!(branch_completed_runs_blocking(&fx.app, &stream.base_branch).unwrap().is_empty());
}

// GRD-FR-BHFP: the completed runs of a branch are the runs with a landing on it
// whose commit the tip reaches, oldest landing first, with their kind, their
// draft and their run summary. A landing on another branch, a landing the tip
// does not reach, and a run that is not completed are not listed.
#[test]
fn the_listing_keeps_reached_landings_in_landing_order() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let first = commit_on_branch(&repo, "feature/x", "a.txt", "a\n");
    let second = commit_on_branch(&repo, "feature/x", "b.txt", "b\n");
    let unreached = commit_on_branch(&repo, "feature/y", "c.txt", "c\n");
    let tip = tip(&repo, "feature/x");

    let mut direct = landed("g-direct", "feature/x", second, "2026-11-06T10:00:02Z");
    direct.direct_target = Some(DirectTarget {
        worktree_path: "/w".to_string(),
        worktree_name: "w".to_string(),
        branch: "feature/x".to_string(),
    });
    let mut summarized = landed("g-early", "feature/x", first, "2026-11-06T10:00:01Z");
    summarized.run_summary =
        Some(RunSummary { title: "Early".to_string(), description: "d".to_string() });
    let tie = landed("g-tie", "feature/x", first, "2026-11-06T10:00:02Z");
    let elsewhere = landed("g-other-branch", "feature/y", unreached, "2026-11-06T10:00:00Z");
    let not_reached = landed("g-unreached", "feature/x", unreached, "2026-11-06T10:00:00Z");
    let mut discarded = landed("g-discarded", "feature/x", first, "2026-11-06T10:00:00Z");
    discarded.state = GraduationRunState::Discarded;
    // A run that is not a draft run is never listed, whatever it carries.
    let mut summary_run = landed("g-pr", "feature/x", first, "2026-11-06T10:00:00Z");
    summary_run.pr_summary = Some(PrSummaryData {
        name: "Summarize feature/x".to_string(),
        head: "feature/x".to_string(),
        base: "main".to_string(),
        mode: PrSummaryMode::Diff,
        head_tip: None,
        merge_base: None,
        inputs: Vec::new(),
        result: None,
    });
    // A landing commit that is no object id, or no object, is not reached.
    let mut garbled = landed("g-garbled", "feature/x", first, "2026-11-06T10:00:00Z");
    garbled.landings[0].commit = "not an object id".to_string();
    let mut missing = landed("g-missing", "feature/x", first, "2026-11-06T10:00:00Z");
    missing.landings[0].commit = "1".repeat(40);

    let listed = crate::graduation::landings::completed_runs_on(
        &repo,
        tip,
        "feature/x",
        vec![
            direct,
            summarized,
            tie,
            elsewhere,
            not_reached,
            discarded,
            summary_run,
            garbled,
            missing,
        ],
    );

    let ids: Vec<&str> = listed.iter().map(|run| run.run_id.as_str()).collect();
    assert_eq!(ids, vec!["g-early", "g-direct", "g-tie"], "oldest first, ties in run order");
    assert_eq!(listed[0].kind, CompletedRunKind::Draft);
    assert_eq!(listed[0].title, "draft-g-early");
    assert_eq!(listed[0].draft_id, "draft-g-early");
    assert_eq!(listed[0].run_summary.as_ref().unwrap().title, "Early");
    assert_eq!(listed[1].kind, CompletedRunKind::Direct);
    assert!(listed[1].run_summary.is_none());
    let wire = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(wire["runId"], "g-early");
    assert_eq!(wire["draftName"], "draft-g-early");
    assert_eq!(wire["kind"], "draft");
    assert_eq!(wire["landing"]["branch"], "feature/x");
    assert_eq!(wire["runSummary"]["title"], "Early");
}

// GRD-FR-BHFP: the command lists a direct run's landing while the branch tip
// reaches it, and drops it once the branch is moved back past it. It writes
// nothing to the run store. A branch that does not exist returns
// `unknown branch`.
#[test]
fn the_command_follows_the_branch_tip_writes_nothing_and_refuses_an_unknown_branch() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    let before_run = tip(&fx.repo(), &fx.branch());
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    let branch = fx.branch();

    let store_before = store_files(&fx);
    let listed = branch_completed_runs_blocking(&fx.app, &branch).expect("listed");
    assert_eq!(store_files(&fx), store_before, "the read writes nothing");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].run_id, done.id);
    assert_eq!(listed[0].kind, CompletedRunKind::Direct);
    assert_eq!(listed[0].title, "editor draft");

    // The branch is reset to before the run's commit: the tip reaches it no more.
    let repo = fx.repo();
    let commit = repo.find_commit(before_run).unwrap();
    repo.reset(commit.as_object(), git2::ResetType::Hard, None).unwrap();
    assert!(branch_completed_runs_blocking(&fx.app, &branch).expect("listed").is_empty());

    assert_eq!(
        branch_completed_runs_blocking(&fx.app, "feature/none").unwrap_err(),
        crate::changes::ERR_UNKNOWN_BRANCH,
    );
}

// GRD-FR-BHFP, GRD-FR-RVXK: a run with a revert commit that the tip reaches is
// not listed. A revert commit that the tip does not reach leaves the run listed.
#[test]
fn a_reached_revert_takes_a_run_off_the_listing() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let work = commit_on_branch(&repo, "feature/x", "a.txt", "a\n");
    let other_revert = commit_on_branch(&repo, "feature/y", "b.txt", "b\n");
    let revert = commit_on_branch(&repo, "feature/x", "a.txt", "undone\n");
    let tip = tip(&repo, "feature/x");

    let mut reverted = landed("g-reverted", "feature/x", work, "2026-11-06T10:00:00Z");
    reverted.reverts = vec![Landing {
        branch: "feature/x".to_string(),
        commit: revert.to_string(),
        at: "2026-11-06T10:00:05Z".to_string(),
    }];
    let mut reverted_elsewhere = landed("g-kept", "feature/x", work, "2026-11-06T10:00:01Z");
    reverted_elsewhere.reverts = vec![Landing {
        branch: "feature/y".to_string(),
        commit: other_revert.to_string(),
        at: "2026-11-06T10:00:05Z".to_string(),
    }];

    let listed = crate::graduation::landings::completed_runs_on(
        &repo,
        tip,
        "feature/x",
        vec![reverted, reverted_elsewhere],
    );

    let ids: Vec<&str> = listed.iter().map(|run| run.run_id.as_str()).collect();
    assert_eq!(ids, vec!["g-kept"]);
}

// GRD-FR-RVXK, GRD-FR-BHFP: the revert of a completed direct run records the
// branch and the last revert commit on the run, and the run leaves the listing
// of that branch. The run stays `completed`.
#[test]
fn a_reverted_direct_run_records_its_revert_and_leaves_the_listing() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    let branch = fx.branch();
    assert_eq!(branch_completed_runs_blocking(&fx.app, &branch).expect("listed").len(), 1);

    let reverted = crate::graduation::revert_graduation_run(fx.app.clone(), done.id.clone())
        .expect("reverted");

    assert_eq!(reverted.state, GraduationRunState::Completed);
    assert_eq!(reverted.reverts.len(), 1);
    assert_eq!(reverted.reverts[0].branch, branch);
    assert_eq!(reverted.reverts[0].commit, tip(&fx.repo(), &branch).to_string());
    let stored = runs::load_run(&fx.app, &done.id).expect("stored");
    assert_eq!(stored.reverts, reverted.reverts, "the revert is durable");
    assert!(branch_completed_runs_blocking(&fx.app, &branch).expect("listed").is_empty());
}

// GRD-FR-MACY: a `combine` run captures the run summaries of the runs whose work
// is on the head branch, in landing order, and skips each run with no summary.
#[test]
fn a_combine_run_captures_the_summaries_of_the_runs_landed_on_its_head() {
    let fx = Fixture::new();
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let stream = fx.stream("editor");
    let first = completed_stream_run(&fx, &stream, "src/a.ts", Some("First change"));
    completed_stream_run(&fx, &stream, "src/b.ts", None);
    let third = completed_stream_run(&fx, &stream, "src/c.ts", Some("Third change"));
    merge(&fx, &stream, commit_publication());
    fx.other_branch("release");

    let summaries = completed_run_summaries(&fx.app, &stream.base_branch);
    let ids: Vec<&str> = summaries.iter().map(|input| input.run_id.as_str()).collect();
    assert_eq!(ids, vec![first.id.as_str(), third.id.as_str()]);
    assert_eq!(summaries[0].title, "First change");
    assert_eq!(summaries[0].description, "d");

    let run = crate::graduation::start_pull_request_summary(
        fx.app.clone(),
        stream.base_branch.clone(),
        "release".to_string(),
        PrSummaryMode::Combine,
    )
    .expect("started");
    assert_eq!(run.pr_summary.as_ref().unwrap().inputs, summaries);
}

// GRD-FR-MACY: a head whose landed runs carry no summary is refused with
// `no_run_summaries`, and nothing is enqueued.
#[test]
fn a_combine_run_over_landed_runs_with_no_summary_is_refused() {
    let fx = Fixture::new();
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let stream = fx.stream("editor");
    completed_stream_run(&fx, &stream, "src/a.ts", None);
    merge(&fx, &stream, commit_publication());
    fx.other_branch("release");
    // The run landed on the head, and carries no summary.
    let landed = branch_completed_runs_blocking(&fx.app, &stream.base_branch).unwrap();
    assert_eq!(landed.len(), 1);
    assert!(landed[0].run_summary.is_none());
    let runs_before = project_queue(&fx.app).map(|queue| queue.runs.len()).unwrap_or(0);

    let refused = crate::graduation::start_pull_request_summary(
        fx.app.clone(),
        stream.base_branch.clone(),
        "release".to_string(),
        PrSummaryMode::Combine,
    )
    .unwrap_err();

    assert_eq!(refused, ERR_NO_RUN_SUMMARIES);
    assert_eq!(project_queue(&fx.app).map(|queue| queue.runs.len()).unwrap_or(0), runs_before);
}

// GRD-FR-MOWZ, WKS-FR-WUUH: the merge request ran the check of the base branch.
// The merge run that it handed the merge to applies it with no further check,
// also after a relaunch, and the stream's runs land on the base branch. A new
// merge request on the unconfirmed base branch is refused and lands nothing.
#[test]
fn a_merge_run_applies_with_no_further_check_after_a_relaunch() {
    let fx = Fixture::new();
    let (stream, work, run_id) = conflicted_handoff(&fx, commit_publication());
    // The session after a relaunch holds no confirmation.
    fx.app.manage(crate::git::write_guard::commands::WriteGuardState::offline());

    let merge_run = fx.drive(
        &fx.reload(&run_id),
        ScriptedDispatch::new(vec![
            Turn::work().writing("README.md", "both sides\n"),
            Turn::ready(),
        ]),
    );

    assert_eq!(merge_run.state, GraduationRunState::Completed, "{merge_run:?}");
    let merge_commit = merge_run.merge.as_ref().unwrap().result.as_ref().unwrap().commit.clone();
    let landing = fx.reload(&work.id).landing_on(&stream.base_branch).cloned().expect("landed");
    assert_eq!(Some(landing.commit), merge_commit);
    assert_eq!(merge_commit, Some(tip(&fx.repo(), &stream.base_branch).to_string()));

    let later = completed_stream_run(&fx, &stream, "src/b.ts", None);
    let refused = super::merge_handoff::request(&fx, &stream, commit_publication()).unwrap_err();
    assert!(refused.starts_with(crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED), "{refused}");
    assert!(fx.reload(&later.id).landings.is_empty(), "a refused merge lands nothing");
}

// GRD-FR-RVXK, GRD-FR-BHFP, GRD-FR-MACY: a stream run that the author reverted
// in the stream lands with the merge, and its revert keeps it off the listing
// of the base branch and out of the inputs of a `combine` run.
#[test]
fn a_run_reverted_in_its_stream_stays_off_the_base_branch_listing_and_the_combine() {
    let fx = Fixture::new();
    fx.app.manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let stream = fx.stream("editor");
    let kept = completed_stream_run(&fx, &stream, "src/a.ts", Some("Kept change"));
    let undone = completed_stream_run(&fx, &stream, "src/b.ts", Some("Undone change"));

    let reverted = crate::graduation::revert_graduation_run(fx.app.clone(), undone.id.clone())
        .expect("reverted in the stream");
    assert_eq!(reverted.reverts[0].branch, stream.branch);
    merge(&fx, &stream, commit_publication());
    fx.other_branch("release");

    let listed = branch_completed_runs_blocking(&fx.app, &stream.base_branch).unwrap();
    let ids: Vec<&str> = listed.iter().map(|run| run.run_id.as_str()).collect();
    assert_eq!(ids, vec![kept.id.as_str()], "the reverted run is not listed");
    let run = crate::graduation::start_pull_request_summary(
        fx.app.clone(),
        stream.base_branch.clone(),
        "release".to_string(),
        PrSummaryMode::Combine,
    )
    .expect("started");
    let inputs = &run.pr_summary.as_ref().unwrap().inputs;
    let ids: Vec<&str> = inputs.iter().map(|input| input.run_id.as_str()).collect();
    assert_eq!(ids, vec![kept.id.as_str()]);
    assert_eq!(inputs[0].title, "Kept change");
}
