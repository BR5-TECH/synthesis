//! The sync state and the push across the repository set (GTC-FR-OQJB,
//! GTC-FR-PQQV; GSM-FR-REIT, GSM-FR-MJHC, GSM-FR-GTPI, GSM-FR-WPKJ).
//!
//! One part of `mod.rs`. Each repository pushes to a local bare remote, so no
//! network is involved.

use super::*;
use crate::changes::repo_set::tests::fixture::{commit_all, write, Superproject};

type Emitted = RefCell<Vec<(String, serde_json::Value)>>;

/// Push the current branch of the set at `root`, and return the one
/// terminal event.
fn push(
    root: &Path,
    buffer: &'static LogBuffer,
) -> (Result<(), String>, serde_json::Value, Emitted) {
    let sink = RecordingSink::default();
    let registry = ProgressRegistry::default();
    let emitted: Emitted = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        emitted.borrow_mut().push((name.to_string(), payload));
    };
    let store = GlobalSettingsStore::default();
    let tokens = GithubTokens::default();
    let access = GithubAccess { root, store: &store, tokens: &tokens, project_key: "key" };
    let result = push_affected_at(&sink, buffer, &emit, &registry, access);
    let finished: Vec<serde_json::Value> = emitted
        .borrow()
        .iter()
        .filter(|(name, _)| name == GIT_OPERATION_FINISHED)
        .map(|(_, payload)| payload.clone())
        .collect();
    assert_eq!(finished.len(), 1, "exactly one terminal event");
    (result, finished[0].clone(), emitted)
}

fn remote_tip(bare: &Path) -> git2::Oid {
    Repository::open_bare(bare).unwrap().refname_to_id("refs/heads/main").unwrap()
}

fn head_of(repo: &Repository) -> git2::Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

fn new_commit(repo: &Repository, body: &str) -> git2::Oid {
    write(repo, "work.md", body);
    commit_all(repo, "work")
}

// GTC-FR-OQJB: one state per repository, the main one first, and the top
// level describes the main repository.
#[test]
fn the_sync_state_lists_every_repository() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    p.bare_remote(&p.main());
    p.bare_remote(&sub);
    new_commit(&sub, "w\n");

    let state = upstream_sync_state(&p.root()).unwrap();
    let repos: Vec<&str> = state.repositories.iter().map(|r| r.repository.as_str()).collect();
    assert_eq!(repos, ["", "libs/sub"]);
    let main = &state.repositories[0];
    assert_eq!(
        (state.has_remote, state.has_upstream, state.ahead),
        (main.has_remote, main.has_upstream, main.ahead)
    );
    let sub_state = &state.repositories[1];
    assert_eq!(sub_state.branch.as_deref(), Some("main"));
    assert_eq!(sub_state.ahead, Some(1));
    assert!(sub_state.is_affected());
    assert!(!main.is_affected());
}

// GSM-FR-REIT: whether a push publishes something, as a table.
#[test]
fn a_repository_is_affected_when_its_push_publishes_something() {
    let state = |branch: Option<&str>, has_remote, has_upstream, ahead| RepositorySyncState {
        repository: "x".into(),
        branch: branch.map(str::to_string),
        has_remote,
        has_upstream,
        ahead,
        behind: ahead.map(|_| 0),
    };
    assert!(state(Some("main"), true, true, Some(2)).is_affected());
    assert!(state(Some("main"), true, false, None).is_affected(), "never published");
    assert!(!state(Some("main"), true, true, Some(0)).is_affected());
    assert!(!state(Some("main"), false, false, None).is_affected(), "no remote");
    assert!(!state(None, true, false, None).is_affected(), "detached");
}

// GTC-FR-PQQV, GSM-FR-REIT: the affected submodules are pushed deepest first
// and the main repository last; one that is not affected is in no list.
#[test]
fn affected_repositories_are_pushed_deepest_first() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    let quiet = p.add_submodule("", "quiet", &[("q.md", "q\n")]);
    let remotes = [
        p.bare_remote(&inner),
        p.bare_remote(&sub),
        p.bare_remote(&p.main()),
        p.bare_remote(&quiet),
    ];
    new_commit(&inner, "i\n");
    new_commit(&sub, "s\n");
    commit_all(&p.main(), "record pointers");

    let (result, finished, _) = push(&p.root(), &SCRATCH_BUFFER);
    result.unwrap();
    assert_eq!(finished["ok"], serde_json::json!(true));
    assert_eq!(
        finished["pushSummary"]["pushed"],
        serde_json::json!(["libs/sub/inner", "libs/sub", ""])
    );
    assert_eq!(finished["pushSummary"]["skipped"], serde_json::json!([]));
    assert_eq!(remote_tip(&remotes[0]), head_of(&inner));
    assert_eq!(remote_tip(&remotes[1]), head_of(&sub));
    assert_eq!(remote_tip(&remotes[2]), head_of(&p.main()));
}

// GSM-FR-MJHC, GSM-FR-WPKJ: a detached submodule and one with no remote are
// skipped with their reasons and logged, and the push goes on.
#[test]
fn detached_and_remoteless_submodules_are_skipped() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let detached = p.add_submodule("", "a", &[("a.md", "a\n")]);
    let lonely = p.add_submodule("", "b", &[("b.md", "b\n")]);
    let main_remote = p.bare_remote(&p.main());
    new_commit(&detached, "d\n");
    detached.set_head_detached(head_of(&detached)).unwrap();
    lonely.remote_delete("origin").unwrap();
    commit_all(&p.main(), "record pointers");

    let buffer = own_buffer();
    let (result, finished, _) = push(&p.root(), buffer);
    result.unwrap();
    assert_eq!(
        finished["pushSummary"]["skipped"],
        serde_json::json!([
            { "repository": "a", "reason": "detached_head" },
            { "repository": "b", "reason": "no_remote" },
        ])
    );
    assert_eq!(finished["pushSummary"]["pushed"], serde_json::json!([""]));
    assert_eq!(remote_tip(&main_remote), head_of(&p.main()));
    let skips: Vec<(serde_json::Value, serde_json::Value)> =
        records_for(buffer, "push skipped a repository")
            .into_iter()
            .map(|r| (r.fields["repository"].clone(), r.fields["reason"].clone()))
            .collect();
    assert_eq!(
        skips,
        [
            (serde_json::json!("a"), serde_json::json!("detached_head")),
            (serde_json::json!("b"), serde_json::json!("no_remote")),
        ]
    );
}

// GSM-FR-GTPI, GTC-FR-PQQV: a failed submodule push stops the run; the push
// made before it stays and the main repository is not pushed.
#[test]
fn a_failed_push_stops_the_run() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    let inner_remote = p.bare_remote(&inner);
    p.bare_remote(&sub);
    let main_remote = p.bare_remote(&p.main());
    let main_before = remote_tip(&main_remote);
    new_commit(&inner, "i\n");
    new_commit(&sub, "s\n");
    commit_all(&p.main(), "record pointers");
    // The submodule's remote is gone, so its push fails.
    sub.remote_set_url("origin", &p.tmp.path().join("gone").to_string_lossy()).unwrap();

    let (result, finished, _) = push(&p.root(), &SCRATCH_BUFFER);
    assert!(result.is_err());
    assert_eq!(finished["ok"], serde_json::json!(false));
    assert_eq!(finished["pushSummary"]["pushed"], serde_json::json!(["libs/sub/inner"]));
    assert_eq!(remote_tip(&inner_remote), head_of(&inner));
    assert_eq!(remote_tip(&main_remote), main_before, "the main repository was not pushed");
}

// GTC-FR-04, GTC-FR-PQQV: the push of a named branch and a pull carry no
// push summary.
#[test]
fn a_finished_event_with_no_summary_has_no_summary_key() {
    let finished = GitOperationFinished {
        operation: OPERATION_PUSH.into(),
        ok: true,
        error: None,
        push_summary: None,
    };
    let json = serde_json::to_value(finished).unwrap();
    assert!(json.get("pushSummary").is_none());
}

// GSM-FR-REIT, GSM-FR-WPKJ: with submodules, a main repository that has
// nothing to publish is not pushed, and each push is logged with its
// repository.
#[test]
fn a_level_main_repository_is_not_pushed() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let sub_remote = p.bare_remote(&sub);
    p.bare_remote(&p.main());
    new_commit(&sub, "s\n");

    let buffer = own_buffer();
    let (result, finished, _) = push(&p.root(), buffer);
    result.unwrap();
    assert_eq!(finished["pushSummary"]["pushed"], serde_json::json!(["libs/sub"]));
    assert_eq!(remote_tip(&sub_remote), head_of(&sub));
    let pushed = record_for(buffer, "pushed a repository");
    assert_eq!(pushed.fields["repository"], serde_json::json!("libs/sub"));
}

// GSM-FR-GTPI, GSM-FR-WPKJ: the failure record of a stopped run names the
// repository that failed, and the error stays the typed cause.
#[test]
fn a_failed_push_names_its_repository() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    p.bare_remote(&sub);
    new_commit(&sub, "s\n");
    sub.remote_set_url("origin", &p.tmp.path().join("gone").to_string_lossy()).unwrap();

    let buffer = own_buffer();
    let (result, _, _) = push(&p.root(), buffer);
    let err = result.unwrap_err();
    assert!(!err.starts_with("libs/sub"), "the typed cause is kept: {err}");
    let failed: Vec<_> = records_for(buffer, MSG_PUSH_FAILED)
        .into_iter()
        .filter(|r| r.fields.get("repository") == Some(&serde_json::json!("libs/sub")))
        .collect();
    assert_eq!(failed.len(), 1);
}

// GSM-FR-MJHC, GSM-FR-REIT: with submodules, a main repository with no
// remote is skipped like any member, and the submodule is still pushed.
#[test]
fn a_main_repository_with_no_remote_is_skipped() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let sub_remote = p.bare_remote(&sub);
    new_commit(&sub, "s\n");
    let (result, finished, _) = push(&p.root(), &SCRATCH_BUFFER);
    result.unwrap();
    assert_eq!(finished["pushSummary"]["pushed"], serde_json::json!(["libs/sub"]));
    assert_eq!(
        finished["pushSummary"]["skipped"],
        serde_json::json!([{ "repository": "", "reason": "no_remote" }])
    );
    assert_eq!(remote_tip(&sub_remote), head_of(&sub));
}
