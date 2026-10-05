//! A note file that is malformed, and an entity identifier that is unusable.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-14 (malformed) ----------------------------------------------

#[test]
fn a_malformed_note_file_is_skipped_and_left_on_disk() {
    // NTC-FR-14.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create(root, NoteScope::Project, "one", "2026-01-01T00:00:00Z");
    create(root, NoteScope::Project, "two", "2026-01-02T00:00:00Z");
    let damaged = root.join(".synthesis/notes/broken.toml");
    std::fs::write(&damaged, b"this is not = valid = toml [[[").unwrap();
    // A file that parses but does not describe a note is skipped the same
    // way, rather than being served with an invented scope.
    let wrong_shape = root.join(".synthesis/notes/wrong.toml");
    std::fs::write(
        &wrong_shape,
        b"id = \"x\"\nscope = \"galaxy\"\nbody = \"b\"\ncreated_at = \"t\"\nupdated_at = \"t\"\n",
    )
    .unwrap();
    // And a non-TOML file in the folder is not a note at all.
    std::fs::write(root.join(".synthesis/notes/README.md"), b"# notes").unwrap();

    let items = list_all_notes_in(root, root);
    assert_eq!(items.len(), 2, "the well-formed notes are still served");
    assert!(damaged.exists(), "the damaged file is left untouched");
    assert!(wrong_shape.exists());
}

#[test]
fn a_note_file_whose_name_disagrees_with_its_id_is_skipped() {
    // NTC-FR-01 / NTC-FR-14. Serving such a file would render a row that
    // `update_note` and `delete_note` both answer "not found" for — a note
    // the user can see, cannot edit, and cannot remove.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "well-formed", "2026-01-01T00:00:00Z");
    let stored = std::fs::read_to_string(
        root.join(".synthesis/notes").join(format!("{}.toml", note.id)),
    )
    .unwrap();
    std::fs::write(root.join(".synthesis/notes/renamed.toml"), &stored).unwrap();

    let items = list_all_notes_in(root, root);
    assert_eq!(
        items.len(),
        1,
        "the mis-named copy is skipped rather than served twice: {items:?}"
    );
    assert!(root.join(".synthesis/notes/renamed.toml").exists());
}

#[test]
fn an_entity_id_that_names_a_directory_is_unresolved() {
    // NTC-FR-10: a note's entity is an artifact or a Harness. A directory
    // is no node of the scan, so it must not be served as a resolved one.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specs")).unwrap();
    create(root, entity("specs"), "on a folder", "2026-01-01T00:00:00Z");

    let items = list_all_notes_in(root, root);
    assert!(items[0].unresolved);
    assert!(items[0].entity_name.is_none());
}

#[test]
fn an_entity_id_that_escapes_the_root_is_never_resolved() {
    // NTC-FR-13: the escape gate applies to the read side too, so a scope
    // pointing outside the project can never be served as a live entity.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create(root, entity("../outside.md"), "escapee", "2026-01-01T00:00:00Z");
    assert!(list_all_notes_in(root, root)[0].unresolved);
}

#[test]
fn an_id_is_rejected_beyond_the_length_bound() {
    assert!(is_valid_id(&"a".repeat(128)));
    assert!(!is_valid_id(&"a".repeat(129)));
}

#[test]
fn an_unusable_id_is_rejected_as_such_rather_than_reported_missing() {
    // The two failures are different: "not found" invites a retry, an
    // invalid id never will be. Keeping them distinct is what lets the UI
    // (and a future caller) tell them apart.
    let dir = temp_root();
    let err = delete_note_in(
        &crate::fs::RootFs::for_root(dir.path()),
        &crate::fs::RootFs::for_root(dir.path()),
        "../victim",
    ).unwrap_err();
    assert!(err.contains("invalid note id"), "{err}");
    assert_ne!(err, ERR_NOTE_NOT_FOUND);
}
