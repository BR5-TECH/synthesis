//! The assertions that read the repository a run wrote into (GTE-FR-MDVQ,
//! GTE-FR-PFEO).
//!
//! Every read opens the project's primary repository, which shares its
//! objects and its branches with every working copy, so an assertion still
//! answers after a scenario removed the stream's working copy.

use tauri::Manager;

use super::outcome::{sorted, Outcome};
use super::settings::guard;

impl Outcome {
    pub(crate) fn expect_commits(self, count: usize) -> Self {
        assert_eq!(
            self.run.commits.len(),
            count,
            "[{}] the commits this run created",
            self.name
        );
        self
    }

    /// GRD-FR-ARLT: the commits hold exactly the paths that differ from the
    /// revision the stream stood at before the run, and a path the turn did not
    /// write is not swept into them.
    /// An **empty** expectation is only worth something beside
    /// [`Self::expect_commits`]: a run that committed nothing holds no paths
    /// either, so the two together are what tell "committed nothing" from
    /// "committed exactly the right nothing".
    pub(crate) fn expect_committed_paths(self, paths: &[&str]) -> Self {
        let found = match (self.base.as_deref(), self.run.commits.last()) {
            (Some(base), Some(last)) => self.paths_between(base, last),
            _ => Vec::new(),
        };
        assert_eq!(found, sorted(paths), "[{}] the paths this run's commits hold", self.name);
        self
    }

    /// GRD-FR-HQPD: the paths the stream branch holds against the run's own
    /// base commit, which a standing commit may have moved past the revision
    /// the stream stood at.
    pub(crate) fn expect_paths_since_run_base(self, paths: &[&str]) -> Self {
        let base = self
            .run
            .base_commit
            .clone()
            .unwrap_or_else(|| panic!("[{}] the run holds a base commit", self.name));
        let tip = self.branch_tip().expect("the stream branch");
        let found = self.paths_between(&base, &tip);
        assert_eq!(found, sorted(paths), "[{}] the paths since the run's base", self.name);
        self
    }

    /// GRD-FR-SWOJ: an abandoned turn's commit says so in its message.
    pub(crate) fn expect_commit_message_contains(self, index: usize, fragment: &str) -> Self {
        let message = self
            .run
            .commits
            .get(index)
            .map(|revision| self.message_of(revision))
            .unwrap_or_default();
        assert!(
            message.contains(fragment),
            "[{}] commit {index} says {fragment:?}, but it says {message:?}",
            self.name
        );
        self
    }

    /// The message of the commit the stream branch stands at.
    pub(crate) fn expect_branch_head_message_contains(self, fragment: &str) -> Self {
        let message = self
            .branch_tip()
            .map(|tip| self.message_of(&tip))
            .unwrap_or_default();
        assert!(
            message.contains(fragment),
            "[{}] the branch head says {fragment:?}, but it says {message:?}",
            self.name
        );
        self
    }

    /// GRD-FR-WQTN: the revision the run records last is the one the stream
    /// branch stands at, so a rewritten commit is recorded rather than lost.
    pub(crate) fn expect_last_commit_is_branch_head(self) -> Self {
        assert_eq!(
            self.run.commits.last().cloned(),
            self.branch_tip(),
            "[{}] the run's last recorded commit is the branch head",
            self.name
        );
        self
    }

    /// How many commits the stream branch holds past the revision the stream
    /// stood at before the run, whatever the record says it made.
    pub(crate) fn expect_branch_commits_since_base(self, count: usize) -> Self {
        let found = self.history_since(self.base.as_deref()).len();
        assert_eq!(
            found, count,
            "[{}] the commits on the stream branch since the run started",
            self.name
        );
        self
    }

    /// GRD-FR-BLCR: every commit the branch held before the revert is still in
    /// its history, and the revert added exactly `count` more.
    pub(crate) fn expect_history_kept_and_grown_by(self, count: usize) -> Self {
        let now = self.history_since(None);
        for revision in &self.history_before {
            assert!(
                now.contains(revision),
                "[{}] the revert kept {revision} in the branch's history",
                self.name
            );
        }
        assert_eq!(
            now.len(),
            self.history_before.len() + count,
            "[{}] the commits the revert added",
            self.name
        );
        self
    }

    /// GRD-FR-BNTC / WKS-FR-JQJA: whether the stream is free once the run has
    /// come to rest, in memory and on the stream's own durable record.
    pub(crate) fn expect_stream_released(self) -> Self {
        let (holder, busy) = self.stream_holders();
        assert_eq!(
            holder, None,
            "[{}] the stream is free once the run rests",
            self.name
        );
        assert_eq!(
            busy, None,
            "[{}] the stream's durable busy mark is cleared",
            self.name
        );
        self
    }

    pub(crate) fn expect_stream_held(self) -> Self {
        let (holder, busy) = self.stream_holders();
        assert_eq!(
            holder.as_deref(),
            Some(self.run.id.as_str()),
            "[{}] the run still holds its stream",
            self.name
        );
        assert_eq!(
            busy.as_deref(),
            Some(self.run.id.as_str()),
            "[{}] the stream's durable busy mark names the run",
            self.name
        );
        self
    }

    /// Who holds the stream in memory, and whom its durable record names.
    pub(super) fn stream_holders(&self) -> (Option<String>, Option<String>) {
        let holder = self
            .fx
            .app
            .state::<crate::graduation::GraduationState>()
            .holder_of(&self.stream_id);
        let busy = crate::streams::stream_of(&self.fx.app, &self.stream_id)
            .and_then(|stream| stream.busy_run_id);
        (holder, busy)
    }

    /// The working copy's files, so a scenario can assert what a turn left
    /// standing there.
    ///
    /// Read through the guarded handle rather than through `std::fs`, on the
    /// terms FSA-FR-19 binds every other read in the backend.
    pub(crate) fn worktree_holds(self, path: &str, content: &str) -> Self {
        let found = guard(&self.worktree)
            .read_text(self.worktree.join(path))
            .unwrap_or_default();
        assert_eq!(
            found, content,
            "[{}] what the working copy holds at {path}",
            self.name
        );
        self
    }

    pub(crate) fn expect_worktree_lacks(self, path: &str) -> Self {
        assert!(
            guard(&self.worktree)
                .read_text(self.worktree.join(path))
                .is_err(),
            "[{}] the working copy holds nothing at {path}",
            self.name
        );
        self
    }

    /// GRD-FR-GMTX: the stream's working copy and its branch outlive the run.
    pub(crate) fn expect_working_copy_present(self) -> Self {
        assert!(
            git2::Repository::open(&self.worktree).is_ok(),
            "[{}] the stream's working copy is still there",
            self.name
        );
        assert!(
            self.branch_tip().is_some(),
            "[{}] the stream's branch is still there",
            self.name
        );
        self
    }

    /// GRL-FR-YKRI: the review checkout, its registration and its scratch
    /// branch are all gone.
    pub(crate) fn expect_review_checkout_reclaimed(self) -> Self {
        let repo = self.fx.repo();
        assert!(
            !crate::streams::registers_worktree(&repo, &format!("{}-rv", self.run.id)),
            "[{}] no review checkout is registered",
            self.name
        );
        assert!(
            repo.find_branch(
                &format!("synthesis/review/{}", self.run.id),
                git2::BranchType::Local
            )
            .is_err(),
            "[{}] no review scratch branch is left",
            self.name
        );
        let checkout = crate::graduation::store_base(&self.fx.app)
            .expect("the run store")
            .review_checkout(&self.run.id);
        assert!(
            guard(checkout.parent().expect("the run's own directory"))
                .file_info(&checkout)
                .is_err(),
            "[{}] no review checkout directory is left",
            self.name
        );
        self
    }

    // -- reading the repository ---------------------------------------------

    /// The revision the stream branch stands at now.
    pub(super) fn branch_tip(&self) -> Option<String> {
        let repo = self.fx.repo();
        let branch = repo
            .find_branch(&self.branch, git2::BranchType::Local)
            .ok()?;
        let tip = branch.get().peel_to_commit().ok()?;
        Some(tip.id().to_string())
    }

    /// The stream branch's history, newest first, down to and excluding
    /// `until` where one is named.
    pub(super) fn history_since(&self, until: Option<&str>) -> Vec<String> {
        let Some(tip) = self.branch_tip() else {
            return Vec::new();
        };
        let repo = self.fx.repo();
        let mut walk = repo.revwalk().expect("a history walk");
        walk.push(git2::Oid::from_str(&tip).expect("a revision"))
            .expect("the tip");
        if let Some(until) = until {
            walk.hide(git2::Oid::from_str(until).expect("a revision"))
                .expect("the base");
        }
        walk.filter_map(|oid| oid.ok().map(|oid| oid.to_string()))
            .collect()
    }

    fn message_of(&self, revision: &str) -> String {
        let repo = self.fx.repo();
        let oid = git2::Oid::from_str(revision).expect("a revision");
        let message = repo
            .find_commit(oid)
            .expect("a commit")
            .message()
            .unwrap_or_default()
            .to_string();
        message
    }

    /// The paths two revisions differ at, sorted and unique.
    fn paths_between(&self, from: &str, to: &str) -> Vec<String> {
        let repo = self.fx.repo();
        let tree_of = |revision: &str| {
            repo.find_commit(git2::Oid::from_str(revision).expect("a revision"))
                .and_then(|commit| commit.tree())
                .expect("a tree")
        };
        let (from, to) = (tree_of(from), tree_of(to));
        let diff = repo
            .diff_tree_to_tree(Some(&from), Some(&to), None)
            .expect("a diff");
        let mut paths: Vec<String> = Vec::new();
        for delta in diff.deltas() {
            for file in [delta.new_file(), delta.old_file()] {
                if let Some(path) = file.path() {
                    paths.push(path.to_string_lossy().into_owned());
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }
}
