//! The pull reconcile (`GTC-git.md` GTC-FR-NJUY .. GTC-FR-TRWC) and the pull run
//! it hands off (`GRD-graduation.md` GRD-FR-PLSB, GRD-FR-PQKY).

use tauri::Listener;

use super::pull_support::*;
use super::*;
use crate::git::{
    PullStrategy, ERR_PULL_RUN_ACTIVE, ERR_PULL_TIPS_MOVED, ERR_PULL_WOULD_OVERWRITE,
};

/// A fixture whose branch holds `a.md` and tracks `origin`, with one commit
/// of its own and one the upstream holds. Returns `(local, remote)`.
fn diverged(
    fx: &Fixture,
    local: &[(&str, &str)],
    remote: &[(&str, &str)],
) -> (git2::Oid, git2::Oid) {
    local_commit(fx, &[("a.md", "one\ntwo\nthree\n")], "base");
    track_origin(fx);
    let local = local_commit(fx, local, "local work");
    let remote = upstream_commit(fx, remote, "upstream work");
    (local, remote)
}

fn reconcile(
    fx: &Fixture,
    strategy: PullStrategy,
    local: git2::Oid,
    remote: git2::Oid,
) -> crate::git::PullReconcileOutcome {
    reconcile_pull_blocking(&fx.app, strategy, &local.to_string(), &remote.to_string())
}

// GTC-FR-SBLF: a merge Git settles lands one merge commit whose parents are the
// local tip and then the upstream tip, with the message that names both.
#[test]
fn a_merge_git_settles_lands_one_merge_commit() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);

    let outcome = reconcile(&fx, PullStrategy::Merge, local, remote);

    assert_eq!(outcome.state, crate::git::RECONCILE_MERGED, "{outcome:?}");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(outcome.commit, Some(head.id().to_string()));
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![local, remote]);
    let branch = branch_of(&fx);
    assert_eq!(head.message().ok(), Some(format!("Merge origin/{branch} into {branch}").as_str()));
    assert_eq!(read(&fx, "b.md"), "remote\n");
    assert_eq!(read(&fx, "c.md"), "local\n");
    assert_eq!(outcome.updated_paths, vec!["b.md".to_string()]);
    assert!(crate::streams::checked_out_uncommitted_paths_of(&fx.root()).is_empty());
}

// GTC-FR-SBLF: a rebase Git settles replays the local commit onto the upstream
// tip with its own author and message, and the history is linear.
#[test]
fn a_rebase_git_settles_replays_the_local_commits_onto_the_upstream() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let repo = fx.repo();
    let local_commit = repo.find_commit(local).unwrap();
    let author = local_commit.author().name().ok().map(str::to_string);
    let message = local_commit.message().ok().map(str::to_string);

    let outcome = reconcile(&fx, PullStrategy::Rebase, local, remote);

    assert_eq!(outcome.state, crate::git::RECONCILE_REBASED, "{outcome:?}");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![remote]);
    assert_eq!(head.author().name().ok().map(str::to_string), author);
    assert_eq!(head.message().ok().map(str::to_string), message);
    assert_eq!(read(&fx, "b.md"), "remote\n");
    assert_eq!(read(&fx, "c.md"), "local\n");
}

// GTC-FR-SBLF, GRB-FR-EPYG: a replay step that conflicts but whose tips merge
// cleanly is answered by that merge, and the rebase still lands.
#[test]
fn a_rebase_the_tip_merge_settles_still_lands() {
    let fx = Fixture::new();
    local_commit(&fx, &[("a.md", "one\ntwo\nthree\n")], "base");
    track_origin(&fx);
    local_commit(&fx, &[("a.md", "one\nLOCAL\nthree\n")], "change a line");
    let local = local_commit(&fx, &[("a.md", "one\ntwo\nthree\n")], "change it back");
    let remote = upstream_commit(&fx, &[("a.md", "one\nREMOTE\nthree\n")], "upstream");

    let outcome = reconcile(&fx, PullStrategy::Rebase, local, remote);

    assert_eq!(outcome.state, crate::git::RECONCILE_REBASED, "{outcome:?}");
    assert_eq!(read(&fx, "a.md"), "one\nREMOTE\nthree\n");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent(0).unwrap().parent_ids().collect::<Vec<_>>(), vec![remote]);
}

// GTC-FR-NJUY: a tip other than the one the author was shown refuses the
// reconcile, and nothing is written.
#[test]
fn a_tip_the_author_was_not_shown_refuses_the_reconcile() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let before = refs_of(&fx);

    let stale = reconcile(&fx, PullStrategy::Merge, remote, remote);
    assert_eq!(stale.state, crate::git::PULL_REFUSED);
    assert_eq!(stale.error.as_deref(), Some(ERR_PULL_TIPS_MOVED));
    let stale = reconcile(&fx, PullStrategy::Rebase, local, local);
    assert_eq!(stale.error.as_deref(), Some(ERR_PULL_TIPS_MOVED));

    assert_eq!(refs_of(&fx), before);
    assert_eq!(read(&fx, "b.md"), "");
}

// GTC-FR-VOXB: a reconcile that would overwrite an uncommitted path is refused
// with that path, and nothing is written.
#[test]
fn a_reconcile_that_would_overwrite_uncommitted_work_is_refused() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    std::fs::write(fx.root().join("b.md"), "mine\n").unwrap();
    let before = refs_of(&fx);

    let outcome = reconcile(&fx, PullStrategy::Merge, local, remote);

    assert_eq!(outcome.error, Some(format!("{ERR_PULL_WOULD_OVERWRITE}: b.md")));
    assert_eq!(refs_of(&fx), before);
    assert_eq!(read(&fx, "b.md"), "mine\n");
}

// GTC-FR-EGIP: an integration Git cannot settle writes no branch and no
// worktree; a refusal of the hand-off (here the image preflight) leaves no run
// and no snapshot.
#[test]
fn a_conflict_refused_at_the_hand_off_writes_nothing_and_leaves_no_run() {
    let fx = Fixture::new();
    let (local, remote) =
        diverged(&fx, &[("a.md", "one\nLOCAL\nthree\n")], &[("a.md", "one\nREMOTE\nthree\n")]);
    let before = refs_of(&fx);

    for strategy in [PullStrategy::Merge, PullStrategy::Rebase] {
        let outcome = reconcile(&fx, strategy, local, remote);
        assert_eq!(outcome.state, crate::git::PULL_REFUSED, "{outcome:?}");
        assert_eq!(
            outcome.error.as_deref(),
            Some(crate::graduation::ERR_VENDOR_EXECUTION_UNSUPPORTED),
            "the refusal is the image preflight's"
        );
        assert!(outcome.run_id.is_none());
        // `refs_of` names every reference, the private snapshot refs included.
        assert_eq!(refs_of(&fx), before);
        assert!(!refs_of(&fx).keys().any(|name| name.starts_with("refs/synthesis/merge/")));
        assert_eq!(read(&fx, "a.md"), "one\nLOCAL\nthree\n");
    }
    super::merge_support::assert_no_run_anywhere(&fx);
}

// GTC-FR-TRWC: a reconcile is refused while the repository update guard is held.
#[test]
fn a_reconcile_is_refused_while_the_update_guard_is_held() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let state = fx.app.state::<StreamState>();
    let _held = state.begin_repository_update("some-stream", Reconciliation::Update).unwrap();

    let outcome = reconcile(&fx, PullStrategy::Merge, local, remote);

    assert_eq!(outcome.error.as_deref(), Some(ERR_UPDATE_IN_PROGRESS));
    assert_eq!(head_of(&fx), local);
}

// GTC-FR-TRWC, WKS-FR-HLGN: a pull holds the guard under its own kind, so a
// merge or an update asked meanwhile is told a pull runs.
#[test]
fn a_held_pull_guard_refuses_other_reconciliations_as_a_pull() {
    let fx = Fixture::new();
    local_commit(&fx, &[("a.md", "one\n")], "base");
    track_origin(&fx);
    let branch = branch_of(&fx);
    let hold = begin_pull(&fx.app, &fx.root(), &branch).unwrap();
    assert!(hold.is_some());
    let state = fx.app.state::<StreamState>();
    let refused = state.begin_repository_update("stream", Reconciliation::Merge).err();
    assert_eq!(refused.as_deref(), Some(ERR_PULL_IN_PROGRESS));
    drop(hold);
    assert!(state.begin_repository_update("stream", Reconciliation::Merge).is_ok());
}

// GRD-FR-PLSB, GRD-FR-PQKY, GRD-FR-VCTH: a pull run is a merge run with a pull
// subject, titled for its upstream and branch, waiting in its worktree's queue
// with a direct target, and it is never a stream's merge run.
#[test]
fn a_pull_run_is_queued_for_its_worktree_and_is_no_streams_merge_run() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("a.md", "x\n")], &[("a.md", "y\n")]);
    let request = pull_request(&fx, local, remote, PullStrategy::Rebase);

    let run = crate::graduation::enqueue_merge_run(&fx.app, request).unwrap();

    let branch = branch_of(&fx);
    let merge = run.merge.as_ref().unwrap();
    assert_eq!(merge.name, format!("Pull origin/{branch} into {branch}"));
    assert_eq!(merge.pull.as_ref().unwrap().strategy, PullStrategy::Rebase);
    assert!(run.is_pull() && run.is_merge());
    let root = fx.root().to_string_lossy().into_owned();
    let target = run.direct_target.as_ref().unwrap();
    assert_eq!(
        (target.worktree_path.as_str(), target.branch.as_str()),
        (root.as_str(), branch.as_str())
    );
    assert_eq!(run.queue_key(), crate::graduation::worktree_queue_key(&root));

    let fs = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let index = crate::graduation::read_queue_index(&fs, &base, &key);
    let entry = index.runs.iter().find(|e| e.run_id == run.id).unwrap();
    assert!(entry.merge);
    assert_eq!(entry.worktree_path, root);
    // WKS-FR-KHJS: a pull run on a stream's copy names that stream, and is still
    // not the stream's merge run.
    let mut index = index.clone();
    for entry in index.runs.iter_mut() {
        entry.stream_id = "a-stream".to_string();
    }
    crate::graduation::write_queue_index(&fs, &base, &index).unwrap();
    assert!(crate::graduation::merge_runs_by_stream(&fs, &base, &key).is_empty());
}

// GTC-FR-TRWC: a live pull run for the worktree refuses another pull of it.
#[test]
fn a_live_pull_run_refuses_another_pull_of_its_worktree() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("a.md", "x\n")], &[("a.md", "y\n")]);
    let run = crate::graduation::enqueue_merge_run(
        &fx.app,
        pull_request(&fx, local, remote, PullStrategy::Merge),
    )
    .unwrap();

    let outcome = reconcile(&fx, PullStrategy::Merge, local, remote);

    assert_eq!(outcome.error, Some(format!("{ERR_PULL_RUN_ACTIVE}: {}", run.id)));
    assert_eq!(head_of(&fx), local);
}

// GTC-FR-OYDA, GTC-FR-04: a reconcile streams its lines under the operation
// `pull` and ends in one terminal event.
#[test]
fn a_reconcile_streams_its_lines_and_ends_once() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let seen: std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>> =
        Default::default();
    for name in [crate::git::GIT_OUTPUT_LINE, crate::git::GIT_OPERATION_FINISHED] {
        let seen = seen.clone();
        fx.app.listen_any(name, move |event| {
            let payload = serde_json::from_str(event.payload()).unwrap_or_default();
            seen.lock().unwrap().push((name.to_string(), payload));
        });
    }

    reconcile(&fx, PullStrategy::Merge, local, remote);

    let seen = seen.lock().unwrap();
    let finished: Vec<_> =
        seen.iter().filter(|(name, _)| name == crate::git::GIT_OPERATION_FINISHED).collect();
    assert_eq!(finished.len(), 1, "{seen:?}");
    assert_eq!(finished[0].1["ok"], serde_json::json!(true));
    assert!(seen.iter().all(|(_, payload)| payload["operation"] == serde_json::json!("pull")));
    assert!(seen
        .iter()
        .any(|(_, p)| p["line"].as_str().is_some_and(|l| l.starts_with("Merged at "))));
}

// GTC-FR-TRWC: a direct run dispatched on the worktree refuses a pull of it,
// through the guard the pull itself takes.
#[test]
fn a_dispatched_direct_run_on_the_worktree_refuses_the_pull() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let fs = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let worktree = crate::worktree::active_entry_for(&fx.root()).unwrap().path;
    let index = crate::graduation::QueueIndex {
        project_key: key.clone(),
        runs: vec![crate::graduation::QueueIndexEntry {
            run_id: "direct-1".to_string(),
            state: crate::graduation::GraduationRunState::Working,
            worktree_path: worktree,
            dispatched_direct: true,
            dispatched: true,
            ..Default::default()
        }],
    };
    crate::graduation::write_queue_index(&fs, &base, &index).unwrap();

    let refused = begin_pull(&fx.app, &fx.root(), &branch_of(&fx)).err();
    assert_eq!(
        refused,
        Some(format!("{}: direct-1", crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE))
    );
    let outcome = reconcile(&fx, PullStrategy::Merge, local, remote);
    assert_eq!(outcome.error, refused);
    assert_eq!(head_of(&fx), local);
}

// GRB-FR-QIHE, GTC-FR-VOXB: a merge Git settles lands over an uncommitted path
// it does not change, which keeps its content.
#[test]
fn a_settled_merge_keeps_uncommitted_work_it_does_not_change() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    std::fs::write(fx.root().join("README.md"), "the author's edit\n").unwrap();
    std::fs::write(fx.root().join("notes.txt"), "untracked\n").unwrap();

    let outcome = reconcile(&fx, PullStrategy::Rebase, local, remote);

    assert_eq!(outcome.state, crate::git::RECONCILE_REBASED, "{outcome:?}");
    assert_eq!(read(&fx, "README.md"), "the author's edit\n");
    assert_eq!(read(&fx, "notes.txt"), "untracked\n");
    assert_eq!(read(&fx, "b.md"), "remote\n");
}

// WTC-FR-PLHU, WTC-FR-25, GTC-FR-QHMT, GTC-FR-KDRW, GTC-FR-IHCE: the pull runs
// the refresh against a real upstream, announces the branch set once, moves no
// active worktree, fast-forwards the branch with its files, tells the change
// set, and reports itself as a git operation named for its branch.
#[test]
fn a_pull_composes_the_refresh_and_announces_what_it_changed() {
    let fx = Fixture::new();
    fx.app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    fx.app.manage(crate::github_tokens::GithubTokens::default());
    local_commit(&fx, &[("a.md", "one\n")], "base");
    let upstream_dir = TempDir::new().unwrap();
    let upstream = git2::Repository::clone(
        &format!("file://{}", fx.root().to_string_lossy()),
        upstream_dir.path().join("upstream"),
    )
    .unwrap();
    {
        let mut config = upstream.config().unwrap();
        config.set_str("user.name", "Upstream").unwrap();
        config.set_str("user.email", "upstream@example.com").unwrap();
    }
    std::fs::write(upstream.workdir().unwrap().join("b.md"), "upstream\n").unwrap();
    let remote_tip = commit_all(&upstream, "upstream work");
    let branch = branch_of(&fx);
    let repo = fx.repo();
    repo.remote("origin", &format!("file://{}", upstream.workdir().unwrap().to_string_lossy()))
        .unwrap();
    let mut config = repo.config().unwrap();
    config.set_str(&format!("branch.{branch}.remote"), "origin").unwrap();
    config.set_str(&format!("branch.{branch}.merge"), &format!("refs/heads/{branch}")).unwrap();

    let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    for name in [
        crate::worktree::BRANCHES_CHANGED,
        crate::worktree::WORKTREE_CONTEXT_CHANGED,
        crate::changes::CHANGES_UPDATED,
    ] {
        let seen = seen.clone();
        fx.app.listen_any(name, move |_| seen.lock().unwrap().push(name.to_string()));
    }

    let outcome = crate::git::pull_current_branch_blocking(&fx.app, &fx.root(), "").unwrap();

    assert_eq!(outcome.state, crate::git::PULL_FAST_FORWARDED, "{outcome:?}");
    assert_eq!(outcome.refresh.remote_state, crate::worktree::REMOTE_REFRESHED);
    assert_eq!(head_of(&fx), remote_tip);
    assert_eq!(read(&fx, "b.md"), "upstream\n");
    let seen = seen.lock().unwrap().clone();
    let count = |name: &str| seen.iter().filter(|n| n.as_str() == name).count();
    assert_eq!(count(crate::worktree::BRANCHES_CHANGED), 1, "{seen:?}");
    assert_eq!(count(crate::worktree::WORKTREE_CONTEXT_CHANGED), 0, "{seen:?}");
    assert_eq!(count(crate::changes::CHANGES_UPDATED), 1, "{seen:?}");
}

// GTC-FR-IHCE: a reconcile reports under `kind = "git"`, labelled for its branch,
// with no activation.
#[test]
fn a_reconcile_reports_itself_as_a_git_operation_named_for_its_branch() {
    let fx = Fixture::new();
    let (local, remote) = diverged(&fx, &[("c.md", "local\n")], &[("b.md", "remote\n")]);
    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    {
        let seen = seen.clone();
        fx.app.listen_any(crate::progress::OPERATION_PROGRESS, move |event| {
            let payload = serde_json::from_str(event.payload()).unwrap_or_default();
            seen.lock().unwrap().push(payload);
        });
    }
    reconcile(&fx, PullStrategy::Merge, local, remote);
    let label = format!("Pulling {}", branch_of(&fx));
    let seen = seen.lock().unwrap().clone();
    let pulls: Vec<_> = seen.iter().filter(|op| op["label"] == serde_json::json!(label)).collect();
    assert!(!pulls.is_empty(), "{seen:?}");
    assert!(pulls.iter().all(|op| op["kind"] == serde_json::json!(crate::git::PROGRESS_KIND_GIT)));
    assert!(pulls.iter().all(|op| op.get("activation").is_none_or(|a| a.is_null())));
}
