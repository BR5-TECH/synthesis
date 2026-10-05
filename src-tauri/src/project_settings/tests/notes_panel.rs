//! The Notes panel state (PSS-FR-19, PSS-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// PSS-FR-19 / PSS-FR-16 — the Notes panel state
// -----------------------------------------------------------------------

#[test]
fn an_unconfigured_worktree_defaults_to_the_entity_position_and_no_filter() {
    // PSS-FR-19 first half.
    let dir = TempDir::new().unwrap();
    let state = load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.scope_position, NotesScopePosition::Entity);
    assert_eq!(state.text_filter, "");
    assert_eq!(state, NotesPanelState::default());
}

#[test]
fn the_notes_state_round_trips_across_a_relaunch_in_local_toml() {
    // PSS-FR-19 second half: both values survive a relaunch, and they land
    // in the gitignored per-machine store rather than the committed one —
    // the notes themselves are committed (NTC-FR-01), the *view* of them is
    // not.
    let dir = TempDir::new().unwrap();
    let saved = notes_state(NotesScopePosition::All, "handoff");
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), saved.clone()).unwrap();

    assert_eq!(load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())), saved);

    let local = dir.path().join(".synthesis/local.toml");
    assert!(local.is_file());
    let text = std::fs::read_to_string(&local).unwrap();
    assert!(text.contains("handoff"), "{text}");
    assert!(
        !dir.path().join(".synthesis/project.toml").exists(),
        "the committed project config must not be touched"
    );
    assert!(dir.path().join(".synthesis/.gitignore").is_file());
}

#[test]
fn two_worktrees_keep_independent_notes_panel_state() {
    // PSS-FR-19 first half / PSS-FR-16.
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(a.path()), notes_state(NotesScopePosition::All, "spec")).unwrap();

    let from_b = load_notes_panel_state_from(&crate::fs::RootFs::for_root(b.path()));
    assert_eq!(from_b, NotesPanelState::default(), "B's own default");
    assert!(
        !b.path().join(".synthesis/local.toml").exists(),
        "nothing was copied from A into B"
    );
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(a.path())).scope_position,
        NotesScopePosition::All,
        "and A's record is untouched"
    );
}

#[test]
fn the_entity_position_is_returned_unchanged_and_never_rewritten() {
    // PSS-FR-16 second half / PSS-FR-19: this store performs no validation.
    // Whether the entity position is renderable at all is NTS-FR-09's
    // decision, and it must be able to make it without the store having
    // silently downgraded the value first.
    let dir = TempDir::new().unwrap();
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), notes_state(NotesScopePosition::Entity, "")).unwrap();
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).scope_position,
        NotesScopePosition::Entity
    );
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).scope_position,
        NotesScopePosition::Entity,
        "a second read is still the persisted value"
    );
}

#[test]
fn a_notes_whole_record_write_replaces_every_field() {
    // PSS-FR-19: the record is written whole, so a caller changing one
    // field supplies the other unchanged.
    let dir = TempDir::new().unwrap();
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), notes_state(NotesScopePosition::All, "spec"))
        .unwrap();
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), notes_state(NotesScopePosition::Project, ""))
        .unwrap();
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        notes_state(NotesScopePosition::Project, "")
    );
}

#[test]
fn saving_notes_state_preserves_every_other_project_local_section() {
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["specifications"], ArtifactTypeFilter::AllFiles, ""),
    )
    .unwrap();
    save_notes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), notes_state(NotesScopePosition::All, "x")).unwrap();

    assert_eq!(load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())), branch_state("develop"));
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).expanded_paths,
        vec!["specifications".to_string()]
    );
    // And the reverse: a Library write leaves the Notes section alone.
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["src"], ArtifactTypeFilter::AllArtifacts, ""),
    )
    .unwrap();
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        notes_state(NotesScopePosition::All, "x")
    );
}

#[test]
fn one_damaged_notes_field_does_not_cost_the_other() {
    // PSS-FR-10 / PSS-FR-19: a position written by a newer build must not
    // throw away the filter text beside it.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "[notes_panel]\nscopePosition = \"constellation\"\ntextFilter = \"spec\"\n",
    )
    .unwrap();

    let state = load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.text_filter, "spec");
    assert_eq!(
        state.scope_position,
        NotesScopePosition::Entity,
        "only the unreadable field falls back"
    );
}

#[test]
fn a_malformed_local_file_repairs_the_notes_state_to_defaults() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(dir.path().join(".synthesis/local.toml"), "not = [valid").unwrap();
    assert_eq!(
        load_notes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        NotesPanelState::default()
    );
}

#[test]
fn notes_state_serialises_the_wire_shape_the_frontend_types_against() {
    let json = serde_json::to_value(notes_state(NotesScopePosition::All, "spec")).unwrap();
    assert_eq!(json["scopePosition"], "all");
    assert_eq!(json["textFilter"], "spec");
    for (variant, text) in [
        (NotesScopePosition::Entity, "entity"),
        (NotesScopePosition::Project, "project"),
        (NotesScopePosition::All, "all"),
    ] {
        assert_eq!(serde_json::to_value(variant).unwrap(), text);
        let parsed: NotesScopePosition =
            serde_json::from_str(&format!("\"{text}\"")).unwrap();
        assert_eq!(parsed, variant);
    }
}
