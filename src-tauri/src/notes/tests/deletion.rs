//! Deletion of a note, and the folder it must stay inside.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-08, NTC-FR-13, NTC-FR-21 (delete) -------------------------------------------------

#[test]
fn deleting_a_note_removes_its_file_and_a_second_delete_is_not_found() {
    // NTC-FR-21 / NTC-FR-08 / NTC-FR-13.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "bye", "2026-01-01T00:00:00Z");
    let file = root.join(".synthesis/notes").join(format!("{}.toml", note.id));
    assert!(file.exists());

    delete_note_in(root, root, &note.id).unwrap();
    assert!(!file.exists());
    assert!(list_all_notes_in(root, root).is_empty());
    assert_eq!(delete_note_in(root, root, &note.id).unwrap_err(), ERR_NOTE_NOT_FOUND);
}

#[test]
fn nothing_is_written_or_removed_outside_the_notes_folder() {
    // NTC-FR-13: the escape gate, exercised through the two mutators that
    // take an id from the frontend.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("victim.md"), b"precious").unwrap();
    assert!(delete_note_in(root, root, "../victim").is_err());
    assert!(delete_note_in(root, root, "../../etc/passwd").is_err());
    assert!(update_note_in(
        root,
        "../victim",
        NoteFields {
            body: Some("clobbered".into()),
            ..Default::default()
        },
        "2026-01-01T00:00:00Z"
    )
    .is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("victim.md")).unwrap(),
        "precious"
    );
}
