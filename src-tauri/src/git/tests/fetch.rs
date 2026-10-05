//! The fetch primitive (GTC-FR-12 .. GTC-FR-15): which refs it writes, which
//! it prunes, and how it resolves a credential.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -- GTC-FR-12 .. GTC-FR-15: the fetch primitive -------------------------

#[test]
fn a_fetch_adds_advances_and_prunes_remote_tracking_refs() {
    // GTC-FR-12: a branch that appeared gains a ref, one that moved has its
    // ref advanced, one deleted on the remote has its ref dropped — and
    // nothing about any checkout changes.
    let f = RemoteFixture::new();
    let main = f.upstream.current();
    f.upstream.branch("spike");
    f.fetch().unwrap();
    assert!(
        f.tracking().contains(&"origin/spike".to_string()),
        "precondition: the first fetch brought the remote's branches down"
    );
    let before_main = f
        .downstream
        .repo()
        .find_branch(&format!("origin/{main}"), BranchType::Remote)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();

    // The remote gains a branch, advances another, and deletes a third.
    f.upstream.branch("feature-new");
    f.upstream.write("a.md", "one\ntwo\n");
    f.upstream.commit();
    let advanced = f.upstream.repo().head().unwrap().peel_to_commit().unwrap().id();
    f.upstream
        .repo()
        .find_branch("spike", BranchType::Local)
        .unwrap()
        .delete()
        .unwrap();

    let head_before = f.downstream.repo().head().unwrap().peel_to_commit().unwrap().id();
    let branch_before = f.downstream.current();
    // A fetch legitimately writes refs, reflogs, `FETCH_HEAD`, and objects.
    // Everything else — the working tree and each worktree's `HEAD` and index
    // — is what "changes no checkout" means, so that is what is compared.
    let checkout_state = |f: &WorktreeFixture| {
        f.snapshot_all()
            .into_iter()
            .filter(|(path, _)| {
                !path.contains("/refs/")
                    && !path.contains("/logs/")
                    && !path.contains("/objects/")
                    && !path.ends_with("FETCH_HEAD")
                    && !path.ends_with("packed-refs")
            })
            .collect::<std::collections::BTreeMap<String, String>>()
    };
    let worktree_before = checkout_state(&f.downstream);

    f.fetch().unwrap();

    let tracking = f.tracking();
    assert!(
        tracking.contains(&"origin/feature-new".to_string()),
        "the branch that appeared gained a remote-tracking ref: {tracking:?}"
    );
    assert!(
        !tracking.contains(&"origin/spike".to_string()),
        "and the one deleted on the remote had its ref pruned: {tracking:?}"
    );
    let after_main = f
        .downstream
        .repo()
        .find_branch(&format!("origin/{main}"), BranchType::Remote)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();
    assert_ne!(after_main, before_main, "the moved branch's ref advanced");
    assert_eq!(after_main, advanced, "to the remote's current commit");

    // GTC-FR-12: a fetch writes nothing but refs.
    assert_eq!(
        f.downstream.repo().head().unwrap().peel_to_commit().unwrap().id(),
        head_before,
        "HEAD is on the same commit"
    );
    assert_eq!(f.downstream.current(), branch_before, "and the same branch");
    assert_eq!(
        worktree_before,
        checkout_state(&f.downstream),
        "every worktree's index and working tree are byte-identical — the \
         remote's new commit is fetched, not checked out"
    );
    assert!(
        worktree_before.contains_key("repo/.git/index")
            && worktree_before.contains_key("repo/a.md"),
        "guard on the filter above: the index and the working tree must still \
         be inside the compared set, or it proves nothing"
    );
}

#[test]
fn a_prune_leaves_the_local_branch_whose_upstream_vanished_intact() {
    // GTC-FR-13 / GTC-FR-06: the prune is bounded to remote-tracking refs.
    // The local branch survives with its commits, and nothing on the remote
    // is touched — which is why dropping a stale ref is not a deletion.
    let f = RemoteFixture::new();
    f.upstream.branch("spike");
    f.fetch().unwrap();
    // A local branch tracking the remote one, as a checkout would create.
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "origin/spike").unwrap();
    let local_head = f
        .downstream
        .repo()
        .find_branch("spike", BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();

    let remote_refs_before: Vec<String> = f
        .upstream
        .repo()
        .branches(Some(BranchType::Local))
        .unwrap()
        .filter_map(|b| b.ok())
        .filter_map(|(b, _)| b.name().ok().flatten().map(|s| s.to_string()))
        .collect();

    f.upstream
        .repo()
        .find_branch("spike", BranchType::Local)
        .unwrap()
        .delete()
        .unwrap();
    f.fetch().unwrap();

    let downstream = f.downstream.repo();
    let local = downstream
        .find_branch("spike", BranchType::Local)
        .expect("the local branch survives the prune");
    assert_eq!(
        local.get().peel_to_commit().unwrap().id(),
        local_head,
        "with its commits intact"
    );
    assert!(
        !f.tracking().contains(&"origin/spike".to_string()),
        "only its remote-tracking ref was dropped"
    );

    // And the fetch deleted nothing on the remote — only the test did.
    let remote_refs_after: Vec<String> = f
        .upstream
        .repo()
        .branches(Some(BranchType::Local))
        .unwrap()
        .filter_map(|b| b.ok())
        .filter_map(|(b, _)| b.name().ok().flatten().map(|s| s.to_string()))
        .collect();
    let mut expected = remote_refs_before;
    expected.retain(|n| n != "spike");
    assert_eq!(remote_refs_after, expected);
}

#[test]
fn a_fetch_with_no_remote_is_the_typed_error_and_writes_no_ref() {
    // GTC-FR-14, GTC-FR-10, first clause.
    let f = WorktreeFixture::new();
    let before = f.snapshot_all();
    let err = fetch_remote_branches(
        &CollectingSink::default(),
        &SCRATCH_BUFFER,
        &ProgressRegistry::default(),
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "/dev/acme",
    )
    .unwrap_err();
    assert_eq!(err, ERR_NO_REMOTE_CONFIGURED);
    assert_eq!(before, f.snapshot_all(), "no ref was written");
}

#[test]
fn a_github_remote_with_no_resolvable_token_refuses_before_any_network_request() {
    // GTC-FR-14, middle clauses (GTC-FR-10 / GTC-FR-14): the two token
    // failures the UI answers differently, raised without contacting
    // anything — the URL below is unroutable, so a test that reached the
    // network would hang or time out rather than pass.
    let f = WorktreeFixture::new();
    f.repo()
        .remote("origin", "https://github.com/acme/does-not-exist.git")
        .unwrap();
    let before = f.snapshot_all();

    let fetch = |store: &GlobalSettingsStore| {
        fetch_remote_branches(
            &CollectingSink::default(),
            &SCRATCH_BUFFER,
            &ProgressRegistry::default(),
            &f.root(),
            store,
            &GithubTokens::default(),
            "/dev/acme",
        )
    };

    // Nothing stored at all: there is nothing to choose between.
    let empty = GlobalSettingsStore::in_memory();
    assert_eq!(
        fetch(&empty).unwrap_err(),
        crate::github_tokens::ERR_TOKEN_MISSING
    );

    // Two stored and no binding: the project has not been pointed at one.
    let two = GlobalSettingsStore::in_memory();
    two.save_github_token_registry(vec![
        crate::github_tokens::GithubTokenRecord {
            id: "a".into(),
            label: "one".into(),
            ..Default::default()
        },
        crate::github_tokens::GithubTokenRecord {
            id: "b".into(),
            label: "two".into(),
            ..Default::default()
        },
    ])
    .unwrap();
    assert_eq!(
        fetch(&two).unwrap_err(),
        crate::github_tokens::ERR_SELECTION_REQUIRED
    );

    assert_eq!(
        before,
        f.snapshot_all(),
        "neither refusal wrote a ref, because the credential is resolved \
         before any transport is opened"
    );
}

#[test]
fn a_fetch_reports_itself_as_a_git_operation_and_streams_no_output() {
    // GTC-FR-11 (GTC-FR-15): an operation with `kind = "git"` is in flight
    // for the fetch's duration and reaches a terminal state when it ends. It
    // emits nothing on `"git output line"` — which this module has no emitter
    // for at all, so a fetch cannot leave a transcript in the Git panel's
    // push/pull output area.
    use crate::progress::OperationState;
    let f = RemoteFixture::new();
    let sink = CollectingSink::default();
    let registry = ProgressRegistry::default();

    fetch_remote_branches(
        &sink,
        &SCRATCH_BUFFER,
        &registry,
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "",
    )
    .unwrap();

    let published = sink.published.lock().unwrap().clone();
    assert!(
        published.iter().all(|op| op.kind == PROGRESS_KIND_GIT),
        "every event a fetch publishes is a git operation: {published:?}"
    );
    assert!(
        published.first().map(|op| op.state) == Some(OperationState::Running),
        "it registers as running: {published:?}"
    );
    assert!(
        published.last().map(|op| op.state) == Some(OperationState::Finished),
        "and reaches a terminal state when it ends: {published:?}"
    );
    assert!(
        registry.in_flight().is_empty(),
        "leaving nothing stranded in the in-flight set"
    );
    // GTC-FR-15, PRG-FR-TBZN: a fetch has no destination in the Git panel.
    assert!(
        published.iter().all(|op| op.activation.is_none()),
        "a fetch carries no activation: {published:?}"
    );
    // GTC-FR-11: no label or error this module publishes names the remote, so
    // an embedded credential in a remote URL cannot ride out on one.
    assert!(
        published.iter().all(|op| !op.label.contains("://")),
        "no remote URL reaches the status bar: {published:?}"
    );
}

#[test]
fn a_failed_fetch_still_terminates_its_operation() {
    // PRG-FR-09 through GTC-FR-15: a fetch that fails must not strand an
    // operation as permanently running in the status bar.
    use crate::progress::OperationState;
    let f = WorktreeFixture::new();
    // A remote that exists but cannot be reached: the path is not a
    // repository, so libgit2 fails locally without touching a network.
    f.repo()
        .remote("origin", &format!("file://{}", f.sibling("nowhere").display()))
        .unwrap();
    let sink = CollectingSink::default();
    let registry = ProgressRegistry::default();

    let err = fetch_remote_branches(
        &sink,
        &SCRATCH_BUFFER,
        &registry,
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "",
    )
    .unwrap_err();

    // Pinned to the exact cause, not "one of the two": the caller presents
    // them differently, and reporting a path that is not a repository as a
    // refused credential would send the author off to their token over what
    // is not a credential problem at all.
    // Pinned to the exact cause, not "one of the two": the caller presents
    // them differently, and reporting a path that is not a repository as a
    // refused credential would send the author off to their token over what
    // is not a credential problem at all.
    //
    // Deterministic across platforms because `classify_transfer_error` keys
    // off libgit2's error *class* — a missing local path fails inside
    // libgit2's own repository open, which is never `Http`, `Ssh`, `Callback`,
    // or `ErrorCode::Auth`. A platform where that stopped holding would be a
    // reason to correct the classifier, not to loosen this.
    assert_eq!(err, crate::github_tokens::ERR_GITHUB_UNREACHABLE);
    assert!(
        sink.published
            .lock()
            .unwrap()
            .last()
            .map(|op| op.state)
            == Some(OperationState::Failed),
        "and its operation reached a terminal state"
    );
    assert!(registry.in_flight().is_empty());
}

#[test]
fn only_a_github_https_remote_is_the_one_a_token_authenticates() {
    // GTC-FR-09: an SSH remote — to github.com or anywhere — authenticates
    // with the author's own key, not with a personal access token, so it must
    // not be routed through token resolution. Getting this wrong would make a
    // refresh fail on "no token stored" for a repository cloned over SSH,
    // where a token has nothing to do with it.
    for url in [
        "https://github.com/acme/platform.git",
        "https://GitHub.com/acme/platform",
        "https://github.com:443/acme/platform.git",
        "https://user:pw@github.com/acme/platform.git",
    ] {
        assert!(is_github_https_remote(url), "{url} is a token-authenticated remote");
    }
    for url in [
        // SSH, in both spellings — the author's key, never a token.
        "git@github.com:acme/platform.git",
        "ssh://git@github.com/acme/platform.git",
        // Another forge, and a host that merely ends in the same letters.
        "https://gitlab.com/acme/platform.git",
        "https://notgithub.com/acme/platform.git",
        "https://github.com.evil.example/acme/platform.git",
        // The host is what follows the LAST `@`, so a hostname parked in the
        // userinfo does not make this GitHub's remote.
        "https://github.com@evil.example/acme/platform.git",
        "file:///dev/acme",
        "",
    ] {
        assert!(!is_github_https_remote(url), "{url} is not");
    }
}

#[test]
fn the_credential_chain_offers_each_credential_once_and_then_gives_up() {
    // The loop guard. libgit2 calls its credential callback repeatedly for
    // one connection, and handing back the same rejected credential every
    // time hangs the fetch instead of failing it — so the cursor must
    // advance, and then run out.
    //
    // The chain is built explicitly rather than from `credential_chain`, so
    // the assertions do not depend on which keys happen to be in the
    // developer's `~/.ssh`.
    let chain = vec![
        Credential::Token,
        Credential::SshAgent,
        Credential::SshKey(PathBuf::from("/nonexistent/id_ed25519")),
    ];
    let cursor = Cell::new(0usize);
    let ask = |allowed| next_credential(&chain, &cursor, Some("t0ken"), "acme", allowed);

    // A username request is answered without consuming a credential, since
    // answering it is not an authentication attempt.
    assert!(
        ask(git2::CredentialType::USERNAME).is_ok(),
        "an SSH URL with no user in it gets a username first"
    );
    assert_eq!(cursor.get(), 0, "and the cursor has not moved");

    // The token is offered once…
    assert!(ask(git2::CredentialType::USER_PASS_PLAINTEXT).is_ok());
    // …and a second ask for the same kind runs out rather than repeating it,
    // which is what turns a refused credential into a prompt failure.
    assert!(
        ask(git2::CredentialType::USER_PASS_PLAINTEXT).is_err(),
        "a rejected token is not handed back a second time"
    );
}

#[test]
fn an_ssh_remote_offers_the_authors_key_rather_than_a_token() {
    // GTC-FR-09's other half: a remote that is not a `github.com` HTTPS one
    // authenticates as it otherwise would. No token is in its chain at all,
    // so there is nothing to present even if one were stored — and the agent
    // comes before any key file, because it is the only path that works for
    // a passphrase-protected key.
    let chain = credential_chain(None);
    assert!(
        !chain.iter().any(|c| matches!(c, Credential::Token)),
        "a remote with no token resolved never offers one"
    );
    assert!(
        matches!(chain.first(), Some(Credential::SshAgent)),
        "the author's ssh-agent is tried first"
    );

    // And with a token, it leads — an HTTPS GitHub remote will not take a key.
    let chain = credential_chain(Some("t0ken"));
    assert!(matches!(chain.first(), Some(Credential::Token)));
    assert!(
        chain.iter().any(|c| matches!(c, Credential::SshAgent)),
        "with SSH still behind it, so a token stored for an HTTPS remote does \
         not break a repository whose primary remote is SSH"
    );
}

#[test]
fn a_transfer_failure_separates_a_refused_credential_from_an_unreachable_remote() {
    // GTC-FR-14: the caller presents these differently — one sends the author
    // to their token, the other tells them the remote could not be reached —
    // so the classification is pinned rather than left to a message match.
    use git2::{ErrorClass, ErrorCode};
    let refused = git2::Error::new(ErrorCode::Auth, ErrorClass::Http, "401");
    assert_eq!(
        classify_transfer_error(&refused),
        crate::github_tokens::ERR_INVALID_TOKEN
    );
    let unreachable = git2::Error::new(ErrorCode::GenericError, ErrorClass::Net, "no route");
    assert_eq!(
        classify_transfer_error(&unreachable),
        crate::github_tokens::ERR_GITHUB_UNREACHABLE
    );
    // GTC-FR-11: neither carries libgit2's message, so a remote URL with an
    // embedded credential cannot leak through an error.
    let leaky = git2::Error::new(
        ErrorCode::GenericError,
        ErrorClass::Net,
        "failed to connect to https://x-access-token:ghp_SECRET@github.com/a/b",
    );
    assert!(!classify_transfer_error(&leaky).contains("ghp_SECRET"));
}

#[test]
fn fetch_is_not_a_registered_command() {
    // GTC-FR-12 / WTC-FR-22: the fetch is a primitive `WTC` composes, not a
    // command the frontend can reach — the same shape as the checkout
    // primitive beside it. Registering it would let a caller fetch without
    // the re-enumeration and the `"branches changed"` announcement that make
    // a refresh coherent.
    let source = include_str!("../../lib.rs");
    let handler = source
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(!handler.contains("fetch_remote_branches"));
    assert!(!handler.contains("checkout_branch"));
}
