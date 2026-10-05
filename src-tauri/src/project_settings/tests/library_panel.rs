//! The Library panel state (PSS-FR-18, PSS-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// PSS-FR-18 / PSS-FR-16 — the Library panel state
// -----------------------------------------------------------------------

#[test]
fn an_unconfigured_worktree_defaults_to_no_expansion_all_artifacts_and_no_text() {
    // PSS-FR-18 first half: the defaults LIB-FR-12 / LIB-FR-15 render.
    let dir = TempDir::new().unwrap();
    let state = load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.expanded_paths, Vec::<String>::new());
    assert_eq!(state.artifact_type_filter, ArtifactTypeFilter::AllArtifacts);
    assert_eq!(state.text_filter, "");
    assert_eq!(state, LibraryPanelState::default());
}

#[test]
fn the_library_state_round_trips_across_a_relaunch_in_local_toml() {
    // PSS-FR-18 second half: all three values survive, and they land in the
    // gitignored per-machine store rather than the committed one.
    let dir = TempDir::new().unwrap();
    let saved = library_state(
        &["specifications", ".claude/skills"],
        ArtifactTypeFilter::AllFiles,
        "readme",
    );
    save_library_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), saved.clone()).unwrap();

    // "Relaunch" is a fresh read of the same root — nothing held in memory.
    assert_eq!(load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())), saved);

    let local = dir.path().join(".synthesis/local.toml");
    assert!(local.is_file(), "the state lands in .synthesis/local.toml");
    let text = std::fs::read_to_string(&local).unwrap();
    assert!(text.contains("specifications"), "{text}");
    assert!(text.contains("readme"), "{text}");
    assert!(
        !dir.path().join(".synthesis/project.toml").exists(),
        "the committed project config must not be touched"
    );
    // PSS-FR-03: and the ignore rule that keeps it out of the repository.
    assert!(dir.path().join(".synthesis/.gitignore").is_file());
}

#[test]
fn expanded_paths_keep_their_order_and_are_not_deduplicated_or_validated() {
    // PSS-FR-18: the store persists what it is given. Ordering is the
    // caller's, and nothing here inspects the filesystem.
    let dir = TempDir::new().unwrap();
    let saved = library_state(
        &["z", "a", "nested/deep/path"],
        ArtifactTypeFilter::AllArtifacts,
        "",
    );
    save_library_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), saved.clone()).unwrap();
    assert_eq!(load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).expanded_paths, saved.expanded_paths);
}

#[test]
fn a_path_naming_a_folder_that_does_not_exist_is_returned_unchanged() {
    // PSS-FR-18, PSS-FR-16 second half / LIB-FR-16: an absent folder's path is retained
    // rather than pruned, so a checkout that restores the folder brings its
    // expansion back. Nothing on disk backs any of these paths.
    let dir = TempDir::new().unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["docs", "gone/for/now"], ArtifactTypeFilter::AllArtifacts, ""),
    )
    .unwrap();
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).expanded_paths,
        vec!["docs".to_string(), "gone/for/now".to_string()]
    );
}

#[test]
fn two_worktrees_keep_independent_library_state() {
    // PSS-FR-18, PSS-FR-16 first half: the store is resolved against the active
    // worktree's root (PSS-FR-16), so switching roots reads the other
    // worktree's record and copies nothing between them.
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(a.path()),
        library_state(&["specifications"], ArtifactTypeFilter::AllArtifacts, ""),
    )
    .unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(b.path()),
        library_state(&["src"], ArtifactTypeFilter::Skill, "hook"),
    )
    .unwrap();

    let from_b = load_library_panel_state_from(&crate::fs::RootFs::for_root(b.path()));
    assert_eq!(from_b.expanded_paths, vec!["src".to_string()]);
    assert_eq!(from_b.artifact_type_filter, ArtifactTypeFilter::Skill);
    assert_eq!(from_b.text_filter, "hook");
    // A's record is untouched by B's write.
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(a.path())).expanded_paths,
        vec!["specifications".to_string()]
    );
    assert!(
        !std::fs::read_to_string(b.path().join(".synthesis/local.toml"))
            .unwrap()
            .contains("specifications"),
        "nothing was copied from A into B"
    );
}

#[test]
fn a_whole_record_write_replaces_every_field() {
    // LIB-FR-17: `save_library_panel_state` writes the record whole, so a
    // value dropped from the payload is dropped from the store rather than
    // lingering from the previous write.
    let dir = TempDir::new().unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["a", "b"], ArtifactTypeFilter::AllFiles, "readme"),
    )
    .unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["a"], ArtifactTypeFilter::AllArtifacts, ""),
    )
    .unwrap();
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        library_state(&["a"], ArtifactTypeFilter::AllArtifacts, "")
    );
}

#[test]
fn saving_library_state_preserves_other_project_local_sections() {
    // `local.toml` is shared with the Changes panel and the recently-edited
    // MRU; a save rewrites the whole file, so their sections must survive.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    // The third occupant of `local.toml` (PSS-FR-12), which this module's
    // typed payloads do not declare and must therefore not drop.
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "[recentlyEdited]\nids = [\"a.md\", \"b.md\"]\n",
    )
    .unwrap();
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("develop")).unwrap();
    save_library_panel_state_to(
        &crate::fs::RootFs::for_root(dir.path()),
        library_state(&["specifications"], ArtifactTypeFilter::AllFiles, ""),
    )
    .unwrap();

    assert_eq!(
        load_changes_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        branch_state("develop"),
        "the Changes panel's section survives a Library write"
    );
    assert_eq!(
        load_local(&crate::fs::RootFs::for_root(dir.path()))
            .get("recentlyEdited")
            .and_then(|v| v.get("ids"))
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2),
        "and so does the recently-edited MRU"
    );
    // And the reverse: a Changes write leaves the Library section alone.
    save_changes_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), branch_state("main")).unwrap();
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())).expanded_paths,
        vec!["specifications".to_string()]
    );
}

#[test]
fn a_malformed_local_file_repairs_the_library_state_to_defaults() {
    // PSS-FR-10: project-local damage is silent.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(dir.path().join(".synthesis/local.toml"), "not = [valid").unwrap();
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        LibraryPanelState::default()
    );
}

#[test]
fn one_damaged_field_does_not_cost_the_others() {
    // A lens written by a newer build is unknown to this one. The expanded
    // set and the text filter beside it must still restore — losing the
    // user's open folders over an unreadable enum would be the worse bug.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "[library_panel]\nexpandedPaths = [\"specifications\"]\nartifactTypeFilter = \"all_diagrams\"\ntextFilter = \"spec\"\n",
    )
    .unwrap();

    let state = load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path()));
    assert_eq!(state.expanded_paths, vec!["specifications".to_string()]);
    assert_eq!(state.text_filter, "spec");
    assert_eq!(
        state.artifact_type_filter,
        ArtifactTypeFilter::AllArtifacts,
        "only the unreadable field falls back"
    );
}

#[test]
fn a_library_section_of_the_wrong_shape_falls_back_and_a_save_repairs_it() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        "library_panel = \"not-a-table\"\n",
    )
    .unwrap();
    assert_eq!(
        load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())),
        LibraryPanelState::default()
    );

    let saved = library_state(&["src"], ArtifactTypeFilter::Flow, "flow");
    save_library_panel_state_to(&crate::fs::RootFs::for_root(dir.path()), saved.clone()).unwrap();
    assert_eq!(load_library_panel_state_from(&crate::fs::RootFs::for_root(dir.path())), saved);
}

#[test]
fn library_state_serialises_camel_case_with_snake_case_filters() {
    // The wire shape the frontend types against. `all_artifacts` /
    // `all_files` are the sentinels of PSS-FR-18; the eight types serialise
    // as the ASC-FR-02 ids the tree already tags nodes with.
    let json = serde_json::to_value(library_state(
        &["specifications"],
        ArtifactTypeFilter::AllFiles,
        "readme",
    ))
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "expandedPaths": ["specifications"],
            "artifactTypeFilter": "all_files",
            "textFilter": "readme",
        })
    );
    assert_eq!(
        serde_json::to_value(ArtifactTypeFilter::AllArtifacts).unwrap(),
        serde_json::json!("all_artifacts")
    );
    assert_eq!(
        serde_json::to_value(ArtifactTypeFilter::Scratchpad).unwrap(),
        serde_json::json!("scratchpad")
    );
}

#[test]
fn library_state_deserialises_from_the_frontend_shape_and_from_a_partial_one() {
    let full: LibraryPanelState = serde_json::from_str(
        r#"{"expandedPaths":["a","b"],"artifactTypeFilter":"skill","textFilter":"x"}"#,
    )
    .unwrap();
    assert_eq!(full, library_state(&["a", "b"], ArtifactTypeFilter::Skill, "x"));
    // A partial payload defaults the rest, and an empty object is legal.
    let partial: LibraryPanelState =
        serde_json::from_str(r#"{"expandedPaths":["a"]}"#).unwrap();
    assert_eq!(partial, library_state(&["a"], ArtifactTypeFilter::AllArtifacts, ""));
    let empty: LibraryPanelState = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, LibraryPanelState::default());
}

#[test]
fn the_artifact_type_filter_rejects_an_unknown_value() {
    assert!(serde_json::from_str::<ArtifactTypeFilter>("\"all_diagrams\"").is_err());
}

/// The persisted vocabulary, spelled out. The frontend's own round-trip
/// test cannot catch a divergence from this side: a rename to
/// `"instruction"` here would leave `filterToLens` quietly falling back to
/// the default lens rather than failing. The same ten strings are asserted
/// in `src/artifactTypes.test.ts`'s
/// "pins the exact wire vocabulary shared with the Rust side", so a rename
/// on either side breaks a named assertion.
#[test]
fn the_wire_vocabulary_is_exactly_these_ten_strings() {
    use ArtifactTypeFilter::*;
    let all = [
        AllArtifacts,
        Skill,
        Agent,
        Prompt,
        Spec,
        Flow,
        Instructions,
        Scenario,
        Scratchpad,
        AllFiles,
    ];
    let wire: Vec<String> = all
        .iter()
        .map(|v| serde_json::to_value(v).unwrap().as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        wire,
        vec![
            "all_artifacts",
            "skill",
            "agent",
            "prompt",
            "spec",
            "flow",
            "instructions",
            "scenario",
            "scratchpad",
            "all_files",
        ]
    );
    // And every one of them deserialises back to the variant it came from.
    for (variant, text) in all.iter().zip(wire.iter()) {
        let parsed: ArtifactTypeFilter =
            serde_json::from_str(&format!("\"{text}\"")).unwrap();
        assert_eq!(&parsed, variant);
    }
}
