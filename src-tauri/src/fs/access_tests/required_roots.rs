//! A write under a required root that is gone (FSA-FR-RBGJ, FSA-FR-YYSR,
//! FSA-FR-05).

use super::*;
use crate::fs::FsResult;

/// A required root inside a temporary directory, so the test can remove it.
fn required() -> (TempDir, FsAccess, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap().join("worktree");
    fs::create_dir_all(&root).unwrap();
    let access = FsAccess::builder().required_root(&root).build().unwrap();
    (tmp, access, root)
}

fn is_root_gone<T>(result: FsResult<T>) -> bool {
    matches!(result, Err(FsError::RootGone { .. }))
}

// FSA-FR-RBGJ: each write primitive refuses and creates nothing, so a deleted
// worktree does not come back as a directory with no Git files.
#[test]
fn every_write_primitive_refuses_under_a_gone_required_root() {
    let (_tmp, access, root) = required();
    fs::remove_dir_all(&root).unwrap();
    let target = root.join("deep").join("file.md");

    assert!(is_root_gone(access.write_bytes_atomic(&target, b"x")));
    assert!(is_root_gone(access.write_text_atomic(&target, "x")));
    assert!(is_root_gone(access.write_toml_atomic(&target, &vec!["x"])));
    assert!(is_root_gone(access.create_file(&target, CreateMode::Truncate)));
    assert!(is_root_gone(access.open_file(&target, OpenMode::Append).map(|_| ())));
    assert!(is_root_gone(access.append_lines(&target, &["x".to_string()])));
    assert!(is_root_gone(access.create_dir(root.join("deep"))));
    assert!(is_root_gone(access.ensure_gitignored(root.join(".synthesis"))));

    assert!(!root.exists(), "nothing came back");
}

// FSA-FR-RBGJ: a path the author chose is a write like any other.
#[test]
fn a_user_chosen_write_under_a_gone_required_root_is_refused() {
    let (_tmp, access, root) = required();
    fs::remove_dir_all(&root).unwrap();
    let chosen = UserChosenPath::from_frontend_response(root.join("a.log")).unwrap();

    assert!(is_root_gone(access.write_bytes_at_user_choice(&chosen, b"x")));
    assert!(is_root_gone(access.write_text_at_user_choice(&chosen, "x")));
    assert!(!root.exists());
}

// FSA-FR-RBGJ: a user-chosen write outside every root is not refused by this.
#[test]
fn a_user_chosen_write_elsewhere_still_lands() {
    let (tmp, access, root) = required();
    fs::remove_dir_all(&root).unwrap();
    let outside = fs::canonicalize(tmp.path()).unwrap().join("export.log");
    let chosen = UserChosenPath::from_frontend_response(&outside).unwrap();

    access.write_bytes_at_user_choice(&chosen, b"x").unwrap();

    assert_eq!(fs::read(&outside).unwrap(), b"x");
}

// FSA-FR-RBGJ: the typed refusal says what it is.
#[test]
fn the_refusal_reads_as_the_typed_root_gone() {
    let (_tmp, access, root) = required();
    fs::remove_dir_all(&root).unwrap();

    let refusal = access.write_bytes_atomic(root.join("a.md"), b"x").unwrap_err();

    assert!(refusal.to_string().starts_with("root_gone"), "{refusal}");
}

// FSA-FR-RBGJ: a read of a gone root still answers on its own terms.
#[test]
fn a_read_under_a_gone_required_root_is_not_refused_as_a_write() {
    let (_tmp, access, root) = required();
    fs::remove_dir_all(&root).unwrap();

    assert!(matches!(access.read_text(root.join("a.md")), Err(FsError::NotFound { .. })));
    assert!(!is_root_gone(access.open_file(root.join("a.md"), OpenMode::ReadOnly).map(|_| ())));
}

// FSA-FR-RBGJ: a move or a copy is refused by where it lands.
#[test]
fn a_move_or_a_copy_into_a_gone_required_root_is_refused() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let root = base.join("worktree");
    let other = base.join("other");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("a.md"), "x").unwrap();
    let access = FsAccess::builder().required_root(&root).allow_root(&other).build().unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(is_root_gone(access.copy_path(other.join("a.md"), root.join("a.md"))));
    assert!(is_root_gone(access.move_path(other.join("a.md"), root.join("a.md"))));
    assert!(other.join("a.md").exists(), "the source is untouched");
    assert!(!root.exists());
}

// FSA-FR-RBGJ, FSA-FR-05: a missing directory under a root that exists is made.
#[test]
fn a_missing_directory_under_a_live_required_root_is_created() {
    let (_tmp, access, root) = required();
    let target = root.join("deep").join("er").join("file.md");

    access.write_bytes_atomic(&target, b"x").unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"x");
}

// FSA-FR-RBGJ: a root reached by another spelling is the same required root.
#[cfg(unix)]
#[test]
fn a_required_root_is_matched_in_the_spelling_it_was_given() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let real = base.join("real");
    let link = base.join("link");
    fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let access = FsAccess::builder().required_root(&link).build().unwrap();
    fs::remove_dir_all(&real).unwrap();

    assert!(is_root_gone(access.write_bytes_atomic(link.join("a.md"), b"x")));
    assert!(is_root_gone(access.write_bytes_atomic(real.join("a.md"), b"x")));
    assert!(!real.exists());
}

// FSA-FR-YYSR: a root that is not required is made on first use, also on an
// instance that holds a required root.
#[test]
fn a_root_that_is_not_required_is_created_on_first_write() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let worktree = base.join("worktree");
    let owned = base.join("owned");
    fs::create_dir_all(&worktree).unwrap();
    fs::create_dir_all(&owned).unwrap();
    let access = FsAccess::builder().required_root(&worktree).owned_root(&owned).build().unwrap();
    fs::remove_dir_all(&owned).unwrap();

    access.write_bytes_atomic(owned.join("a.md"), b"x").unwrap();

    assert_eq!(fs::read(owned.join("a.md")).unwrap(), b"x");
}

// FSA-FR-YYSR, FSA-FR-21, FSA-FR-29: the IDE instance and each agent instance
// hold the active worktree as a required root.
#[test]
fn the_ide_and_agent_instances_require_the_active_worktree() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let worktree = base.join("worktree");
    let store = base.join("store");
    fs::create_dir_all(&worktree).unwrap();
    fs::create_dir_all(&store).unwrap();
    let state = FsAccessState::default();
    state.install_for_worktree_and(&worktree, &store).unwrap();
    let ide = state.get().unwrap();
    let agent = state.open_agent_session("s1").unwrap();
    fs::remove_dir_all(&worktree).unwrap();

    assert!(is_root_gone(ide.write_bytes_atomic(worktree.join("a.md"), b"x")));
    assert!(is_root_gone(agent.write_bytes_atomic(worktree.join("a.md"), b"x")));
    assert!(!worktree.exists());
}

// FSA-FR-YYSR, FSA-FR-21: the production IDE instance, and no test twin of it.
#[test]
fn the_production_ide_instance_requires_the_active_worktree() {
    let tmp = TempDir::new().unwrap();
    let worktree = fs::canonicalize(tmp.path()).unwrap().join("worktree");
    fs::create_dir_all(&worktree).unwrap();
    let state = FsAccessState::default();
    state.install_for_worktree(&worktree).unwrap();
    let ide = state.get().unwrap();
    fs::remove_dir_all(&worktree).unwrap();

    assert!(is_root_gone(ide.write_bytes_atomic(worktree.join("a.md"), b"x")));
    assert!(!worktree.exists());
}

// FSA-FR-YYSR: a session temp directory is never a required root.
#[test]
fn a_session_temp_directory_is_not_a_required_root() {
    let (_tmp, _access, root) = required();
    let access = FsAccess::builder().required_root(&root).session_temp(true).build().unwrap();
    let temp = access.session_temp_dir().unwrap().to_path_buf();
    fs::remove_dir_all(&temp).unwrap();

    access.write_bytes_atomic(temp.join("a.tmp"), b"x").unwrap();

    assert_eq!(fs::read(temp.join("a.tmp")).unwrap(), b"x");
}

// FSA-FR-YYSR: `required_root` allowlists its directory, as `allow_root` does.
#[test]
fn a_required_root_is_an_allowed_root() {
    let (_tmp, access, root) = required();

    assert!(access.roots().contains(&root));
}
