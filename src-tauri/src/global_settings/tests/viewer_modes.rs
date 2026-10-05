//! The viewer preferences: the diff modes, the search query mode, and the
//! width of the rail.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Diff viewer modes (GSS-FR-24) -----------------------------------

#[test]
fn the_diff_viewer_modes_default_to_unified_and_source() {
    // GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-32, GSS-FR-33, GSS-FR-34: a user who has never chosen reads back both defaults.
    let p = AppPreferences::default();
    assert_eq!(p.diff_visualization_mode, DiffVisualizationMode::Unified);
    assert_eq!(p.diff_rendering_mode, DiffRenderingMode::Source);

    let store = GlobalSettingsStore::in_memory();
    let loaded = store.load_app_preferences().unwrap();
    assert_eq!(
        loaded.diff_visualization_mode,
        DiffVisualizationMode::Unified
    );
    assert_eq!(loaded.diff_rendering_mode, DiffRenderingMode::Source);
}

#[test]
fn the_diff_viewer_modes_have_the_wire_vocabulary_the_frontend_speaks() {
    // camelCase keys, snake_case values. A typo here is a silent no-op
    // rather than a compile error, so it is pinned.
    let json = serde_json::to_value(AppPreferences {
        diff_visualization_mode: DiffVisualizationMode::SideBySide,
        diff_rendering_mode: DiffRenderingMode::Rich,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(json.get("diffVisualizationMode").unwrap(), "side_by_side");
    assert_eq!(json.get("diffRenderingMode").unwrap(), "rich");

    for (mode, wire) in [
        (DiffVisualizationMode::Unified, "unified"),
        (DiffVisualizationMode::SideBySide, "side_by_side"),
        (DiffVisualizationMode::Final, "final"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), wire);
    }
    assert!(serde_json::from_str::<DiffVisualizationMode>("\"inline\"").is_err());
    assert!(serde_json::from_str::<DiffRenderingMode>("\"pretty\"").is_err());
}

#[test]
fn a_record_written_before_the_diff_modes_existed_still_loads() {
    // GSS-FR-13: an existing synthesis.toml carries neither field. It must
    // load with both defaulted rather than erroring, or every existing
    // install boots to a repaired (empty) store.
    let decoded: AppPreferences =
        serde_json::from_str(r#"{"theme":"dark","mainWindowFullscreen":true}"#).unwrap();
    assert_eq!(decoded.theme, Theme::Dark);
    assert_eq!(
        decoded.diff_visualization_mode,
        DiffVisualizationMode::Unified
    );
    assert_eq!(decoded.diff_rendering_mode, DiffRenderingMode::Source);
}

#[test]
fn gss_ts21_the_diff_modes_survive_a_relaunch_a_theme_write_and_another_project() {
    // GSS-FR-24 / GSS-FR-20: they live outside the per-project
    // slot, so every project reads the same two values.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.diff_visualization_mode = DiffVisualizationMode::SideBySide;
        prefs.diff_rendering_mode = DiffRenderingMode::Rich;
        store.save_app_preferences(prefs).unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path);
    let after_relaunch = reloaded.load_app_preferences().unwrap();
    assert_eq!(
        after_relaunch.diff_visualization_mode,
        DiffVisualizationMode::SideBySide,
        "both modes must survive a relaunch"
    );
    assert_eq!(after_relaunch.diff_rendering_mode, DiffRenderingMode::Rich);

    // The Appearance section then saves a theme change (GLS-FR-14).
    let mut editing = reloaded.load_app_preferences().unwrap();
    editing.theme = Theme::Light;
    reloaded.save_app_preferences(editing).unwrap();

    // And two projects each get their own layout slot; neither carries a
    // diff mode, so the values are the same whichever project is open.
    for key in ["/projects/alpha", "/projects/beta"] {
        reloaded
            .save_project_layout(key, crate::layout::LayoutPreferences::default())
            .unwrap();
        let prefs = reloaded.load_app_preferences().unwrap();
        assert_eq!(
            prefs.diff_visualization_mode,
            DiffVisualizationMode::SideBySide,
            "the modes are user-global, so {key} reads the same pair"
        );
        assert_eq!(prefs.diff_rendering_mode, DiffRenderingMode::Rich);
    }
    assert_eq!(reloaded.load_app_preferences().unwrap().theme, Theme::Light);
}

#[test]
fn gss_ts17_the_query_mode_survives_a_relaunch_and_a_later_theme_write() {
    // GSS-FR-21 / GSS-FR-20.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.search_query_mode = SearchQueryMode::Regex;
        store.save_app_preferences(prefs).unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reloaded.load_app_preferences().unwrap().search_query_mode,
        SearchQueryMode::Regex,
        "the mode must survive a relaunch"
    );

    // The Appearance section then saves a theme change (GLS-FR-14, GSS-FR-20).
    let mut editing = reloaded.load_app_preferences().unwrap();
    editing.theme = Theme::Light;
    reloaded.save_app_preferences(editing).unwrap();
    let after = reloaded.load_app_preferences().unwrap();
    assert_eq!(after.theme, Theme::Light);
    assert_eq!(
        after.search_query_mode,
        SearchQueryMode::Regex,
        "the persisted record still carries the query mode"
    );
}

#[test]
fn the_query_mode_speaks_the_same_wire_vocabulary_as_the_search_engine() {
    // GSS-FR-21 defers the modes themselves to SCC-FR-05, so the two must
    // decode from the identical strings — the preference is handed straight
    // back to `start_search` as its `mode`.
    for (mode, wire) in [
        (SearchQueryMode::LiteralInsensitive, "literal_insensitive"),
        (SearchQueryMode::SmartCase, "smart_case"),
        (SearchQueryMode::Regex, "regex"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), wire);
        assert!(
            serde_json::from_str::<crate::search::SearchMode>(&format!("\"{wire}\"")).is_ok(),
            "{wire} must also decode as a SearchMode"
        );
    }
    assert!(serde_json::from_str::<SearchQueryMode>("\"fuzzy\"").is_err());
}

#[test]
fn a_record_written_before_the_query_mode_existed_still_loads() {
    // GSS-FR-13: an existing synthesis.toml carries only `theme` and the
    // full-screen flag. Adding a field must not make it unreadable.
    let decoded: AppPreferences =
        serde_json::from_str(r#"{"theme":"dark","mainWindowFullscreen":true}"#).unwrap();
    assert_eq!(decoded.theme, Theme::Dark);
    assert!(decoded.main_window_fullscreen);
    assert_eq!(decoded.search_query_mode, SearchQueryMode::LiteralInsensitive);
}

#[test]
fn gss_ts33_the_rail_width_defaults_and_survives_everything_but_its_own_write() {
    // GSS-FR-34 / GSS-FR-20 / GSS-FR-17.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    // A record carrying a theme and all nine font values but **no**
    // `graduationRailWidthFraction` at all — which is every record written
    // before this field existed. What is under test is exactly what serde
    // does with it missing: `f64::default()` is `0.0`, so a rail of no
    // width is what every existing install would read back if the field
    // ever lost its explicit default.
    //
    // The file is produced by serialising a real record and then striking
    // that one line out, rather than by hand-writing TOML. A literal would
    // have to restate the wire shape — the camelCase renaming, the nesting
    // of the font tables — and a literal that drifted from it would be
    // silently ignored by serde and leave this test asserting the defaults
    // against nothing at all.
    let seeded = {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.theme = Theme::Dark;
        for (role, family, size, height) in [
            ("ui", "Inter", 13.0, 1.5),
            ("rich", "Georgia", 16.0, 1.6),
            ("source", "Menlo", 12.0, 1.4),
        ] {
            let slot = match role {
                "ui" => &mut prefs.fonts.ui,
                "rich" => &mut prefs.fonts.rich,
                _ => &mut prefs.fonts.source,
            };
            slot.family = Some(family.to_string());
            slot.size_px = Some(size);
            slot.line_height = Some(height);
        }
        store.save_app_preferences(prefs).unwrap();
        std::fs::read_to_string(&path).unwrap()
    };
    let without_width: String = seeded
        .lines()
        .filter(|line| !line.starts_with("graduationRailWidthFraction"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        seeded.contains("graduationRailWidthFraction"),
        "the field is written under the wire key the frontend reads; got {seeded}",
    );
    assert!(!without_width.contains("graduationRailWidthFraction"));
    std::fs::write(&path, &without_width).unwrap();

    let store = GlobalSettingsStore::with_path(path.clone());
    let read = store.load_app_preferences().unwrap();
    assert_eq!(
        read.graduation_rail_width_fraction, 0.20,
        "a record written before the field existed reads back at the default \
         width rather than at a rail of no width",
    );
    assert_eq!(read.theme, Theme::Dark, "and everything else is unchanged");
    assert_eq!(read.fonts.ui.family.as_deref(), Some("Inter"));
    assert_eq!(read.fonts.source.size_px, Some(12.0));

    // The graduation section then persists a width after a drag.
    let mut editing = store.load_app_preferences().unwrap();
    editing.graduation_rail_width_fraction = 0.28;
    store.save_app_preferences(editing).unwrap();

    // It survives a relaunch, and it reads the same whichever project is
    // open, because it lives outside the per-project slot.
    let reloaded = GlobalSettingsStore::with_path(path);
    for key in ["/projects/alpha", "/projects/beta"] {
        reloaded
            .save_project_layout(key, crate::layout::LayoutPreferences::default())
            .unwrap();
        assert_eq!(
            reloaded
                .load_app_preferences()
                .unwrap()
                .graduation_rail_width_fraction,
            0.28,
            "the width is user-global, so {key} reads the same value",
        );
    }

    // GSS-FR-20: an unrelated whole-record write leaves it standing.
    let mut editing = reloaded.load_app_preferences().unwrap();
    editing.diff_visualization_mode = DiffVisualizationMode::SideBySide;
    reloaded.save_app_preferences(editing).unwrap();
    let after = reloaded.load_app_preferences().unwrap();
    assert_eq!(after.graduation_rail_width_fraction, 0.28);
    assert_eq!(after.theme, Theme::Dark);
    assert_eq!(after.fonts.rich.family.as_deref(), Some("Georgia"));

    // GSS-FR-34: stored whatever it is given. The 5 %–30 % clamp is the
    // graduation section's (GRH-FR-MCHQ), so a value outside it round-trips
    // unchanged rather than being corrected on the way through here.
    let mut editing = reloaded.load_app_preferences().unwrap();
    editing.graduation_rail_width_fraction = 0.90;
    reloaded.save_app_preferences(editing).unwrap();
    assert_eq!(
        reloaded
            .load_app_preferences()
            .unwrap()
            .graduation_rail_width_fraction,
        0.90,
        "this module bounds nothing",
    );
}

#[test]
fn the_rail_width_is_not_a_layout_field() {
    // GSS-FR-34 / GSS-FR-17: the rail width is user-global, and the
    // per-project layout slot is where the shell's shape lives. If someone
    // adds it to `LayoutPreferences`, this fails.
    let json = serde_json::to_value(crate::layout::LayoutPreferences::default()).unwrap();
    assert!(
        json.get("graduationRailWidthFraction").is_none(),
        "the rail width must not appear in the layout payload; got {json}"
    );
}

#[test]
fn the_rail_width_wire_key_is_the_one_the_graduation_section_reads() {
    // The frontend speaks camelCase (`src/types.ts`), and a typo in a wire
    // key is a silent no-op rather than a compile error on either side.
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(
        json.get("graduationRailWidthFraction").and_then(|v| v.as_f64()),
        Some(0.20),
        "the wire key and the default the section renders at; got {json}"
    );
}

#[test]
fn the_fullscreen_flag_is_not_a_layout_field() {
    // SNV-FR-08 / GSS-FR-19: full-screen is deliberately NOT part of the
    // per-project layout payload — geometry is per project, full-screen is
    // user-global. If someone adds it to `LayoutPreferences`, this fails.
    let json = serde_json::to_value(crate::layout::LayoutPreferences::default()).unwrap();
    assert!(
        json.get("mainWindowFullscreen").is_none(),
        "full-screen must not appear in the layout payload; got {json}"
    );
}

// ---- Paths column width (GSS-FR-QDNV) -------------------------------

// GSS-FR-QDNV, GSS-FR-04, GSS-FR-20: the paths width defaults, survives a
// relaunch and other writes, and is stored unbounded.
#[test]
fn the_paths_width_defaults_and_round_trips_untouched() {
    // A fresh record holds 0.30, under the wire key the graduation section
    // reads.
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(
        json.get("graduationPathsWidthFraction").and_then(|v| v.as_f64()),
        Some(0.30),
        "the wire key and the default the region renders at; got {json}"
    );

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.toml");

    // A record written before the field existed: a real record, serialised,
    // with that one line struck out.
    let seeded = {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.theme = Theme::Dark;
        prefs.graduation_rail_width_fraction = 0.12;
        // Not the default, so a strip that removed nothing would read back
        // as this value and fail below.
        prefs.graduation_paths_width_fraction = 0.55;
        store.save_app_preferences(prefs).unwrap();
        std::fs::read_to_string(&path).unwrap()
    };
    assert!(seeded.contains("graduationPathsWidthFraction"));
    let without_width: String = seeded
        .lines()
        .filter(|line| !line.starts_with("graduationPathsWidthFraction"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!without_width.contains("graduationPathsWidthFraction"));
    std::fs::write(&path, &without_width).unwrap();

    let store = GlobalSettingsStore::with_path(path.clone());
    let read = store.load_app_preferences().unwrap();
    assert_eq!(
        read.graduation_paths_width_fraction, 0.30,
        "a record written before the field existed reads back at the default"
    );
    assert_eq!(read.graduation_rail_width_fraction, 0.12);
    assert_eq!(read.theme, Theme::Dark);

    // GSS-FR-20: the section's own write leaves every other field standing,
    // and the value survives a relaunch.
    let mut editing = read;
    editing.graduation_paths_width_fraction = 0.41;
    store.save_app_preferences(editing).unwrap();
    let reloaded = GlobalSettingsStore::with_path(path).load_app_preferences().unwrap();
    assert_eq!(reloaded.graduation_paths_width_fraction, 0.41);
    assert_eq!(reloaded.graduation_rail_width_fraction, 0.12);
    assert_eq!(reloaded.theme, Theme::Dark);

    // GSS-FR-QDNV: this module bounds nothing. The clamp is the region's.
    let store = GlobalSettingsStore::in_memory();
    let mut editing = store.load_app_preferences().unwrap();
    editing.graduation_paths_width_fraction = 0.95;
    store.save_app_preferences(editing).unwrap();
    assert_eq!(
        store.load_app_preferences().unwrap().graduation_paths_width_fraction,
        0.95
    );
}

// GSS-FR-QDNV / GSS-FR-17: the paths width is user-global, so it is no field
// of the per-project layout slot.
#[test]
fn the_paths_width_is_not_a_layout_field() {
    let json = serde_json::to_value(crate::layout::LayoutPreferences::default()).unwrap();
    assert!(
        json.get("graduationPathsWidthFraction").is_none(),
        "the paths width must not appear in the layout payload; got {json}"
    );
}

// GSS-FR-MSPQ, GSS-FR-04, GSS-FR-20: the Git panel's files width defaults,
// reads a record written before it existed as the default, survives a
// relaunch, and is stored unbounded.
#[test]
fn the_git_files_width_defaults_and_round_trips_untouched() {
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(
        json.get("gitFilesWidthFraction").and_then(|v| v.as_f64()),
        Some(0.30),
        "the wire key and the default the Git panel renders at; got {json}"
    );

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.toml");
    let seeded = {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.theme = Theme::Dark;
        prefs.git_files_width_fraction = 0.55;
        store.save_app_preferences(prefs).unwrap();
        std::fs::read_to_string(&path).unwrap()
    };
    assert!(seeded.contains("gitFilesWidthFraction"));
    let without_width: String = seeded
        .lines()
        .filter(|line| !line.starts_with("gitFilesWidthFraction"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, &without_width).unwrap();

    let store = GlobalSettingsStore::with_path(path.clone());
    let read = store.load_app_preferences().unwrap();
    assert_eq!(read.git_files_width_fraction, 0.30);
    assert_eq!(read.theme, Theme::Dark);

    let mut editing = read;
    editing.git_files_width_fraction = 0.92;
    store.save_app_preferences(editing).unwrap();
    let reloaded = GlobalSettingsStore::with_path(path).load_app_preferences().unwrap();
    assert_eq!(reloaded.git_files_width_fraction, 0.92, "this module bounds nothing");
    assert_eq!(reloaded.theme, Theme::Dark);
}
