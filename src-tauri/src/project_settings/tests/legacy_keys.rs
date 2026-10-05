//! The legacy recently-edited key (PSS-FR-14).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// PSS-FR-14 — the legacy recently-edited key (PSS-FR-14)
// -----------------------------------------------------------------------

/// PSS-FR-14: the key is ignored on every read — no `load_*` command applies
/// it to anything it returns, so persisted data from the earlier layout
/// influences no command and no widget.
#[test]
fn a_legacy_recently_edited_key_reaches_no_returned_value() {
    let dir = TempDir::new().unwrap();
    let root = local_with_legacy_key(&dir);

    let rendered = format!(
        "{:?}{:?}{:?}{:?}",
        load_changes_panel_state_from(&root),
        load_library_panel_state_from(&root),
        load_notes_panel_state_from(&root),
        load_drafts_panel_state_from(&root),
    );
    for id in ["a.md", "b.md", "c.md"] {
        assert!(
            !rendered.contains(id),
            "no load command may carry a legacy recently-edited id: {rendered}"
        );
    }
    // And the section that IS supported still reads back, so the assertion
    // above is about the key rather than about an unreadable file.
    assert_eq!(load_notes_panel_state_from(&root).text_filter, "handoff");
}

/// PSS-FR-14: the key is dropped by the ordinary write path — no migration
/// command exists and none is needed — while every other persisted value
/// survives the write.
#[test]
fn the_next_write_drops_the_legacy_key_and_keeps_everything_else() {
    let dir = TempDir::new().unwrap();
    let root = local_with_legacy_key(&dir);
    assert!(
        std::fs::read_to_string(local_toml_path(&root))
            .unwrap()
            .contains("recently_edited"),
        "precondition: the key is on disk before the write"
    );

    save_changes_panel_state_to(&root, branch_state("develop")).unwrap();

    let after = std::fs::read_to_string(local_toml_path(&root)).unwrap();
    assert!(
        !after.contains("recently_edited"),
        "the key does not survive the next write: {after}"
    );
    assert_eq!(
        load_notes_panel_state_from(&root).text_filter,
        "handoff",
        "and every other persisted value came through the write unchanged"
    );
    assert_eq!(
        load_changes_panel_state_from(&root).target_branch.as_deref(),
        Some("develop"),
    );
}

/// PSS-FR-14, second clause: a project whose local store is never written
/// again keeps the key, and it still changes nothing.
#[test]
fn a_project_never_written_again_keeps_a_key_that_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let root = local_with_legacy_key(&dir);

    // A reopen is a fresh read of the same root; nothing here writes.
    let reread = crate::fs::RootFs::for_root(dir.path());
    assert_eq!(load_notes_panel_state_from(&reread).text_filter, "handoff");
    assert!(
        std::fs::read_to_string(local_toml_path(&root))
            .unwrap()
            .contains("recently_edited"),
        "no migration ran, because reading is not writing"
    );
}

#[test]
fn command_functions_are_in_scope() {
    let _load = load_changes_panel_state;
    let _save = save_changes_panel_state;
    let _load_public = load_project_config;
    let _save_public = save_project_config::<tauri::test::MockRuntime>;
    let _load_library = load_library_panel_state;
    let _save_library = save_library_panel_state;
    let _load_notes = load_notes_panel_state;
    let _save_notes = save_notes_panel_state;
    let _load_drafts = load_drafts_panel_state;
    let _save_drafts = save_drafts_panel_state;
}
