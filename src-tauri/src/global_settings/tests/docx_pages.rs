//! The DOCX page settings of the app preferences (GSS-FR-JTZE, GSS-FR-TLHZ,
//! GSS-FR-MGNV).
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// GSS-FR-JTZE, GSS-FR-TLHZ, GSS-FR-MGNV: a fresh machine and a record written
// before the fields existed have the landscape pages off, a threshold of 75,
// and margins of 35, 30, 30, and 30 mm.
#[test]
fn docx_page_settings_default_also_for_a_record_without_the_fields() {
    let fresh = AppPreferences::default();
    let legacy: AppPreferences = toml::from_str("theme = \"dark\"\n").unwrap();
    for prefs in [fresh, legacy] {
        assert!(!prefs.docx_landscape_diagrams);
        assert_eq!(prefs.docx_landscape_threshold_percent, 75);
        assert_eq!(
            prefs.docx_margins_mm,
            DocxMarginsMm { top: 35, bottom: 30, left: 30, right: 30 }
        );
    }
}

// GSS-FR-TLHZ: a threshold out of 10 to 100, or not a whole number, reads as
// 75, and the rest of the record still decodes. The limits themselves stay.
#[test]
fn a_threshold_out_of_range_reads_as_75() {
    for (stored, expected) in [
        ("9", 75),
        ("10", 10),
        ("100", 100),
        ("101", 75),
        ("-5", 75),
        ("\"60\"", 75),
        ("60.5", 75),
        ("60", 60),
    ] {
        let record = format!("theme = \"dark\"\ndocxLandscapeThresholdPercent = {stored}\n");
        let decoded: AppPreferences = toml::from_str(&record).unwrap();
        assert_eq!(decoded.docx_landscape_threshold_percent, expected, "{stored}");
        assert_eq!(decoded.theme, Theme::Dark, "{stored}");
    }
}

// GSS-FR-MGNV: a side out of 5 to 60 mm, or not a whole number, reads as its
// default, and the other sides stay. A value that is not a table reads as the
// default margins.
#[test]
fn a_margin_out_of_range_reads_as_its_default_and_the_other_sides_stay() {
    let record = "docxMarginsMm = { top = 61, bottom = 4, left = \"x\", right = 12 }\n";
    let decoded: AppPreferences = toml::from_str(record).unwrap();
    assert_eq!(decoded.docx_margins_mm, DocxMarginsMm { top: 35, bottom: 30, left: 30, right: 12 });
    let record = "docxMarginsMm = { top = 5, bottom = 60 }\n";
    let decoded: AppPreferences = toml::from_str(record).unwrap();
    assert_eq!(decoded.docx_margins_mm, DocxMarginsMm { top: 5, bottom: 60, left: 30, right: 30 });
    let decoded: AppPreferences = toml::from_str("docxMarginsMm = 7\n").unwrap();
    assert_eq!(decoded.docx_margins_mm, DocxMarginsMm::default());
}

// GSS-FR-JTZE, GSS-FR-TLHZ, GSS-FR-MGNV, GSS-FR-20: the settings round-trip
// through the store and the wire, in their camelCase names.
#[test]
fn docx_page_settings_round_trip_through_the_store_and_the_wire() {
    let store = GlobalSettingsStore::in_memory();
    let mut prefs = store.load_app_preferences().unwrap();
    prefs.docx_landscape_diagrams = true;
    prefs.docx_landscape_threshold_percent = 40;
    prefs.docx_margins_mm = DocxMarginsMm { top: 10, bottom: 11, left: 12, right: 13 };
    store.save_app_preferences(prefs.clone()).unwrap();
    assert_eq!(store.load_app_preferences().unwrap(), prefs);
    let wire = serde_json::to_value(&prefs).unwrap();
    assert_eq!(wire["docxLandscapeDiagrams"], serde_json::json!(true));
    assert_eq!(wire["docxLandscapeThresholdPercent"], serde_json::json!(40));
    assert_eq!(
        wire["docxMarginsMm"],
        serde_json::json!({ "top": 10, "bottom": 11, "left": 12, "right": 13 })
    );
    let read: AppPreferences = serde_json::from_value(wire).unwrap();
    assert_eq!(read, prefs);
    let stored = toml::to_string(&prefs).unwrap();
    assert_eq!(toml::from_str::<AppPreferences>(&stored).unwrap(), prefs);
}
