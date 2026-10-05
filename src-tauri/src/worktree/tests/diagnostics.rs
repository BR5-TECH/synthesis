//! The diagnostic buffer across a switch, and the last reads a closed project
//! makes.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

#[test]
fn a_checkout_discards_the_diagnostic_buffer_before_it_runs_not_after() {
    // LGC-FR-15: the clear precedes the operation that performs the switch,
    // so that operation's own records land in the fresh buffer. A checkout
    // is the one switch that does its work *before* re-rooting, so the
    // ordering is load-bearing in both directions — and this test fails if
    // either half moves: no clear at all leaves the marker behind, and a
    // clear at the re-rooting instead takes the checkout's own records with
    // it, making a successful checkout the one outcome a reader could never
    // find in the Logs panel.
    let log = own_buffer();
    let f = Fixture::new();
    f.branch("develop");
    let app = mock_app_on(&f.root(), &f.root());
    let handle = app.handle().clone();
    let store = GlobalSettingsStore::in_memory();
    mark(&handle, log);

    check_out_branch_at(
        &handle,
        log,
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        "develop",
    )
    .unwrap();

    let messages = messages_in(log);
    assert!(
        !messages.iter().any(|m| m == "from before the switch"),
        "the buffer is discarded for the switch: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m == "branch checked out"),
        "and the checkout's own record survives the re-rooting that follows \
         it: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m == "content root changed"),
        "the re-rooting reports itself into the same fresh buffer: \
         {messages:?}"
    );
}

#[test]
fn a_refused_checkout_leaves_its_refusal_in_the_fresh_buffer() {
    // LGC-FR-14, LGC-FR-15's second clause: a switch that fails still explains itself,
    // in the buffer the clear made room for. Nothing is re-rooted, so no
    // `"content root changed"` follows.
    let log = own_buffer();
    let f = Fixture::new();
    let app = mock_app_on(&f.root(), &f.root());
    let handle = app.handle().clone();
    let store = GlobalSettingsStore::in_memory();
    mark(&handle, log);

    let err = check_out_branch_at(
        &handle,
        log,
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        "no-such-branch",
    )
    .unwrap_err();
    assert_eq!(err, crate::changes::ERR_UNKNOWN_BRANCH);

    let messages = messages_in(log);
    assert!(!messages.iter().any(|m| m == "from before the switch"));
    assert!(
        messages.iter().any(|m| m == "checkout failed"),
        "the refusal is on the record: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m == "content root changed"),
        "a refused checkout re-roots nothing (WTC-FR-12): {messages:?}"
    );
}

#[test]
fn the_refresh_command_wires_the_real_primitive_and_the_anchors_slot_key() {
    // The command itself cannot be called under the mock runtime (it is async
    // and takes a `Wry` AppHandle), so its wiring is pinned at the source
    // level — the same trick `git.rs` uses to prove the fetch is unregistered.
    //
    // Two claims, both silent if broken: that the composed function is GTC's
    // primitive rather than something reimplemented here (WTC-FR-22), and that
    // the token is resolved against the project's ANCHOR slot. Using the
    // active worktree's path instead would make a project resolve a different
    // token depending on which checkout happened to be active.
    let source = include_str!("../../worktree.rs");
    let body = source
        .split_once("pub async fn refresh_worktrees_and_branches")
        .expect("the refresh command")
        .1
        .split_once("\n}")
        .expect("end of the command body")
        .0;
    assert!(
        body.contains("crate::git::fetch_remote_branches"),
        "the refresh must compose GTC's fetch primitive"
    );
    assert!(
        body.contains("project.slot_key()"),
        "the token binding is keyed by the project's anchor, not by the \
         active worktree"
    );
    assert!(
        !body.contains("reroot("),
        "a refresh re-roots nothing (WTC-FR-24)"
    );
}

#[test]
fn the_branches_changed_event_name_is_one_tauri_will_actually_deliver() {
    // Same silent-failure trap as the channel above: a name Tauri rejects
    // leaves the Git panel's branch listing permanently stale while every
    // test that stubs the sink stays green.
    use tauri::Emitter;
    let app = tauri::test::mock_app();
    assert!(
        app.emit(BRANCHES_CHANGED, ()).is_ok(),
        "{BRANCHES_CHANGED:?} is not a deliverable Tauri event name"
    );
}

#[test]
fn a_pruned_remote_ref_leaves_the_local_branch_that_tracked_it_intact() {
    // WTC-FR-04, WTC-FR-06, second half (WTC-FR-19 / GTC-FR-13): a remote-tracking ref
    // is a local cache of what the remote publishes. Dropping one deletes no
    // branch — which is what keeps the prune outside this module's
    // no-deletion rule rather than an exception to it.
    let f = Fixture::new();
    f.branch("spike");
    f.remote_ref("origin/spike");
    let head_before = f
        .repo()
        .find_branch("spike", BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();

    refresh_outcome_for(&f.root(), || {
        fake_fetch(&f, &[], &["origin/spike"]);
        Ok(())
    })
    .unwrap();

    let repo = f.repo();
    let local = repo
        .find_branch("spike", BranchType::Local)
        .expect("the local branch survives the prune");
    assert_eq!(
        local.get().peel_to_commit().unwrap().id(),
        head_before,
        "with its commits intact"
    );
    assert_eq!(
        context_for(&f.root()).unwrap().worktrees.len(),
        1,
        "and no worktree was removed"
    );
}

#[test]
fn a_closed_project_leaves_this_module_with_nothing_to_reach() {
    // WTC-FR-20: this module holds no state of its own beyond the content
    // root, so `close_project`'s teardown is its teardown — afterwards
    // every command returns "no project open" rather than touching the
    // closed project's files, and nothing is left to emit an event.
    let state = ProjectState::default();
    state.set_root(PathBuf::from("/tmp/whatever"));
    state.set_anchor("/tmp/whatever".into());
    crate::project::deactivate_project(
        &state,
        &ProjectWatcher::default(),
        &crate::artifacts::ContentTracker::default(),
        &crate::draft_watcher::DraftsWatcher::default(),
        &crate::scanning::CandidateStore::default(),
        &crate::scanning::AttributionBaseline::default(),
    );
    assert_eq!(state.require_root().unwrap_err(), "no project open");
    assert_eq!(state.anchor(), None);
}
