//! The Terminal theme of the app preferences record (GSS-FR-FRBK).
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// GSS-FR-FRBK, GSS-FR-04: a fresh record and a record from an earlier build
// both read the Terminal theme as `app`.
#[test]
fn terminal_theme_defaults_to_app() {
    assert_eq!(AppPreferences::default().terminal_theme, TerminalThemePreference::App);
    let read: AppPreferences = toml::from_str("theme = \"dark\"\n").unwrap();
    assert_eq!(read.terminal_theme, TerminalThemePreference::App);
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(json["terminalTheme"], "app");
}

// GSS-FR-FRBK: each of the six values reads back as stored.
#[test]
fn terminal_theme_reads_each_value() {
    for (stored, want) in [
        ("app", TerminalThemePreference::App),
        ("paper", TerminalThemePreference::Paper),
        ("sand", TerminalThemePreference::Sand),
        ("mist", TerminalThemePreference::Mist),
        ("night", TerminalThemePreference::Night),
        ("ember", TerminalThemePreference::Ember),
    ] {
        let read: AppPreferences =
            toml::from_str(&format!("terminalTheme = \"{stored}\"\n")).unwrap();
        assert_eq!(read.terminal_theme, want, "stored {stored}");
    }
}

// GSS-FR-FRBK: a missing or unknown value reads as `app`, and the other fields
// of the file stay.
#[test]
fn unknown_terminal_theme_reads_as_app_and_the_rest_stays() {
    for stored in ["\"neon\"", "\"Night\"", "3", "true"] {
        let text = format!("theme = \"dark\"\nterminalTheme = {stored}\neditorZoomPercent = 120\n");
        let read: AppPreferences = toml::from_str(&text).unwrap();
        assert_eq!(read.terminal_theme, TerminalThemePreference::App, "stored {stored}");
        assert_eq!(read.theme, Theme::Dark);
        assert_eq!(read.editor_zoom_percent, 120);
    }
}

// GSS-FR-FRBK, GSS-FR-20: the Terminal theme round-trips through the file,
// and a write of another field carries it through.
#[test]
fn terminal_theme_round_trips_and_survives_a_write_of_another_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let prefs =
        AppPreferences { terminal_theme: TerminalThemePreference::Ember, ..Default::default() };
    store.save_app_preferences(prefs).unwrap();
    let mut editing = store.load_app_preferences().unwrap();
    editing.theme = Theme::Light;
    store.save_app_preferences(editing).unwrap();
    let held = GlobalSettingsStore::with_path(path).load_app_preferences().unwrap();
    assert_eq!(held.terminal_theme, TerminalThemePreference::Ember);
    assert_eq!(held.theme, Theme::Light);
}
