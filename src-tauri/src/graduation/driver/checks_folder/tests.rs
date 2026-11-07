//! The check folder against a real repository
//! (`../ai/GRL-graduation-loop.md` GRL-FR-XQMT, GRL-FR-LXWU, GRL-FR-DCKG,
//! GRL-FR-KYYW, GRL-FR-AMQE, GRL-FR-FESZ).

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// A repository with one commit, at a canonical path.
fn repository() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "seed", &tree, &[]).unwrap();
    (dir, root)
}

fn exclude_of(root: &Path) -> String {
    std::fs::read_to_string(root.join(".git/info/exclude")).unwrap_or_default()
}

fn write_report(root: &Path) {
    std::fs::create_dir_all(root.join(DIR).join("logs")).unwrap();
    std::fs::write(root.join(REPORT_PATH), "| cargo test | 0 |\n").unwrap();
    std::fs::write(root.join(DIR).join("logs/cargo-test.log"), "test result: ok\n").unwrap();
}

// GRL-FR-DCKG: the folder and every path in it are check paths, and a path that
// only starts with the same letters is not.
#[test]
fn a_check_path_is_the_folder_or_a_path_inside_it() {
    assert!(is_check_path(".synthesis-checks"));
    assert!(is_check_path(".synthesis-checks/"), "the form an ignored directory reads as");
    assert!(is_check_path(".synthesis-checks/report.md"));
    assert!(is_check_path(".synthesis-checks/logs/cargo-test.log"));
    assert!(is_check_path("./.synthesis-checks/report.md"));
    assert!(!is_check_path(".synthesis-checks-old/report.md"));
    assert!(!is_check_path("src/.synthesis-checks/report.md"));
    assert!(!is_check_path(".synthesis/drafts/a.md"));
    assert!(!is_check_path(""));
}

// GRL-FR-LXWU: the line is added once, and the lines the author wrote stay.
#[test]
fn the_exclude_line_is_written_once_and_keeps_the_author_lines() {
    let (_dir, root) = repository();
    std::fs::create_dir_all(root.join(".git/info")).unwrap();
    std::fs::write(root.join(".git/info/exclude"), "# mine\n*.local").unwrap();

    assert!(ensure_excluded(&root).unwrap(), "the first call adds the line");
    assert!(!ensure_excluded(&root).unwrap(), "the second call adds nothing");

    let text = exclude_of(&root);
    assert!(text.starts_with("# mine\n*.local\n"), "the author lines stay: {text:?}");
    assert_eq!(text.lines().filter(|l| l.trim() == EXCLUDE_LINE).count(), 1);
    assert!(text.ends_with('\n'));
    assert!(!root.join(".gitignore").exists(), "no .gitignore is written");
}

// GRL-FR-LXWU: a repository with no `info` directory gets one.
#[test]
fn the_exclude_line_is_written_where_no_info_directory_stands() {
    let (_dir, root) = repository();
    let _ = std::fs::remove_dir_all(root.join(".git/info"));
    assert!(ensure_excluded(&root).unwrap());
    assert_eq!(exclude_of(&root), format!("{EXCLUDE_LINE}\n"));
}

// GRL-FR-LXWU: a linked worktree writes the line into the common directory it
// shares with the primary checkout, which is where Git reads it.
#[test]
fn a_linked_worktree_writes_the_line_into_the_common_directory() {
    let (_dir, root) = repository();
    let linked_dir = TempDir::new().unwrap();
    let linked = std::fs::canonicalize(linked_dir.path()).unwrap().join("wt");
    let repo = git2::Repository::open(&root).unwrap();
    repo.worktree("wt", &linked, None).unwrap();

    assert!(ensure_excluded(&linked).unwrap());
    assert!(exclude_of(&root).contains(EXCLUDE_LINE));

    // Git now hides the folder in the linked worktree.
    write_report(&linked);
    let linked_repo = git2::Repository::open(&linked).unwrap();
    assert!(linked_repo.status_should_ignore(Path::new(".synthesis-checks/report.md")).unwrap());
}

// GRL-FR-KYYW: the folder is removed whole, and a working copy without one
// is not an error.
#[test]
fn clearing_removes_the_folder_and_accepts_its_absence() {
    let (_dir, root) = repository();
    write_report(&root);
    assert!(clear(&root).unwrap());
    assert!(!root.join(DIR).exists());
    assert!(root.join("README.md").exists(), "nothing else is removed");
    assert!(!clear(&root).unwrap(), "no folder is not an error");
}

// GRL-FR-AMQE, GRL-FR-FESZ: the folder is copied whole into the review
// checkout, and the review is then told where the report is.
#[test]
fn the_folder_is_copied_into_the_checkout_and_named_to_the_review() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    write_report(&root);

    assert_eq!(report_ref(&checkout), MISSING);
    let copied = copy_into(&root, &checkout).unwrap();
    assert_eq!(copied, Copied { found: true, files: 2, skipped: 0 });
    assert_eq!(
        std::fs::read_to_string(checkout.join(REPORT_PATH)).unwrap(),
        "| cargo test | 0 |\n"
    );
    assert!(checkout.join(DIR).join("logs/cargo-test.log").is_file());
    assert_eq!(report_ref(&checkout), REPORT_PATH);
}

// GRL-FR-AMQE, GRL-FR-FESZ: a working copy with no folder gives the checkout
// none, and the review is told the report is missing.
#[test]
fn no_folder_means_no_copy_and_a_missing_report() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    assert!(!copy_into(&root, &checkout).unwrap().found);
    assert!(!checkout.join(DIR).exists());
    assert_eq!(report_ref(&checkout), MISSING);
}

// GRL-FR-FESZ: a folder that holds logs and no report is still a missing
// report.
#[test]
fn a_folder_without_its_report_is_a_missing_report() {
    let (_dir, root) = repository();
    std::fs::create_dir_all(root.join(DIR).join("logs")).unwrap();
    assert_eq!(report_ref(&root), MISSING);
    std::fs::create_dir_all(root.join(REPORT_PATH)).unwrap();
    assert_eq!(report_ref(&root), MISSING, "a directory named report.md is not a report");
}

// GRL-FR-AMQE: a folder the checkout already holds, such as one the base commit
// tracks, is replaced by the work turn's record and never passed off as it.
#[test]
fn a_folder_already_in_the_checkout_is_replaced() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    std::fs::create_dir_all(checkout.join(DIR).join("logs")).unwrap();
    std::fs::write(checkout.join(REPORT_PATH), "stale\n").unwrap();
    std::fs::write(checkout.join(DIR).join("logs/old.log"), "old\n").unwrap();

    assert!(!copy_into(&root, &checkout).unwrap().found);
    assert!(!checkout.join(DIR).exists(), "a working copy with no record leaves none");

    std::fs::create_dir_all(checkout.join(DIR)).unwrap();
    std::fs::write(checkout.join(REPORT_PATH), "stale\n").unwrap();
    write_report(&root);
    copy_into(&root, &checkout).unwrap();
    assert_eq!(
        std::fs::read_to_string(checkout.join(REPORT_PATH)).unwrap(),
        "| cargo test | 0 |\n"
    );
}

// GRL-FR-KYYW: a link the agent planted in the folder is unlinked, and what it
// points at stays untouched.
#[cfg(unix)]
#[test]
fn clearing_unlinks_a_planted_link_and_follows_none() {
    let (_dir, root) = repository();
    let outside_dir = TempDir::new().unwrap();
    let outside = std::fs::canonicalize(outside_dir.path()).unwrap();
    std::fs::write(outside.join("keep.txt"), "keep\n").unwrap();
    write_report(&root);
    std::os::unix::fs::symlink(&outside, root.join(DIR).join("logs/outside")).unwrap();
    std::os::unix::fs::symlink(outside.join("keep.txt"), root.join(DIR).join("latest.log"))
        .unwrap();

    assert!(clear(&root).unwrap());
    assert!(!root.join(DIR).exists());
    assert_eq!(std::fs::read_to_string(outside.join("keep.txt")).unwrap(), "keep\n");
}

// GRL-FR-KYYW: a check folder that is itself a link is unlinked, and its target
// stays.
#[cfg(unix)]
#[test]
fn clearing_unlinks_a_folder_that_is_a_link() {
    let (_dir, root) = repository();
    let outside_dir = TempDir::new().unwrap();
    let outside = std::fs::canonicalize(outside_dir.path()).unwrap();
    std::fs::write(outside.join("report.md"), "elsewhere\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join(DIR)).unwrap();

    assert!(clear(&root).unwrap());
    assert!(std::fs::symlink_metadata(root.join(DIR)).is_err());
    assert!(outside.join("report.md").exists());
}

// GRL-FR-AMQE: the copy leaves links out rather than following them, and still
// copies the record around them.
#[cfg(unix)]
#[test]
fn the_copy_leaves_links_out_and_keeps_the_record() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    let outside_dir = TempDir::new().unwrap();
    let outside = std::fs::canonicalize(outside_dir.path()).unwrap();
    std::fs::write(outside.join("secret.txt"), "not for the review\n").unwrap();
    write_report(&root);
    std::os::unix::fs::symlink(outside.join("secret.txt"), root.join(DIR).join("logs/leak.log"))
        .unwrap();
    std::os::unix::fs::symlink(&outside, root.join(DIR).join("dir-link")).unwrap();

    let copied = copy_into(&root, &checkout).unwrap();
    assert_eq!(copied, Copied { found: true, files: 2, skipped: 2 });
    assert_eq!(report_ref(&checkout), REPORT_PATH);
    assert!(std::fs::symlink_metadata(checkout.join(DIR).join("logs/leak.log")).is_err());
    assert!(std::fs::symlink_metadata(checkout.join(DIR).join("dir-link")).is_err());
}

// GRL-FR-AMQE: a check folder that is itself a link is not copied.
#[cfg(unix)]
#[test]
fn a_folder_that_is_a_link_is_not_copied() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    let outside_dir = TempDir::new().unwrap();
    let outside = std::fs::canonicalize(outside_dir.path()).unwrap();
    std::fs::write(outside.join("report.md"), "elsewhere\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join(DIR)).unwrap();

    let copied = copy_into(&root, &checkout).unwrap();
    assert!(!copied.found);
    assert_eq!(report_ref(&checkout), MISSING);
}

// GRL-FR-AMQE, GRL-FR-FESZ: a report that is a link is not copied, so the review
// is told the report is missing.
#[cfg(unix)]
#[test]
fn a_report_that_is_a_link_reads_as_missing() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    std::fs::create_dir_all(root.join(DIR)).unwrap();
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    std::os::unix::fs::symlink(root.join("README.md"), root.join(REPORT_PATH)).unwrap();

    let copied = copy_into(&root, &checkout).unwrap();
    assert_eq!(copied, Copied { found: true, files: 0, skipped: 1 });
    assert_eq!(report_ref(&checkout), MISSING);
}

// GRL-FR-AMQE: a check folder that is a regular file is not a folder to copy.
#[test]
fn a_check_folder_that_is_a_file_is_not_copied() {
    let (_dir, root) = repository();
    let (_checkout_dir, checkout) = repository();
    std::fs::write(root.join(DIR), "not a folder\n").unwrap();
    let copied = copy_into(&root, &checkout).unwrap();
    assert_eq!(copied, Copied { found: false, files: 0, skipped: 1 });
    assert!(!checkout.join(DIR).exists());
}

// GRL-FR-LXWU: an exclude path that cannot be read or written is an error the
// caller reports, and nothing else is changed.
#[test]
fn an_exclude_that_cannot_be_written_is_an_error() {
    let (_dir, root) = repository();
    let exclude = root.join(".git/info/exclude");
    let _ = std::fs::remove_file(&exclude);
    std::fs::create_dir_all(&exclude).unwrap();
    assert!(ensure_excluded(&root).is_err());
    assert!(exclude.is_dir());
}
