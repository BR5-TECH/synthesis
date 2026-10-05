//! Deleting a draft, and listing a project that holds none.

use super::*;

// -- deletion ----------------------------------------------------------

#[test]
fn deleting_a_draft_removes_it_and_writes_nothing_into_the_project() {
    // DRS-FR-21 / DRP-FR-12.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("README.md"), b"untouched").unwrap();
    let id = draft(root, "d");
    save_draft_file_impl(root, &id, FIRST_FILE, "# working").unwrap();

    delete_draft_impl(root, root, &id).unwrap();

    assert!(!root.join(DRAFTS_REL).join(&id).exists());
    assert_eq!(listed(root).len(), 0);
    assert_eq!(std::fs::read_to_string(root.join("README.md")).unwrap(), "untouched");
}

#[test]
fn a_directory_with_no_readable_record_is_skipped_rather_than_fatal() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let good = draft(root, "good");
    std::fs::create_dir_all(root.join(DRAFTS_REL).join("orphan-dir")).unwrap();

    let listed = listed(root);

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, good);
}

#[test]
fn listing_a_project_that_has_never_held_a_draft_is_empty_rather_than_an_error() {
    let dir = project();
    assert_eq!(listed(&fs::RootFs::for_root(dir.path())), Vec::new());
}
