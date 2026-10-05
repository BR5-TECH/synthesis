//! The upstream sync state (GTC-FR-21): what each side counts against the
//! upstream, and the states in which there is nothing to count.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Upstream sync state (GTC-FR-21)
// -----------------------------------------------------------------------

#[test]
fn upstream_sync_state_reports_no_remote_when_none_is_configured() {
    // GTC-FR-21, third leg.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let state = upstream_sync_state(&f.root()).unwrap();
    assert_eq!(
        state,
        UpstreamSyncState {
            has_remote: false,
            has_upstream: false,
            ahead: None,
            behind: None,
        }
    );
}

#[test]
fn upstream_sync_state_reports_a_branch_that_has_never_been_published() {
    // GTC-FR-21, second leg: a remote is configured but the branch tracks
    // nothing, so a push would publish it.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.repo()
        .remote("origin", "https://github.com/acme/repo.git")
        .unwrap();
    let state = upstream_sync_state(&f.root()).unwrap();
    assert!(state.has_remote);
    assert!(!state.has_upstream);
    assert_eq!((state.ahead, state.behind), (None, None));
}

#[test]
fn upstream_sync_state_counts_each_side_against_the_upstream() {
    // GTC-FR-21, first leg. The upstream is a remote-tracking ref built
    // locally, so nothing here reaches a network.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    let base = f.commit("base");
    f.repo()
        .remote("origin", "https://github.com/acme/repo.git")
        .unwrap();
    let branch = f.current_branch();

    {
        let repo = f.repo();
        // The upstream sits one commit ahead of the fork point.
        let sig = repo.signature().unwrap();
        let base_commit = repo.find_commit(base).unwrap();
        let remote_only = repo
            .commit(
                None,
                &sig,
                &sig,
                "remote work",
                &base_commit.tree().unwrap(),
                &[&base_commit],
            )
            .unwrap();
        repo.reference(
            &format!("refs/remotes/origin/{branch}"),
            remote_only,
            true,
            "test",
        )
        .unwrap();
        let mut local = repo.find_branch(&branch, BranchType::Local).unwrap();
        local
            .set_upstream(Some(&format!("origin/{branch}")))
            .unwrap();
    }
    // Two local commits the upstream does not have.
    f.write("a.md", "two\n");
    f.commit("local one");
    f.write("a.md", "three\n");
    f.commit("local two");

    let state = upstream_sync_state(&f.root()).unwrap();
    assert!(state.has_remote && state.has_upstream);
    assert_eq!((state.ahead, state.behind), (Some(2), Some(1)));
}

#[test]
fn upstream_sync_state_reports_a_detached_head_as_tracking_nothing() {
    // GTC-FR-21, fourth leg.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    let base = f.commit("base");
    f.repo()
        .remote("origin", "https://github.com/acme/repo.git")
        .unwrap();
    f.repo().set_head_detached(base).unwrap();

    let state = upstream_sync_state(&f.root()).unwrap();
    assert!(state.has_remote);
    assert!(!state.has_upstream);
    assert_eq!((state.ahead, state.behind), (None, None));
}

#[test]
fn upstream_sync_state_reports_an_unborn_branch_as_tracking_nothing() {
    // GTC-FR-21: a repository with a remote and no commits yet has no ref to
    // compare, and must read as "never published" rather than error.
    let f = Fixture::new();
    f.repo()
        .remote("origin", "https://github.com/acme/repo.git")
        .unwrap();
    let state = upstream_sync_state(&f.root()).unwrap();
    assert!(state.has_remote);
    assert!(!state.has_upstream);
    assert_eq!((state.ahead, state.behind), (None, None));
}

#[test]
fn upstream_sync_state_outside_a_repository_is_the_typed_error() {
    // GTC-FR-02: what the Changes panel's inline state is keyed on.
    let dir = TempDir::new().unwrap();
    assert_eq!(
        upstream_sync_state(dir.path()).unwrap_err(),
        ERR_NOT_A_REPO
    );
}
