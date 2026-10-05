//! The serialised shapes: the wire record, the file on disk, and the commands.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- Wire shape ---------------------------------------------------------

#[test]
fn the_wire_shape_is_the_one_the_contract_surface_declares() {
    let note = Note {
        id: "n1".into(),
        scope: NoteScope::Entity {
            entity_id: "specs/a.md".into(),
            entity_path: "specs/a.md".into(),
        },
        body: "b".into(),
        reminder: None,
        revision: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-02T00:00:00Z".into(),
    };
    let json = serde_json::to_value(&note).unwrap();
    assert_eq!(json["scope"]["kind"], "entity");
    assert_eq!(json["scope"]["entityId"], "specs/a.md");
    assert_eq!(json["scope"]["entityPath"], "specs/a.md");
    assert_eq!(json["createdAt"], "2026-01-01T00:00:00Z");
    assert_eq!(json["updatedAt"], "2026-01-02T00:00:00Z");
    assert!(
        json.get("reminder").is_none(),
        "an absent reminder is absent on the wire, not null"
    );

    let project = serde_json::to_value(NoteScope::Project).unwrap();
    assert_eq!(project["kind"], "project");
    // And the scope round-trips from the shape the frontend sends.
    let parsed: NoteScope =
        serde_json::from_str(r#"{"kind":"entity","entityId":"a.md"}"#).unwrap();
    assert_eq!(parsed.entity_id(), Some("a.md"));
}

#[test]
fn a_note_file_is_flat_diffable_toml() {
    // The on-disk shape is deliberately flat (TOML orders scalars before
    // tables); this pins that a note file stays one key per line.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let note = create(root, entity("specs/a.md"), "hello", "2026-01-01T00:00:00Z");
    let text =
        std::fs::read_to_string(root.join(".synthesis/notes").join(format!("{}.toml", note.id)))
            .unwrap();
    assert!(!text.contains('['), "no sub-tables: {text}");
    assert!(text.contains("scope = \"entity\""));
    assert!(text.contains("entity_id = \"specs/a.md\""));
    assert!(text.contains("created_at = \"2026-01-01T00:00:00Z\""));
}

#[test]
fn commands_are_in_scope() {
    // A rename of any command fn fails to compile here, mirroring the
    // `*_are_in_scope` guard the other domain modules keep.
    let _ = list_notes_for_entity;
    let _ = list_project_notes;
    let _ = list_all_notes;
    let _ = create_note::<tauri::Wry>;
    let _ = update_note::<tauri::Wry>;
    let _ = delete_note::<tauri::Wry>;
}
