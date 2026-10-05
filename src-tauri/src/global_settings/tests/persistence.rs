//! Persistence to disk: the first write, the reload, and the repair of a
//! malformed file.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Disk persistence (GSS-FR-01 / FR-02 / FR-03 / TS-01 / TS-02) ---

#[test]
fn fresh_path_has_no_file_and_lists_empty() {
    // GSS-FR-01, GSS-FR-05: fresh machine, no synthesis.toml yet.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    assert!(!path.exists());
    let store = GlobalSettingsStore::with_path(path.clone());
    assert!(store.list_recent_projects().unwrap().is_empty());
    // No mutation yet -> file still absent.
    assert!(!path.exists());
}

#[test]
fn first_save_creates_file_and_state_survives_reload() {
    // GSS-FR-01, GSS-FR-05 (file created on first save) + GSS-FR-04 (relaunch).
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_app_preferences(AppPreferences { theme: Theme::Dark, ..Default::default() })
            .unwrap();
        store.record_recent_project("acme", "~/dev/acme").unwrap();
        store.install_plugin("https://x/git-pr").unwrap();
        store.install_adapter("https://x/claude-code").unwrap();
        assert!(path.exists(), "first save must create synthesis.toml");
    }

    // Simulate relaunch: a brand-new store reading the same file.
    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(reloaded.load_app_preferences().unwrap().theme, Theme::Dark);
    let recents = reloaded.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].name, "acme");
    assert_eq!(reloaded.list_installed_plugins().unwrap().len(), 1);
    assert_eq!(reloaded.list_agent_adapters().unwrap().len(), 1);
}

#[test]
fn malformed_file_repairs_to_defaults_without_panic() {
    // GSS-FR-13 (disk half): a corrupt synthesis.toml loads as
    // defaults, and the next save overwrites it cleanly.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(&path, b"this is not valid toml = = =\n").unwrap();

    let store = GlobalSettingsStore::with_path(path.clone());
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::System);
    assert!(store.list_recent_projects().unwrap().is_empty());

    // Next save repairs the file to a valid, reloadable state.
    store
        .save_app_preferences(AppPreferences { theme: Theme::Light, ..Default::default() })
        .unwrap();
    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(reloaded.load_app_preferences().unwrap().theme, Theme::Light);
}

#[test]
fn pinned_and_missing_flags_survive_reload() {
    // GSS-FR-05 / FR-09 persistence: the additive flags round-trip through
    // synthesis.toml (guards against a future `#[serde(skip)]` slip).
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store.replace_recents(vec![entry("pinme", "2026-01-01T00:00:00Z", true, true)]);
        // replace_recents bypasses persist; pin to force a write.
        store.pin_recent_project("~/dev/pinme").unwrap();
    }
    let reloaded = GlobalSettingsStore::with_path(path);
    let list = reloaded.list_recent_projects().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].pinned, "pinned flag must persist");
    assert!(list[0].missing, "missing flag must persist for a pinned entry");
}

#[test]
fn record_recent_project_persists_across_reload() {
    // The exact bug the user hit: opening a project must show up in recents
    // on the next launch.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .record_recent_project("my-real-project", "/Users/me/code/my-real-project")
            .unwrap();
    }
    let reloaded = GlobalSettingsStore::with_path(path);
    let recents = reloaded.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].name, "my-real-project");
    assert_eq!(recents[0].path, "/Users/me/code/my-real-project");
}
