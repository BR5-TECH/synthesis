//! Deleting a branch whose linked worktree is locked (GTC-FR-IVPE,
//! GTC-FR-MSXC, GTC-FR-JUZP, `WTC-worktree-context.md` WTC-FR-KELA).
//!
//! An agent tool locks the worktree it works in (`git worktree lock`). The lock
//! is named in the plan and does not stop a deletion the author confirmed.

use super::branch_deletion::{registered_worktrees, DeletionFixture};
use super::*;

const REASON: &str = "claude agent agent-x (pid 89346 start Sun Oct 11 14:19:41 2026)";

/// The administrative directory of the registration of the worktree at `path`.
fn registration_dir(fx: &DeletionFixture, path: &std::path::Path) -> PathBuf {
    let repo = fx.f.repo();
    let wanted = crate::changes::canonicalize_lenient(path);
    let name = repo
        .worktrees()
        .unwrap()
        .iter()
        .filter_map(|n| n.ok().flatten())
        .find(|n| {
            let wt = repo.find_worktree(n).unwrap();
            crate::changes::canonicalize_lenient(wt.path()) == wanted
        })
        .expect("a registration for the path")
        .to_string();
    repo.commondir().join("worktrees").join(name)
}

/// Lock the worktree at `path` with these exact bytes in its `locked` file.
/// `git worktree lock --reason X` writes `X` and a newline; without a reason it
/// writes an empty file.
fn lock_bytes(fx: &DeletionFixture, path: &std::path::Path, contents: &[u8]) {
    std::fs::write(registration_dir(fx, path).join("locked"), contents).unwrap();
}

fn lock(fx: &DeletionFixture, path: &std::path::Path, reason: Option<&str>) {
    let line = reason.map(|r| format!("{r}\n")).unwrap_or_default();
    lock_bytes(fx, path, line.as_bytes());
}

// GTC-FR-MSXC, WTC-FR-KELA
#[test]
fn deleting_the_branch_of_a_locked_worktree_removes_the_worktree_and_the_branch() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(REASON));
    let admin = registration_dir(&fx, &path);

    let outcome = fx.delete("feature", false, false).unwrap();

    assert!(outcome.removed_worktree_path.is_some());
    assert!(!path.exists());
    assert!(!admin.exists(), "the lock goes with the registration");
    assert_eq!(registered_worktrees(&fx.f), 0);
    assert!(!fx.has_branch("feature"));
    assert_eq!(fx.events(), 1);
}

// GTC-FR-MSXC, WTC-FR-KELA, GTC-FR-IVPE
#[test]
fn a_locked_worktree_whose_directory_is_gone_is_planned_with_its_lock_and_deleted() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(REASON));
    std::fs::remove_dir_all(&path).unwrap();

    let plan = fx.inspect("feature").unwrap();
    assert_eq!(plan.worktree_lock.and_then(|l| l.reason).as_deref(), Some(REASON));

    fx.delete("feature", false, false).unwrap();

    assert_eq!(registered_worktrees(&fx.f), 0);
    assert!(!fx.has_branch("feature"));
}

// GTC-FR-IVPE, GTC-FR-UMXA
#[test]
fn the_plan_carries_the_trimmed_lock_reason_in_the_camel_case_contract() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(&format!("  {REASON}\n")));

    let json = serde_json::to_value(fx.inspect("feature").unwrap()).unwrap();

    assert_eq!(json["worktreeLock"], serde_json::json!({ "reason": REASON }));
}

// GTC-FR-IVPE
#[test]
fn a_lock_with_no_reason_or_a_blank_one_carries_no_reason() {
    for reason in [None, Some("   \n")] {
        let fx = DeletionFixture::new();
        let path = fx.linked("wt-feature", "feature");
        lock(&fx, &path, reason);

        let json = serde_json::to_value(fx.inspect("feature").unwrap()).unwrap();

        assert_eq!(json["worktreeLock"], serde_json::json!({}), "reason {reason:?}");
    }
}

// GTC-FR-IVPE
#[test]
fn an_unlocked_worktree_and_a_branch_checked_out_nowhere_carry_no_lock() {
    let fx = DeletionFixture::new();
    fx.linked("wt-feature", "feature");
    fx.f.branch("loose");

    for name in ["feature", "loose"] {
        let json = serde_json::to_value(fx.inspect(name).unwrap()).unwrap();
        assert!(json.get("worktreeLock").is_none(), "{name}: {json}");
    }
}

// GTC-FR-IVPE
#[test]
fn the_lock_of_another_worktree_is_not_named_in_the_plan() {
    let fx = DeletionFixture::new();
    fx.linked("wt-feature", "feature");
    let other = fx.linked("wt-other", "other");
    lock(&fx, &other, Some(REASON));

    let plan = fx.inspect("feature").unwrap();

    assert!(plan.worktree_lock.is_none());
    assert!(fx.inspect("other").unwrap().worktree_lock.is_some());
}

// GTC-FR-JUZP
#[test]
fn records_name_the_lock_as_a_boolean_and_never_the_reason() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(REASON));

    fx.inspect("feature").unwrap();
    fx.delete("feature", false, false).unwrap();

    let locked = Some(&serde_json::json!(true));
    assert_eq!(record_for(fx.buffer, "branch deletion inspected").fields.get("worktreeLocked"), locked);
    assert_eq!(record_for(fx.buffer, "branch deleted").fields.get("worktreeLocked"), locked);
    let text = buffer_text(fx.buffer);
    assert!(!text.contains("pid 89346"), "no lock reason in a record: {text}");
}

// GTC-FR-JUZP
#[test]
fn records_of_an_unlocked_worktree_say_it_is_not_locked() {
    let fx = DeletionFixture::new();
    fx.linked("wt-feature", "feature");

    fx.inspect("feature").unwrap();
    fx.delete("feature", false, false).unwrap();

    let unlocked = Some(&serde_json::json!(false));
    assert_eq!(record_for(fx.buffer, "branch deletion inspected").fields.get("worktreeLocked"), unlocked);
    assert_eq!(record_for(fx.buffer, "branch deleted").fields.get("worktreeLocked"), unlocked);
}

// GTC-FR-MSXC, GTC-FR-WNZH
#[test]
fn a_lock_does_not_bypass_the_dirty_refusal_and_survives_it() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(REASON));
    std::fs::write(path.join("notes.txt"), "draft\n").unwrap();

    let refusal = fx.delete("feature", false, false).unwrap_err();

    assert!(refusal.starts_with(ERR_WORKTREE_DIRTY), "{refusal}");
    assert!(path.join("notes.txt").exists());
    assert!(registration_dir(&fx, &path).join("locked").exists());
    assert!(fx.has_branch("feature"));
    assert_eq!(fx.events(), 0);

    fx.delete("feature", false, true).unwrap();

    assert_eq!(registered_worktrees(&fx.f), 0);
    assert!(!fx.has_branch("feature"));
}

// GTC-FR-IVPE, GTC-FR-UMXA
#[test]
fn the_inspection_reads_the_lock_and_leaves_it_in_place() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock(&fx, &path, Some(REASON));

    fx.inspect("feature").unwrap();

    assert!(registration_dir(&fx, &path).join("locked").exists());
    assert!(path.is_dir());
    assert_eq!(fx.events(), 0);
}

// GTC-FR-IVPE, GTC-FR-MSXC
#[test]
fn a_lock_reason_that_is_not_utf8_is_read_lossily_and_the_deletion_goes_on() {
    let fx = DeletionFixture::new();
    let path = fx.linked("wt-feature", "feature");
    lock_bytes(&fx, &path, b"agent \xff\xfe\n");

    let plan = fx.inspect("feature").unwrap();
    let reason = plan.worktree_lock.and_then(|l| l.reason).expect("a reason");
    assert_eq!(reason, "agent \u{FFFD}\u{FFFD}");

    fx.delete("feature", false, false).unwrap();
    assert_eq!(registered_worktrees(&fx.f), 0);
}

// GTC-FR-IVPE, GTC-FR-MSXC
#[test]
fn a_lock_is_read_and_removed_while_the_project_is_rooted_in_another_linked_worktree() {
    let fx = DeletionFixture::new();
    let active = fx.linked("wt-a", "a");
    fx.activate(&active);
    let path = fx.linked("wt-b", "b");
    lock(&fx, &path, Some(REASON));

    assert!(fx.inspect("b").unwrap().worktree_lock.is_some());
    fx.delete("b", false, false).unwrap();

    assert!(!path.exists());
    assert!(active.is_dir());
    assert!(fx.has_branch("a"));
    assert!(!fx.has_branch("b"));
}

// GTC-FR-IVPE
#[test]
fn the_plan_of_a_stream_branch_carries_the_lock_of_its_working_copy() {
    let fx = DeletionFixture::new();
    let stream = crate::streams::create_work_stream(fx.app.clone(), "Editor".into(), None).unwrap();
    lock(&fx, std::path::Path::new(&stream.worktree_path), Some(REASON));

    let plan = fx.inspect(&stream.branch).unwrap();

    assert!(plan.stream.is_some());
    assert_eq!(plan.worktree_lock.and_then(|l| l.reason).as_deref(), Some(REASON));
}

// GTC-FR-IVPE, GTC-FR-MSXC
#[test]
fn a_registration_whose_name_differs_from_its_directory_name_is_read_and_removed() {
    let fx = DeletionFixture::new();
    fx.linked("wt-first", "first");
    fx.f.branch("second");
    let path = fx.f.sibling("wt-second");
    let repo = fx.f.repo();
    let second = repo.find_branch("second", BranchType::Local).unwrap();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(second.get()));
    repo.worktree("agent-registration", &path, Some(&opts)).unwrap();
    let admin = registration_dir(&fx, &path);
    assert_eq!(admin.file_name().unwrap(), "agent-registration");
    lock(&fx, &path, Some(REASON));

    assert_eq!(
        fx.inspect("second").unwrap().worktree_lock.and_then(|l| l.reason).as_deref(),
        Some(REASON)
    );
    fx.delete("second", false, false).unwrap();

    assert!(!admin.exists());
    assert!(!fx.has_branch("second"));
    assert_eq!(registered_worktrees(&fx.f), 1, "the other worktree stays");
}
