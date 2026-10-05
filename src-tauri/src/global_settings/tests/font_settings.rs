//! The font settings of the typographic roles.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Typographic roles' font settings (GSS-FR-29 / GSS-FR-20) ---------

#[test]
fn gss_ts03_every_role_defaults_to_unset() {
    // GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-32, GSS-FR-33, GSS-FR-34 / GSS-FR-29: a fresh machine has no opinion about any of
    // the nine values, and "unset" is a real state rather than a stand-in
    // for a number this module invented — the built-in default for a role
    // is the *application's*, not the store's.
    let store = GlobalSettingsStore::in_memory();
    let fonts = store.load_app_preferences().unwrap().fonts;
    for (role, setting) in [("ui", &fonts.ui), ("rich", &fonts.rich), ("source", &fonts.source)]
    {
        assert_eq!(setting.family, None, "{role} family is unset");
        assert_eq!(setting.size_px, None, "{role} size is unset");
        assert_eq!(setting.line_height, None, "{role} line height is unset");
    }
}

#[test]
fn the_font_settings_wire_shape_is_the_one_the_appearance_section_reads() {
    // The frontend speaks camelCase, and a typo in a wire key is a silent
    // no-op rather than a compile error. An unset field is *absent* from
    // the payload rather than null, which is what lets the Appearance
    // section tell "never chosen" from "chosen and then cleared".
    let json = serde_json::to_value(AppPreferences {
        fonts: FontSettings {
            source: FontRole {
                family: Some("Fira Code".into()),
                size_px: Some(14.0),
                line_height: Some(1.7),
            },
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    let source = json.get("fonts").unwrap().get("source").unwrap();
    assert_eq!(source.get("family").unwrap(), "Fira Code");
    assert_eq!(source.get("sizePx").and_then(|v| v.as_f64()), Some(14.0));
    assert_eq!(source.get("lineHeight").and_then(|v| v.as_f64()), Some(1.7));
    let ui = json.get("fonts").unwrap().get("ui").unwrap();
    assert!(ui.get("family").is_none(), "an unset field is absent, not null");

    let decoded: AppPreferences = serde_json::from_str(
        r#"{"theme":"dark","fonts":{"rich":{"family":"Georgia","sizePx":16.5}}}"#,
    )
    .unwrap();
    assert_eq!(decoded.fonts.rich.family.as_deref(), Some("Georgia"));
    assert_eq!(decoded.fonts.rich.size_px, Some(16.5));
    assert_eq!(
        decoded.fonts.rich.line_height, None,
        "each of the three fields is independently unset (GSS-FR-29)"
    );
    assert_eq!(decoded.fonts.ui.family, None);
}

#[test]
fn a_record_written_before_the_font_settings_existed_still_loads() {
    // GSS-FR-13: an existing synthesis.toml carries no `fonts` table. It
    // must load with the roles defaulted rather than erroring — otherwise
    // every existing install boots to a repaired (empty) store.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(
        &path,
        "[appPreferences]\ntheme = \"dark\"\nmainWindowFullscreen = true\n",
    )
    .unwrap();
    let store = GlobalSettingsStore::with_path(path);
    let prefs = store.load_app_preferences().unwrap();
    assert_eq!(prefs.theme, Theme::Dark);
    assert!(prefs.main_window_fullscreen);
    assert_eq!(prefs.fonts, FontSettings::default());
}

#[test]
fn a_malformed_fonts_table_repairs_to_defaults_rather_than_panicking() {
    // GSS-FR-13. The one place this record's new shape can cost
    // an existing install: a type-mismatched scalar inside `fonts` is a hard
    // deserialisation error, not a defaulted field, so a hand-edited or
    // half-written file must repair rather than bring the store down. Each
    // case below is a different way the sub-table can be wrong.
    for malformed in [
        // A size that is not a number.
        "[appPreferences]\ntheme = \"dark\"\n[appPreferences.fonts.ui]\nsizePx = \"big\"\n",
        // A role that is a scalar where a table belongs.
        "[appPreferences]\ntheme = \"dark\"\nfonts = { ui = \"Inter\" }\n",
        // `fonts` itself the wrong shape entirely.
        "[appPreferences]\ntheme = \"dark\"\nfonts = 3\n",
        // Truncated mid-table, as an interrupted write leaves it.
        "[appPreferences]\ntheme = \"dark\"\n[appPreferences.fonts.source]\nfamily = ",
    ] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("synthesis.toml");
        std::fs::write(&path, malformed).unwrap();

        let store = GlobalSettingsStore::with_path(path.clone());
        let prefs = store.load_app_preferences().unwrap();
        assert_eq!(
            prefs.fonts,
            FontSettings::default(),
            "a malformed record repairs to defaults: {malformed:?}"
        );

        // And the repaired defaults are what the next save persists, so the
        // store recovers rather than re-reading the broken file forever.
        store.save_app_preferences(prefs).unwrap();
        let reloaded = GlobalSettingsStore::with_path(path);
        assert_eq!(reloaded.load_app_preferences().unwrap().fonts, FontSettings::default());
    }
}

#[test]
fn gss_ts28_all_nine_font_values_survive_a_relaunch_a_diff_write_and_another_project() {
    // GSS-FR-29 / GSS-FR-20. `with_path` rather than
    // `in_memory` deliberately: `fonts` is the app-preferences record's
    // only sub-table, and the TOML serialiser rejects a struct that emits a
    // sub-table before its scalars — only a real persist proves the
    // declaration order holds.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    let chosen = FontSettings {
        ui: FontRole {
            family: Some("Helvetica Neue".into()),
            size_px: Some(12.0),
            line_height: Some(1.4),
        },
        rich: FontRole {
            family: Some("Georgia".into()),
            size_px: Some(17.0),
            line_height: Some(1.8),
        },
        source: FontRole {
            // Chosen while installed, and uninstalled from the machine by
            // the time it is read back.
            family: Some("Fira Code".into()),
            size_px: Some(14.0),
            line_height: Some(1.6),
        },
    };
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.fonts = chosen.clone();
        store.save_app_preferences(prefs).unwrap();
    }

    // Relaunched into a different project.
    let reloaded = GlobalSettingsStore::with_path(path);
    reloaded
        .save_project_layout("/projects/somewhere-else", crate::layout::LayoutPreferences::default())
        .unwrap();
    assert_eq!(
        reloaded.load_app_preferences().unwrap().fonts,
        chosen,
        "all nine values live outside the per-project slot"
    );

    // A Diff tab then writes a visualization-mode change (GSS-FR-20): it
    // supplies the other fields unchanged, so the fonts come through.
    let mut editing = reloaded.load_app_preferences().unwrap();
    editing.diff_visualization_mode = DiffVisualizationMode::Final;
    reloaded.save_app_preferences(editing).unwrap();
    let after = reloaded.load_app_preferences().unwrap();
    assert_eq!(after.diff_visualization_mode, DiffVisualizationMode::Final);
    assert_eq!(after.fonts, chosen);
    assert_eq!(
        after.fonts.source.family.as_deref(),
        Some("Fira Code"),
        "a family since uninstalled reads back exactly as stored: this \
         module never checks that a stored family is available (GSS-FR-29)"
    );
}

#[test]
fn a_font_write_leaves_every_other_field_of_the_record_alone() {
    // GSS-FR-20, the other direction of GSS-FR-29: a font change from the
    // Appearance section never resets the theme, the full-screen flag, the
    // query mode, either diff mode, or the commit action.
    let store = GlobalSettingsStore::in_memory();
    store
        .save_app_preferences(AppPreferences {
            theme: Theme::Dark,
            main_window_fullscreen: true,
            search_query_mode: SearchQueryMode::Regex,
            diff_visualization_mode: DiffVisualizationMode::SideBySide,
            diff_rendering_mode: DiffRenderingMode::Rich,
            changes_commit_action: ChangesCommitAction::Push,
            ..Default::default()
        })
        .unwrap();

    let mut editing = store.load_app_preferences().unwrap();
    editing.fonts.ui.size_px = Some(15.0);
    store.save_app_preferences(editing).unwrap();

    let after = store.load_app_preferences().unwrap();
    assert_eq!(after.fonts.ui.size_px, Some(15.0));
    assert_eq!(after.theme, Theme::Dark);
    assert!(after.main_window_fullscreen);
    assert_eq!(after.search_query_mode, SearchQueryMode::Regex);
    assert_eq!(
        after.diff_visualization_mode,
        DiffVisualizationMode::SideBySide
    );
    assert_eq!(after.diff_rendering_mode, DiffRenderingMode::Rich);
    assert_eq!(after.changes_commit_action, ChangesCommitAction::Push);
}

#[test]
fn the_store_bounds_no_size_and_validates_no_family() {
    // GSS-FR-29: bounding is `GLS-global-settings.md` GLS-FR-19's and
    // resolving an absent family is GLS-FR-22's. This module stores
    // whatever it is given, so a value the section would never commit still
    // round-trips unchanged if something else writes it — the store is not
    // a second line of defence, and a test that expected it to clamp would
    // be encoding the wrong ownership.
    let store = GlobalSettingsStore::in_memory();
    let mut prefs = store.load_app_preferences().unwrap();
    prefs.fonts.source.size_px = Some(400.0);
    prefs.fonts.source.family = Some("A Font Nobody Has".into());
    store.save_app_preferences(prefs).unwrap();
    let after = store.load_app_preferences().unwrap();
    assert_eq!(after.fonts.source.size_px, Some(400.0));
    assert_eq!(
        after.fonts.source.family.as_deref(),
        Some("A Font Nobody Has")
    );
}
