//! The timestamp rendering and the note identifiers.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

// -- Timestamps ---------------------------------------------------------

#[test]
fn rfc3339_rendering_is_utc_and_fixed_width() {
    assert_eq!(format_rfc3339_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_rfc3339_utc(1), "1970-01-01T00:00:01Z");
    // A leap day, and the turn of a century that is a leap year.
    assert_eq!(format_rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(format_rfc3339_utc(1_709_164_800), "2024-02-29T00:00:00Z");
    assert_eq!(format_rfc3339_utc(1_800_000_000), "2027-01-15T08:00:00Z");
    // Before the epoch, so a clock skewed backwards still renders a date.
    assert_eq!(format_rfc3339_utc(-1), "1969-12-31T23:59:59Z");
}

#[test]
fn rfc3339_millis_rendering_is_utc_and_fixed_width() {
    // LGC-FR-06: a diagnostic log orders records that arrive inside the same
    // second, so the fraction is always three digits and always present —
    // a variable-width stamp would break both the column and the string
    // ordering `rfc3339_strings_sort_chronologically` relies on.
    assert_eq!(format_rfc3339_millis_utc(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(format_rfc3339_millis_utc(1), "1970-01-01T00:00:00.001Z");
    assert_eq!(format_rfc3339_millis_utc(999), "1970-01-01T00:00:00.999Z");
    assert_eq!(format_rfc3339_millis_utc(1_000), "1970-01-01T00:00:01.000Z");
    assert_eq!(
        format_rfc3339_millis_utc(1_800_000_000_902),
        "2027-01-15T08:00:00.902Z"
    );
    // Before the epoch: the fraction stays in [0, 999] rather than going
    // negative, so the second borrows exactly as `div_euclid` intends.
    assert_eq!(format_rfc3339_millis_utc(-1), "1969-12-31T23:59:59.999Z");
    for ms in [0i64, 1, 999, 1_000, -1, 1_800_000_000_902] {
        assert_eq!(format_rfc3339_millis_utc(ms).len(), 24);
    }
}

#[test]
fn rfc3339_millis_strings_sort_chronologically() {
    // The millisecond stamp inherits the property its second-granularity
    // sibling has, which is what lets a log page order by `ts` as a string.
    let mut stamps: Vec<String> = [1_000i64, 1, 0, 999, 1_800_000_000_902, 1_001]
        .into_iter()
        .map(format_rfc3339_millis_utc)
        .collect();
    stamps.sort();
    assert_eq!(
        stamps,
        [0i64, 1, 999, 1_000, 1_001, 1_800_000_000_902]
            .into_iter()
            .map(format_rfc3339_millis_utc)
            .collect::<Vec<_>>()
    );
}

#[test]
fn rfc3339_strings_sort_chronologically() {
    // NTC-FR-09 rests on this: the list commands order by comparing
    // `updated_at` as a string rather than parsing it.
    let mut stamps: Vec<String> = [1_000_000_000, 0, 2_000_000_000, 1_500_000_000]
        .iter()
        .map(|s| format_rfc3339_utc(*s))
        .collect();
    stamps.sort();
    assert_eq!(
        stamps,
        [0, 1_000_000_000, 1_500_000_000, 2_000_000_000]
            .iter()
            .map(|s| format_rfc3339_utc(*s))
            .collect::<Vec<_>>()
    );
}

#[test]
fn note_ids_are_unique_opaque_and_filename_safe() {
    // NTC-FR-02.
    let ids: Vec<String> = (0..500).map(|_| new_note_id()).collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "ids must not collide");
    for id in &ids {
        assert!(is_valid_id(id), "{id} must be filename-safe");
    }
}

#[test]
fn ids_that_could_name_a_file_outside_the_notes_folder_are_rejected() {
    // NTC-FR-13: the escape gate is not the only guard — a separator or a
    // dot segment would name a file elsewhere *inside* the project.
    for bad in ["", "../evil", "a/b", "a\\b", ".", "..", "a b", "a.toml"] {
        assert!(!is_valid_id(bad), "{bad:?} must be rejected");
        assert!(note_rel(bad).is_err());
    }
    assert!(is_valid_id("0197ab-0001-deadbeef"));
}
