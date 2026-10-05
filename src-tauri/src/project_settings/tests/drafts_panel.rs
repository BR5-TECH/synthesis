//! The Drafts panel state (PSS-FR-20, PSS-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Drafts panel state (PSS-FR-20, PSS-FR-16)
// -----------------------------------------------------------------------

#[test]
fn ts23_drafts_panel_state_defaults_round_trips_and_is_never_validated() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());

    // Nothing persisted yet: the `active` position, empty filter, nothing
    // expanded (PSS-FR-20).
    assert_eq!(load_drafts_panel_state_from(&root), DraftsPanelState::default());
    assert_eq!(
        load_drafts_panel_state_from(&root).status_filter,
        DraftsStatusFilter::Active
    );

    let saved = drafts_state(
        DraftsStatusFilter::Archived,
        "window",
        &["UI", "UI/Components"],
    );
    save_drafts_panel_state_to(&root, saved.clone()).unwrap();
    assert_eq!(load_drafts_panel_state_from(&root), saved);

    // Project-local, not project-public.
    assert!(dir.path().join(".synthesis/local.toml").is_file());
    assert!(!dir.path().join(".synthesis/project.toml").exists());

    // A path naming a folder since deleted is returned unchanged rather than
    // pruned: this store performs no validation.
    assert!(load_drafts_panel_state_from(&root)
        .expanded_folders
        .contains(&"UI/Components".to_string()));
}

#[test]
fn ts23_a_drafts_write_leaves_every_other_section_alone() {
    // PSS-FR-20 writes the record whole and `local.toml` in place.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());

    save_library_panel_state_to(
        &root,
        library_state(&["specifications"], ArtifactTypeFilter::AllFiles, "lib"),
    )
    .unwrap();
    save_drafts_panel_state_to(&root, drafts_state(DraftsStatusFilter::All, "d", &["UI"]))
        .unwrap();

    assert_eq!(
        load_library_panel_state_from(&root).expanded_paths,
        vec!["specifications".to_string()]
    );
    assert_eq!(
        load_drafts_panel_state_from(&root).expanded_folders,
        vec!["UI".to_string()]
    );
}

#[test]
fn ts24_two_worktrees_keep_independent_drafts_panel_state() {
    // PSS-FR-20 / PSS-FR-16: nothing is carried over from the outgoing
    // worktree's store and nothing is copied into the incoming one.
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    std::fs::create_dir_all(a.path().join(".synthesis")).unwrap();
    std::fs::create_dir_all(b.path().join(".synthesis")).unwrap();

    save_drafts_panel_state_to(
        &crate::fs::RootFs::for_root(a.path()),
        drafts_state(DraftsStatusFilter::All, "spec", &["UI"]),
    )
    .unwrap();

    let from_b = load_drafts_panel_state_from(&crate::fs::RootFs::for_root(b.path()));
    assert_eq!(from_b, DraftsPanelState::default());
    assert!(!b.path().join(".synthesis/local.toml").exists());
    assert_eq!(
        load_drafts_panel_state_from(&crate::fs::RootFs::for_root(a.path())).expanded_folders,
        vec!["UI".to_string()],
        "and A's own record is untouched"
    );
}

#[test]
fn one_damaged_drafts_field_does_not_cost_the_other() {
    // PSS-FR-10 / PSS-FR-20: a position written by a newer build must not
    // throw away the author's expanded folders.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "[drafts_panel]\nstatusFilter = \"constellation\"\ntextFilter = \"spec\"\nexpandedFolders = [\"UI\"]\n",
    )
    .unwrap();

    let state = load_drafts_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.text_filter, "spec");
    assert_eq!(state.expanded_folders, vec!["UI".to_string()]);
    assert_eq!(
        state.status_filter,
        DraftsStatusFilter::Active,
        "only the unreadable field falls back"
    );
}

#[test]
fn drafts_state_serialises_the_wire_shape_the_frontend_types_against() {
    let json =
        serde_json::to_value(drafts_state(DraftsStatusFilter::All, "spec", &["UI"])).unwrap();
    assert_eq!(json["statusFilter"], "all");
    assert_eq!(json["textFilter"], "spec");
    assert_eq!(json["expandedFolders"][0], "UI");
    for (variant, text) in [
        (DraftsStatusFilter::Active, "active"),
        (DraftsStatusFilter::Archived, "archived"),
        (DraftsStatusFilter::All, "all"),
    ] {
        assert_eq!(serde_json::to_value(variant).unwrap(), text);
        let parsed: DraftsStatusFilter =
            serde_json::from_str(&format!("\"{text}\"")).unwrap();
        assert_eq!(parsed, variant);
    }
}
