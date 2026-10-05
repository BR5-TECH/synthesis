//! Direct graduation: a run that works in the author's own worktree
//! (`GSU-graduation-start.md` GSU-FR-PVFP, `GRD-graduation.md` GRD-FR-BSNI).
//!
//! Every scenario drives the production start, queue, loop and commit with the
//! execution agent removed at the dispatch seam.

use super::*;
use tauri::Listener;

impl Fixture {
    /// The branch the project's worktree holds.
    fn branch(&self) -> String {
        self.repo()
            .head()
            .expect("a head")
            .shorthand()
            .expect("a branch")
            .to_string()
    }

    /// Commit everything standing in the project's worktree, so it is clean.
    fn commit_all(&self, message: &str) {
        let repo = self.repo();
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("add");
        index.write().expect("write");
        let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let signature = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&parent])
            .expect("commit");
    }

    /// A second branch at the current head, without checking it out.
    pub(super) fn other_branch(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).expect("a branch");
    }

    /// Move the project's worktree onto a branch, as an outside checkout does.
    pub(super) fn check_out(&self, name: &str) {
        let repo = self.repo();
        let object = repo
            .revparse_single(&format!("refs/heads/{name}"))
            .expect("the branch");
        repo.checkout_tree(&object, Some(git2::build::CheckoutBuilder::new().force()))
            .expect("checkout");
        repo.set_head(&format!("refs/heads/{name}")).expect("head");
    }

    /// A draft made committed, so the worktree stands clean around it.
    fn committed_draft(&self, name: &str) -> String {
        let id = self.draft(name);
        self.commit_all("draft");
        id
    }

    /// A queued direct run on the project's own worktree.
    pub(super) fn direct(&self, prompt: &str, draft_id: &str) -> GraduationRun {
        self.direct_in(&self.root(), &self.branch(), None, prompt, draft_id)
    }

    /// A queued direct run on a worktree and branch the caller names.
    pub(super) fn direct_in(
        &self,
        worktree: &Path,
        branch: &str,
        stream: Option<&crate::streams::WorkStream>,
        prompt: &str,
        draft_id: &str,
    ) -> GraduationRun {
        let input = CapturedGraduationInput {
            draft_id: draft_id.to_string(),
            draft_name: "editor draft".to_string(),
            prompt: prompt.to_string(),
            prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
            captured_at: crate::notes::now_rfc3339(),
        };
        let mut run = crate::graduation::commands::build_run(
            &self.project_key(),
            stream.map(|s| s.id.clone()).unwrap_or_default(),
            stream.map(|s| s.name.clone()).unwrap_or_default(),
            Some(DirectTarget {
                worktree_path: worktree.to_string_lossy().into_owned(),
                worktree_name: "wt".to_string(),
                branch: branch.to_string(),
            }),
            input,
            StandingWork::Keep,
            None,
        );
        crate::graduation::save_run(&self.app, &mut run).expect("a saved run");
        run
    }

    fn start_direct(&self, draft: &str) -> Result<GraduationRun, String> {
        crate::graduation::start_direct_graduation(
            self.app.clone(),
            draft.to_string(),
            self.root().to_string_lossy().into_owned(),
            self.branch(),
        )
    }

    fn runs(&self) -> Vec<GraduationRun> {
        crate::graduation::list_graduation_queue(self.app.clone())
            .expect("the queue")
            .runs
    }

    fn tripwire(&self) -> Vec<(String, bool)> {
        self.app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take()
    }
}

fn head_tree_holds(root: &Path, path: &str) -> bool {
    let repo = git2::Repository::open(root).expect("repo");
    let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
    tree.get_path(Path::new(path)).is_ok()
}

fn tripwire_fixture() -> Fixture {
    let fx = Fixture::new();
    fx.app
        .manage(crate::graduation::driver::drive::SpawnTripwire::default());
    fx
}

// ---------------------------------------------------------------------------
// The preflight and the start (GSU)
// ---------------------------------------------------------------------------

/// GSU-FR-MZGD: the preflight names the active worktree, its branch and every
/// uncommitted path, and creates nothing.
#[test]
fn the_preflight_reports_the_branch_and_every_uncommitted_path() {
    let fx = Fixture::new();
    let clean = crate::graduation::preflight_direct_graduation(fx.app.clone()).expect("read");
    assert_eq!(clean.branch.as_deref(), Some(fx.branch().as_str()));
    assert!(!clean.is_detached);
    assert!(clean.dirty_paths.is_empty());
    assert_eq!(clean.worktree_path, fx.root().to_string_lossy());
    assert!(clean.stream.is_none());

    std::fs::write(fx.root().join("b.txt"), "b\n").unwrap();
    std::fs::write(fx.root().join("a.txt"), "a\n").unwrap();
    let dirty = crate::graduation::preflight_direct_graduation(fx.app.clone()).expect("read");
    assert_eq!(dirty.dirty_paths, vec!["a.txt".to_string(), "b.txt".to_string()]);
    assert!(fx.runs().is_empty(), "the read created no run");
}

/// GSU-FR-MZGD, WKS-FR-JVLM: a direct start checks no worktree out, so no path
/// of the application-owned storage is uncommitted work, tracked or untracked.
/// The author's own work beside it is still named.
#[test]
fn a_direct_start_reads_past_the_application_owned_storage() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let draft = fx.committed_draft("direct");
    let tracked = fx
        .repo()
        .index()
        .expect("index")
        .iter()
        .map(|entry| String::from_utf8(entry.path).expect("a utf-8 path"))
        .find(|path| {
            path.starts_with(".synthesis/drafts/")
                && path.contains(&draft)
                && path.contains("/files/")
        })
        .expect("the committed prompt of the draft");
    let mut prompt = std::fs::read_to_string(fx.root().join(&tracked)).unwrap();
    prompt.push_str("An edit after the commit.\n");
    std::fs::write(fx.root().join(&tracked), prompt).unwrap();
    std::fs::write(fx.root().join(".synthesis/drafts/stray.txt"), "x\n").unwrap();
    std::fs::write(fx.root().join("a.txt"), "a\n").unwrap();

    let read = crate::graduation::preflight_direct_graduation(fx.app.clone()).expect("read");
    assert_eq!(read.dirty_paths, vec!["a.txt".to_string()]);

    // GSU-FR-SZTZ: the start uses the same reading, so only the author's own
    // work refuses it, and a refused start asks for no commit (GSU-FR-RNOM).
    let root = crate::fs::RootFs::for_root(fx.root());
    let graduation_events = || {
        crate::storage_floor::commit::pending_events(&root)
            .into_iter()
            .filter(|(id, event, _)| {
                id == &draft
                    && *event == crate::storage_floor::commit::DraftEvent::GraduationStarted
            })
            .map(|(_, _, message)| message)
            .collect::<Vec<_>>()
    };
    let refused = fx.start_direct(&draft).unwrap_err();
    assert_eq!(refused, format!("{ERR_DIRECT_WORKTREE_DIRTY}: a.txt"));
    assert!(graduation_events().is_empty(), "a refused start commits nothing");

    // GSU-FR-RNOM / PST-FR-YWXF: an enqueued start asks for one commit.
    std::fs::remove_file(fx.root().join("a.txt")).unwrap();
    fx.start_direct(&draft).expect("the draft storage does not refuse it");
    assert_eq!(graduation_events(), vec!["draft: graduate \"direct\"".to_string()]);
}

/// GSU-FR-MZGD: a worktree that is a stream's working copy says which stream.
#[test]
fn the_preflight_names_the_stream_of_a_stream_worktree() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    fx.app
        .state::<crate::project::ProjectState>()
        .set_root_with_access(
            stream.worktree(),
            fx.app.state::<crate::fs::FsAccessState>().get(),
        );
    let found = crate::graduation::preflight_direct_graduation(fx.app.clone()).expect("read");
    assert_eq!(found.stream.expect("a stream").stream_id, stream.id);
}

/// GSU-FR-PVFP, GSU-FR-ZTTS, GRD-FR-BSNI, GRD-FR-VLFO: a start pins the worktree
/// and branch, records the choice that commits nothing, and takes the latest
/// place in the project's order.
#[test]
fn a_direct_start_pins_the_worktree_and_the_branch() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let draft = fx.committed_draft("direct");
    let before = fx.stream("other");
    let earlier = fx.enqueue(&before, "an earlier run");

    let run = fx.start_direct(&draft).expect("a direct run");

    let target = run.direct_target.as_ref().expect("a pinned target");
    assert_eq!(target.worktree_path, fx.root().to_string_lossy());
    assert_eq!(target.branch, fx.branch());
    assert_eq!(run.standing_work, StandingWork::Keep);
    assert!(run.standing_work_message.is_none());
    assert!(run.stream_id.is_empty(), "an ordinary worktree names no stream");
    assert_eq!(run.state, GraduationRunState::Queued);
    assert!(run.auto_start);
    let order: Vec<String> = fx.runs().into_iter().map(|r| r.id).collect();
    assert_eq!(order, vec![earlier.id, run.id.clone()]);
    // GRD-FR-BSNI: the pin is durable.
    assert_eq!(fx.reload(&run.id).direct_target, run.direct_target);
}

/// GSU-FR-SZTZ, GSU-FR-PVFP: each refusal names its cause and leaves no run,
/// no queue entry and no draft lock.
#[test]
fn a_direct_start_refuses_a_worktree_it_cannot_start_from() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let draft = fx.committed_draft("direct");
    let root = fx.root().to_string_lossy().into_owned();
    let branch = fx.branch();

    // The active worktree is not the one the author confirmed.
    let moved = crate::graduation::start_direct_graduation(
        fx.app.clone(),
        draft.clone(),
        "/somewhere/else".into(),
        branch.clone(),
    )
    .unwrap_err();
    assert!(crate::git::is_worktree_identity_changed(&moved), "{moved}");

    // The branch is not the one the author confirmed.
    let changed = crate::graduation::start_direct_graduation(
        fx.app.clone(),
        draft.clone(),
        root.clone(),
        "elsewhere".into(),
    )
    .unwrap_err();
    assert_eq!(changed, ERR_DIRECT_BRANCH_CHANGED);

    // Uncommitted work: the complete set is named.
    std::fs::write(fx.root().join("b.txt"), "b\n").unwrap();
    std::fs::write(fx.root().join("a.txt"), "a\n").unwrap();
    let dirty = fx.start_direct(&draft).unwrap_err();
    assert_eq!(dirty, format!("{ERR_DIRECT_WORKTREE_DIRTY}: a.txt, b.txt"));
    std::fs::remove_file(fx.root().join("a.txt")).unwrap();
    std::fs::remove_file(fx.root().join("b.txt")).unwrap();

    // A detached worktree has no branch to pin.
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(head).unwrap();
    let detached = crate::graduation::start_direct_graduation(
        fx.app.clone(),
        draft.clone(),
        root,
        branch,
    )
    .unwrap_err();
    assert_eq!(detached, ERR_DIRECT_WORKTREE_DETACHED);

    assert!(fx.runs().is_empty(), "no refusal left a run");
    assert!(
        crate::graduation::require_unlocked_draft(&fx.app, &draft).is_ok(),
        "no refusal locked the draft"
    );
}

/// GSU-FR-SZTZ: the image preflight follows the worktree checks, and a machine
/// that cannot execute an agent costs no run.
#[test]
fn a_direct_start_takes_the_image_preflight() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let refused = fx.start_direct(&draft).unwrap_err();
    assert!(refused.starts_with("vendor_"), "{refused}");
    assert!(fx.runs().is_empty());
}

/// GSU-FR-ZTTS, GRD-FR-RFRU: cleanliness is checked at the start and nowhere
/// else, so work the author adds afterwards is neither refused nor lost: it is
/// part of the commit at review.
#[test]
fn edits_made_after_confirmation_are_part_of_the_result_commit() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    std::fs::write(fx.root().join("mine.md"), "written after confirmation\n").unwrap();
    let root = fx.root();
    let during = root.clone();

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work()
                .writing("src/panel.ts", "1\n")
                .doing(move || std::fs::write(during.join("later.md"), "written while it worked\n").unwrap()),
            Turn::ready(),
        ]),
    );

    assert_eq!(run.state, GraduationRunState::Completed);
    assert!(head_tree_holds(&root, "src/panel.ts"));
    assert!(head_tree_holds(&root, "mine.md"), "an edit made before dispatch");
    assert!(head_tree_holds(&root, "later.md"), "an edit made during the run");
    let outcome = run.standing_work_outcome.expect("an outcome");
    assert_eq!(outcome.commit, None, "no standing work was committed first");
}

// ---------------------------------------------------------------------------
// Queues and the project's limit (GRD)
// ---------------------------------------------------------------------------

/// GRD-FR-ZVNO, GRD-FR-VLFO, GRD-FR-BNTC: runs on one ordinary worktree share
/// one durable queue, and the second waits while the first holds the worktree.
#[test]
fn direct_runs_on_one_worktree_share_a_queue_and_never_work_together() {
    let fx = Fixture::new();
    // A captured prompt of nothing is refused before any container exists, which
    // is the cheapest way to reach a run that holds its worktree.
    let first = fx.direct("   ", "d1");
    let second = fx.direct("second", "d2");
    assert_eq!(first.queue_key(), second.queue_key());
    assert!(is_worktree_queue_key(&first.queue_key()));

    let blocked = fx.drive(&first, ScriptedDispatch::new(Vec::new()));
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    let held = fx
        .app
        .state::<GraduationState>()
        .holder_of(&first.queue_key())
        .expect("the worktree is held");
    assert_eq!(held, first.id);

    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    assert!(!fx.try_drive(&second, dispatch.clone()), "the worktree is held");
    assert_eq!(dispatch.turns_taken(), 0);
    let index = crate::graduation::load_queue(
        &crate::graduation::store_fs(&fx.app).unwrap(),
        &crate::graduation::store_base(&fx.app).unwrap(),
        &fx.project_key(),
    );
    assert_eq!(
        crate::graduation::position_in_queue(&index, &second.id),
        Some(0),
        "the run waiting is first in the worktree's queue"
    );
}

/// GRD-FR-ZVNO, WKS-FR-NDSV: a direct run on a stream's working copy joins that
/// stream's queue, behind the stream runs that came first.
#[test]
fn a_direct_run_on_a_stream_worktree_shares_the_streams_queue() {
    let fx = tripwire_fixture();
    let stream = fx.stream("editor");
    let first = fx.enqueue_for(&stream, "a stream run", "d1");
    let second = fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "direct", "d2");
    let third = fx.enqueue_for(&stream, "another stream run", "d3");
    assert_eq!(second.queue_key(), stream.id);

    let queue = crate::graduation::project_queue(&fx.app).unwrap();
    let ids: Vec<&str> = crate::graduation::queue_of(&queue, &stream.id)
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, vec![first.id.as_str(), second.id.as_str(), third.id.as_str()]);

    // The first run works; the direct run waits for it and holds the stream's
    // one lock when its turn comes.
    let done = fx.drive(
        &first,
        ScriptedDispatch::new(vec![Turn::work().writing("a.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    let direct = fx.drive(
        &second,
        ScriptedDispatch::new(vec![Turn::work().writing("b.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(direct.state, GraduationRunState::Completed);
    assert!(!direct.commits.is_empty(), "committed on the stream's branch");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some(stream.branch.as_str()));
}

/// GRD-FR-ZVNO, WKS-FR-CYAG, WKS-FR-YBST, WKS-FR-OVLQ: a direct run on a stream
/// worktree marks the stream busy, counts in its queue depth, and keeps the
/// stream from being deleted.
#[test]
fn a_direct_run_on_a_stream_worktree_is_a_run_of_that_stream() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let direct = fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "   ", "d2");

    let listed = crate::streams::list_work_streams(fx.app.clone()).expect("listed");
    let found = listed.iter().find(|s| s.stream.id == stream.id).expect("the stream");
    assert_eq!(found.queued_run_count, 1, "the direct run is in the queue depth");

    // `force` reaches neither refusal: a run that has not ended keeps the stream.
    let refused =
        crate::streams::delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).unwrap_err();
    assert!(refused.starts_with(crate::streams::ERR_STREAM_HAS_RUNS), "{refused}");

    // Working: the stream is busy, and a switch to it is refused.
    let blocked = fx.drive(&direct, ScriptedDispatch::new(Vec::new()));
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    let record = crate::streams::stream_of(&fx.app, &stream.id).unwrap();
    assert_eq!(record.busy_run_id.as_deref(), Some(direct.id.as_str()));
}

/// WKS-FR-YSUB: a direct run on a stream's working copy is refused the claim
/// while a merge check of that stream holds the repository update guard, and
/// stays queued. A merge of a stream that holds such a run is refused.
#[test]
fn a_direct_run_on_a_stream_worktree_yields_to_a_merge_and_holds_one_off() {
    let fx = tripwire_fixture();
    let stream = fx.stream("editor");
    let direct = fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "work", "d1");

    // A merge check of the stream holds the guard: the claim is refused and the
    // run waits.
    let state = fx.app.state::<crate::streams::StreamState>();
    let hold = state
        .begin_repository_update(&stream.id, crate::streams::Reconciliation::Merge)
        .expect("the hold");
    assert_eq!(
        crate::streams::claim_stream(&fx.app, &stream.id, &direct.id).unwrap_err(),
        crate::streams::ERR_STREAM_BUSY
    );
    assert_eq!(fx.reload(&direct.id).state, GraduationRunState::Queued);
    drop(hold);

    // Free again, the same claim is taken.
    crate::streams::claim_stream(&fx.app, &stream.id, &direct.id).expect("claimed");
    let busy = crate::streams::stream_of(&fx.app, &stream.id).unwrap();
    assert_eq!(busy.busy_run_id.as_deref(), Some(direct.id.as_str()));
    crate::streams::release_stream(&fx.app, &stream.id, &direct.id).unwrap();

    // A merge of the stream is refused while the direct run has not ended.
    let refused = crate::streams::merge_work_stream_blocking(
        &fx.app,
        &stream.id,
        crate::streams::StreamMergePublication::Uncommitted,
    )
    .unwrap_err();
    assert!(refused.starts_with(crate::streams::ERR_STREAM_BUSY), "{refused}");
}

/// GRD-FR-KKKN: stream runs and direct runs share the project's limit, and a
/// run the limit refused starts when a place is free.
#[test]
fn stream_runs_and_direct_runs_share_the_projects_limit() {
    let fx = tripwire_fixture();
    let stream = fx.stream("editor");
    let direct = fx.direct("a direct run", "d2");

    // The limit is one: a stream run holds the only place.
    fx.hold_stream(&stream.id);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    assert!(!fx.try_drive(&direct, dispatch.clone()), "the limit is spent");
    assert_eq!(dispatch.turns_taken(), 0);

    // The place is freed, and the next dispatch of any queue offers it.
    fx.app.state::<GraduationState>().release(&stream.id);
    fx.tripwire();
    crate::graduation::advance_all_queues(&fx.app);
    let attempts = fx.tripwire();
    assert!(
        attempts.iter().any(|(run, _)| run == &direct.id),
        "the direct run was offered a place: {attempts:?}"
    );
}

// ---------------------------------------------------------------------------
// The pinned branch (GRD-FR-XRDY, GRD-FR-VAUE)
// ---------------------------------------------------------------------------

/// GRD-FR-XRDY: a queued direct run whose pinned branch is not checked out is
/// not dispatched, records why, and starts when the branch is back.
#[test]
fn a_run_waits_for_its_branch_and_starts_when_it_is_restored() {
    let fx = tripwire_fixture();
    let pinned = fx.branch();
    fx.other_branch("elsewhere");
    let run = fx.direct("held", "d1");

    fx.check_out("elsewhere");
    crate::graduation::advance_all_queues(&fx.app);
    assert!(fx.tripwire().is_empty(), "nothing was dispatched");
    let held = fx.reload(&run.id);
    assert_eq!(held.state, GraduationRunState::Queued);
    let hold = held.target_hold.expect("a recorded hold");
    assert_eq!(hold.code, HOLD_TARGET_BRANCH_CHANGED);
    assert_eq!(hold.expected_branch, pinned);
    assert_eq!(hold.actual_branch.as_deref(), Some("elsewhere"));
    assert_eq!(
        held.direct_target.unwrap().worktree_path,
        fx.root().to_string_lossy(),
        "the run was never redirected"
    );

    fx.check_out(&pinned);
    crate::graduation::advance_all_queues(&fx.app);
    let attempts = fx.tripwire();
    assert_eq!(attempts.len(), 1, "the run was offered a dispatch");
    assert_eq!(attempts[0].0, run.id);
    assert!(fx.reload(&run.id).target_hold.is_none(), "the hold is lifted");
}

/// GRD-FR-XRDY: the author's Continue offers a held run a dispatch at once.
#[test]
fn continue_offers_a_held_run_a_dispatch() {
    let fx = tripwire_fixture();
    let pinned = fx.branch();
    fx.other_branch("elsewhere");
    let run = fx.direct("held", "d1");
    fx.check_out("elsewhere");
    crate::graduation::advance_all_queues(&fx.app);
    fx.tripwire();

    fx.check_out(&pinned);
    crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone()).expect("continued");
    assert_eq!(fx.tripwire().len(), 1);
}

/// GRD-FR-XRDY: a held run is passed over, so the run behind it that pinned the
/// branch now checked out still starts.
#[test]
fn a_held_run_does_not_hold_back_a_run_pinned_to_the_present_branch() {
    let fx = tripwire_fixture();
    fx.other_branch("elsewhere");
    let held = fx.direct("held", "d1");
    fx.check_out("elsewhere");
    let behind = fx.direct_in(&fx.root(), "elsewhere", None, "behind", "d2");

    crate::graduation::advance_all_queues(&fx.app);
    let attempts = fx.tripwire();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].0, behind.id);
    assert_eq!(fx.reload(&held.id).state, GraduationRunState::Queued);
}

/// GRD-FR-VAUE: a branch that moved while the run worked receives no commit, and
/// the run rests blocked with its cause.
#[test]
fn a_commit_never_lands_on_another_branch() {
    let fx = Fixture::new();
    let pinned = fx.branch();
    fx.other_branch("elsewhere");
    let run = fx.direct("work", "d1");
    let root = fx.root();
    let heads_before = fx.repo().head().unwrap().peel_to_commit().unwrap().id();

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work().writing("src/panel.ts", "1\n"),
            Turn::ready().doing({
                let root = root.clone();
                move || {
                    let repo = git2::Repository::open(&root).unwrap();
                    repo.set_head("refs/heads/elsewhere").unwrap();
                }
            }),
        ]),
    );

    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(run.blocker.expect("a blocker").code, ERR_DIRECT_TARGET_CHANGED);
    assert!(run.commits.is_empty(), "nothing was committed");
    let repo = git2::Repository::open(&root).unwrap();
    let other = repo.find_branch("elsewhere", git2::BranchType::Local).unwrap();
    assert_eq!(other.get().peel_to_commit().unwrap().id(), heads_before);
    let pinned_branch = repo.find_branch(&pinned, git2::BranchType::Local).unwrap();
    assert_eq!(pinned_branch.get().peel_to_commit().unwrap().id(), heads_before);
}

// ---------------------------------------------------------------------------
// Switching (WTC)
// ---------------------------------------------------------------------------

/// WTC-FR-FBJQ, GRD-FR-OYPY: a run that was never dispatched blocks no switch.
/// From its first dispatch to its end it blocks every one, including while it
/// waits for the author or is paused.
#[test]
fn a_dispatched_direct_run_blocks_a_switch_until_it_ends() {
    let fx = Fixture::new();
    let run = fx.direct("work", "d1");
    assert!(
        crate::graduation::require_no_dispatched_direct_run(&fx.app).is_ok(),
        "queued and never dispatched"
    );

    // Dispatched, then resting on an escalation: still blocking.
    let asked = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Escalate(vec!["Which?".into()]))]),
    );
    assert_eq!(asked.state, GraduationRunState::AwaitingAuthor);
    let refused = crate::graduation::require_no_dispatched_direct_run(&fx.app).unwrap_err();
    assert!(refused.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{refused}");
    assert!(refused.ends_with(&run.id));

    // Discarded: the switch is free again.
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).unwrap();
    assert!(crate::graduation::require_no_dispatched_direct_run(&fx.app).is_ok());
}

/// WTC-FR-FBJQ, WTC-FR-TXKY: a paused run still blocks, and so does an
/// interrupted one; each route refuses with the same typed error and changes
/// nothing.
#[test]
fn every_switching_route_refuses_while_a_direct_run_stands() {
    let fx = Fixture::new();
    fx.other_branch("elsewhere");
    let run = fx.direct("work", "d1");
    let interrupted = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))]),
    );
    assert_eq!(interrupted.state, GraduationRunState::Interrupted);
    fx.app.manage(crate::watcher::ProjectWatcher::default());
    let before = fx.branch();
    let project = fx.app.state::<crate::project::ProjectState>();
    let watcher = fx.app.state::<crate::watcher::ProjectWatcher>();
    let store = fx.app.state::<crate::global_settings::GlobalSettingsStore>();

    let checkout = crate::worktree::check_out_branch_at(
        &fx.app,
        &crate::logging::BUFFER,
        &project,
        &watcher,
        &store,
        "elsewhere",
    )
    .unwrap_err();
    assert!(checkout.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{checkout}");
    assert_eq!(fx.branch(), before, "the checkout moved nothing");

    let activate = crate::worktree::activate_worktree_at(
        &fx.app,
        &project,
        &watcher,
        &store,
        &fx.root().to_string_lossy(),
    )
    .unwrap_err();
    assert!(activate.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{activate}");

    let copy = fx.root().with_extension("copy");
    let create = crate::worktree::create_worktree_in(
        &fx.app,
        &project,
        &watcher,
        &store,
        "elsewhere",
        &copy.to_string_lossy(),
    )
    .unwrap_err();
    assert!(create.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{create}");
    assert!(!copy.exists(), "nothing was created");

    // WTC-FR-TXKY: the three routes that open or create a project pass the
    // same guard when they land the open project on another worktree.
    let alt = fx.root().with_extension("alt");
    crate::worktree::create_worktree_at(&fx.root(), "alt", &alt.to_string_lossy())
        .expect("a second worktree");
    let active_before = project.root();
    let created = crate::project::create_project_routed(
        &fx.app,
        &store,
        &project,
        &watcher,
        "renamed".into(),
        crate::project::CreateMode::Standalone,
        alt.to_string_lossy().into_owned(),
    )
    .unwrap_err();
    assert!(created.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{created}");
    assert_eq!(project.root(), active_before, "create moved nothing");

    let handle = crate::project::ProjectHandle {
        name: "acme".into(),
        path: fx.root().to_string_lossy().into_owned(),
        active_worktree_path: alt.to_string_lossy().into_owned(),
        remembered_worktree_unavailable: false,
    };
    let opened = crate::project::open_resolved(&fx.app, &store, &project, &watcher, handle)
        .unwrap_err();
    assert!(opened.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{opened}");
    assert_eq!(project.root(), active_before, "open moved nothing");
}

/// WTC-FR-FBJQ: a direct run is claimed in memory before its index entry is
/// saved as dispatched. A switch in that window is refused too.
#[test]
fn a_claimed_direct_run_blocks_a_switch_before_it_is_saved() {
    use crate::tools::agent_exec::runtime::CancellationToken;
    let fx = Fixture::new();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = fx.direct("work", "d1");
    let state = fx.app.state::<GraduationState>();
    assert!(
        crate::graduation::require_no_dispatched_direct_run(&fx.app).is_ok(),
        "queued, not claimed"
    );

    assert!(state.claim(&run.queue_key(), &run.id, true, crate::project_settings::GraduationConcurrency::limited(2), CancellationToken::new()));
    let refused = crate::graduation::require_no_dispatched_direct_run(&fx.app).unwrap_err();
    assert!(refused.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{refused}");
    assert!(refused.ends_with(&run.id));

    state.release(&run.queue_key());
    assert!(crate::graduation::require_no_dispatched_direct_run(&fx.app).is_ok());

    // A claim of a stream run is not a direct claim.
    assert!(state.claim("stream-1", "other-run", false, crate::project_settings::GraduationConcurrency::limited(2), CancellationToken::new()));
    assert!(crate::graduation::require_no_dispatched_direct_run(&fx.app).is_ok());
}

/// WTC-FR-TXKY: opening the open project at another of its worktrees is a
/// switch, and opening it where it already stands, or opening another project,
/// is not one.
#[test]
fn a_project_open_that_lands_on_another_worktree_is_a_switch() {
    let fx = Fixture::new();
    let project = fx.app.state::<crate::project::ProjectState>();
    let anchor = fx.root().to_string_lossy().into_owned();
    let other = fx.root().with_extension("alt");
    crate::worktree::create_worktree_at(&fx.root(), "alt", &other.to_string_lossy())
        .expect("a second worktree");

    assert!(crate::worktree::opens_other_worktree(
        &project,
        &anchor,
        &other.to_string_lossy()
    ));
    assert!(!crate::worktree::opens_other_worktree(&project, &anchor, &anchor));
    assert!(!crate::worktree::opens_other_worktree(
        &project,
        "/some/other/project",
        &other.to_string_lossy()
    ));
}

// ---------------------------------------------------------------------------
// Restart, draft lifecycle and events
// ---------------------------------------------------------------------------

/// GRD-FR-PNMU, GSU-FR-NPFB: a restart of a discarded direct run works on the
/// worktree and branch the discarded run pinned, whatever is active now, and
/// makes no clean check.
#[test]
fn a_restart_uses_the_pinned_worktree_and_branch() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let pinned = fx.branch();
    let draft = fx.committed_draft("direct");
    // Made after the commit, so the branch it checks out still holds the
    // project's own settings.
    fx.other_branch("elsewhere");
    let original = fx.start_direct(&draft).expect("a direct run");
    crate::graduation::discard_graduation_run(fx.app.clone(), original.id.clone()).unwrap();
    // The discarded run's work stays in the worktree, and the author moved on.
    std::fs::write(fx.root().join("left-over.md"), "kept\n").unwrap();
    fx.check_out("elsewhere");

    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        original.id.clone(),
        "a-stream-the-caller-names".into(),
        StandingWork::CommitAndPush,
        Some("ignored".into()),
    )
    .expect("restarted");

    assert_eq!(restarted.direct_target, original.direct_target);
    assert_eq!(restarted.direct_target.as_ref().unwrap().branch, pinned);
    assert_eq!(restarted.standing_work, StandingWork::Keep);
    assert!(restarted.standing_work_message.is_none());
    assert_eq!(restarted.restarted_from_run_id.as_deref(), Some(original.id.as_str()));
    assert_eq!(fx.reload(&original.id).state, GraduationRunState::Discarded);
}

/// GSU-FR-NPFB: a worktree whose directory is gone cannot be restarted onto.
#[test]
fn a_restart_onto_a_missing_worktree_is_refused() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = fx.direct_in(Path::new("/no/such/worktree"), "main", None, "p", "d1");
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).unwrap();

    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        run.id,
        String::new(),
        StandingWork::Keep,
        None,
    )
    .unwrap_err();
    assert_eq!(refused, ERR_DIRECT_WORKTREE_MISSING);
}

/// DRS-FR-NZSQ, DRS-FR-KQTW: a direct run locks its draft, graduates it when it
/// commits, and releases it when it is discarded, as a stream run does.
#[test]
fn a_direct_run_applies_the_graduated_draft_lifecycle() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("work", &draft);
    assert!(crate::graduation::require_unlocked_draft(&fx.app, &draft).is_err());

    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    let queue = crate::graduation::project_queue(&fx.app).unwrap();
    assert!(crate::graduation::draft_graduation_stands(&queue, &draft));

    let access = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let root = crate::fs::RootFs::new(fx.root(), access);
    let mut record = crate::drafts::draft_record(&root, &draft).unwrap();
    crate::drafts::resolve_graduated_status(&fx.app, &mut record);
    assert_eq!(record.status, crate::drafts::DraftStatus::Graduated);
}

/// DRS-FR-KQTW: a discarded direct run releases the status it held.
#[test]
fn discarding_the_only_committing_direct_run_releases_the_draft() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("work", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    let mut stored = fx.reload(&run.id);
    stored.state = GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut stored).unwrap();
    let queue = crate::graduation::project_queue(&fx.app).unwrap();
    assert!(!crate::graduation::draft_graduation_stands(&queue, &draft));
}

/// GRD-FR-EFAU: a queue change names the stream and the worktree it is about.
#[test]
fn a_queue_event_names_the_worktree_of_a_direct_run() {
    let fx = Fixture::new();
    let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let sink = seen.clone();
    fx.app.listen(GRADUATION_QUEUE_CHANGED, move |event| {
        sink.lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    let stream = fx.stream("editor");
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let direct = fx.direct("work", "d1");
    let mut again = fx.reload(&direct.id);
    again.auto_start = false;
    crate::graduation::set_graduation_auto_start(fx.app.clone(), direct.id.clone(), false).unwrap();
    let _ = stream;

    let events = seen.lock().unwrap().clone();
    let payload = events.last().expect("a queue event");
    assert_eq!(payload["streamId"], "");
    assert_eq!(payload["worktreePath"], fx.root().to_string_lossy().as_ref());
}

/// GSU-FR-GTRO: a refused start deletes no stream, whichever start refused.
#[test]
fn a_refused_start_keeps_the_stream_that_stood_before_it() {
    let fx = Fixture::new();
    let stream = fx.stream("created in the dialog");
    // The image preflight refuses on a machine with nothing configured.
    let draft = fx.committed_draft("direct");
    let refused = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    );
    assert!(refused.is_err());
    let refused_direct = fx.start_direct(&draft);
    assert!(refused_direct.is_err());
    let listed = crate::streams::list_work_streams(fx.app.clone()).unwrap();
    assert!(listed.iter().any(|summary| summary.stream.id == stream.id));
}
