//! The recent-projects list as the store presents it.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Store: recent projects ----------------------------------------

#[test]
fn fresh_store_lists_no_recents() {
    // GSS-FR-01, GSS-FR-05: fresh machine -> empty list (no demo seed).
    let store = GlobalSettingsStore::in_memory();
    assert!(store.list_recent_projects().unwrap().is_empty());
}

#[test]
fn record_recent_project_is_mru_upsert() {
    // GSS-FR-10: recording an open inserts it most-recent-first; recording
    // an already-present path moves it to the front without duplicating.
    let store = GlobalSettingsStore::in_memory();
    // Far-past seed timestamps so the freshly-recorded (wall-clock "now")
    // entries sort ahead of them regardless of the host clock.
    store.replace_recents(vec![
        entry("alpha", "2000-01-01T00:00:00Z", false, false),
        entry("beta", "2000-01-02T00:00:00Z", false, false),
    ]);

    // Re-open alpha: it should jump to the front, still 2 entries.
    store.record_recent_project("alpha", "~/dev/alpha").unwrap();
    let list = store.list_recent_projects().unwrap();
    assert_eq!(list.len(), 2, "upsert must not duplicate");
    assert_eq!(list[0].name, "alpha", "re-opened project moves to front");

    // Open a brand-new project: inserted at the front.
    store.record_recent_project("gamma", "~/dev/gamma").unwrap();
    let list = store.list_recent_projects().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].name, "gamma");
}

#[test]
fn record_recent_project_preserves_pinned_on_reopen() {
    let store = GlobalSettingsStore::in_memory();
    store.replace_recents(vec![entry("alpha", "2026-01-01T00:00:00Z", true, false)]);
    store.record_recent_project("alpha", "~/dev/alpha").unwrap();
    let list = store.list_recent_projects().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].pinned, "reopening a pinned project keeps it pinned");
}

#[test]
fn record_recent_project_clears_stale_missing_flag_on_reopen() {
    // A pinned entry previously flagged `missing` (its project was gone);
    // reopening it means it exists again, so `missing` must clear while
    // `pinned` is preserved.
    let store = GlobalSettingsStore::in_memory();
    store.replace_recents(vec![entry("alpha", "2026-01-01T00:00:00Z", true, true)]);
    store.record_recent_project("alpha", "~/dev/alpha").unwrap();
    let list = store.list_recent_projects().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].pinned, "pinned must be preserved");
    assert!(!list[0].missing, "reopening a missing project clears the flag");
}

#[test]
fn same_second_opens_keep_most_recent_first() {
    // GSS-FR-05 tiebreak: two entries sharing an identical `last_opened_at`
    // must order most-recently-recorded first. Force the equal-timestamp
    // case deterministically (rather than racing the clock) by seeding an
    // entry, then recording another and stamping the seed to match.
    let store = GlobalSettingsStore::in_memory();
    store.record_recent_project("first", "~/dev/first").unwrap();
    let ts = store.list_recent_projects().unwrap()[0].last_opened_at.clone();
    // Seed a second entry with the *same* timestamp, placed AFTER "first"
    // in the vec, then record a brand-new "third" which front-inserts.
    store.replace_recents(vec![
        entry("first", &ts, false, false),
        entry("second", &ts, false, false),
    ]);
    store.record_recent_project("third", "~/dev/third").unwrap();
    // "third" front-inserted with a fresh (>= ts) timestamp -> first.
    let names: Vec<String> = store
        .list_recent_projects()
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names[0], "third", "newest open is first even on a timestamp tie");
}

#[test]
fn remove_recent_project_drops_only_that_entry() {
    // GSS-FR-08 (remove).
    let store = GlobalSettingsStore::in_memory();
    store.record_recent_project("acme", "~/dev/acme").unwrap();
    store.record_recent_project("design", "~/dev/design").unwrap();
    store.remove_recent_project("~/dev/design").unwrap();
    let names: Vec<String> = store
        .list_recent_projects()
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(!names.contains(&"design".to_string()));
    assert!(names.contains(&"acme".to_string()));
}

#[test]
fn clear_recent_projects_empties_the_list() {
    // GSS-FR-08 (clear).
    let store = GlobalSettingsStore::in_memory();
    store.record_recent_project("acme", "~/dev/acme").unwrap();
    store.clear_recent_projects().unwrap();
    assert!(store.list_recent_projects().unwrap().is_empty());
}

#[test]
fn pin_then_unpin_toggles_ordering() {
    // GSS-FR-09 / GSS-FR-05.
    let store = GlobalSettingsStore::in_memory();
    store.replace_recents(vec![
        entry("newer", "2026-02-01T00:00:00Z", false, false),
        entry("older", "2026-01-01T00:00:00Z", false, false),
    ]);
    store.pin_recent_project("~/dev/older").unwrap();
    let pinned_first = store.list_recent_projects().unwrap();
    assert_eq!(pinned_first[0].name, "older");
    assert!(pinned_first[0].pinned);

    store.unpin_recent_project("~/dev/older").unwrap();
    let restored = store.list_recent_projects().unwrap();
    assert_eq!(restored[0].name, "newer", "unpin restores recency order");
}

#[test]
fn pin_missing_path_is_a_noop_not_an_error() {
    let store = GlobalSettingsStore::in_memory();
    assert!(store.pin_recent_project("~/dev/does-not-exist").is_ok());
}

#[test]
fn prune_and_cap_applied_through_store_listing() {
    let store = GlobalSettingsStore::in_memory();
    store.replace_recents(vec![
        entry("present", "2026-05-01T00:00:00Z", false, false),
        entry("ghost", "2026-05-02T00:00:00Z", false, true),
    ]);
    let names: Vec<String> = store
        .list_recent_projects()
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["present".to_string()]);
}

#[test]
fn store_methods_return_defaults_and_never_panic_on_fresh_store() {
    // GSS-FR-13 posture: every read yields a typed value (defaults) rather
    // than panicking.
    let store = GlobalSettingsStore::in_memory();
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::System);
    assert!(store.list_recent_projects().unwrap().is_empty());
    assert!(store.list_installed_plugins().unwrap().is_empty());
    assert!(store.list_agent_adapters().unwrap().is_empty());
}
