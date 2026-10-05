//! Creation of a note, and the semantics of a partial update.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- NTC-FR-01, NTC-FR-04 ----------------------------------------------------------

#[test]
fn create_writes_one_toml_file_and_the_note_lists_for_its_entity() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");

    let note = create(root, entity("specs/a.md"), "hello", "2026-01-01T00:00:00Z");

    let file = root.join(".synthesis/notes").join(format!("{}.toml", note.id));
    assert!(file.exists(), "one committed TOML file per note (NTC-FR-01)");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("hello"));
    assert_eq!(note.created_at, note.updated_at, "NTC-FR-04");

    let listed = list_notes_for_entity_in(root, root, "specs/a.md");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].note.id, note.id);
    assert_eq!(listed[0].note.body, "hello");
    assert!(!listed[0].unresolved);
    assert_eq!(listed[0].entity_name.as_deref(), Some("a.md"));
}

#[test]
fn a_created_note_carries_its_scope_path_and_optional_fields() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    let note = create_note_in(
        root,
        entity("specs/a.md"),
        "body".into(),
        Some("2026-03-04T09:00:00Z".into()),
        Some("7e3f1a2".into()),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        note.scope,
        NoteScope::Entity {
            entity_id: "specs/a.md".into(),
            entity_path: "specs/a.md".into()
        },
        "entity_path is recorded from the entity's current path (NTC-FR-04)"
    );
    assert_eq!(note.reminder.as_deref(), Some("2026-03-04T09:00:00Z"));
    assert_eq!(note.revision.as_deref(), Some("7e3f1a2"));

    // And it round-trips through disk unchanged.
    let reloaded = load_one(root, &note.id).unwrap();
    assert_eq!(reloaded, note);
}

#[test]
fn an_entity_scope_with_no_id_is_rejected() {
    let dir = temp_root();
    assert!(create_note_in(
        &crate::fs::RootFs::for_root(dir.path()),
        entity("  "),
        "b".into(),
        None,
        None,
        "2026-01-01T00:00:00Z"
    )
    .is_err());
}

// -- NTC-FR-06 / NTC-FR-05 (update semantics) ---------------------------

#[test]
fn editing_the_body_stamps_updated_at_and_never_rewrites_created_at() {
    // NTC-FR-06.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "first", "2026-01-01T00:00:00Z");

    let updated = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some("changed".into()),
            ..Default::default()
        },
        "2026-02-02T10:00:00Z",
    )
    .unwrap();

    assert_eq!(updated.body, "changed");
    assert_eq!(updated.updated_at, "2026-02-02T10:00:00Z");
    assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
    assert_eq!(load_one(root, &note.id).unwrap(), updated);
}

#[test]
fn a_partial_update_carries_every_absent_field_through_unchanged() {
    // NTC-FR-05.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create_note_in(
        root,
        NoteScope::Project,
        "body".into(),
        Some("2026-05-05T09:00:00Z".into()),
        Some("abc1234".into()),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    let after_body = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some("changed".into()),
            ..Default::default()
        },
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        after_body.reminder.as_deref(),
        Some("2026-05-05T09:00:00Z"),
        "an absent reminder field leaves the reminder alone"
    );
    assert_eq!(after_body.revision.as_deref(), Some("abc1234"));
    assert_eq!(after_body.scope, NoteScope::Project);

    let cleared = update_note_in(
        root,
        &note.id,
        NoteFields {
            reminder: Some(None),
            ..Default::default()
        },
        "2026-01-03T00:00:00Z",
    )
    .unwrap();
    assert!(cleared.reminder.is_none(), "an explicit null clears it");
    assert_eq!(cleared.body, "changed", "and the body is untouched");
}

#[test]
fn an_update_that_changes_nothing_leaves_updated_at_alone() {
    // NTC-FR-06: the stamp follows a change, not a submission — so
    // re-saving an unedited note does not reorder the panel (NTS-FR-15).
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, NoteScope::Project, "body", "2026-01-01T00:00:00Z");

    let untouched = update_note_in(
        root,
        &note.id,
        NoteFields::default(),
        "2026-09-09T00:00:00Z",
    )
    .unwrap();
    assert_eq!(untouched.updated_at, "2026-01-01T00:00:00Z");

    let resubmitted = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some("body".into()),
            scope: Some(NoteScope::Project),
            ..Default::default()
        },
        "2026-09-09T00:00:00Z",
    )
    .unwrap();
    assert_eq!(resubmitted.updated_at, "2026-01-01T00:00:00Z");
}

#[test]
fn a_note_emptied_of_its_body_survives_every_read() {
    // A note may legitimately hold no text: the panel lets a body be edited
    // away and keeps the note (NTS-FR-16), so an empty body has to round-trip
    // through the file like any other. It is the one string field written
    // unconditionally — were it skipped when empty, the file would fail to
    // parse and the note would silently vanish as malformed (NTC-FR-14).
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    touch(root, "specs/a.md");
    let note = create(root, entity("specs/a.md"), "had a body", "2026-01-01T00:00:00Z");

    let emptied = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some(String::new()),
            ..Default::default()
        },
        "2026-02-02T10:00:00Z",
    )
    .unwrap();

    assert_eq!(emptied.body, "");
    // Emptying is a change, so it is stamped and the note leads its list
    // (NTC-FR-06 / NTS-FR-15).
    assert_eq!(emptied.updated_at, "2026-02-02T10:00:00Z");
    assert!(
        std::fs::read_to_string(notes_dir(root).join(format!("{}.toml", note.id)))
            .unwrap()
            .contains("body = \"\""),
        "the empty body is written out, not omitted"
    );
    assert_eq!(load_one(root, &note.id).unwrap(), emptied);
    assert_eq!(
        list_notes_for_entity_in(root, root, "specs/a.md")
            .into_iter()
            .map(|i| i.note)
            .collect::<Vec<_>>(),
        vec![emptied.clone()],
        "and it is still listed for its entity"
    );
    assert_eq!(list_all_notes_in(root, root).len(), 1);

    // Re-saving the same empty body changes nothing, so the panel does not
    // reorder on a no-op submission.
    let again = update_note_in(
        root,
        &note.id,
        NoteFields {
            body: Some(String::new()),
            ..Default::default()
        },
        "2026-03-03T10:00:00Z",
    )
    .unwrap();
    assert_eq!(again.updated_at, "2026-02-02T10:00:00Z");
}

#[test]
fn the_wire_shape_of_a_partial_update_distinguishes_absent_from_null() {
    // The double-option is what makes "clear the reminder" expressible at
    // all; without it an absent field and an explicit null are the same
    // payload and one of NTC-FR-05's two cases becomes unreachable.
    let absent: NoteFields = serde_json::from_str(r#"{"body":"x"}"#).unwrap();
    assert!(absent.reminder.is_none());
    let null: NoteFields = serde_json::from_str(r#"{"reminder":null}"#).unwrap();
    assert_eq!(null.reminder, Some(None));
    let set: NoteFields = serde_json::from_str(r#"{"reminder":"2026-01-01T00:00:00Z"}"#).unwrap();
    assert_eq!(set.reminder, Some(Some("2026-01-01T00:00:00Z".into())));
}

#[test]
fn updating_or_deleting_an_unknown_id_is_a_typed_not_found() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert_eq!(
        delete_note_in(root, root, "0197ab-0000-00000000").unwrap_err(),
        ERR_NOTE_NOT_FOUND
    );
    assert_eq!(
        update_note_in(
            root,
            "0197ab-0000-00000000",
            NoteFields::default(),
            "2026-01-01T00:00:00Z"
        )
        .unwrap_err(),
        ERR_NOTE_NOT_FOUND
    );
}
