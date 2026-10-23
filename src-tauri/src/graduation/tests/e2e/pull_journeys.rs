//! Journey 42 of the coverage matrix: a pull run, from the hand-off of a
//! diverged pull to its apply
//! (`../../../../../specifications/core/GRD-graduation.md` GRD-FR-PLSB,
//! GRD-FR-PQKY, GRD-FR-PAPX, GRD-FR-PRBZ, and `GTC-git.md` GTC-FR-EGIP).
//!
//! The project's own branch tracks `origin`, holds one commit of its own, and
//! its upstream holds another; both rewrote `pull.md`. Git cannot settle that,
//! so `reconcile_pull` hands it to a pull run, which the scenario drives
//! through the production loop.

use tauri::Manager;

use super::merge_arrange::commit_file;
use super::scenario::{Outcome, Scenario, TurnScript as Script};
use crate::git::{PullStrategy, RECONCILE_HANDED_TO_RUN};
use crate::graduation::GraduationRunState;

impl Outcome {
    /// The project's branch.
    fn project_branch(&self) -> String {
        let repo = self.fx.repo();
        let head = repo.head().expect("a branch head");
        head.shorthand().expect("a branch").to_string()
    }

    fn project_head(&self) -> git2::Oid {
        let repo = self.fx.repo();
        repo.head().and_then(|h| h.peel_to_commit()).map(|c| c.id()).expect("a head commit")
    }

    /// GTC-FR-NJUY / GTC-FR-EGIP: the project's branch and its upstream both
    /// rewrote `pull.md`, and the author chooses `strategy`. The focus moves to
    /// the pull run the conflict is handed to.
    pub(crate) fn pulling_a_conflict(mut self, strategy: PullStrategy) -> Self {
        let repo = self.fx.repo();
        let branch = self.project_branch();
        commit_file(&repo, "pull.md", "base\n");
        let base = self.project_head();
        repo.remote("origin", "file:///nowhere/synthesis-pull-e2e").expect("a remote");
        let mut config = repo.config().expect("config");
        config.set_str(&format!("branch.{branch}.remote"), "origin").expect("remote");
        config
            .set_str(&format!("branch.{branch}.merge"), &format!("refs/heads/{branch}"))
            .expect("merge");
        // The upstream's commit, which a fetch brought: a tree with `pull.md`
        // rewritten, under the remote-tracking ref.
        let parent = repo.find_commit(base).expect("the base commit");
        let mut builder =
            repo.treebuilder(Some(&parent.tree().expect("a tree"))).expect("a builder");
        let blob = repo.blob(b"upstream side\n").expect("a blob");
        builder.insert("pull.md", blob, 0o100644).expect("an entry");
        let tree = repo.find_tree(builder.write().expect("a tree")).expect("a tree");
        let signature =
            git2::Signature::now("Upstream", "upstream@example.com").expect("a signature");
        let remote = repo
            .commit(None, &signature, &signature, "Upstream work", &tree, &[&parent])
            .expect("the upstream commit");
        repo.reference(&format!("refs/remotes/origin/{branch}"), remote, true, "fetch")
            .expect("the remote-tracking ref");
        let local = commit_file(&repo, "pull.md", "local side\n");

        let answer = crate::streams::reconcile_pull_blocking(
            &self.fx.app,
            strategy,
            &local.to_string(),
            &remote.to_string(),
        );
        self.offered =
            self.fx.app.state::<crate::graduation::driver::drive::SpawnTripwire>().take();
        assert_eq!(answer.state, RECONCILE_HANDED_TO_RUN, "[{}] {answer:?}", self.name);
        assert_eq!(
            answer.run_title.as_deref(),
            Some(format!("Pull origin/{branch} into {branch}").as_str())
        );
        let run_id = answer.run_id.expect("the pull run");
        let root = self.fx.root();
        self.ctx = self.ctx.for_run(&run_id, "", &root);
        self.run = self.fx.reload(&run_id);
        self.tips_before = Some((Some(remote.to_string()), local.to_string()));
        self
    }

    fn pulled_tips(&self) -> (git2::Oid, git2::Oid) {
        let (remote, local) = self.tips_before.clone().expect("a pull was handed off");
        let parse = |id: &str| git2::Oid::from_str(id).expect("an object id");
        (parse(&local), parse(&remote.expect("the upstream tip")))
    }

    /// GRD-FR-PLSB / GRD-FR-PQKY: what the hand-off recorded.
    pub(crate) fn expect_a_queued_pull_run(self, strategy: PullStrategy) -> Self {
        let branch = self.project_branch();
        let (local, remote) = self.pulled_tips();
        let run = self.fx.reload(&self.run.id);
        assert_eq!(run.state, GraduationRunState::Queued, "[{}] the run waits", self.name);
        let merge = run.merge.as_ref().expect("merge data");
        let pull = merge.pull.as_ref().expect("a pull subject");
        assert_eq!(pull.strategy, strategy);
        assert_eq!(merge.base_branch, branch);
        assert_eq!(merge.stream_branch, format!("origin/{branch}"));
        assert_eq!(merge.base_tip, local.to_string());
        assert_eq!(merge.stream_tip, remote.to_string());
        assert_eq!(merge.unresolved_paths, vec!["pull.md".to_string()]);
        assert!(matches!(
            &merge.publication,
            crate::streams::StreamMergePublication::Commit { message }
                if message == &format!("Merge origin/{branch} into {branch}")
        ));
        let root = crate::changes::canonicalize_lenient(&self.fx.root());
        let target = run.direct_target.as_ref().expect("a direct target");
        assert_eq!(
            crate::changes::canonicalize_lenient(std::path::Path::new(&target.worktree_path)),
            root
        );
        assert_eq!(target.branch, branch);
        assert_eq!(run.queue_key(), crate::graduation::worktree_queue_key(&target.worktree_path));
        assert!(run.input.draft_id.is_empty() && run.input.draft_name.is_empty());
        assert!(
            crate::streams::has_snapshot_ref(&self.fx.repo(), &run.id),
            "[{}] the snapshot is kept",
            self.name
        );
        // Nothing reached the branch or the working copy.
        assert_eq!(self.project_head(), local);
        self
    }

    /// GRD-FR-PAPX: the branch stands on a merge commit of both pinned tips.
    pub(crate) fn expect_the_pull_merged(self) -> Self {
        let (local, remote) = self.pulled_tips();
        let repo = self.fx.repo();
        let head = repo.head().and_then(|h| h.peel_to_commit()).expect("a head");
        assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![local, remote], "[{}]", self.name);
        self
    }

    /// GRD-FR-PRBZ: the branch stands on the local commit replayed onto the
    /// upstream tip.
    pub(crate) fn expect_the_pull_rebased(self) -> Self {
        let (_, remote) = self.pulled_tips();
        let repo = self.fx.repo();
        let head = repo.head().and_then(|h| h.peel_to_commit()).expect("a head");
        assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![remote], "[{}]", self.name);
        assert_eq!(head.message().ok(), Some("Edit pull.md"));
        self
    }

    /// Nothing of the pull reached the branch.
    pub(crate) fn expect_the_branch_at_its_local_tip(self) -> Self {
        let (local, _) = self.pulled_tips();
        assert_eq!(self.project_head(), local, "[{}] the branch did not move", self.name);
        self
    }

    pub(crate) fn expect_project_file(self, path: &str, content: &str) -> Self {
        let root = self.fx.root();
        let held = super::settings::guard(&root).read_text(root.join(path)).unwrap_or_default();
        assert_eq!(held, content, "[{}] {path}", self.name);
        self
    }

    /// The local branch gains a commit after the hand-off.
    pub(crate) fn committing_on_the_project_branch(self, path: &str, content: &str) -> Self {
        commit_file(&self.fx.repo(), path, content);
        self
    }
}

fn settling() -> Script {
    Script::reports_success().writes("pull.md", "both sides\n")
}

fn a_pull(name: &'static str) -> Scenario {
    Scenario::named(name).with_execution_allowed().without_a_draft_run()
}

// GTC-FR-EGIP, GRB-FR-PADR, GRB-FR-CYWM, GRD-FR-MRNQ, GRD-FR-PLSB, GRD-FR-PQKY,
// GRD-FR-VCTH: a conflict
// Git cannot settle becomes a queued pull run: the local branch is its base, the
// upstream its stream, its publication one commit, it waits in its worktree's
// queue with a direct target, and nothing was written.
#[test]
fn a_diverged_pull_git_cannot_settle_is_handed_to_a_queued_pull_run() {
    a_pull("pull hand-off")
        .run()
        .pulling_a_conflict(PullStrategy::Rebase)
        .expect_a_queued_pull_run(PullStrategy::Rebase)
        .expect_project_file("pull.md", "local side\n");
}

// GRD-FR-PAPX, GRD-FR-AQNW: a merge pull run lands one merge commit of the
// settled tree onto the local branch, and records the commit publication.
#[test]
fn a_merge_pull_run_lands_one_merge_commit() {
    a_pull("pull run, merge")
        .run()
        .pulling_a_conflict(PullStrategy::Merge)
        .driving_the_merge(vec![settling(), Script::answers_ready("Both sides stand.")])
        .expect_state(GraduationRunState::Completed)
        .expect_merge_result("commit", true, &["pull.md"])
        .expect_the_pull_merged()
        .expect_project_file("pull.md", "both sides\n");
}

// GRD-FR-PRBZ: a rebase pull run replays the local commit onto the upstream
// from the settled tree, and records `rebased` with the new tip.
#[test]
fn a_rebase_pull_run_replays_the_local_commit_onto_the_upstream() {
    a_pull("pull run, rebase")
        .run()
        .pulling_a_conflict(PullStrategy::Rebase)
        .driving_the_merge(vec![settling(), Script::answers_ready("Both sides stand.")])
        .expect_state(GraduationRunState::Completed)
        .expect_merge_result("rebased", true, &["pull.md"])
        .expect_the_pull_rebased()
        .expect_project_file("pull.md", "both sides\n");
}

// GRD-FR-PRBZ, GRD-FR-JSBE: a settled tree the replay cannot reproduce blocks
// the run with `pull_rebase_unsettled` at the apply, and writes nothing.
#[test]
fn a_rebase_the_replay_cannot_reproduce_blocks_at_the_apply() {
    a_pull("pull run, unsettled rebase")
        .run()
        .pulling_a_conflict(PullStrategy::Rebase)
        .driving_the_merge(vec![
            settling().writes("extra.md", "the agent's own\n"),
            Script::answers_ready("Both sides stand."),
        ])
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("pull_rebase_unsettled")
        .expect_resume_phase("apply")
        .expect_the_branch_at_its_local_tip()
        .expect_project_file("pull.md", "local side\n");
}

// GRD-FR-PAPX: a worktree that left the branch while the run reviewed ends the
// run `failed` with `pull_target_changed`, and nothing is written.
#[test]
fn a_worktree_that_left_the_branch_fails_the_pull_run() {
    a_pull("pull run, target changed")
        .run()
        .pulling_a_conflict(PullStrategy::Merge)
        .driving_the_merge(vec![
            settling(),
            Script::answers_ready("Both sides stand.").then(|ctx| {
                let repo = git2::Repository::open(&ctx.project_root).expect("the project");
                let head = repo.head().and_then(|h| h.peel_to_commit()).expect("a head");
                repo.branch("elsewhere", &head, false).expect("a branch");
                repo.set_head("refs/heads/elsewhere").expect("the head moves");
            }),
        ])
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("pull_target_changed");
}

// GRD-FR-XHSE, GRB-FR-BXEJ: a local branch that moved after the hand-off ends
// the run `failed` with `merge_branch_moved` before any turn.
#[test]
fn a_local_branch_that_moved_fails_the_pull_run() {
    a_pull("pull run, local branch moved")
        .run()
        .pulling_a_conflict(PullStrategy::Merge)
        .committing_on_the_project_branch("later.md", "more\n")
        .driving_the_merge(Vec::new())
        .expect_state(GraduationRunState::Failed)
        .expect_failure_code("merge_branch_moved");
}
