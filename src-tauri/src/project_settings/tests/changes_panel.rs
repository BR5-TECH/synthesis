//! The Changes panel state (PSS-FR-15) and its wire shape.
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// PSS-FR-15
// -----------------------------------------------------------------------

#[test]
fn an_unconfigured_project_defaults_to_uncommitted_with_no_target() {
    let dir = TempDir::new().unwrap();
    let state = load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.mode, ChangesMode::Uncommitted);
    assert_eq!(state.target_branch, None);
}

#[test]
fn the_panel_state_round_trips_across_a_relaunch() {
    // "Relaunch" is a fresh read of the same project root — nothing is held
    // in memory between the write and the read.
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    let restored = load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(restored, branch_state("develop"));
}

#[test]
fn the_panel_state_is_written_to_local_toml_and_not_project_toml() {
    // PSS-FR-15: project-local scope, so each machine keeps its
    // own target branch rather than committing it to the repository.
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();

    let local = dir.path().join(".synthesis/local.toml");
    assert!(local.is_file(), "the state lands in .synthesis/local.toml");
    let text = std::fs::read_to_string(&local).unwrap();
    assert!(text.contains("develop"), "{text}");
    assert!(
        !dir.path().join(".synthesis/project.toml").exists(),
        "the committed project config must not be touched"
    );
}

#[test]
fn saving_ensures_the_synthesis_dir_is_gitignored() {
    // PSS-FR-03: `local.toml` is per-machine and must not reach the repo.
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    let ignore = dir.path().join(".synthesis/.gitignore");
    assert!(ignore.is_file(), "an ignore file is written alongside it");
}

#[test]
fn a_later_save_replaces_the_previous_state() {
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    save_changes_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        ChangesPanelState {
            mode: ChangesMode::Uncommitted,
            target_branch: Some("main".into()),
        },
    )
    .unwrap();
    let restored = load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(restored.mode, ChangesMode::Uncommitted);
    assert_eq!(restored.target_branch.as_deref(), Some("main"));
}

#[test]
fn a_malformed_local_file_is_repaired_to_defaults() {
    // PSS-FR-10: project-local damage is silent; only project-public is a
    // typed error.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(dir.path().join(".synthesis/local.toml"), "not = [valid").unwrap();
    let state = load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state, ChangesPanelState::default());
}

#[test]
fn a_stale_branch_is_returned_verbatim_without_validation() {
    // PSS-FR-15: this store never checks that the branch still exists; the
    // UI surfaces it per CHG-FR-27.
    let dir = TempDir::new().unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("feature/old")).unwrap();
    assert_eq!(
        load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).target_branch.as_deref(),
        Some("feature/old")
    );
}

#[test]
fn saving_preserves_other_project_local_sections() {
    // `local.toml` is shared with every other project-local consumer
    // (layout preferences per PSS-FR-06, the recently-edited MRU per
    // PSS-FR-12), and a save rewrites the whole file — so a section this
    // module does not know about must survive the round-trip untouched.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "someScalar = 7\n\n[layout]\nverticalPanelWidth = 280\n\n[recentlyEdited]\nids = [\"a.md\"]\n",
    )
    .unwrap();

    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();

    let config = load_local(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(
        config.get("someScalar").and_then(|v| v.as_integer()),
        Some(7),
        "a top-level scalar another writer owns must survive"
    );
    assert_eq!(
        config
            .get("layout")
            .and_then(|v| v.get("verticalPanelWidth"))
            .and_then(|v| v.as_integer()),
        Some(280),
        "another consumer's table must survive"
    );
    assert!(
        config.get("recentlyEdited").is_some(),
        "and so must every other one"
    );
    // And this module's own section landed.
    assert_eq!(
        load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        branch_state("develop")
    );
}

#[test]
fn a_local_file_whose_changes_panel_section_is_the_wrong_shape_falls_back() {
    // PSS-FR-10: project-local damage is repaired, not surfaced. A section
    // written by an older or newer build must not break the panel.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "changes_panel = \"not-a-table\"\n",
    )
    .unwrap();
    assert_eq!(
        load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        ChangesPanelState::default()
    );
    // And a save over it repairs the section rather than failing.
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    assert_eq!(
        load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        branch_state("develop")
    );
}

// -----------------------------------------------------------------------
// Wire shape
// -----------------------------------------------------------------------

#[test]
fn state_serialises_camel_case_with_lowercase_modes() {
    let json = serde_json::to_value(branch_state("develop")).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "mode": "branch", "targetBranch": "develop" })
    );
    let default = serde_json::to_value(ChangesPanelState::default()).unwrap();
    assert_eq!(
        default,
        serde_json::json!({ "mode": "uncommitted" }),
        "an absent target branch is omitted, not null"
    );
}

#[test]
fn state_deserialises_from_the_frontend_shape_and_from_a_partial_one() {
    let full: ChangesPanelState =
        serde_json::from_str(r#"{"mode":"branch","targetBranch":"develop"}"#).unwrap();
    assert_eq!(full, branch_state("develop"));
    // A payload with no target branch is legal (Uncommitted mode).
    let partial: ChangesPanelState = serde_json::from_str(r#"{"mode":"uncommitted"}"#).unwrap();
    assert_eq!(partial, ChangesPanelState::default());
    // So is an empty object.
    let empty: ChangesPanelState = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, ChangesPanelState::default());
}

#[test]
fn mode_rejects_an_unknown_value() {
    assert!(serde_json::from_str::<ChangesMode>("\"everything\"").is_err());
}
