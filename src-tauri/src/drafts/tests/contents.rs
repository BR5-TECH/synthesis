//! Reading and writing the contents of a draft's one prompt file.

use super::*;

// -- contents ----------------------------------------------------------

#[test]
fn a_path_that_is_not_the_prompt_is_refused_by_both_content_operations() {
    // DRS-FR-12 (DRS-FR-12, DRS-FR-13): `path` names the prompt and any
    // other path is a typed "not found". Without the guard on the write the
    // atomic primitive would *create* whatever path it was given, which is
    // the one way a caller could put a draft into the state DRS-FR-11
    // forbids.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");

    assert!(load_draft_file_impl(root, &id, "never-written.md").is_err());
    assert!(save_draft_file_impl(root, &id, "never-written.md", "x").is_err());
    assert_eq!(files_in(root, &id), vec![FIRST_FILE], "nothing was created");
}

#[test]
fn saving_returns_the_checksum_of_the_bytes_actually_written() {
    // PST-FR-15's contract, applied to a draft file: the checksum is over the
    // normalised bytes, so it is a valid baseline for the editing surface.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");

    let saved = save_draft_file_impl(root, &id, FIRST_FILE, "one\ntwo\n").unwrap();
    let loaded = load_draft_file_impl(root, &id, FIRST_FILE).unwrap();

    assert_eq!(saved.checksum, loaded.checksum);
    assert_eq!(loaded.body, "one\ntwo\n");
}

#[test]
fn a_save_converts_to_the_projects_line_ending_convention() {
    // DRS-FR-12 (DRS-FR-12, per PSS-FR-17): the same convention every
    // artifact write obeys, so a prompt's line endings do not change the
    // moment it graduates — and the checksum is over the bytes that landed
    // rather than over the body submitted.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: project_settings::LineEndings::Crlf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();
    let id = draft(root, "d");

    let saved = save_draft_file_impl(root, &id, FIRST_FILE, "one\ntwo\n").unwrap();

    let on_disk = std::fs::read(
        draft_dir(root, &id).unwrap().join(FILES_DIR).join(FIRST_FILE),
    )
    .unwrap();
    assert_eq!(on_disk, b"one\r\ntwo\r\n");
    assert_eq!(saved.checksum, fs::sha256_bytes(&on_disk));
    assert_ne!(saved.checksum, fs::sha256_bytes(b"one\ntwo\n"));
}
