//! The pure contracts: the serialised wire shapes, the timestamp helpers, the
//! ordering and pruning of the recents list, and the identifier derivation.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Wire-shape contracts ------------------------------------------

#[test]
fn theme_defaults_to_system() {
    // GSS-FR-04 / GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-32, GSS-FR-33, GSS-FR-34: unset theme defaults to system.
    assert_eq!(Theme::default(), Theme::System);
    assert_eq!(AppPreferences::default().theme, Theme::System);
}

#[test]
fn theme_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Theme::Light).unwrap(), "\"light\"");
    assert_eq!(serde_json::to_string(&Theme::Dark).unwrap(), "\"dark\"");
    assert_eq!(serde_json::to_string(&Theme::System).unwrap(), "\"system\"");
}

#[test]
fn theme_rejects_unknown() {
    assert!(serde_json::from_str::<Theme>("\"sepia\"").is_err());
}

#[test]
fn app_preferences_missing_theme_defaults_to_system() {
    let p: AppPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(p.theme, Theme::System);
}

#[test]
fn app_preferences_unknown_keys_repair_to_defaults() {
    // GSS-FR-13 (logic-level): a forward/garbage payload with unknown keys
    // and no recognised `theme` deserialises to defaults rather than
    // erroring or panicking.
    let p: AppPreferences =
        serde_json::from_str(r#"{"future_flag": true, "nested": {"x": 1}}"#).unwrap();
    assert_eq!(p.theme, Theme::System);
}

#[test]
fn recent_entry_serializes_camelcase_with_last_opened_at() {
    let v = serde_json::to_value(entry("a", "2026-01-01T00:00:00Z", true, false)).unwrap();
    for key in ["name", "path", "lastOpenedAt", "pinned", "missing"] {
        assert!(v.get(key).is_some(), "missing key {key:?} in {v}");
    }
    assert_eq!(v.get("lastOpenedAt").unwrap(), "2026-01-01T00:00:00Z");
}

#[test]
fn plugin_and_adapter_serialize_camelcase() {
    let p = serde_json::to_value(InstalledPlugin {
        id: "x".into(),
        name: "x".into(),
        source: "s".into(),
    })
    .unwrap();
    for key in ["id", "name", "source"] {
        assert!(p.get(key).is_some(), "plugin missing {key}");
    }
}

// ---- now_iso8601 / unix_secs_to_iso8601 ----------------------------

#[test]
fn unix_epoch_formats_as_iso8601() {
    assert_eq!(unix_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
}

#[test]
fn known_unix_timestamp_formats_correctly() {
    // 1_700_000_000 == 2023-11-14T22:13:20Z (independently verifiable).
    assert_eq!(unix_secs_to_iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
    // A leap-year date: 2024-02-29T12:00:00Z == 1_709_208_000.
    assert_eq!(unix_secs_to_iso8601(1_709_208_000), "2024-02-29T12:00:00Z");
}

#[test]
fn now_iso8601_has_iso_shape() {
    let s = now_iso8601();
    assert!(s.contains('T') && s.ends_with('Z'), "got {s:?}");
    assert_eq!(s.len(), 20, "YYYY-MM-DDTHH:MM:SSZ is 20 chars, got {s:?}");
}

// ---- ordered_visible_recents (GSS-FR-05/06/07) ---------------------

#[test]
fn ordering_recent_first_then_pinned_ahead() {
    // GSS-FR-05.
    let older = entry("older", "2026-01-01T00:00:00Z", false, false);
    let newer = entry("newer", "2026-02-01T00:00:00Z", false, false);

    let unpinned = ordered_visible_recents(&[older.clone(), newer.clone()]);
    assert_eq!(unpinned[0].name, "newer", "most-recent first when neither pinned");
    assert_eq!(unpinned[1].name, "older");

    let older_pinned = entry("older", "2026-01-01T00:00:00Z", true, false);
    let pinned = ordered_visible_recents(&[older_pinned, newer]);
    assert_eq!(pinned[0].name, "older", "pinned sorts ahead of newer unpinned");
    assert_eq!(pinned[1].name, "newer");
}

#[test]
fn prune_drops_unpinned_missing_keeps_pinned_missing() {
    // GSS-FR-06.
    let present = entry("present", "2026-03-01T00:00:00Z", false, false);
    let gone_unpinned = entry("gone_unpinned", "2026-03-02T00:00:00Z", false, true);
    let gone_pinned = entry("gone_pinned", "2026-03-03T00:00:00Z", true, true);

    let out = ordered_visible_recents(&[present, gone_unpinned, gone_pinned]);
    let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"present"));
    assert!(!names.contains(&"gone_unpinned"), "unpinned missing must be pruned");
    assert!(names.contains(&"gone_pinned"), "pinned missing must be retained");
    assert!(out.iter().find(|e| e.name == "gone_pinned").unwrap().missing);
}

#[test]
fn cap_evicts_oldest_unpinned_and_keeps_all_pinned() {
    // GSS-FR-07 with cap=2.
    let entries = vec![
        entry("u_new", "2026-04-03T00:00:00Z", false, false),
        entry("u_mid", "2026-04-02T00:00:00Z", false, false),
        entry("u_old", "2026-04-01T00:00:00Z", false, false),
        entry("pinned_ancient", "2020-01-01T00:00:00Z", true, false),
    ];
    let out = ordered_visible_recents_with_cap(&entries, 2);
    let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"pinned_ancient"), "pinned exempt from cap");
    assert!(names.contains(&"u_new"));
    assert!(names.contains(&"u_mid"));
    assert!(!names.contains(&"u_old"), "oldest unpinned must be evicted");
    assert_eq!(out[0].name, "pinned_ancient");
    assert_eq!(out[1].name, "u_new");
    assert_eq!(out[2].name, "u_mid");
}

#[test]
fn empty_list_orders_to_empty() {
    assert!(ordered_visible_recents(&[]).is_empty());
}

#[test]
fn production_cap_keeps_exactly_cap_unpinned_evicting_oldest() {
    // GSS-FR-07 at the *production* cap via `ordered_visible_recents`.
    let n = RECENT_UNPINNED_CAP + 5;
    let entries: Vec<RecentProjectEntry> = (0..n)
        .map(|i| {
            entry(
                &format!("u{i:03}"),
                &format!("2026-01-01T00:{:02}:00Z", i % 60),
                false,
                false,
            )
        })
        .collect();

    let out = ordered_visible_recents(&entries);
    assert_eq!(
        out.len(),
        RECENT_UNPINNED_CAP,
        "production listing must retain exactly RECENT_UNPINNED_CAP unpinned entries"
    );
    let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&"u000"), "oldest must be evicted");
    assert!(
        names.contains(&format!("u{:03}", n - 1).as_str()),
        "newest must be retained"
    );
}

// ---- derive_id_from_source -----------------------------------------

#[test]
fn derive_id_strips_path_and_git_suffix() {
    assert_eq!(derive_id_from_source("git@github.com:org/markdown-toolbar.git"), "markdown-toolbar");
    assert_eq!(derive_id_from_source("https://example.com/plugins/git-pr"), "git-pr");
    assert_eq!(derive_id_from_source("/local/path/mcp-bridge/"), "mcp-bridge");
    assert_eq!(derive_id_from_source("plain"), "plain");
}
