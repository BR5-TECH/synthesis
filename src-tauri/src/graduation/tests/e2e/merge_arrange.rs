//! What a merge scenario arranges and what it compares afterwards
//! (GTE-FR-TNZF, GTE-FR-LMXV).
//!
//! A merge writes to no branch and no working copy until a review has let it
//! through, so a scenario needs a record of what both branches and both working
//! copies held before it started. This file holds that record and the commits
//! an arrangement makes on each branch as another author does.

use std::collections::BTreeMap;
use std::path::Path;

use super::outcome::Outcome;
use super::scenario::Scenario;
use super::settings::guard;
use crate::graduation::GraduationRun;
use crate::streams::StreamMergePublication;

/// One file written and everything committed in a repository's working copy, as
/// an author outside the application does it.
pub(super) fn commit_file(repo: &git2::Repository, path: &str, content: &str) -> git2::Oid {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    // The write creates its own parents (FSA-FR-05).
    guard(&workdir)
        .write_text_atomic(workdir.join(path), content)
        .expect("the file is writable");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("the change is staged");
    index.write().expect("the index is written");
    let tree = repo.find_tree(index.write_tree().expect("a tree")).expect("a tree");
    let signature = repo.signature().expect("a signature");
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("the branch head");
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &format!("Edit {path}"),
        &tree,
        &[&head],
    )
    .expect("the change is committed")
}

/// Everything standing in a repository's working copy, committed. The settings
/// a scenario writes into the project are the author's own files, and an
/// uncommitted one would make the base working copy dirty.
pub(super) fn commit_everything(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("the change is staged");
    index.write().expect("the index is written");
    let tree = repo.find_tree(index.write_tree().expect("a tree")).expect("a tree");
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("the branch head");
    if head.tree_id() == tree.id() {
        return;
    }
    let signature = repo.signature().expect("a signature");
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .expect("the change is committed");
}

/// A merge that makes one commit under a message of the author's.
pub(super) fn as_one_commit() -> StreamMergePublication {
    StreamMergePublication::Commit {
        message: "Merge the feature stream".to_string(),
    }
}

/// A stream named `feature` whose branch and whose base branch both changed
/// `README.md`, and which holds a file of its own. The machine can execute an
/// agent, so the handoff is reachable.
pub(super) fn conflicted_merge(name: &'static str) -> Scenario {
    bare_conflict(name).with_execution_allowed()
}

/// The same stream and branches on a machine that is not set up to execute an
/// agent, so the image preflight refuses a handoff.
pub(super) fn bare_conflict(name: &'static str) -> Scenario {
    Scenario::named(name)
        .with_stream("feature")
        .with_stream_commit("README.md", "stream side\n")
        .with_stream_commit("stream-only.txt", "kept\n")
        .with_base_commit("README.md", "base side\n")
}

/// Whether anything stands at a path, read through the guarded handle.
pub(super) fn present(path: &Path) -> bool {
    let parent = path.parent().expect("a path with a parent");
    guard(parent).file_info(path).is_ok()
}

/// A focus that names no run, for a scenario that starts none. It is replaced
/// the moment an act gives the scenario a run.
pub(super) fn no_run() -> GraduationRun {
    GraduationRun::new_for_test("no-run", "", "2026-10-03T00:00:00Z")
}

/// GTE-FR-LMXV: what both branches and both working copies hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiveState {
    stream_tip: Option<String>,
    base_tip: String,
    base_files: BTreeMap<String, String>,
    stream_files: BTreeMap<String, String>,
    base_dirty: Vec<String>,
    stream_dirty: Vec<String>,
}

fn files_under(root: &Path) -> BTreeMap<String, String> {
    super::super::tree_under(root)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".synthesis/"))
        .collect()
}

impl LiveState {
    pub(super) fn base_tip_id(&self) -> String {
        self.base_tip.clone()
    }

    pub(super) fn stream_tip_id(&self) -> Option<String> {
        self.stream_tip.clone()
    }
}

impl Outcome {
    /// What both branches and both working copies hold now.
    pub(super) fn live_state(&self) -> LiveState {
        let root = self.fx.root();
        LiveState {
            stream_tip: self.branch_tip(),
            base_tip: self.base_tip(),
            base_files: files_under(&root),
            stream_files: files_under(&self.worktree),
            base_dirty: crate::streams::uncommitted_paths_of(&root),
            stream_dirty: crate::streams::uncommitted_paths_of(&self.worktree),
        }
    }

    /// GTE-FR-LMXV, GTE-FR-RDPE: both branch heads, the base working copy and
    /// the stream working copy hold exactly what they held before the merge
    /// started.
    pub(crate) fn expect_live_state_unchanged(self) -> Self {
        let before = self
            .live_before
            .clone()
            .unwrap_or_else(|| panic!("[{}] no merge was started, so nothing was recorded", self.name));
        assert_eq!(
            self.live_state(),
            before,
            "[{}] both branches and both working copies hold what they held before the merge",
            self.name
        );
        self
    }
}
