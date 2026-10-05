//! What the author, another author and the machine do around a merge
//! (GTE-FR-TNZF, GTE-FR-LMXV, GTE-FR-KBVN, GTE-FR-CZPM, GTE-FR-DQLR).
//!
//! The author's Merge goes through `merge_work_stream`, the production command,
//! with the queue's own dispatch on. The tripwire of GTE-FR-YAEB holds what the
//! queue offers, so a scenario drives the merge run through the scripted seam
//! itself. Each act that drives the run rebinds the seam, so the dispatch
//! assertions after it read the turns of that act alone.

use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tauri::Manager;

use super::hooks::{index_lock_of, without_loop, TurnContext};
use super::merge_arrange::commit_file;
use super::outcome::Outcome;
use super::scenario::bind;
use super::script::TurnScript;
use super::settings::guard;
use crate::graduation::GraduationState;
use crate::streams::{Reconciliation, StreamMergePublication, StreamMergeResult, StreamState};

/// GSU-FR-GLVQ: how far the machine is set up to execute an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MachineSetup {
    /// No vendor is configured.
    Nothing,
    /// A verified CLI vendor, and no project image for it.
    Vendor,
    /// The vendor and the project's image, and no verified Docker backend.
    Image,
    /// Everything a turn needs.
    Complete,
}

/// GTE-FR-CZPM: a thread that holds the repository update guard until it is told
/// to let go. The guard borrows the application's stream state, so a thread that
/// owns an application handle is the way to hold it across several steps.
pub(crate) struct GuardHolder {
    release: Sender<()>,
    thread: JoinHandle<()>,
}

/// The guard a hook or an act holds, shared by every context of a scenario.
pub(crate) type HeldGuard = Arc<Mutex<Option<GuardHolder>>>;

impl GuardHolder {
    fn take(
        app: tauri::AppHandle<super::super::Runtime>,
        stream_id: String,
        kind: Reconciliation,
    ) -> Self {
        let (ready_tx, ready_rx) = channel::<()>();
        let (release, release_rx) = channel::<()>();
        let thread = std::thread::spawn(move || {
            let state = app.state::<StreamState>();
            let hold = state
                .begin_repository_update(&stream_id, kind)
                .expect("the repository update guard was free");
            ready_tx.send(()).expect("the holder is waited for");
            let _ = release_rx.recv();
            drop(hold);
        });
        ready_rx.recv().expect("the guard was taken");
        Self { release, thread }
    }

    fn let_go(self) {
        let _ = self.release.send(());
        self.thread.join().expect("the holder ended");
    }
}

impl TurnContext {
    /// GTE-FR-CZPM: an update takes the repository update guard and keeps it.
    pub(crate) fn hold_the_update_guard(&self) {
        self.hold_the_guard_as(Reconciliation::Update);
    }

    /// GTE-FR-TNZF: a reconciliation of the kind takes the repository update
    /// guard and keeps it.
    pub(crate) fn hold_the_guard_as(&self, kind: Reconciliation) {
        let holder = GuardHolder::take(self.app.clone(), self.stream_id.clone(), kind);
        *self.guard.lock().unwrap() = Some(holder);
    }

    /// GTE-FR-CZPM: the update lets go of the guard.
    pub(crate) fn release_the_update_guard(&self) {
        if let Some(holder) = self.guard.lock().unwrap().take() {
            holder.let_go();
        }
    }
}

impl TurnScript {
    /// GTE-FR-CZPM: an update starts and holds the repository update guard while
    /// this turn runs and after it.
    pub(crate) fn holding_the_update_guard(self) -> Self {
        self.then(|ctx| ctx.hold_the_update_guard())
    }

    /// GTE-FR-CZPM: the author leaves uncommitted work in the base branch's
    /// working copy while this turn runs.
    pub(crate) fn dirtying_the_base_meanwhile(self, path: &str, content: &str) -> Self {
        let (path, content) = (path.to_string(), content.to_string());
        self.then(move |ctx| {
            guard(&ctx.project_root)
                .write_text_atomic(ctx.project_root.join(&path), &content)
                .expect("the base file is writable");
        })
    }

    /// GTE-FR-CZPM: the author leaves uncommitted work in the stream's working
    /// copy while this turn runs.
    pub(crate) fn dirtying_the_stream_meanwhile(self, path: &str, content: &str) -> Self {
        let (path, content) = (path.to_string(), content.to_string());
        self.then(move |ctx| {
            guard(&ctx.worktree)
                .write_text_atomic(ctx.worktree.join(&path), &content)
                .expect("the stream file is writable");
        })
    }

    /// GTE-FR-CZPM: another write takes the index of the base branch's working
    /// copy while this turn runs.
    pub(crate) fn holding_the_base_index(self) -> Self {
        self.then(|ctx| super::hooks::hold_index_of(&ctx.project_root))
    }

    /// GTE-FR-RDPE: at this turn both branches still stand at the tips the run
    /// pinned, and neither working copy holds an uncommitted path.
    pub(crate) fn checking_the_live_trees_stand(self) -> Self {
        self.then(|ctx| {
            let run = crate::graduation::load_run(&ctx.app, &ctx.run_id).expect("the run record");
            let data = run.merge.expect("a merge run");
            let repo = git2::Repository::open(&ctx.project_root).expect("the base repository");
            let tip = |branch: &str| {
                repo.find_branch(branch, git2::BranchType::Local)
                    .and_then(|found| found.get().peel_to_commit())
                    .map(|commit| commit.id().to_string())
                    .expect("a branch head")
            };
            assert_eq!(tip(&data.base_branch), data.base_tip, "the base branch stands at its pin");
            assert_eq!(tip(&data.stream_branch), data.stream_tip, "the stream branch stands at its pin");
            assert!(
                crate::streams::uncommitted_paths_of(&ctx.project_root).is_empty(),
                "the base working copy holds nothing uncommitted"
            );
            assert!(
                crate::streams::uncommitted_paths_of(&ctx.worktree).is_empty(),
                "the stream working copy holds nothing uncommitted"
            );
        })
    }

    /// GTE-FR-YCQW: while this turn runs, the run is in the state of the turn,
    /// holds its stream in memory and on the stream's record, and holds one
    /// project slot.
    pub(crate) fn holding_the_stream_and_a_slot(self, state: crate::graduation::GraduationRunState) -> Self {
        self.then(move |ctx| {
            let run = crate::graduation::load_run(&ctx.app, &ctx.run_id).expect("the run record");
            assert_eq!(run.state, state, "the run's state during the turn");
            let graduation = ctx.app.state::<GraduationState>();
            assert_eq!(
                graduation.holder_of(&ctx.stream_id).as_deref(),
                Some(ctx.run_id.as_str()),
                "the run holds its stream"
            );
            assert_eq!(graduation.working_count(), 1, "the run holds one project slot");
            let busy = crate::streams::stream_of(&ctx.app, &ctx.stream_id).and_then(|stream| stream.busy_run_id);
            assert_eq!(
                busy.as_deref(),
                Some(ctx.run_id.as_str()),
                "the stream's own record names the run"
            );
        })
    }

    /// GTE-FR-KBVN: another author commits on the base branch while this turn
    /// runs.
    pub(crate) fn moving_the_base_tip_meanwhile(self) -> Self {
        self.then(|ctx| {
            let repo = git2::Repository::open(&ctx.project_root).expect("the base repository");
            commit_file(&repo, "moved-on-base.txt", "another author\n");
        })
    }

    /// GTE-FR-KBVN: another author commits on the stream branch while this turn
    /// runs.
    pub(crate) fn moving_the_stream_tip_meanwhile(self) -> Self {
        self.then(|ctx| {
            let repo = git2::Repository::open(&ctx.worktree).expect("the stream repository");
            commit_file(&repo, "moved-on-stream.txt", "another author\n");
        })
    }
}

impl Outcome {
    // -- the author's Merge ---------------------------------------------------

    /// GTE-FR-TNZF: the author starts Merge on a publication. The answer is kept
    /// for the assertions. A merge Git cannot settle moves the focus to the run
    /// it was handed to, which waits `queued`.
    pub(crate) fn merging(self, publication: StreamMergePublication) -> Self {
        let stream_id = self.stream_id.clone();
        self.merging_stream(&stream_id, publication)
    }

    /// GTE-FR-TNZF: the same request, naming a stream the scenario chose.
    pub(crate) fn merging_stream(self, stream_id: &str, publication: StreamMergePublication) -> Self {
        self.request_merge(stream_id, publication, false)
    }

    /// GTE-FR-TNZF: the same request, made while the project holds no Git
    /// repository. The repository is back when the answer is read.
    pub(crate) fn merging_without_a_repository(self, publication: StreamMergePublication) -> Self {
        let stream_id = self.stream_id.clone();
        self.request_merge(&stream_id, publication, true)
    }

    fn request_merge(
        mut self,
        stream_id: &str,
        publication: StreamMergePublication,
        hidden: bool,
    ) -> Self {
        self.live_before = Some(self.live_state());
        self.tips_before = Some((self.branch_tip(), self.base_tip()));
        let root = self.fx.root();
        if hidden {
            guard(&root)
                .rename_path(root.join(".git"), ".git-hidden")
                .expect("the repository is hidden");
        }
        let answer =
            crate::streams::merge_work_stream_blocking(&self.fx.app, stream_id, publication);
        if hidden {
            guard(&root)
                .rename_path(root.join(".git-hidden"), ".git")
                .expect("the repository is back");
        }
        self.offered = self
            .fx
            .app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take();
        self.dispatch = super::super::ScriptedDispatch::new(Vec::new());
        if let Ok(StreamMergeResult::Conflicted { run_id, .. }) = &answer {
            self.previous = (self.run.id != "no-run").then(|| self.run.id.clone());
            self.ctx = self.ctx.for_run(run_id, &self.stream_id, &self.worktree);
            self.run = self.fx.reload(run_id);
        }
        self.merge_answer = Some(answer);
        self
    }

    /// GTE-FR-LMXV: the id of the merge run the focus is on, where it is on one.
    pub(crate) fn merge_run_id(&self) -> Option<String> {
        self.run.is_merge().then(|| self.run.id.clone())
    }

    /// GTE-FR-RDPE: the merge run in focus is driven through the scripted seam,
    /// as the queue would drive it once it had a slot.
    pub(crate) fn driving_the_merge(mut self, scripts: Vec<TurnScript>) -> Self {
        assert!(
            self.run.is_merge(),
            "[{}] the focus is on a merge run",
            self.name
        );
        self.dispatch = bind(&self.ctx, scripts);
        self.run = self.fx.drive(&self.run, self.dispatch.clone());
        self
    }

    /// GTE-FR-YCQW: the merge run cannot claim its stream or a project slot now,
    /// so it takes no turn and stays queued.
    pub(crate) fn expect_the_merge_cannot_start(mut self) -> Self {
        let dispatch = super::super::ScriptedDispatch::new(Vec::new());
        let started = self.fx.try_drive(&self.run, dispatch.clone());
        assert!(!started, "[{}] the merge run could not start", self.name);
        assert_eq!(dispatch.turns_taken(), 0, "[{}] it took no turn", self.name);
        self.run = self.fx.reload(&self.run.id);
        assert_eq!(
            self.run.state,
            crate::graduation::GraduationRunState::Queued,
            "[{}] it is still queued",
            self.name
        );
        self
    }

    /// GTE-FR-YCQW: the claims the scenario held on other streams are let go, so
    /// the project slots they took are free again.
    pub(crate) fn releasing_the_held_slots(self) -> Self {
        let streams = crate::streams::list_work_streams(self.fx.app.clone()).expect("the streams");
        let state = self.fx.app.state::<GraduationState>();
        for row in streams {
            if row.stream.id != self.stream_id {
                state.release(&row.stream.id);
            }
        }
        self
    }

    // -- the controls every run has --------------------------------------------------

    /// GRD-FR-TKUR: the author turns auto-start of the queued merge run off.
    pub(crate) fn turning_auto_start_off(self) -> Self {
        without_loop(&self.fx.app, || {
            crate::graduation::set_graduation_auto_start(self.fx.app.clone(), self.run.id.clone(), false)
        })
        .unwrap_or_else(|reason| panic!("[{}] auto-start is settable: {reason}", self.name));
        self
    }

    /// WKS-FR-RQVM: the stream's queue is offered a dispatch again, as it is
    /// whenever a stream is freed. What it offers is kept for the assertions.
    pub(crate) fn offering_the_queue_a_dispatch(mut self) -> Self {
        crate::graduation::advance_stream_queue(&self.fx.app, &self.stream_id);
        self.offered = self
            .fx
            .app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take();
        self
    }

    /// GRD-FR-JOFE: the author files the merge run away.
    pub(crate) fn archiving(self) -> Self {
        crate::graduation::archive_graduation_run(self.fx.app.clone(), self.run.id.clone())
            .unwrap_or_else(|reason| panic!("[{}] the archive was refused: {reason}", self.name));
        self
    }

    /// GRD-FR-RHNP: the author moves the merge run within its stream's queue.
    pub(crate) fn reordering_the_merge_run(self, from: usize, to: usize) -> Self {
        without_loop(&self.fx.app, || {
            crate::graduation::reorder_graduation_run(self.fx.app.clone(), self.run.id.clone(), from, to)
        })
        .unwrap_or_else(|reason| panic!("[{}] the reorder was refused: {reason}", self.name));
        self
    }

    // -- the machine and the repository ----------------------------------------------

    /// GSU-FR-GLVQ: the machine is set up as far as the stage says and no
    /// further, so the image preflight refuses on what is missing.
    pub(crate) fn setting_the_machine_up_to(self, stage: MachineSetup) -> Self {
        use crate::agentic::{AgenticRecord, PathOrigin};
        if stage == MachineSetup::Nothing {
            return self;
        }
        let store = self.fx.app.state::<crate::global_settings::GlobalSettingsStore>();
        store
            .save_agentic_registry(
                vec![AgenticRecord {
                    vendor: "claude_code".into(),
                    binary_path: Some("/opt/agents/claude".into()),
                    path_origin: PathOrigin::Detected,
                    ..AgenticRecord::default()
                }],
                Some("claude_code".into()),
            )
            .expect("a registry");
        if stage == MachineSetup::Vendor {
            return self;
        }
        let access = self
            .fx
            .app
            .state::<crate::fs::FsAccessState>()
            .get()
            .expect("an instance");
        let root = crate::fs::RootFs::new(self.fx.root(), access);
        crate::project_settings::save_project_vendor_image_to(
            &root,
            "claude_code",
            &crate::project_settings::images::ProjectVendorImage {
                image_name: "acme/agent".into(),
                tag: Some("latest".into()),
                dockerfile: None,
            },
        )
        .expect("an image");
        if stage != MachineSetup::Image {
            self.fx.allow_execution();
        }
        super::merge_arrange::commit_everything(&self.fx.repo(), "Project settings");
        self
    }

    /// GTE-FR-TNZF: the author switches the project's working copy to another
    /// branch, so the base branch is checked out nowhere.
    pub(crate) fn checking_out(self, branch: &str) -> Self {
        let repo = self.fx.repo();
        let object = repo
            .revparse_single(&format!("refs/heads/{branch}"))
            .unwrap_or_else(|reason| panic!("[{}] the branch {branch} exists: {reason}", self.name));
        repo.checkout_tree(&object, Some(git2::build::CheckoutBuilder::new().force()))
            .expect("the checkout");
        repo.set_head(&format!("refs/heads/{branch}")).expect("the head moves");
        self
    }

    /// GTE-FR-TNZF: an update of the stream holds the repository update guard.
    pub(crate) fn holding_the_update_guard_now(self) -> Self {
        self.ctx.hold_the_update_guard();
        self
    }

    /// GTE-FR-TNZF: another merge check holds the repository update guard.
    pub(crate) fn holding_another_merge_now(self) -> Self {
        self.ctx.hold_the_guard_as(Reconciliation::Merge);
        self
    }

    // -- another author --------------------------------------------------------

    /// GTE-FR-KBVN: another author commits on the stream's branch.
    pub(crate) fn committing_on_the_stream(self, path: &str, content: &str) -> Self {
        let repo = git2::Repository::open(&self.worktree).expect("the stream repository");
        commit_file(&repo, path, content);
        self
    }

    /// GTE-FR-KBVN: the base branch advances, as another author makes it.
    pub(crate) fn moving_the_base_tip(self) -> Self {
        commit_file(&self.fx.repo(), "moved-on-base.txt", "another author\n");
        self
    }

    /// GTE-FR-KBVN: the stream branch advances, as another author makes it.
    pub(crate) fn moving_the_stream_tip(self) -> Self {
        self.committing_on_the_stream("moved-on-stream.txt", "another author\n")
    }

    // -- what repairs an obstruction of the apply --------------------------------

    /// GTE-FR-CZPM: the update that held the repository update guard has ended.
    pub(crate) fn releasing_the_guard(self) -> Self {
        self.ctx.release_the_update_guard();
        self
    }

    /// GTE-FR-TNZF: the author removes the uncommitted file from the stream's
    /// working copy.
    pub(crate) fn cleaning_the_stream(self, path: &str) -> Self {
        guard(&self.worktree)
            .delete_path(self.worktree.join(path), false)
            .unwrap_or_else(|reason| panic!("[{}] the stream file was removable: {reason}", self.name));
        self
    }

    /// GTE-FR-CZPM: the author removes the uncommitted file from the base
    /// branch's working copy.
    pub(crate) fn cleaning_the_base(self, path: &str) -> Self {
        let root = self.fx.root();
        guard(&root)
            .delete_path(root.join(path), false)
            .unwrap_or_else(|reason| panic!("[{}] the base file was removable: {reason}", self.name));
        self
    }

    /// GTE-FR-CZPM: the other write lets go of the base working copy's index.
    pub(crate) fn releasing_the_base_index(self) -> Self {
        let root = self.fx.root();
        let (git_dir, lock) = index_lock_of(&root);
        guard(&git_dir)
            .delete_path(&lock, false)
            .unwrap_or_else(|reason| panic!("[{}] the base index lock was held: {reason}", self.name));
        self
    }

    // -- the application restarts --------------------------------------------------

    /// GTE-FR-DQLR: the application stops and starts again. No claim of the old
    /// process stands, and the first read of the project's queue and of its
    /// streams is where a stopped run is found. The stores are read again from
    /// disk, so nothing but what is durable reaches what follows.
    pub(crate) fn relaunching(mut self) -> Self {
        let state = self.fx.app.state::<GraduationState>();
        state.release(&self.stream_id);
        without_loop(&self.fx.app, || {
            crate::graduation::list_graduation_queue(self.fx.app.clone())
                .unwrap_or_else(|reason| panic!("[{}] the queue was not readable: {reason}", self.name))
        });
        crate::streams::list_work_streams(self.fx.app.clone())
            .unwrap_or_else(|reason| panic!("[{}] the streams were not readable: {reason}", self.name));
        self.dispatch = super::super::ScriptedDispatch::new(Vec::new());
        if self.run.id != "no-run" {
            self.run = self.fx.reload(&self.run.id);
        }
        self
    }

    /// GTE-FR-DQLR: what an ended merge run owns is put back by hand, as a crash
    /// between the end of the run and its cleanup leaves it.
    pub(crate) fn stranding_what_the_run_owned(self) -> Self {
        let run = self.fx.reload(&self.run.id);
        let data = run.merge.clone().expect("a merge run");
        crate::graduation::driver::merge_workspace::ensure(&self.fx.app, &run)
            .unwrap_or_else(|reason| panic!("[{}] the merge worktree was made again: {reason}", self.name));
        let snapshot = git2::Oid::from_str(&data.snapshot_commit).expect("a snapshot id");
        self.fx
            .repo()
            .reference(&format!("refs/synthesis/merge/{}", run.id), snapshot, true, "stranded")
            .expect("the snapshot ref is written");
        self
    }

    /// The directory the merge run in focus owns for its worktree.
    pub(crate) fn merge_worktree_path(&self) -> std::path::PathBuf {
        crate::graduation::store_base(&self.fx.app)
            .expect("the run store")
            .merge_worktree(&self.run.id)
    }

    /// The directory the review turn stands in.
    pub(crate) fn review_checkout_path(&self) -> std::path::PathBuf {
        crate::graduation::store_base(&self.fx.app)
            .expect("the run store")
            .review_checkout(&self.run.id)
    }

    /// A file of the merge worktree, where one stands.
    pub(crate) fn merge_worktree_holds(self, path: &str, content: &str) -> Self {
        let root = self.merge_worktree_path();
        let found = guard(&root).read_text(Path::new(&root).join(path)).unwrap_or_default();
        assert_eq!(
            found, content,
            "[{}] what the merge worktree holds at {path}",
            self.name
        );
        self
    }
}
