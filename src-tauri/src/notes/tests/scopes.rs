//! Movement between scopes, the ordering of every list, and a note whose
//! entity is gone.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-02, NTC-FR-07 (moving between scopes) ----------------------

#[test]
fn moving_a_note_between_entities_keeps_its_id_and_filename() {
    // NTC-FR-02 / NTC-FR-07.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "a.md");
    touch(root, "b.md");
    let note = create(root, entity("a.md"), "note", "2026-01-01T00:00:00Z");
    let file = root.join(".synthesis/notes").join(format!("{}.toml", note.id));

    let moved = update_note_in(
        root,
        &note.id,
        NoteFields {
            scope: Some(entity("b.md")),
            ..Default::default()
        },
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    assert_eq!(moved.id, note.id, "the id is stable across a move");
    assert!(file.exists(), "and so is the filename");
    assert_eq!(
        moved.scope,
        NoteScope::Entity {
            entity_id: "b.md".into(),
            entity_path: "b.md".into()
        }
    );
    assert_eq!(list_notes_for_entity_in(root, root, "b.md").len(), 1);
    assert!(list_notes_for_entity_in(root, root, "a.md").is_empty());
}

#[test]
fn moving_a_note_to_project_scope_moves_it_between_the_two_lists() {
    // NTC-FR-07.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "a.md");
    let note = create(root, entity("a.md"), "note", "2026-01-01T00:00:00Z");

    update_note_in(
        root,
        &note.id,
        NoteFields {
            scope: Some(NoteScope::Project),
            ..Default::default()
        },
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    assert_eq!(list_project_notes_in(root, root).len(), 1);
    assert!(list_notes_for_entity_in(root, root, "a.md").is_empty());
}

// -- NTC-FR-09 / NTC-FR-10 (ordering + list membership) -----------------

#[test]
fn every_list_returns_most_recently_edited_first() {
    // NTC-FR-09.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "a.md");
    let x = create(root, entity("a.md"), "X", "2026-01-01T00:00:00Z");
    let y = create(root, NoteScope::Project, "Y", "2026-01-02T00:00:00Z");
    let z = create(root, NoteScope::Project, "Z", "2026-01-03T00:00:00Z");

    let all: Vec<String> = list_all_notes_in(root, root)
        .into_iter()
        .map(|i| i.note.id)
        .collect();
    assert_eq!(all, vec![z.id.clone(), y.id.clone(), x.id.clone()]);

    let project: Vec<String> = list_project_notes_in(root, root)
        .into_iter()
        .map(|i| i.note.id)
        .collect();
    assert_eq!(
        project,
        vec![z.id.clone(), y.id.clone()],
        "the project-scoped subset keeps that relative order"
    );

    // And an edit re-orders the list rather than leaving creation order.
    update_note_in(
        root,
        &x.id,
        NoteFields {
            body: Some("X edited".into()),
            ..Default::default()
        },
        "2026-01-04T00:00:00Z",
    )
    .unwrap();
    let reordered: Vec<String> = list_all_notes_in(root, root)
        .into_iter()
        .map(|i| i.note.id)
        .collect();
    assert_eq!(reordered, vec![x.id, z.id, y.id]);
}

#[test]
fn list_all_returns_both_kinds_each_carrying_its_resolution() {
    // NTC-FR-09 / NTC-FR-10.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    touch(root, "specs/b.md");
    create(root, entity("specs/a.md"), "one", "2026-01-01T00:00:00Z");
    create(root, entity("specs/b.md"), "two", "2026-01-02T00:00:00Z");
    create(root, NoteScope::Project, "three", "2026-01-03T00:00:00Z");

    let items = list_all_notes_in(root, root);
    assert_eq!(items.len(), 3);
    for item in &items {
        assert!(!item.unresolved);
        match item.note.scope.entity_id() {
            Some(id) => assert_eq!(item.entity_name.as_deref(), Some(basename(id).as_str())),
            None => assert!(item.entity_name.is_none()),
        }
    }
}

// -- NTC-FR-10, NTC-FR-12 / NTC-FR-07 (unresolved) ---------------------------------

#[test]
fn a_note_whose_entity_is_gone_is_marked_rather_than_dropped() {
    // NTC-FR-10 / NTC-FR-12.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specifications/a.md");
    let note = create(
        root,
        entity("specifications/a.md"),
        "orphan",
        "2026-01-01T00:00:00Z",
    );
    std::fs::remove_file(root.join("specifications/a.md")).unwrap();

    let items = list_all_notes_in(root, root);
    assert_eq!(items.len(), 1, "never dropped from a list result");
    assert!(items[0].unresolved);
    assert!(items[0].entity_name.is_none());
    assert_eq!(
        items[0].note.scope,
        NoteScope::Entity {
            entity_id: "specifications/a.md".into(),
            entity_path: "specifications/a.md".into()
        },
        "the last-known path is what the panel renders"
    );
    assert!(
        root.join(".synthesis/notes")
            .join(format!("{}.toml", note.id))
            .exists(),
        "and the note file itself is never deleted"
    );
    // It is still listed for its (now unresolvable) entity id, so the
    // entity position does not silently lose it either.
    assert_eq!(list_notes_for_entity_in(root, root, "specifications/a.md").len(), 1);
}

#[test]
fn reattaching_an_unresolved_note_resolves_it_again() {
    // NTC-FR-07 / NTC-FR-10.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "gone.md");
    touch(root, "c.md");
    let note = create(root, entity("gone.md"), "orphan", "2026-01-01T00:00:00Z");
    std::fs::remove_file(root.join("gone.md")).unwrap();
    assert!(list_all_notes_in(root, root)[0].unresolved);

    update_note_in(
        root,
        &note.id,
        NoteFields {
            scope: Some(entity("c.md")),
            ..Default::default()
        },
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    let items = list_all_notes_in(root, root);
    assert!(!items[0].unresolved);
    assert_eq!(items[0].entity_name.as_deref(), Some("c.md"));
}
