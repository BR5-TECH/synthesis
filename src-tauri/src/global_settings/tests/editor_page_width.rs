//! The page width of the document page in the app preferences (GSS-FR-UYXH).
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// GSS-FR-UYXH: a fresh machine and a record written before the field existed
// have a page width of 820.
#[test]
fn page_width_defaults_to_820_also_for_a_record_without_the_field() {
    assert_eq!(AppPreferences::default().editor_page_width_px, 820);
    let legacy: AppPreferences = toml::from_str("theme = \"dark\"\n").unwrap();
    assert_eq!(legacy.editor_page_width_px, 820);
}

// GSS-FR-UYXH: a width out of 560 to 1800, or not a whole number, reads as
// 820, and the rest of the record still decodes. The limits themselves stay.
#[test]
fn a_page_width_out_of_range_reads_as_820() {
    for (stored, expected) in [
        ("559", 820),
        ("560", 560),
        ("1800", 1800),
        ("1801", 820),
        ("-5", 820),
        ("0", 820),
        ("\"1200\"", 820),
        ("1200.5", 820),
        ("1200", 1200),
    ] {
        let record = format!("theme = \"dark\"\neditorPageWidthPx = {stored}\n");
        let decoded: AppPreferences = toml::from_str(&record).unwrap();
        assert_eq!(decoded.editor_page_width_px, expected, "{stored}");
        assert_eq!(decoded.theme, Theme::Dark, "{stored}");
    }
}

// GSS-FR-UYXH, GSS-FR-20: the width round-trips through the file, and a
// write of another field keeps it.
#[test]
fn page_width_round_trips_and_survives_a_write_of_another_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let prefs = AppPreferences { editor_page_width_px: 1240, ..Default::default() };
    store.save_app_preferences(prefs).unwrap();
    let mut editing = store.load_app_preferences().unwrap();
    editing.theme = Theme::Dark;
    store.save_app_preferences(editing).unwrap();

    let held = GlobalSettingsStore::with_path(path).load_app_preferences().unwrap();
    assert_eq!(held.editor_page_width_px, 1240);
    assert_eq!(held.theme, Theme::Dark);
}

// GSS-FR-UYXH, GSS-FR-04: the wire name of the field is `editorPageWidthPx`,
// and a wire record with it decodes back to the same width.
#[test]
fn page_width_has_the_camel_case_wire_name() {
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(json["editorPageWidthPx"], 820);
    let mut wire = json;
    wire["editorPageWidthPx"] = serde_json::json!(1500);
    let read: AppPreferences = serde_json::from_value(wire).unwrap();
    assert_eq!(read.editor_page_width_px, 1500);
}
