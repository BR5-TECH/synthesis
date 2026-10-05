//! The pre-conflict check of a merge: what it does when Git settles the merge,
//! and what it reports while it runs (`WKS-work-streams.md`,
//! `GRB-graduation-rebase.md`).

use std::sync::{Arc, Mutex};

use tauri::Listener;

use super::merge_support::*;
use super::*;

fn commit_publication(message: &str) -> StreamMergePublication {
    StreamMergePublication::Commit {
        message: message.to_string(),
    }
}

// WKS-FR-GKPX, WKS-FR-QNHF, WKS-FR-TVBM, WKS-FR-PZKD, GRB-FR-HRTB, GRB-FR-FLIB,
// GRB-FR-ASWC: a clean merge on the `uncommitted` publication leaves the result
// unstaged in the base worktree, moves no branch, and creates no run.
#[test]
fn a_clean_merge_left_uncommitted_is_unstaged_and_moves_no_branch() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "mergeable");
    let base_before = tip_text(&fx.repo(), &stream.base_branch);
    let stream_before = tip_text(&fx.repo(), &stream.branch);
    let store_before = names_under(&canonical(fx.store.path()));

    let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");

    match result {
        StreamMergeResult::Merged {
            merged_paths,
            commit,
        } => {
            assert_eq!(merged_paths, vec!["added.txt".to_string()]);
            assert_eq!(commit, None, "no commit was made");
        }
        other => panic!("expected a merge, got {other:?}"),
    }
    let repo = fx.repo();
    assert_eq!(tip_text(&repo, &stream.base_branch), base_before);
    assert_eq!(tip_text(&repo, &stream.branch), stream_before);
    assert_eq!(
        std::fs::read_to_string(fx.root().join("added.txt")).unwrap(),
        "work\n"
    );
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    let status = statuses
        .iter()
        .find(|entry| entry.path() == Ok("added.txt"))
        .expect("the merged path has a status")
        .status();
    assert!(status.is_wt_new(), "the result is unstaged: {status:?}");
    assert!(!status.is_index_new(), "and not staged: {status:?}");
    assert_eq!(
        names_under(&canonical(fx.store.path())),
        store_before,
        "the application's store holds nothing new"
    );
    assert_no_run_anywhere(&fx);
}

// WKS-FR-TVBM, GRB-FR-ASWC, GRB-FR-FLIB: a clean merge on the `commit`
// publication makes one merge commit with both parents and the author's message.
#[test]
fn a_clean_merge_committed_makes_one_merge_commit_with_both_parents() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "committed");
    let base_before = tip_of(&fx.repo(), &stream.base_branch);
    let stream_before = tip_of(&fx.repo(), &stream.branch);

    let result = merge(&fx, &stream.id, commit_publication("Land the committed stream"))
        .expect("merged");

    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    match result {
        StreamMergeResult::Merged {
            merged_paths,
            commit,
        } => {
            assert_eq!(merged_paths, vec!["added.txt".to_string()]);
            assert_eq!(commit, Some(head.id().to_string()));
        }
        other => panic!("expected a merge, got {other:?}"),
    }
    assert_eq!(head.message().unwrap(), "Land the committed stream");
    let parents: Vec<git2::Oid> = head.parent_ids().collect();
    assert_eq!(parents, vec![base_before, stream_before]);
    assert_eq!(tip_of(&repo, &stream.branch), stream_before);
    assert_eq!(
        std::fs::read_to_string(fx.root().join("added.txt")).unwrap(),
        "work\n"
    );
    assert!(
        status_of_base(&repo).is_empty(),
        "the commit leaves the base worktree clean"
    );
    assert_no_run_anywhere(&fx);
}

fn status_of_base(repo: &git2::Repository) -> Vec<String> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    repo.statuses(Some(&mut options))
        .unwrap()
        .iter()
        .map(|entry| entry.path().unwrap_or_default().to_string())
        .collect()
}

// GRB-FR-EPYG, GRB-FR-FLIB: a path both sides changed in different regions is
// merged by Git and needs no run, whichever publication the author chose.
#[test]
fn a_path_both_sides_changed_in_different_places_merges_without_a_run() {
    for publication in [
        StreamMergePublication::Uncommitted,
        commit_publication("both sides"),
    ] {
        let fx = Fixture::new();
        commit_file(&fx.repo(), "notes.txt", "top\nmiddle\nbottom\n", "seed notes");
        let stream = fx.create("both sides", None).expect("created");
        commit_file(
            &stream_repo(&stream),
            "notes.txt",
            "stream top\nmiddle\nbottom\n",
            "stream edit",
        );
        commit_file(
            &fx.repo(),
            "notes.txt",
            "top\nmiddle\nbase bottom\n",
            "base edit",
        );

        let result = merge(&fx, &stream.id, publication).expect("merged");

        assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(fx.root().join("notes.txt")).unwrap(),
            "stream top\nmiddle\nbase bottom\n",
        );
        assert_no_run_anywhere(&fx);
    }
}

// WKS-FR-FOTC, WKS-FR-PZKD, GRB-FR-FLIB: a stream that holds nothing its base
// does not reports `nothing_to_merge` and writes nothing, on either publication.
#[test]
fn a_stream_with_nothing_to_merge_writes_nothing() {
    for publication in [
        StreamMergePublication::Uncommitted,
        commit_publication("nothing"),
    ] {
        let fx = Fixture::new();
        let stream = fx.create("empty", None).expect("created");
        let before = observe(&fx, &stream);

        let result = merge(&fx, &stream.id, publication).expect("answered");

        assert_eq!(result, StreamMergeResult::NothingToMerge);
        assert_eq!(observe(&fx, &stream), before, "nothing was written");
        assert_no_run_anywhere(&fx);
    }
}

// WKS-FR-FOTC: a stream whose work the base already holds has nothing to merge,
// although it stands behind its base.
#[test]
fn a_stream_behind_its_base_with_no_work_of_its_own_has_nothing_to_merge() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let before = observe(&fx, &stream);

    let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("answered");

    assert_eq!(result, StreamMergeResult::NothingToMerge);
    assert_eq!(observe(&fx, &stream), before);
}

// WKS-FR-GKPX, GRB-FR-ASWC: a merge writes into the worktree that holds the
// base branch, and into no worktree that stands on another branch. With no
// worktree on the base branch a `commit` publication still advances the branch.
#[test]
fn a_merge_commit_lands_on_a_base_branch_no_worktree_holds() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "sideways");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    commit_file(&fx.repo(), "elsewhere.txt", "e\n", "elsewhere work");
    let elsewhere_before = tip_of(&fx.repo(), "elsewhere");
    let base_before = tip_of(&fx.repo(), &stream.base_branch);

    let result = merge(&fx, &stream.id, commit_publication("onto the base")).expect("merged");

    assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
    let repo = fx.repo();
    let base_after = repo
        .find_branch(&stream.base_branch, git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    assert_ne!(base_after.id(), base_before);
    assert_eq!(base_after.parent_count(), 2);
    assert_eq!(tip_of(&repo, "elsewhere"), elsewhere_before);
    assert!(fx.root().join("elsewhere.txt").exists());
    assert!(
        !fx.root().join("added.txt").exists(),
        "the stream's file was not written into a worktree on another branch"
    );
}

// WKS-FR-TVBM, WKS-FR-UZHT: an `uncommitted` publication has no worktree to
// land in when none holds the base branch, so it is refused and writes nothing.
#[test]
fn an_uncommitted_merge_with_no_worktree_on_the_base_branch_is_refused() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "no base worktree");
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    let before = observe(&fx, &stream);

    let refusal =
        merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, ERR_BASE_NOT_CHECKED_OUT);
    assert_eq!(observe(&fx, &stream), before);
    assert_no_run_anywhere(&fx);
}

/// What the progress event of the application carried, in order.
fn watch_operations(fx: &Fixture) -> Arc<Mutex<Vec<serde_json::Value>>> {
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    fx.app.listen(crate::progress::OPERATION_PROGRESS, move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            sink.lock().unwrap().push(value);
        }
    });
    seen
}

// WKS-FR-MWYD, GRB-FR-RNGX: the check is one in-flight operation named
// `Merging <stream name>`, registered when it starts and finished when it
// settles, whatever the outcome.
#[test]
fn the_check_is_one_operation_that_is_registered_and_finished() {
    let cases: Vec<(&str, Box<dyn Fn(&Fixture) -> WorkStream>)> = vec![
        ("merged", Box::new(|fx| stream_ahead(fx, "Reported"))),
        (
            "nothing to merge",
            Box::new(|fx| fx.create("Reported", None).expect("created")),
        ),
        (
            "conflict refused at the preflight",
            Box::new(|fx| {
                let stream = fx.create("Reported", None).expect("created");
                commit_file(&stream_repo(&stream), "README.md", "stream\n", "stream");
                commit_file(&fx.repo(), "README.md", "base\n", "base");
                stream
            }),
        ),
        (
            "refused for a dirty stream",
            Box::new(|fx| {
                let stream = fx.create("Reported", None).expect("created");
                std::fs::write(Path::new(&stream.worktree_path).join("x.txt"), "x\n").unwrap();
                stream
            }),
        ),
    ];
    for (name, make) in cases {
        let fx = Fixture::new();
        let stream = make(&fx);
        let seen = watch_operations(&fx);

        let _ = merge(&fx, &stream.id, StreamMergePublication::Uncommitted);

        let events = seen.lock().unwrap().clone();
        let merging: Vec<&serde_json::Value> = events
            .iter()
            .filter(|event| event["label"] == "Merging Reported")
            .collect();
        assert_eq!(
            merging.first().map(|e| e["state"].as_str()),
            Some(Some("running")),
            "{name}: the operation is registered first",
        );
        assert_eq!(
            merging.last().map(|e| e["state"].as_str()),
            Some(Some("finished")),
            "{name}: and finished last",
        );
        assert_eq!(merging.first().unwrap()["kind"], "merge", "{name}");
        let id = merging.first().unwrap()["id"].clone();
        assert!(
            merging.iter().all(|event| event["id"] == id),
            "{name}: it is one operation"
        );
        assert!(
            fx.app
                .state::<crate::progress::ProgressRegistry>()
                .in_flight()
                .is_empty(),
            "{name}: nothing stays in flight"
        );
        assert!(
            events
                .iter()
                .all(|event| event["label"] == "Merging Reported"),
            "{name}: no other progress is reported for a merge"
        );
    }
}

// WKS-FR-HLGN, WKS-FR-TSOA, WKS-FR-RQVM, GRB-FR-JIRD: the repository update
// guard is released on every exit, so no outcome and no refusal wedges the
// repository or the stream.
#[test]
fn the_guard_is_released_on_every_exit() {
    let cases: Vec<(&str, Box<dyn Fn(&Fixture) -> WorkStream>)> = vec![
        ("nothing to merge", Box::new(|fx| fx.create("hold", None).expect("created"))),
        ("merged", Box::new(|fx| stream_ahead(fx, "hold"))),
        (
            "refused for a dirty stream",
            Box::new(|fx| {
                let stream = fx.create("hold", None).expect("created");
                std::fs::write(Path::new(&stream.worktree_path).join("x.txt"), "x\n").unwrap();
                stream
            }),
        ),
        (
            "conflict refused at the preflight",
            Box::new(|fx| {
                let stream = fx.create("hold", None).expect("created");
                commit_file(&stream_repo(&stream), "README.md", "stream\n", "stream");
                commit_file(&fx.repo(), "README.md", "base\n", "base");
                stream
            }),
        ),
    ];
    for (name, make) in cases {
        let fx = Fixture::new();
        let stream = make(&fx);

        let _ = merge(&fx, &stream.id, StreamMergePublication::Uncommitted);

        let state = fx.app.state::<StreamState>();
        assert!(!state.is_merging(&stream.id), "{name}: the guard was kept");
        let again = state
            .begin_repository_update(&stream.id, Reconciliation::Update)
            .map(|_| ());
        assert!(again.is_ok(), "{name}: the guard cannot be taken again");
    }
}

// WKS-FR-RQVM: a run's claim of a stream is refused with `stream_busy` while the
// merge check holds the guard, and is accepted once it is released.
#[test]
fn a_claim_of_the_stream_is_refused_while_the_check_holds_the_guard() {
    let fx = Fixture::new();
    let stream = fx.create("claimed", None).expect("created");
    let state = fx.app.state::<StreamState>();

    let hold = state
        .begin_repository_update(&stream.id, Reconciliation::Merge)
        .expect("the guard is free");
    let refused = claim_stream(&fx.app, &stream.id, "g1").expect_err("refused");
    assert_eq!(refused, ERR_STREAM_BUSY);
    drop(hold);

    claim_stream(&fx.app, &stream.id, "g1").expect("the claim is accepted after the release");
}

// WKS-FR-WULF: a merge check that settles tells every listing to reload, once the
// guard is released, whether it merged, found nothing or handed off.
#[test]
fn a_settled_check_tells_every_listing_to_reload() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "announced");
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    fx.app.listen(WORK_STREAMS_CHANGED, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });

    merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");

    let events = seen.lock().unwrap().clone();
    assert!(!events.is_empty(), "the listing was told");
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let expected = serde_json::json!({ "projectKey": key }).to_string();
    assert!(events.iter().all(|payload| payload == &expected), "{events:?}");
}
