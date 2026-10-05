//! A note that follows the rename of the file or the folder it is on.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-11 (rename following) ---------------------------------------

#[test]
fn a_followed_rename_moves_the_note_without_touching_updated_at() {
    // NTC-FR-11.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specifications/a.md");
    let note = create(
        root,
        entity("specifications/a.md"),
        "keep me",
        "2026-01-01T00:00:00Z",
    );
    std::fs::rename(
        root.join("specifications/a.md"),
        root.join("specifications/b.md"),
    )
    .unwrap();

    let moved = follow_rename(root, "specifications/a.md", "specifications/b.md");
    assert_eq!(moved, 1);

    let items = list_notes_for_entity_in(root, root, "specifications/b.md");
    assert_eq!(items.len(), 1);
    assert!(!items[0].unresolved);
    assert_eq!(
        items[0].note.scope,
        NoteScope::Entity {
            entity_id: "specifications/b.md".into(),
            entity_path: "specifications/b.md".into()
        }
    );
    assert_eq!(
        items[0].note.updated_at, "2026-01-01T00:00:00Z",
        "the note's content did not change, so its edit time must not move"
    );
    assert_eq!(items[0].note.id, note.id);
    assert!(list_notes_for_entity_in(root, root, "specifications/a.md").is_empty());
}

#[test]
fn following_a_folder_rename_moves_the_notes_inside_it() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "old/a.md");
    touch(root, "old/deep/b.md");
    create(root, entity("old/a.md"), "one", "2026-01-01T00:00:00Z");
    create(root, entity("old/deep/b.md"), "two", "2026-01-01T00:00:00Z");
    create(root, entity("other.md"), "untouched", "2026-01-01T00:00:00Z");

    assert_eq!(follow_rename(root, "old", "new"), 2);

    assert_eq!(list_notes_for_entity_in(root, root, "new/a.md").len(), 1);
    assert_eq!(list_notes_for_entity_in(root, root, "new/deep/b.md").len(), 1);
    assert_eq!(
        list_notes_for_entity_in(root, root, "other.md").len(),
        1,
        "a note on an unrelated entity must not be rewritten"
    );
}

#[test]
fn following_a_rename_that_matches_nothing_changes_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create(root, entity("a.md"), "one", "2026-01-01T00:00:00Z");
    create(root, NoteScope::Project, "two", "2026-01-01T00:00:00Z");
    assert_eq!(follow_rename(root, "zzz.md", "yyy.md"), 0);
    assert_eq!(follow_rename(root, "a.md", "a.md"), 0);
    assert_eq!(follow_rename(root, "", "x.md"), 0);
    assert_eq!(list_notes_for_entity_in(root, root, "a.md").len(), 1);
    assert_eq!(list_project_notes_in(root, root).len(), 1);
}

#[test]
fn a_prefix_that_is_not_a_folder_boundary_is_not_followed() {
    // "old" must not match "older.md" — only "old" itself and "old/…".
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create(root, entity("older.md"), "one", "2026-01-01T00:00:00Z");
    assert_eq!(follow_rename(root, "old", "new"), 0);
    assert_eq!(list_notes_for_entity_in(root, root, "older.md").len(), 1);
}
