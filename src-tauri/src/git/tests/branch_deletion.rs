//! Inspecting and deleting a local branch with its linked worktree
//! (GTC-FR-UMXA, GTC-FR-WNZH, GTC-FR-AVKD, GTC-FR-LEBC, GTC-FR-JOWX,
//! GTC-FR-NPCT). The remote part is in `branch_deletion_remote.rs`.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use std::sync::atomic::{AtomicUsize, Ordering};

use tauri::{Listener, Manager};

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::worktree::BRANCHES_CHANGED;

/// A repository with its primary worktree open as the project, in a headless
/// app that carries the managed state a deletion reads: the stream store, the
/// filesystem access, and the project.
pub(super) struct DeletionFixture {
    pub f: WorktreeFixture,
    pub app: tauri::AppHandle<tauri::test::MockRuntime>,
    _store_dir: TempDir,
    pub settings: GlobalSettingsStore,
    pub tokens: GithubTokens,
    pub buffer: &'static LogBuffer,
    changed: Arc<AtomicUsize>,
    _holder: tauri::App<tauri::test::MockRuntime>,
}

impl DeletionFixture {
    pub fn new() -> Self {
        let f = WorktreeFixture::new();
        let store_dir = TempDir::new().unwrap();
        let store = crate::changes::canonicalize_lenient(store_dir.path());
        let holder = tauri::test::mock_app();
        let app = holder.handle().clone();
        app.manage(crate::fs::FsAccessState::default());
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::streams::StreamState::rooted_at(store.clone()));
        app.manage(crate::graduation::GraduationState::rooted_at(store.clone()));
        let root = f.root();
        app.state::<crate::fs::FsAccessState>()
            .install_for_worktree_and(&root, &store)
            .unwrap();
        let access = app.state::<crate::fs::FsAccessState>().get().unwrap();
        let project = app.state::<crate::project::ProjectState>();
        project.set_root_with_access(root.clone(), Some(access));
        project.set_anchor(root.to_string_lossy().into_owned());

        let changed = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&changed);
        app.listen(BRANCHES_CHANGED, move |_event| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        DeletionFixture {
            f,
            app,
            _store_dir: store_dir,
            settings: GlobalSettingsStore::in_memory(),
            tokens: GithubTokens::default(),
            buffer: own_buffer(),
            changed,
            _holder: holder,
        }
    }

    /// The content root the project has right now.
    pub fn active_root(&self) -> PathBuf {
        self.app
            .state::<crate::project::ProjectState>()
            .require_root()
            .unwrap()
            .path()
            .to_path_buf()
    }

    /// Root the project in another worktree, as an activation does.
    pub fn activate(&self, path: &std::path::Path) {
        self.app
            .state::<crate::project::ProjectState>()
            .set_root(path.to_path_buf());
    }

    pub fn inspect(&self, name: &str) -> Result<BranchDeletionPlan, String> {
        inspect_branch_deletion_reported(&self.app, self.buffer, &self.active_root(), name)
    }

    pub fn delete(
        &self,
        name: &str,
        delete_remote: bool,
        discard: bool,
    ) -> Result<BranchDeletionOutcome, String> {
        delete_branch_at(
            &self.app,
            self.buffer,
            &self.active_root(),
            "key",
            &self.settings,
            &self.tokens,
            name,
            delete_remote,
            discard,
        )
    }

    /// A linked worktree at a sibling directory with `branch` checked out.
    pub fn linked(&self, dir: &str, branch: &str) -> PathBuf {
        if self.f.repo().find_branch(branch, BranchType::Local).is_err() {
            self.f.branch(branch);
        }
        crate::worktree::create_worktree_at(
            &self.f.root(),
            branch,
            &self.f.sibling(dir).to_string_lossy(),
        )
        .unwrap()
    }

    pub fn has_branch(&self, name: &str) -> bool {
        self.f.repo().find_branch(name, BranchType::Local).is_ok()
    }

    /// How many `"branches changed"` events the app has delivered.
    pub fn events(&self) -> usize {
        self.changed.load(Ordering::SeqCst)
    }
}

pub(super) fn registered_worktrees(f: &WorktreeFixture) -> usize {
    f.repo().worktrees().unwrap().len()
}

// ---------------------------------------------------------------------------
// Inspection (GTC-FR-UMXA)
// ---------------------------------------------------------------------------

// GTC-FR-UMXA, GTC-FR-AVKD
#[test]
fn inspecting_a_branch_in_an_inactive_linked_worktree_plans_its_removal() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    let before = fx.f.snapshot_all();

    let plan = fx.inspect("feature").unwrap();

    assert_eq!(plan.branch, "feature");
    let worktree = plan.worktree.expect("the worktree the deletion removes");
    assert_eq!(std::path::Path::new(&worktree.path), path.as_path());
    assert!(!worktree.is_primary && !worktree.is_active);
    assert!(plan.stream.is_none());
    assert!(plan.uncommitted_paths.is_empty());
    assert!(plan.remote_branch.is_none());
    assert_eq!(before, fx.f.snapshot_all(), "an inspection writes nothing");
    assert_eq!(fx.events(), 0);
}

// GTC-FR-UMXA
#[test]
fn the_plan_of_a_branch_checked_out_nowhere_names_no_worktree() {
    let fx = DeletionFixture::new();
    fx.f.branch("spare");

    let plan = fx.inspect("spare").unwrap();

    assert!(plan.worktree.is_none());
    let json = serde_json::to_value(&plan).unwrap();
    assert!(json.get("worktree").is_none());
    assert_eq!(json["uncommittedPaths"], serde_json::json!([]));
}

// GTC-FR-UMXA
#[test]
fn the_plan_names_the_remote_branch_the_local_branch_tracks() {
    let fx = DeletionFixture::new();
    fx.f.branch("tracked");
    fx.f.remote_ref("origin/tracked");
    fx.f.branch("by-name");
    fx.f.remote_ref("origin/by-name");
    fx.f.branch("elsewhere");
    fx.f.remote_ref("origin/elsewhere-renamed");
    fx.f.repo()
        .find_branch("tracked", BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/tracked"))
        .unwrap();
    fx.f.repo()
        .find_branch("elsewhere", BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/elsewhere-renamed"))
        .unwrap();

    assert_eq!(fx.inspect("tracked").unwrap().remote_branch.as_deref(), Some("origin/tracked"));
    assert_eq!(
        fx.inspect("by-name").unwrap().remote_branch.as_deref(),
        Some("origin/by-name"),
        "the branch of the same name on the primary remote"
    );
    assert_eq!(
        fx.inspect("elsewhere").unwrap().remote_branch.as_deref(),
        Some("origin/elsewhere-renamed"),
        "the upstream wins over the same name"
    );
    fx.f.branch("lonely");
    assert!(fx.inspect("lonely").unwrap().remote_branch.is_none());
}

// GTC-FR-UMXA, GTC-FR-LEBC
#[test]
fn the_plan_lists_every_uncommitted_path_tracked_and_untracked() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::write(path.join("a.md"), "changed\n").unwrap();
    std::fs::write(path.join("new.txt"), "fresh\n").unwrap();

    let plan = fx.inspect("feature").unwrap();

    assert_eq!(plan.uncommitted_paths, vec!["a.md".to_string(), "new.txt".to_string()]);
}

// GTC-FR-LEBC
#[test]
fn a_worktree_whose_directory_is_missing_reports_no_uncommitted_path() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::remove_dir_all(&path).unwrap();

    let plan = fx.inspect("feature").unwrap();

    assert!(plan.uncommitted_paths.is_empty());
    assert!(plan.worktree.is_some(), "its registration still stands");
}

// GTC-FR-UMXA
#[test]
fn the_plan_serialises_to_the_camel_case_contract() {
    let fx = DeletionFixture::new();
    fx.linked("wt-feature", "feature");
    fx.f.remote_ref("origin/feature");

    let json = serde_json::to_value(fx.inspect("feature").unwrap()).unwrap();

    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["branch", "remoteBranch", "uncommittedPaths", "worktree"]);
    let mut worktree: Vec<&str> = json["worktree"].as_object().unwrap().keys().map(String::as_str).collect();
    worktree.sort();
    assert_eq!(worktree, vec!["isActive", "isPrimary", "name", "path"]);
}

// ---------------------------------------------------------------------------
// Refusals (GTC-FR-WNZH)
// ---------------------------------------------------------------------------

// GTC-FR-WNZH, GTC-FR-UMXA
#[test]
fn an_unknown_name_and_a_remote_tracking_name_are_refused_in_that_order() {
    let fx = DeletionFixture::new();
    fx.f.remote_ref("origin/experiment");
    let before = fx.f.snapshot_all();

    for call in [fx.inspect("nothing"), fx.inspect("")] {
        assert_eq!(call.unwrap_err(), ERR_UNKNOWN_BRANCH);
    }
    assert_eq!(fx.inspect("origin/experiment").unwrap_err(), ERR_NOT_A_LOCAL_BRANCH);
    assert_eq!(fx.delete("nothing", false, false).unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert_eq!(fx.delete("origin/experiment", false, false).unwrap_err(), ERR_NOT_A_LOCAL_BRANCH);

    assert_eq!(before, fx.f.snapshot_all());
    assert_eq!(fx.events(), 0);
}

// GTC-FR-WNZH, GTC-FR-UMXA
#[test]
fn the_branch_of_the_primary_worktree_is_refused_and_nothing_changes() {
    let fx = DeletionFixture::new();
    let primary = fx.f.current();
    fx.linked("wt-feature", "feature");
    let before = fx.f.snapshot_all();

    assert_eq!(fx.inspect(&primary).unwrap_err(), ERR_BRANCH_IN_PRIMARY_WORKTREE);
    assert_eq!(fx.delete(&primary, true, true).unwrap_err(), ERR_BRANCH_IN_PRIMARY_WORKTREE);

    assert!(fx.has_branch(&primary));
    assert_eq!(before, fx.f.snapshot_all());
    assert_eq!(fx.events(), 0, "a refusal emits nothing");
}

// GTC-FR-WNZH, GTC-FR-UMXA
#[test]
fn the_branch_of_the_active_linked_worktree_is_refused_and_the_primary_check_comes_first() {
    let fx = DeletionFixture::new();
    let primary = fx.f.current();
    let path = fx.linked("wt-feature", "feature");
    fx.activate(&path);
    let before = fx.f.snapshot_all();

    assert_eq!(fx.inspect("feature").unwrap_err(), ERR_BRANCH_IN_ACTIVE_WORKTREE);
    assert_eq!(fx.delete("feature", false, true).unwrap_err(), ERR_BRANCH_IN_ACTIVE_WORKTREE);
    // The primary worktree is not active here, and it is still the stronger
    // refusal for its own branch.
    assert_eq!(fx.delete(&primary, false, false).unwrap_err(), ERR_BRANCH_IN_PRIMARY_WORKTREE);

    assert!(fx.has_branch("feature"));
    assert_eq!(before, fx.f.snapshot_all());
    assert_eq!(fx.events(), 0);
}

// GTC-FR-WNZH, GTC-FR-UMXA
#[test]
fn a_dispatched_direct_graduation_run_refuses_a_deletion() {
    let fx = DeletionFixture::new();
    fx.f.branch("spare");
    let claimed = fx.app.state::<crate::graduation::GraduationState>().claim(
        "w1",
        "run-1",
        true,
        crate::project_settings::GraduationConcurrency::limited(2),
        crate::tools::agent_exec::runtime::CancellationToken::new(),
    );
    assert!(claimed);

    let refused = fx.delete("spare", false, false).unwrap_err();

    assert!(refused.starts_with(crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE), "{refused}");
    assert_eq!(fx.inspect("spare").unwrap_err(), refused);
    assert!(fx.has_branch("spare"));
    assert_eq!(fx.events(), 0);
}

// GTC-FR-WNZH
#[test]
fn a_dirty_worktree_is_refused_with_every_path_until_the_discard_is_confirmed() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::write(path.join("a.md"), "changed\n").unwrap();
    std::fs::write(path.join("new.txt"), "fresh\n").unwrap();
    let before = fx.f.snapshot_all();

    let refused = fx.delete("feature", false, false).unwrap_err();

    assert_eq!(refused, format!("{ERR_WORKTREE_DIRTY}: a.md, new.txt"));
    assert!(fx.has_branch("feature"));
    assert!(path.join("new.txt").exists());
    assert_eq!(before, fx.f.snapshot_all(), "a refusal changes nothing");
    assert_eq!(fx.events(), 0);

    let outcome = fx.delete("feature", false, true).unwrap();

    assert_eq!(outcome.branch, "feature");
    assert!(!fx.has_branch("feature"));
    assert!(!path.exists(), "the worktree went with its uncommitted paths");
}

// ---------------------------------------------------------------------------
// Deletion (GTC-FR-AVKD, GTC-FR-NPCT)
// ---------------------------------------------------------------------------

// GTC-FR-AVKD, GTC-FR-NPCT
#[test]
fn deleting_a_branch_in_an_inactive_linked_worktree_removes_both_and_switches_nothing() {
    let fx = DeletionFixture::new();
    let primary = fx.f.current();
    let path = fx.linked("wt-feature", "feature");
    assert_eq!(registered_worktrees(&fx.f), 1);

    let outcome = fx.delete("feature", false, false).unwrap();

    assert_eq!(outcome.branch, "feature");
    assert_eq!(
        outcome.removed_worktree_path.as_deref().map(std::path::Path::new),
        Some(path.as_path())
    );
    assert!(!path.exists(), "the directory is gone");
    assert_eq!(registered_worktrees(&fx.f), 0, "and so is its registration");
    assert!(!fx.has_branch("feature"));
    assert_eq!(fx.f.current(), primary, "no branch was switched");
    assert_eq!(fx.active_root(), fx.f.root(), "no worktree was switched");
    assert_eq!(fx.events(), 1, "one branches-changed event");
    assert!(!outcome.remote.requested);
    assert_eq!(outcome.remote.state, REMOTE_NOT_REQUESTED);
}

// GTC-FR-AVKD, WTC-FR-RDVK, FSA-FR-OWVT
#[cfg(unix)]
#[test]
fn a_linked_worktree_holding_symbolic_links_is_deleted_with_its_branch_and_no_link_is_followed() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    let store = tempfile::TempDir::new().unwrap();
    std::fs::write(store.path().join("dep.js"), "module.exports = {};").unwrap();
    // Ignored by the repository, as an installed dependency store is, so it is
    // in no uncommitted-path list and gives no warning before the removal.
    let exclude = fx.f.root().join(".git/info/exclude");
    std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    std::fs::write(&exclude, "node_modules/\n").unwrap();
    std::fs::create_dir_all(path.join("node_modules")).unwrap();
    std::os::unix::fs::symlink(store.path(), path.join("node_modules/dep")).unwrap();

    let outcome = fx.delete("feature", false, false).unwrap();

    assert_eq!(outcome.branch, "feature");
    assert!(!path.exists(), "the directory is gone");
    assert_eq!(registered_worktrees(&fx.f), 0);
    assert!(!fx.has_branch("feature"));
    assert!(store.path().join("dep.js").exists(), "the link target is untouched");
}

// GTC-FR-AVKD
#[test]
fn deleting_a_branch_checked_out_nowhere_removes_only_the_branch() {
    let fx = DeletionFixture::new();
    fx.f.branch("spare");
    let other = fx.linked("wt-other", "other");

    let outcome = fx.delete("spare", false, false).unwrap();

    assert!(outcome.removed_worktree_path.is_none());
    assert!(!fx.has_branch("spare"));
    assert!(other.exists(), "another worktree is untouched");
    assert!(fx.has_branch("other"));
    assert_eq!(fx.events(), 1);
    let json = serde_json::to_value(&outcome).unwrap();
    assert!(json.get("removedWorktreePath").is_none());
    assert_eq!(json["remote"]["requested"], false);
    assert_eq!(json["remote"]["state"], "not_requested");
}

// GTC-FR-AVKD
#[test]
fn deleting_the_branch_of_a_worktree_whose_directory_is_gone_prunes_its_registration() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::remove_dir_all(&path).unwrap();
    assert_eq!(registered_worktrees(&fx.f), 1);

    let outcome = fx.delete("feature", false, false).unwrap();

    assert!(outcome.removed_worktree_path.is_some());
    assert_eq!(registered_worktrees(&fx.f), 0);
    assert!(!fx.has_branch("feature"));
}

// GTC-FR-LEBC, GTC-FR-AVKD
#[test]
fn an_untracked_path_is_discarded_with_the_worktree_once_confirmed() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::create_dir_all(path.join("notes")).unwrap();
    std::fs::write(path.join("notes/idea.md"), "idea\n").unwrap();

    assert_eq!(
        fx.delete("feature", false, false).unwrap_err(),
        format!("{ERR_WORKTREE_DIRTY}: notes/idea.md")
    );
    fx.delete("feature", false, true).unwrap();

    assert!(!path.exists());
}

// ---------------------------------------------------------------------------
// A work stream's branch (GTC-FR-JOWX)
// ---------------------------------------------------------------------------

// GTC-FR-JOWX, GTC-FR-UMXA
#[test]
fn inspecting_a_stream_branch_names_the_stream_its_worktree_and_its_uncommitted_paths() {
    let fx = DeletionFixture::new();
    let stream = crate::streams::create_work_stream(fx.app.clone(), "Editor".into(), None).unwrap();
    std::fs::write(std::path::Path::new(&stream.worktree_path).join("draft.txt"), "wip\n").unwrap();

    let plan = fx.inspect(&stream.branch).unwrap();

    let named = plan.stream.clone().expect("the stream that owns the branch");
    assert_eq!(named.stream_id, stream.id);
    assert_eq!(named.stream_name, "Editor");
    assert!(named.busy_run_id.is_none());
    assert_eq!(plan.uncommitted_paths, vec!["draft.txt".to_string()]);
    assert!(plan.worktree.is_some());
    let json = serde_json::to_value(&plan).unwrap();
    assert!(json["stream"]["busyRunId"].is_null());
    assert_eq!(named.ahead_of_base, 0, "a fresh stream holds nothing its base lacks");
}

// GTC-FR-JOWX: the plan counts the commits the stream holds that its base does
// not, so the confirmation can warn before a forced deletion.
#[test]
fn inspecting_a_stream_branch_counts_the_commits_its_base_lacks() {
    let fx = DeletionFixture::new();
    let stream = crate::streams::create_work_stream(fx.app.clone(), "Ahead".into(), None).unwrap();
    let worktree = git2::Repository::open(&stream.worktree_path).unwrap();
    for (n, name) in ["one.txt", "two.txt"].iter().enumerate() {
        std::fs::write(std::path::Path::new(&stream.worktree_path).join(name), "work\n").unwrap();
        let mut index = worktree.index().unwrap();
        index.add_path(std::path::Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = worktree.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = worktree.signature().unwrap();
        let parent = worktree.head().unwrap().peel_to_commit().unwrap();
        worktree
            .commit(Some("HEAD"), &sig, &sig, &format!("work {n}"), &tree, &[&parent])
            .unwrap();
    }

    let plan = fx.inspect(&stream.branch).unwrap();

    assert_eq!(plan.stream.as_ref().unwrap().ahead_of_base, 2);
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["stream"]["aheadOfBase"], serde_json::json!(2));
}

// GTC-FR-JOWX, GTC-FR-WNZH
#[test]
fn a_stream_branch_is_refused_by_the_deletion_and_nothing_changes() {
    let fx = DeletionFixture::new();
    let stream = crate::streams::create_work_stream(fx.app.clone(), "Editor".into(), None).unwrap();
    let before = fx.f.snapshot_all();
    // The creation announces its own branch (WKS-FR-SPVK); the refusal adds none.
    let events_before = fx.events();

    let refused = fx.delete(&stream.branch, true, true).unwrap_err();

    assert_eq!(refused, format!("{ERR_BRANCH_BELONGS_TO_WORK_STREAM}: {}", stream.id));
    assert!(fx.has_branch(&stream.branch));
    assert!(std::path::Path::new(&stream.worktree_path).is_dir());
    assert_eq!(before, fx.f.snapshot_all());
    assert_eq!(fx.events(), events_before);
}

// ---------------------------------------------------------------------------
// Records (GTC-FR-NPCT)
// ---------------------------------------------------------------------------

// GTC-FR-NPCT
#[test]
fn a_refusal_and_a_result_are_logged_with_names_and_counts_but_no_path() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    std::fs::write(path.join("secret-notes.txt"), "private\n").unwrap();

    fx.delete("feature", false, false).unwrap_err();
    fx.inspect("feature").unwrap();
    fx.delete("feature", false, true).unwrap();

    let refused = records_for(fx.buffer, "branch deletion refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(refused[0].fields.get("branch"), Some(&serde_json::json!("feature")));
    assert_eq!(refused[0].fields.get("uncommittedCount"), Some(&serde_json::json!(1)));
    assert_eq!(record_for(fx.buffer, "branch deletion inspected").fields.get("uncommittedCount"), Some(&serde_json::json!(1)));
    let done = record_for(fx.buffer, "branch deleted");
    assert_eq!(done.fields.get("removedWorktree"), Some(&serde_json::json!(true)));
    assert_eq!(done.fields.get("discardedCount"), Some(&serde_json::json!(1)));
    let text = buffer_text(fx.buffer);
    assert!(!text.contains("secret-notes"), "no path content in a record: {text}");
}

// GTC-FR-NPCT, GTC-FR-WNZH
#[test]
fn an_unknown_branch_refusal_is_a_warning_record() {
    let fx = DeletionFixture::new();

    fx.delete("nothing", false, false).unwrap_err();

    let refused = record_for(fx.buffer, "branch deletion refused");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert_eq!(refused.fields.get("error"), Some(&serde_json::json!(ERR_UNKNOWN_BRANCH)));
}
