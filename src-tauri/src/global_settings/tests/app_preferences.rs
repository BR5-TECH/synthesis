//! The app preferences record: the theme and the main-window full-screen flag.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Store: app preferences (GSS-FR-04) ----------------------------

#[test]
fn store_app_preferences_default_is_system() {
    let store = GlobalSettingsStore::in_memory();
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::System);
}

#[test]
fn store_app_preferences_save_then_load_roundtrip() {
    let store = GlobalSettingsStore::in_memory();
    store
        .save_app_preferences(AppPreferences { theme: Theme::Dark, ..Default::default() })
        .unwrap();
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::Dark);
}

// ---- Main-window full-screen flag (GSS-FR-19 / GSS-FR-20) ------------

#[test]
fn app_preferences_default_to_system_theme_and_not_fullscreen() {
    // GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-32, GSS-FR-33, GSS-FR-34: an unwritten record reads back as both defaults.
    let p = AppPreferences::default();
    assert_eq!(p.theme, Theme::System);
    assert!(
        !p.main_window_fullscreen,
        "GSS-FR-19: the flag defaults to false when unset"
    );
    assert_eq!(
        p.search_query_mode,
        SearchQueryMode::LiteralInsensitive,
        "GSS-FR-21: the query mode defaults to case-insensitive literal"
    );
    let store = GlobalSettingsStore::in_memory();
    let loaded = store.load_app_preferences().unwrap();
    assert_eq!(loaded.theme, Theme::System);
    assert!(!loaded.main_window_fullscreen);
    assert_eq!(loaded.search_query_mode, SearchQueryMode::LiteralInsensitive);
}

#[test]
fn app_preferences_wire_shape_carries_the_fullscreen_flag() {
    // The frontend speaks camelCase — pin the wire key, since a typo here
    // is a silent no-op rather than a compile error.
    let json = serde_json::to_value(AppPreferences {
        theme: Theme::Dark,
        main_window_fullscreen: true,
        search_query_mode: SearchQueryMode::SmartCase,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(json.get("theme").unwrap(), "dark");
    assert_eq!(
        json.get("mainWindowFullscreen").and_then(|v| v.as_bool()),
        Some(true)
    );

    let decoded: AppPreferences =
        serde_json::from_str(r#"{"theme":"light","mainWindowFullscreen":true}"#).unwrap();
    assert_eq!(decoded.theme, Theme::Light);
    assert!(decoded.main_window_fullscreen);
}

#[test]
fn a_record_written_before_the_fullscreen_flag_existed_still_loads() {
    // GSS-FR-13: an existing synthesis.toml carries only `theme`. It must
    // load with the new field defaulted rather than erroring — otherwise
    // every existing install boots to a repaired (empty) store.
    let decoded: AppPreferences = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert_eq!(decoded.theme, Theme::Dark);
    assert!(!decoded.main_window_fullscreen);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(&path, "[appPreferences]\ntheme = \"dark\"\n").unwrap();
    let store = GlobalSettingsStore::with_path(path);
    let loaded = store.load_app_preferences().unwrap();
    assert_eq!(loaded.theme, Theme::Dark);
    assert!(!loaded.main_window_fullscreen);
}

#[test]
fn fullscreen_flag_survives_a_relaunch_and_is_not_per_project() {
    // GSS-FR-19: the flag round-trips through disk, and because it lives on
    // the app-preferences record rather than in a per-project slot, it reads
    // back the same whichever project is open.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_app_preferences(AppPreferences {
                theme: Theme::Dark,
                main_window_fullscreen: true,
                search_query_mode: SearchQueryMode::LiteralInsensitive,
                ..Default::default()
            })
            .unwrap();
        // Two projects with their own layout slots — neither carries the flag.
        store
            .save_project_layout(
                "/dev/acme",
                crate::layout::LayoutPreferences {
                    vertical_panel_fraction: 0.2,
                    ..Default::default()
                },
            )
            .unwrap();
        store
            .save_project_layout(
                "/dev/other",
                crate::layout::LayoutPreferences {
                    vertical_panel_fraction: 0.4,
                    ..Default::default()
                },
            )
            .unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path);
    let prefs = reloaded.load_app_preferences().unwrap();
    assert_eq!(prefs.theme, Theme::Dark);
    assert!(prefs.main_window_fullscreen, "the flag must survive a relaunch");
    // The per-project slots differ in layout but say nothing about
    // full-screen — one user-global answer serves both projects.
    assert_eq!(
        reloaded
            .load_project_layout("/dev/acme")
            .unwrap()
            .unwrap()
            .vertical_panel_fraction,
        0.2,
    );
    assert_eq!(
        reloaded
            .load_project_layout("/dev/other")
            .unwrap()
            .unwrap()
            .vertical_panel_fraction,
        0.4,
    );
}

// GSS-FR-20, GSS-FR-34, GSS-FR-QDNV: every editor of the record leaves the
// others' fields standing.
#[test]
fn a_whole_record_write_of_one_field_carries_the_other_through() {
    // GSS-FR-20: the Appearance section saves a theme change
    // while full-screen is set, then the shell saves a full-screen change.
    // Neither write may drop the other's field.
    let store = GlobalSettingsStore::in_memory();
    store
        .save_app_preferences(AppPreferences {
            theme: Theme::System,
            main_window_fullscreen: true,
            search_query_mode: SearchQueryMode::Regex,
            diff_visualization_mode: DiffVisualizationMode::SideBySide,
            diff_rendering_mode: DiffRenderingMode::Rich,
            changes_commit_action: ChangesCommitAction::CommitAndPush,
            // Set to the NON-default so a write that silently dropped it
            // would read back as `true` and pass a weaker assertion.
            notifications_enabled: false,
            // Non-default for the same reason (GSS-FR-33).
            selection_follows_tab: false,
            // Non-default for the same reason (GSS-FR-34): a write that
            // dropped it would read back at 0.20 and pass a weaker
            // assertion than the one below.
            graduation_rail_width_fraction: 0.28,
            // Non-default for the same reason (GSS-FR-QDNV).
            graduation_paths_width_fraction: 0.44,
            // Non-default for the same reason (GSS-FR-MSPQ).
            git_files_width_fraction: 0.37,
            // Listed exhaustively rather than defaulted: this test is the
            // canary for GSS-FR-20, so a field added to the record without
            // a carry-through assertion below must fail to compile here.
            fonts: FontSettings {
                rich: FontRole {
                    family: Some("Georgia".into()),
                    size_px: Some(17.0),
                    line_height: Some(1.8),
                },
                ..Default::default()
            },
        })
        .unwrap();

    // Appearance edits `theme`, supplying the loaded `main_window_fullscreen`.
    let mut editing = store.load_app_preferences().unwrap();
    editing.theme = Theme::Dark;
    store.save_app_preferences(editing).unwrap();
    let after_theme = store.load_app_preferences().unwrap();
    assert_eq!(after_theme.theme, Theme::Dark);
    assert!(
        after_theme.main_window_fullscreen,
        "a theme write must not clear the full-screen flag"
    );

    // The shell leaves full-screen, supplying the loaded `theme`.
    let mut editing = store.load_app_preferences().unwrap();
    editing.main_window_fullscreen = false;
    store.save_app_preferences(editing).unwrap();
    let after_fullscreen = store.load_app_preferences().unwrap();
    assert!(!after_fullscreen.main_window_fullscreen);
    assert_eq!(
        after_fullscreen.theme,
        Theme::Dark,
        "a full-screen write must not reset the theme"
    );
    // GSS-FR-20 / GSS-FR-21: neither write may disturb the query mode.
    assert_eq!(
        after_fullscreen.search_query_mode,
        SearchQueryMode::Regex,
        "a theme or full-screen write must not reset the query mode"
    );

    // And the search input's own write leaves the other two alone.
    let mut editing = store.load_app_preferences().unwrap();
    editing.search_query_mode = SearchQueryMode::SmartCase;
    store.save_app_preferences(editing).unwrap();
    let after_mode = store.load_app_preferences().unwrap();
    assert_eq!(after_mode.search_query_mode, SearchQueryMode::SmartCase);
    assert_eq!(after_mode.theme, Theme::Dark);
    assert!(!after_mode.main_window_fullscreen);
    // GSS-FR-20 / GSS-FR-24: and none of those three writes disturbed the
    // two modes a Diff tab owns.
    assert_eq!(
        after_mode.diff_visualization_mode,
        DiffVisualizationMode::SideBySide
    );
    assert_eq!(after_mode.diff_rendering_mode, DiffRenderingMode::Rich);

    // A Diff tab's own write leaves everything else alone in turn.
    let mut editing = store.load_app_preferences().unwrap();
    editing.diff_visualization_mode = DiffVisualizationMode::Final;
    store.save_app_preferences(editing).unwrap();
    let after_diff = store.load_app_preferences().unwrap();
    assert_eq!(
        after_diff.diff_visualization_mode,
        DiffVisualizationMode::Final
    );
    assert_eq!(after_diff.theme, Theme::Dark);
    assert_eq!(after_diff.search_query_mode, SearchQueryMode::SmartCase);
    assert!(!after_diff.main_window_fullscreen);
    assert_eq!(
        after_diff.diff_rendering_mode,
        DiffRenderingMode::Rich,
        "a visualization write must not reset the rendering mode"
    );
    // GSS-FR-20 / GSS-FR-25: and none of those writes disturbed the action
    // the Changes panel owns.
    assert_eq!(
        after_diff.changes_commit_action,
        ChangesCommitAction::CommitAndPush,
        "no other editor's write may reset the selected commit action"
    );
    // GSS-FR-20 / GSS-FR-29: nor the three roles' font settings, which the
    // Appearance section owns.
    assert_eq!(
        after_diff.fonts.rich.family.as_deref(),
        Some("Georgia"),
        "no other editor's write may reset a typographic role"
    );
    assert_eq!(after_diff.fonts.rich.size_px, Some(17.0));
    assert_eq!(after_diff.fonts.rich.line_height, Some(1.8));
    // GSS-FR-20 / GSS-FR-32: nor the notifications switch, which the
    // Notifications section owns. It was written `false` above, so a write
    // that dropped the field would read back `true` (its default) here.
    assert!(
        !after_diff.notifications_enabled,
        "no other editor's write may re-enable notifications"
    );
    // GSS-FR-20 / GSS-FR-33: nor the selection-follows-tab switch, which the
    // Navigation section owns. Written `false` above, so a write that
    // dropped the field would read back `true` (its default) here.
    assert!(
        !after_diff.selection_follows_tab,
        "no other editor's write may re-enable selection-follows-tab"
    );
    // GSS-FR-20 / GSS-FR-34: nor the graduation rail's width, which the
    // Runs panel's graduation section owns. Written `0.28` above, so a
    // write that dropped the field would read back `0.20` (its default).
    assert_eq!(
        after_diff.graduation_rail_width_fraction, 0.28,
        "no other editor's write may reset the graduation rail's width"
    );

    // And the switch's own write leaves every one of them alone in turn.
    let mut editing = store.load_app_preferences().unwrap();
    editing.notifications_enabled = true;
    store.save_app_preferences(editing).unwrap();
    let after_switch = store.load_app_preferences().unwrap();
    assert!(after_switch.notifications_enabled);
    assert_eq!(after_switch.theme, Theme::Dark);
    assert_eq!(after_switch.search_query_mode, SearchQueryMode::SmartCase);
    assert_eq!(
        after_switch.diff_visualization_mode,
        DiffVisualizationMode::Final
    );
    assert_eq!(after_switch.diff_rendering_mode, DiffRenderingMode::Rich);
    assert_eq!(
        after_switch.changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );
    assert_eq!(after_switch.fonts.rich.family.as_deref(), Some("Georgia"));
    assert!(
        !after_switch.selection_follows_tab,
        "the notifications write must not disturb the Navigation switch"
    );

    // GSS-FR-20 / GSS-FR-33: and the Navigation switch's own write leaves
    // every other field alone, which is the last of the record's editors.
    let mut editing = store.load_app_preferences().unwrap();
    editing.selection_follows_tab = true;
    store.save_app_preferences(editing).unwrap();
    let after_follow = store.load_app_preferences().unwrap();
    assert!(after_follow.selection_follows_tab);
    assert_eq!(after_follow.theme, Theme::Dark);
    assert_eq!(after_follow.search_query_mode, SearchQueryMode::SmartCase);
    assert_eq!(
        after_follow.diff_visualization_mode,
        DiffVisualizationMode::Final
    );
    assert_eq!(after_follow.diff_rendering_mode, DiffRenderingMode::Rich);
    assert_eq!(
        after_follow.changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );
    assert!(after_follow.notifications_enabled);
    assert_eq!(after_follow.fonts.rich.family.as_deref(), Some("Georgia"));
    assert_eq!(after_follow.graduation_rail_width_fraction, 0.28);
    assert_eq!(after_follow.graduation_paths_width_fraction, 0.44);
    assert_eq!(after_follow.git_files_width_fraction, 0.37);

    // GSS-FR-20 / GSS-FR-34: and the graduation section's own write is the
    // last editor, leaving every other field of the record alone in turn.
    let mut editing = store.load_app_preferences().unwrap();
    editing.graduation_rail_width_fraction = 0.11;
    store.save_app_preferences(editing).unwrap();
    let after_rail = store.load_app_preferences().unwrap();
    assert_eq!(after_rail.graduation_rail_width_fraction, 0.11);
    assert_eq!(after_rail.graduation_paths_width_fraction, 0.44);
    assert_eq!(after_rail.theme, Theme::Dark);
    assert_eq!(after_rail.search_query_mode, SearchQueryMode::SmartCase);
    assert_eq!(
        after_rail.diff_visualization_mode,
        DiffVisualizationMode::Final
    );
    assert_eq!(after_rail.diff_rendering_mode, DiffRenderingMode::Rich);
    assert_eq!(
        after_rail.changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );
    assert!(after_rail.notifications_enabled);
    assert!(after_rail.selection_follows_tab);
    assert_eq!(after_rail.fonts.rich.family.as_deref(), Some("Georgia"));
    assert_eq!(after_rail.fonts.rich.size_px, Some(17.0));
    assert!(
        !after_rail.main_window_fullscreen,
        "a rail-width write must disturb none of the record's other editors"
    );

    // GSS-FR-20 / GSS-FR-QDNV: the run region's paths-column write is an
    // editor too. Compared whole, so a field it drops fails here.
    let mut editing = store.load_app_preferences().unwrap();
    editing.graduation_paths_width_fraction = 0.52;
    store.save_app_preferences(editing).unwrap();
    let after_paths = store.load_app_preferences().unwrap();
    assert_eq!(
        after_paths,
        AppPreferences {
            graduation_paths_width_fraction: 0.52,
            ..after_rail.clone()
        },
        "a paths-width write must disturb none of the record's other editors"
    );

    // GSS-FR-20 / GSS-FR-MSPQ: the Git panel's files-column write is an editor
    // too. Compared whole, so a field it drops fails here.
    let mut editing = store.load_app_preferences().unwrap();
    editing.git_files_width_fraction = 0.61;
    store.save_app_preferences(editing).unwrap();
    assert_eq!(
        store.load_app_preferences().unwrap(),
        AppPreferences {
            git_files_width_fraction: 0.61,
            ..after_paths.clone()
        },
        "a files-width write must disturb none of the record's other editors"
    );
}
