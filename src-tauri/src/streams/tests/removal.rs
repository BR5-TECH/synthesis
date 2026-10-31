//! The sweep of a stream whose working copy is gone, and its notices.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::Listener;

use super::merge_support::save_plain_run;
use super::*;
use crate::graduation::{GraduationRunState, QueueIndex, QueueIndexEntry};

/// How many events of one channel the app delivers from now on.
fn count_events(fx: &Fixture, channel: &'static str) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    fx.app.listen(channel, move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    count
}

/// Wait until the count arrives. Tauri defers an emit that meets a held lock, and
/// a sandbox stop logs from another thread, so the count is a state and not a
/// result of the call. The bound applies only to a test that is failing.
fn wait_for_count(count: &AtomicUsize, expected: usize) {
    let end = Instant::now() + Duration::from_secs(60);
    while count.load(Ordering::SeqCst) < expected && Instant::now() < end {
        std::thread::yield_now();
    }
    assert_eq!(count.load(Ordering::SeqCst), expected);
}

fn fs_of(fx: &Fixture) -> std::sync::Arc<crate::fs::FsAccess> {
    fx.app.state::<crate::fs::FsAccessState>().get().expect("instance")
}

fn store_of(fx: &Fixture) -> StreamStore {
    StreamStore::new(canonical(fx.store.path()))
}

fn remove_working_copy(stream: &WorkStream) {
    std::fs::remove_dir_all(&stream.worktree_path).expect("removed");
}

/// Delete the branch ref alone. Git refuses `branch -D` of a branch a worktree
/// holds, and this is the state that remains when a ref goes another way.
fn delete_branch_ref(fx: &Fixture, stream: &WorkStream) {
    fx.repo().find_reference(&format!("refs/heads/{}", stream.branch)).unwrap().delete().unwrap();
}

fn write_index(fx: &Fixture, entries: Vec<QueueIndexEntry>) {
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let base = crate::graduation::store_base(&fx.app).expect("base");
    let index = QueueIndex { project_key: key, runs: entries };
    crate::graduation::write_queue_index(&fs_of(fx), &base, &index).expect("written");
}

fn run_of(stream: &WorkStream, state: GraduationRunState) -> QueueIndexEntry {
    QueueIndexEntry {
        run_id: "r1".into(),
        stream_id: stream.id.clone(),
        state,
        ..Default::default()
    }
}

fn notices(fx: &Fixture) -> Vec<RemovedStreamNotice> {
    list_removed_work_streams(fx.app.clone()).expect("notices")
}

// WKS-FR-APUE, WKS-FR-AXRD, WKS-FR-VYIW, WKS-FR-STSZ, WKS-FR-YLBX: a listing
// removes the record, prunes the dead registration, and keeps the branch.
#[test]
fn a_listing_removes_a_stream_whose_working_copy_is_gone_and_keeps_its_branch() {
    let fx = Fixture::new();
    let kept = fx.create("kept", None).expect("created");
    let gone = fx.create("gone", None).expect("created");
    remove_working_copy(&gone);

    let listed = list_work_streams(fx.app.clone()).expect("listed");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].stream.id, kept.id);
    assert!(!store_of(&fx).record(&gone.id).exists(), "the record left");
    assert!(branch_names(&fx.repo()).contains(&gone.branch), "the branch stays");
    let registered = fx.repo().worktrees().expect("worktrees");
    assert!(
        !registered.iter().flatten().flatten().any(|name| name == gone.id),
        "the dead registration left, so the branch counts as checked out nowhere",
    );
    assert_eq!(
        notices(&fx),
        vec![RemovedStreamNotice {
            stream_id: gone.id,
            name: "gone".into(),
            branch: gone.branch,
            branch_survives: true,
        }],
    );
}

// WKS-FR-APUE, WKS-FR-YLBX: with the branch gone too, the notice says so.
#[test]
fn a_notice_says_when_the_branch_went_with_the_working_copy() {
    let fx = Fixture::new();
    let stream = fx.create("both", None).expect("created");
    remove_working_copy(&stream);
    delete_branch_ref(&fx, &stream);

    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());

    assert!(!notices(&fx)[0].branch_survives);
}

// WKS-FR-APUE: the update record goes with the stream.
#[test]
fn the_update_record_leaves_with_the_stream() {
    let fx = Fixture::new();
    let stream = fx.create("updated", None).expect("created");
    let path = store_of(&fx).root().join("wu").join(format!("{}.toml", stream.id));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "state = \"failed\"\n").unwrap();
    remove_working_copy(&stream);

    list_work_streams(fx.app.clone()).expect("listed");

    assert!(!path.exists(), "the update record left");
}

// WKS-FR-RVSU, WKS-FR-AXRD: a stream a run holds stays, flagged missing, and the
// first listing after the hold ends removes it.
#[test]
fn a_held_stream_stays_until_the_hold_ends() {
    let fx = Fixture::new();
    let mut stream = fx.create("held", None).expect("created");
    stream.busy_run_id = Some("r1".into());
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");
    remove_working_copy(&stream);

    let listed = list_work_streams(fx.app.clone()).expect("listed");
    assert_eq!(listed.len(), 1);
    assert!(listed[0].stream.is_missing);
    assert!(notices(&fx).is_empty());

    stream.busy_run_id = None;
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");
    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
}

// WKS-FR-RVSU: every state that is not terminal keeps the stream.
#[test]
fn each_non_terminal_run_state_keeps_the_stream() {
    for state in [
        GraduationRunState::Queued,
        GraduationRunState::Working,
        GraduationRunState::Reviewing,
        GraduationRunState::AwaitingAuthor,
        GraduationRunState::Blocked,
        GraduationRunState::Interrupted,
    ] {
        let fx = Fixture::new();
        let stream = fx.create("queued", None).expect("created");
        write_index(&fx, vec![run_of(&stream, state)]);
        remove_working_copy(&stream);

        assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1, "{state:?}");
    }
}

// WKS-FR-FQAE, WKS-FR-RVSU, WKS-FR-FJYN: a real discard ends the run, the next
// listing removes the stream, and the run's own record stays.
#[test]
fn a_discard_of_the_blocking_run_releases_the_stream_and_keeps_the_run() {
    let fx = Fixture::new();
    let stream = fx.create("blocked", None).expect("created");
    let run = save_plain_run(&fx, &stream, GraduationRunState::Blocked);
    remove_working_copy(&stream);
    assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);

    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("discarded");

    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
    let kept = crate::graduation::get_graduation_run(fx.app.clone(), run.id).expect("kept");
    assert_eq!(kept.state, GraduationRunState::Discarded);
}

// WKS-FR-RVSU: a running merge and a running update keep the stream.
#[test]
fn a_running_merge_or_update_keeps_the_stream() {
    for kind in [Reconciliation::Merge, Reconciliation::Update] {
        let fx = Fixture::new();
        let stream = fx.create("reconciling", None).expect("created");
        let state = fx.app.state::<StreamState>();
        let hold = state.begin_repository_update(&stream.id, kind).expect("held");
        remove_working_copy(&stream);

        assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);

        drop(hold);
        assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
    }
}

// WKS-FR-RVSU: an update job keeps the stream.
#[test]
fn an_update_job_keeps_the_stream() {
    let fx = Fixture::new();
    let stream = fx.create("updating", None).expect("created");
    let state = fx.app.state::<StreamState>();
    assert!(state.claim_update_job(&stream.id));
    remove_working_copy(&stream);

    assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);

    state.release_update_job(&stream.id);
    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
}

// WKS-FR-RVSU, WKS-FR-AXRD: the stream that roots the project stays listed as
// missing while its root is gone, and the first listing after the move removes it.
#[test]
fn a_gone_stream_that_roots_the_project_stays_listed_until_the_project_moves() {
    let fx = Fixture::new();
    let stream = fx.create("active", None).expect("created");
    let project = fx.app.state::<crate::project::ProjectState>();
    project.set_root(canonical(&stream.worktree()));
    remove_working_copy(&stream);

    let listed = list_work_streams(fx.app.clone()).expect("the listing still answers");

    assert_eq!(listed.len(), 1);
    assert!(listed[0].stream.is_missing);
    assert!(notices(&fx).is_empty());

    project.set_root(fx.root());
    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
    assert_eq!(notices(&fx).len(), 1);
}

// WKS-FR-RVSU: a root below the stream's working copy holds it too.
#[test]
fn the_stream_that_roots_the_project_is_held() {
    let fx = Fixture::new();
    let stream = fx.create("active", None).expect("created");
    let live = std::collections::BTreeSet::new();
    let worktree = crate::changes::canonicalize_lenient(&stream.worktree());

    assert!(removal::is_held(None, &stream, &live, &worktree));
    assert!(removal::is_held(None, &stream, &live, &worktree.join("docs")));
    assert!(!removal::is_held(None, &stream, &live, &fx.root()));
}

// WKS-FR-AUSI: an index that exists and cannot be read keeps every stream.
#[test]
fn an_unreadable_run_order_index_keeps_every_stream() {
    let fx = Fixture::new();
    let stream = fx.create("unknown", None).expect("created");
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let path = crate::graduation::store_base(&fx.app).expect("base").queue_path(&key);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "this is = = not toml").unwrap();
    remove_working_copy(&stream);

    assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);
    assert!(notices(&fx).is_empty());
}

// WKS-FR-AUSI: a read that fails other than with "not found" keeps the stream.
// Its home in the store is gone too, so the failed read alone decides this.
#[test]
fn a_working_copy_the_application_cannot_read_keeps_the_stream() {
    let fx = Fixture::new();
    let mut stream = fx.create("elsewhere", None).expect("created");
    remove_working_copy(&stream);
    stream.worktree_path = "/outside/every/allowed/root".into();
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");

    assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);
    assert!(store_of(&fx).record(&stream.id).exists());
}

// WKS-FR-STSZ, WKS-FR-VYIW: after `git worktree move` the registration names a
// live directory. The sweep leaves that directory and that registration alone.
#[test]
fn a_moved_working_copy_keeps_its_directory_and_its_registration() {
    let fx = Fixture::new();
    let stream = fx.create("moved", None).expect("created");
    let moved = canonical(fx.store.path()).join("moved-away");
    std::fs::rename(&stream.worktree_path, &moved).expect("moved");
    let gitdir = fx.repo().path().join("worktrees").join(&stream.id).join("gitdir");
    std::fs::write(&gitdir, format!("{}\n", moved.join(".git").display())).unwrap();

    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());

    assert!(moved.join("README.md").exists(), "the moved copy is untouched");
    let registered = fx.repo().worktrees().expect("worktrees");
    assert!(registered.iter().flatten().flatten().any(|name| name == stream.id));
}

// WKS-FR-AUSI: a record that names another path than the stream's home proves
// nothing while that home stands.
#[test]
fn a_stream_whose_home_stands_stays_whatever_its_record_names() {
    let fx = Fixture::new();
    let mut stream = fx.create("relocated", None).expect("created");
    stream.worktree_path = canonical(fx.store.path()).join("no-such").display().to_string();
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");

    assert_eq!(list_work_streams(fx.app.clone()).expect("listed").len(), 1);
    assert!(notices(&fx).is_empty());
}

// WTC-FR-DZLO, WKS-FR-BSLO: a stream's working copy holds a relative commondir,
// and it reads as healthy. A false `gone` here would discard unsaved changes.
#[test]
fn a_stream_working_copy_reads_as_a_healthy_root() {
    let fx = Fixture::new();
    let stream = fx.create("healthy-root", None).expect("created");
    let root = crate::changes::canonicalize_lenient(&stream.worktree());

    let state = crate::worktree::root_state_of(&root, &fx.root(), Path::new(""));

    assert_eq!(state.state, crate::worktree::ActiveRootHealth::Healthy);
}

// WKS-FR-STSZ: a lock marks a path that is offline on purpose. The stream leaves,
// and the registration and its lock stay.
#[test]
fn a_locked_registration_stays_when_its_stream_leaves() {
    let fx = Fixture::new();
    let stream = fx.create("offline", None).expect("created");
    let admin = fx.repo().path().join("worktrees").join(&stream.id);
    std::fs::write(admin.join("locked"), "on the portable drive\n").unwrap();
    remove_working_copy(&stream);

    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());

    assert!(admin.join("locked").exists());
    assert_eq!(notices(&fx).len(), 1);
}

// WKS-FR-AUSI: a stream of another project is not this listing's to remove.
#[test]
fn a_gone_stream_of_another_project_is_left_alone() {
    let fx = Fixture::new();
    let mut stream = fx.create("theirs", None).expect("created");
    stream.project_key = "another project".into();
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");
    remove_working_copy(&stream);

    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());

    assert!(store_of(&fx).record(&stream.id).exists());
    assert!(notices(&fx).is_empty());
}

// WKS-FR-YLBX, WKS-FR-AIWE: a branch read that fails says nothing. The notice
// promises no branch, and the row does not call the branch missing.
#[test]
fn a_branch_read_that_fails_promises_nothing_and_flags_nothing() {
    let fx = Fixture::new();
    let mut gone = fx.create("unreadable", None).expect("created");
    gone.branch = "bad..name".into();
    store::write_stream(&fs_of(&fx), &store_of(&fx), &gone).expect("written");
    remove_working_copy(&gone);
    let mut live = fx.create("live", None).expect("created");
    live.branch = "bad..name".into();
    store::write_stream(&fs_of(&fx), &store_of(&fx), &live).expect("written");

    let listed = list_work_streams(fx.app.clone()).expect("listed");

    assert!(!notices(&fx)[0].branch_survives);
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].branch_missing);
}

// WKS-FR-AIWE, WKS-FR-EIBC: without the author's word, the uncommitted work of a
// stream with no branch stays.
#[test]
fn a_gone_branch_does_not_let_a_delete_past_uncommitted_work() {
    let fx = Fixture::new();
    let stream = fx.create("dirty", None).expect("created");
    let file = stream.worktree().join("uncommitted.md");
    std::fs::write(&file, "the author's work\n").unwrap();
    delete_branch_ref(&fx, &stream);

    let refusal = delete_work_stream(fx.app.clone(), stream.id, false, false).expect_err("refused");

    assert!(refusal.starts_with(ERR_STREAM_DIRTY), "{refusal}");
    assert!(file.exists());
}

// WKS-FR-AIWE, WKS-FR-SGCM: a healthy stream reports no missing branch.
#[test]
fn a_healthy_stream_reports_its_branch_present() {
    let fx = Fixture::new();
    fx.create("healthy", None).expect("created");

    assert!(!list_work_streams(fx.app.clone()).expect("listed")[0].branch_missing);
}

// WKS-FR-AIWE: the branch gone and the directory present keeps the stream, with
// the author's files, and `delete_work_stream` is what ends it.
#[test]
fn a_gone_branch_keeps_the_stream_and_delete_ends_it() {
    let fx = Fixture::new();
    let stream = fx.create("branchless", None).expect("created");
    let file = stream.worktree().join("uncommitted.md");
    std::fs::write(&file, "the author's work\n").unwrap();
    delete_branch_ref(&fx, &stream);

    let listed = list_work_streams(fx.app.clone()).expect("listed");

    assert_eq!(listed.len(), 1);
    assert!(listed[0].branch_missing);
    assert!(!listed[0].stream.is_missing);
    assert!(file.exists(), "the sweep touched no file");
    assert!(notices(&fx).is_empty());

    delete_work_stream(fx.app.clone(), stream.id.clone(), false, true).expect("deleted");
    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
}

// WKS-FR-AIWE: a stream with no directory is flagged missing rather than as
// missing its branch.
#[test]
fn a_held_stream_with_no_directory_does_not_report_its_branch_missing() {
    let fx = Fixture::new();
    let mut stream = fx.create("both-held", None).expect("created");
    stream.busy_run_id = Some("r1".into());
    store::write_stream(&fs_of(&fx), &store_of(&fx), &stream).expect("written");
    remove_working_copy(&stream);
    delete_branch_ref(&fx, &stream);

    let listed = list_work_streams(fx.app.clone()).expect("listed");

    assert!(listed[0].stream.is_missing);
    assert!(!listed[0].branch_missing);
}

// WKS-FR-DBWC: a listing that removes a stream announces it once.
#[test]
fn a_listing_that_removes_a_stream_emits_each_event_once() {
    let fx = Fixture::new();
    let stream = fx.create("announced", None).expect("created");
    remove_working_copy(&stream);
    let streams = count_events(&fx, WORK_STREAMS_CHANGED);
    let branches = count_events(&fx, crate::worktree::BRANCHES_CHANGED);

    list_work_streams(fx.app.clone()).expect("listed");

    wait_for_count(&streams, 1);
    wait_for_count(&branches, 1);
}

// WKS-FR-DBWC, WKS-FR-YLBX: two removals in one listing announce once, and their
// notices stand in removal order.
#[test]
fn two_removals_in_one_listing_emit_once_and_keep_their_order() {
    let fx = Fixture::new();
    let first = fx.create("first", None).expect("created");
    let second = fx.create("second", None).expect("created");
    remove_working_copy(&first);
    remove_working_copy(&second);
    let streams = count_events(&fx, WORK_STREAMS_CHANGED);

    list_work_streams(fx.app.clone()).expect("listed");

    wait_for_count(&streams, 1);
    let mut expected = [first.id, second.id];
    expected.sort();
    let ids: Vec<String> = notices(&fx).into_iter().map(|n| n.stream_id).collect();
    assert_eq!(ids, expected, "the store is read in id order");
}

// WKS-FR-DBWC: a listing that removes nothing emits nothing, also where it looked
// at a gone stream that a hold keeps. A reader reloads on this event, so an emit
// here would be a loop.
#[test]
fn a_listing_that_removes_nothing_emits_nothing() {
    let fx = Fixture::new();
    let held = fx.create("quiet", None).expect("created");
    let state = fx.app.state::<StreamState>();
    assert!(state.claim_update_job(&held.id));
    remove_working_copy(&held);
    let streams = count_events(&fx, WORK_STREAMS_CHANGED);
    let branches = count_events(&fx, crate::worktree::BRANCHES_CHANGED);

    list_work_streams(fx.app.clone()).expect("listed");
    list_work_streams(fx.app.clone()).expect("listed");

    assert_eq!(streams.load(Ordering::SeqCst), 0);
    assert_eq!(branches.load(Ordering::SeqCst), 0);
}

// WKS-FR-YLBX: a read consumes nothing, a dismissal drops one notice, and an id
// with no notice is no error.
#[test]
fn a_notice_stays_until_it_is_dismissed() {
    let fx = Fixture::new();
    let stream = fx.create("noticed", None).expect("created");
    remove_working_copy(&stream);
    list_work_streams(fx.app.clone()).expect("listed");

    assert_eq!(notices(&fx).len(), 1);
    assert_eq!(notices(&fx).len(), 1, "a read consumed nothing");

    dismiss_removed_work_stream(fx.app.clone(), stream.id).expect("dismissed");
    assert!(notices(&fx).is_empty());
    dismiss_removed_work_stream(fx.app.clone(), "wnothing".into()).expect("no error");
}

// WKS-FR-YLBX: notices are in removal order, and one project reads its own.
#[test]
fn notices_are_in_order_and_belong_to_one_project() {
    let fx = Fixture::new();
    let state = fx.app.state::<StreamState>();
    let notice = |id: &str| RemovedStreamNotice {
        stream_id: id.into(),
        name: id.into(),
        branch: id.into(),
        branch_survives: true,
    };
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    state.record_removed(&key, notice("first"));
    state.record_removed("another project", notice("theirs"));
    state.record_removed(&key, notice("second"));

    let ids: Vec<String> = notices(&fx).into_iter().map(|n| n.stream_id).collect();
    assert_eq!(ids, ["first", "second"]);

    dismiss_removed_work_stream(fx.app.clone(), "theirs".into()).expect("no error");
    assert_eq!(state.removed_notices("another project").len(), 1, "not this project's notice");
}

// WKS-FR-HRUZ: a branch a removed stream left refuses the name, and the refusal
// names the branch. Nothing is made and the branch stays on its commit.
#[test]
fn a_surviving_branch_refuses_its_name_and_names_the_branch() {
    let fx = Fixture::new();
    let stream = fx.create("again", None).expect("created");
    let tip = |fx: &Fixture| {
        fx.repo().refname_to_id(&format!("refs/heads/{}", stream.branch)).expect("a branch")
    };
    let before = tip(&fx);
    remove_working_copy(&stream);
    list_work_streams(fx.app.clone()).expect("listed");

    let refusal = fx.create("again", None).expect_err("refused");

    assert_eq!(refusal, format!("{ERR_STREAM_BRANCH_EXISTS}: {}", stream.branch));
    assert_eq!(tip(&fx), before);
    assert!(list_work_streams(fx.app.clone()).expect("listed").is_empty());
}

// WKS-FR-HRUZ: any branch of that name refuses, not only one a stream left.
#[test]
fn an_unrelated_branch_of_that_name_refuses_too() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("synthesis/stream/taken-by-hand", &head, false).unwrap();

    let refusal = fx.create("taken by hand", None).expect_err("refused");

    assert!(refusal.starts_with(ERR_STREAM_BRANCH_EXISTS), "{refusal}");
}

// WKS-FR-HRUZ: the live stream's refusal comes first, and it stays the typed one.
#[test]
fn a_live_stream_still_refuses_with_name_taken() {
    let fx = Fixture::new();
    fx.create("taken", None).expect("created");

    assert_eq!(fx.create("taken", None).expect_err("refused"), ERR_STREAM_NAME_TAKEN);
}
