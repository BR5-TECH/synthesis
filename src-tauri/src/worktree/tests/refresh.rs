//! Refresh: the re-enumeration that follows a fetch, and what it leaves alone.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-22 .. WTC-FR-25: refresh -------------------------------------

#[test]
fn a_refresh_reports_what_the_fetch_brought_and_took_away() {
    // WTC-FR-22: the fetch runs, the remote leg reports `refreshed`, and the
    // returned context is a re-enumeration — so a branch the remote gained is
    // offered and one it deleted is not.
    let f = Fixture::new();
    f.remote_ref("origin/spike");
    assert!(
        branch_names(&context_for(&f.root()).unwrap().branches)
            .contains(&"origin/spike".to_string()),
        "precondition: the stale remote branch is offered before the refresh"
    );

    let called = std::cell::Cell::new(false);
    let outcome = refresh_outcome_for(&f.root(), || {
        called.set(true);
        fake_fetch(&f, &["origin/feature-new"], &["origin/spike"]);
        Ok(())
    })
    .unwrap();

    assert!(called.get(), "the fetch primitive is composed, not skipped");
    assert_eq!(outcome.remote_state, REMOTE_REFRESHED);
    assert_eq!(outcome.remote_error, None);
    let listed = branch_names(&outcome.context.branches);
    assert!(
        listed.contains(&"origin/feature-new".to_string()),
        "the branch the remote gained is in the refreshed listing: {listed:?}"
    );
    assert!(
        !listed.contains(&"origin/spike".to_string()),
        "and the one it deleted is gone from it: {listed:?}"
    );
}

#[test]
fn the_local_half_of_a_refresh_completes_whatever_the_remote_leg_did() {
    // WTC-FR-23, WTC-FR-02: no remote is `skipped`, every other cause is `failed` with
    // the typed error carried through — and in BOTH cases the context is
    // still a fresh enumeration, which is the whole point of the split.
    let f = Fixture::new();

    // A branch created since the last enumeration, which only a fresh
    // enumeration can report.
    f.branch("wip");

    let skipped = refresh_outcome_for(&f.root(), || {
        Err(crate::git::ERR_NO_REMOTE_CONFIGURED.to_string())
    })
    .unwrap();
    assert_eq!(skipped.remote_state, REMOTE_SKIPPED);
    assert_eq!(
        skipped.remote_error, None,
        "no remote is not a cause the author can act on"
    );
    assert!(
        branch_names(&skipped.context.branches).contains(&"wip".to_string()),
        "the local half still reports a branch created since the last look"
    );

    // Every credential and network failure is `failed`, carrying the cause
    // verbatim so the UI can route selection-required to the picker and
    // missing-token to Global settings (WTS-FR-35).
    for cause in [
        crate::github_tokens::ERR_SELECTION_REQUIRED,
        crate::github_tokens::ERR_TOKEN_MISSING,
        crate::github_tokens::ERR_INVALID_TOKEN,
        crate::github_tokens::ERR_GITHUB_UNREACHABLE,
    ] {
        let failed = refresh_outcome_for(&f.root(), || Err(cause.to_string())).unwrap();
        assert_eq!(failed.remote_state, REMOTE_FAILED, "for {cause}");
        assert_eq!(
            failed.remote_error.as_deref(),
            Some(cause),
            "the typed cause reaches the caller unchanged"
        );
        assert!(
            branch_names(&failed.context.branches).contains(&"wip".to_string()),
            "and the local half still completed, for {cause}"
        );
    }
}

#[test]
fn a_refresh_outside_a_repository_is_the_one_error_the_command_makes() {
    // WTC-FR-23, last clause (WTC-FR-02): no `RefreshOutcome`, and — being a
    // refusal — nothing is fetched either.
    let dir = TempDir::new().unwrap();
    let called = std::cell::Cell::new(false);
    let err = refresh_outcome_for(dir.path(), || {
        called.set(true);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err, ERR_NOT_A_REPO);
    assert!(
        !called.get(),
        "the repository gate runs BEFORE the fetch, so a project outside one \
         contacts no remote at all"
    );
}

#[test]
fn a_refresh_changes_no_checkout_and_re_roots_nothing() {
    // WTC-FR-24: the active worktree, every worktree's branch, HEAD, and both
    // working trees are exactly as they were; nothing was unmounted or
    // remounted; the remembered worktree was not rewritten; and no
    // `"worktree context changed"` was emitted.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::{Listener, Manager};

    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = mock_app_on(&f.root(), &f.root());

    // WTC-FR-24's Given: the scan, the watcher, and the change watch are
    // mounted, and the in-memory baselines hold entries. Without this the
    // assertions below would only prove a refresh does not *mount* anything,
    // which is not the claim — the claim is that it tears nothing down.
    let store = GlobalSettingsStore::in_memory();
    reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        &f.root(),
    )
    .unwrap();
    let tracker = app.state::<crate::artifacts::ContentTracker>();
    tracker.record("a.md", "ck");
    assert!(
        app.state::<ProjectWatcher>().is_active(),
        "precondition: a watcher is mounted on the content root"
    );
    let remembered = store
        .load_active_worktree(&to_string_path(&f.root()))
        .unwrap();
    assert_eq!(
        remembered,
        Some(to_string_path(&f.root())),
        "precondition: the reroot remembered this worktree"
    );

    let switched = Arc::new(AtomicUsize::new(0));
    let s = Arc::clone(&switched);
    app.listen(WORKTREE_CONTEXT_CHANGED, move |_| {
        s.fetch_add(1, Ordering::SeqCst);
    });

    let head_before = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    let branch_before = f.current_branch();
    // A fetch is *supposed* to write remote-tracking refs, so the comparison
    // is over everything else: the working-tree content of both worktrees,
    // and each one's `HEAD` and index — which is what "changes no checkout"
    // actually means.
    let checkout_state = |root: PathBuf| {
        crate::changes::tests_support::snapshot(root)
            .into_iter()
            .filter(|(path, _)| {
                !path.starts_with(".git/refs/") && !path.starts_with(".git/logs/")
            })
            .collect::<std::collections::BTreeMap<String, String>>()
    };
    let before_primary = checkout_state(f.root());
    let before_alpha = checkout_state(alpha.clone());

    let outcome = refresh_and_announce(&app.handle(), &f.root(), || {
        fake_fetch(&f, &["origin/feature-new"], &[]);
        Ok(())
    })
    .unwrap();

    assert_eq!(outcome.remote_state, REMOTE_REFRESHED);
    assert_eq!(
        outcome.context.active_worktree_path,
        to_string_path(&f.root()),
        "the active worktree is where it was"
    );
    assert_eq!(f.current_branch(), branch_before, "and on the same branch");
    assert_eq!(
        f.repo().head().unwrap().peel_to_commit().unwrap().id(),
        head_before,
        "with HEAD on the same commit"
    );
    assert_eq!(
        app.state::<ProjectState>().require_root().unwrap().path(),
        f.root(),
        "the content root did not move"
    );
    // A reroot tears the previous root's state down and mounts a fresh
    // watcher (WTC-FR-08). All three surviving is what proves the refresh did
    // not take that path.
    assert!(
        app.state::<ProjectWatcher>().is_active(),
        "the watcher was neither torn down nor remounted"
    );
    assert_eq!(
        tracker.len(),
        1,
        "and the in-memory checksum baselines survived — a reroot drops them"
    );
    // The remembered worktree is untouched because `refresh_and_announce`
    // takes no settings store at all — this reads back the value the reroot
    // above wrote, and would change only if a refresh gained that ability.
    assert_eq!(
        store
            .load_active_worktree(&to_string_path(&f.root()))
            .unwrap(),
        remembered,
    );
    assert_eq!(
        switched.load(Ordering::SeqCst),
        0,
        "nothing that \"worktree context changed\" describes has changed"
    );
    assert_eq!(
        before_primary,
        checkout_state(f.root()),
        "the primary worktree's content, HEAD and index are untouched — only \
         the remote-tracking ref the fetch wrote differs"
    );
    assert_eq!(
        before_alpha,
        checkout_state(alpha),
        "and so are the linked worktree's"
    );
}

#[test]
fn every_returning_refresh_emits_exactly_one_branches_changed() {
    // WTC-FR-25: one event whatever the remote leg did, and none for a
    // refresh refused under WTC-FR-02.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    let f = Fixture::new();
    let app = mock_app_on(&f.root(), &f.root());
    let count = Arc::new(AtomicUsize::new(0));
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let c = Arc::clone(&count);
    let s = Arc::clone(&seen);
    app.listen(BRANCHES_CHANGED, move |event| {
        c.fetch_add(1, Ordering::SeqCst);
        s.lock().unwrap().push(event.payload().to_string());
    });

    for (label, result) in [
        ("refreshed", Ok(())),
        (
            "skipped",
            Err(crate::git::ERR_NO_REMOTE_CONFIGURED.to_string()),
        ),
        (
            "failed",
            Err(crate::github_tokens::ERR_GITHUB_UNREACHABLE.to_string()),
        ),
    ] {
        count.store(0, Ordering::SeqCst);
        refresh_and_announce(&app.handle(), &f.root(), || result).unwrap();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "exactly one event for a {label} remote leg"
        );
    }

    // The payload names the repository and nothing else: it is a signal, and
    // each consumer reloads its own listing rather than reading a branch set
    // off the event.
    let payload = seen.lock().unwrap().last().cloned().unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        parsed.get("repositoryRoot").and_then(|v| v.as_str()),
        Some(to_string_path(&f.root()).as_str())
    );
    // Parsed rather than string-matched: a temporary directory whose path
    // happens to contain the word would make a substring check pass or fail
    // for the wrong reason.
    assert_eq!(
        parsed.as_object().map(|o| o.len()),
        Some(1),
        "the repository root is the whole payload — no branch set travels on \
         the event, because each consumer reloads its own listing: {payload}"
    );

    // And a refusal announces nothing.
    let outside = TempDir::new().unwrap();
    count.store(0, Ordering::SeqCst);
    assert_eq!(
        refresh_and_announce(&app.handle(), outside.path(), || Ok(())).unwrap_err(),
        ERR_NOT_A_REPO
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "a refresh refused under WTC-FR-02 emits nothing"
    );
}

#[test]
fn a_refresh_composes_the_real_fetch_primitive_end_to_end() {
    // WTC-FR-22 through the production path. Every other refresh test injects
    // a stand-in fetch, so all of them stay green if the command's closure
    // were `|| Ok(())` — this is the one that would not: the branches below
    // can only appear if a real transfer happened.
    //
    // The remote is a local `file://` repository, so this reaches no network.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    let upstream = Fixture::new();
    upstream.branch("feature-new");
    upstream.branch("spike");

    let f = Fixture::new();
    f.repo()
        .remote(
            "origin",
            &format!("file://{}", upstream.root().to_string_lossy()),
        )
        .unwrap();

    let app = mock_app_on(&f.root(), &f.root());
    let store = GlobalSettingsStore::in_memory();
    let tokens = crate::github_tokens::GithubTokens::default();
    let registry = crate::progress::ProgressRegistry::default();
    let handle = app.handle().clone();
    // The real primitive, wired exactly as the command wires it.
    let fetch = || {
        crate::git::fetch_remote_branches(
            &handle,
            &crate::logging::BUFFER,
            &registry,
            &f.root(),
            &store,
            &tokens,
            "",
        )
    };

    let announced = Arc::new(AtomicUsize::new(0));
    let switched = Arc::new(AtomicUsize::new(0));
    let a = Arc::clone(&announced);
    let s = Arc::clone(&switched);
    app.listen(BRANCHES_CHANGED, move |_| {
        a.fetch_add(1, Ordering::SeqCst);
    });
    app.listen(WORKTREE_CONTEXT_CHANGED, move |_| {
        s.fetch_add(1, Ordering::SeqCst);
    });

    let outcome = refresh_and_announce(&app.handle(), &f.root(), fetch).unwrap();

    assert_eq!(outcome.remote_state, REMOTE_REFRESHED);
    let listed = branch_names(&outcome.context.branches);
    assert!(
        listed.contains(&"origin/feature-new".to_string())
            && listed.contains(&"origin/spike".to_string()),
        "the remote's branches were fetched and re-enumerated: {listed:?}"
    );
    assert_eq!(announced.load(Ordering::SeqCst), 1);
    assert_eq!(
        switched.load(Ordering::SeqCst),
        0,
        "a fetch moves no checkout, so it announces none"
    );

    // And a second refresh, after the remote deleted one, prunes it.
    upstream
        .repo()
        .find_branch("spike", BranchType::Local)
        .unwrap()
        .delete()
        .unwrap();
    let fetch = || {
        crate::git::fetch_remote_branches(
            &handle,
            &crate::logging::BUFFER,
            &registry,
            &f.root(),
            &store,
            &tokens,
            "",
        )
    };
    let outcome = refresh_and_announce(&app.handle(), &f.root(), fetch).unwrap();
    let listed = branch_names(&outcome.context.branches);
    assert!(
        listed.contains(&"origin/feature-new".to_string())
            && !listed.contains(&"origin/spike".to_string()),
        "and the pruned one stops being offered: {listed:?}"
    );
}

#[test]
fn a_refresh_from_a_linked_worktree_offers_no_branches_but_still_fetches() {
    // The refresh control renders in a linked worktree too (WTS-FR-29 shares
    // WTS-FR-02's rule, which is about the repository, not the worktree), so
    // this path is live. WTC-FR-06 still applies: nothing can be checked out
    // from there, so nothing is offered — but the fetch itself must run, since
    // a colleague's branch is what the author would make a worktree FOR.
    let upstream = Fixture::new();
    upstream.branch("feature-new");
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    f.repo()
        .remote(
            "origin",
            &format!("file://{}", upstream.root().to_string_lossy()),
        )
        .unwrap();

    let app = mock_app_on(&alpha, &f.root());
    let store = GlobalSettingsStore::in_memory();
    let tokens = crate::github_tokens::GithubTokens::default();
    let registry = crate::progress::ProgressRegistry::default();
    let handle = app.handle().clone();

    let outcome = refresh_and_announce(&app.handle(), &alpha, || {
        crate::git::fetch_remote_branches(
            &handle,
            &crate::logging::BUFFER,
            &registry,
            &alpha,
            &store,
            &tokens,
            "",
        )
    })
    .unwrap();

    assert_eq!(outcome.remote_state, REMOTE_REFRESHED);
    assert!(
        outcome.context.branches.is_empty(),
        "a linked worktree offers none: {:?}",
        outcome.context.branches
    );
    assert_eq!(
        outcome.context.worktrees.len(),
        2,
        "while every worktree is still listed"
    );
    // The fetch nonetheless happened — the ref is in the repository even
    // though this worktree cannot check it out.
    assert!(
        f.repo()
            .find_branch("origin/feature-new", BranchType::Remote)
            .is_ok(),
        "the remote's branch was fetched"
    );
}
