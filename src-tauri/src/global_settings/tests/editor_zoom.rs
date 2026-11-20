//! The editor zoom level of the app preferences record (GSS-FR-KKYO).
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// GSS-FR-KKYO, GSS-FR-04: a fresh record and a record from an earlier build both
// read the level as 100.
#[test]
fn editor_zoom_defaults_to_one_hundred() {
    assert_eq!(AppPreferences::default().editor_zoom_percent, 100);
    let read: AppPreferences = toml::from_str("theme = \"dark\"\n").unwrap();
    assert_eq!(read.editor_zoom_percent, 100);
}

// GSS-FR-KKYO: a value that is not a whole number reads as unset, and the other
// fields of the file stay.
#[test]
fn editor_zoom_of_the_wrong_type_reads_as_unset_and_the_rest_stays() {
    for stored in ["\"big\"", "1.5", "true", "-20"] {
        let text =
            format!("theme = \"dark\"\neditorZoomPercent = {stored}\nvoiceVadSilenceMs = 900\n");
        let read: AppPreferences = toml::from_str(&text).unwrap();
        assert_eq!(read.editor_zoom_percent, 100, "stored {stored}");
        assert_eq!(read.theme, Theme::Dark);
        assert_eq!(read.voice_vad_silence_ms, 900);
    }
}

// GSS-FR-KKYO: the store does not limit the range. The reader does
// (`EZM-editor-zoom.md` EZM-FR-HYQD).
#[test]
fn editor_zoom_out_of_range_is_returned_as_stored() {
    let read: AppPreferences = toml::from_str("editorZoomPercent = 900\n").unwrap();
    assert_eq!(read.editor_zoom_percent, 900);
}

// GSS-FR-KKYO, GSS-FR-20: a level between steps round-trips through the file,
// next to the sub-tables, and a write of another field carries it through.
#[test]
fn editor_zoom_round_trips_and_survives_a_write_of_another_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let prefs = AppPreferences {
        editor_zoom_percent: 137,
        voice_input_device: Some(VoiceDevice { id: "mic".into(), label: "Mic".into() }),
        fonts: FontSettings {
            rich: FontRole { size_px: Some(16.0), ..Default::default() },
            ..Default::default()
        },
        ..Default::default()
    };
    store.save_app_preferences(prefs).unwrap();
    let mut editing = store.load_app_preferences().unwrap();
    editing.theme = Theme::Dark;
    store.save_app_preferences(editing).unwrap();

    let held = GlobalSettingsStore::with_path(path).load_app_preferences().unwrap();
    assert_eq!(held.editor_zoom_percent, 137);
    assert_eq!(held.theme, Theme::Dark);
    assert_eq!(held.fonts.rich.size_px, Some(16.0));
}

// GSS-FR-KKYO: the wire name of the field is `editorZoomPercent`.
#[test]
fn editor_zoom_has_the_camel_case_wire_name() {
    let json = serde_json::to_value(AppPreferences::default()).unwrap();
    assert_eq!(json["editorZoomPercent"], 100);
}

// GSS-FR-KKYO: a number that does not fit, and a number with a fraction, are not
// whole levels, so they read as unset. The frontend sends whole numbers only.
#[test]
fn editor_zoom_that_does_not_fit_or_has_a_fraction_reads_as_unset() {
    let big: AppPreferences = toml::from_str("editorZoomPercent = 4294967296\n").unwrap();
    assert_eq!(big.editor_zoom_percent, 100);
    let json: AppPreferences = serde_json::from_str(r#"{"editorZoomPercent":137.5}"#).unwrap();
    assert_eq!(json.editor_zoom_percent, 100);
    let whole: AppPreferences = serde_json::from_str(r#"{"editorZoomPercent":137}"#).unwrap();
    assert_eq!(whole.editor_zoom_percent, 137);
}
