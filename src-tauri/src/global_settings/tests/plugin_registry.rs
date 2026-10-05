//! The installed-plugin registry, its enablement cascade, and the
//! installed-adapter registry.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Store: plugin registry + cascade ------------------------------

#[test]
fn install_then_list_then_uninstall_plugin() {
    let store = GlobalSettingsStore::in_memory();
    assert!(store.list_installed_plugins().unwrap().is_empty());
    let installed = store
        .install_plugin("git@github.com:org/markdown-toolbar.git")
        .unwrap();
    assert_eq!(installed.id, "markdown-toolbar");
    assert_eq!(store.list_installed_plugins().unwrap().len(), 1);
    store.uninstall_plugin("markdown-toolbar").unwrap();
    assert!(store.list_installed_plugins().unwrap().is_empty());
}

#[test]
fn install_plugin_empty_source_errors() {
    let store = GlobalSettingsStore::in_memory();
    assert!(store.install_plugin("   ").is_err());
}

#[test]
fn install_plugin_source_that_derives_empty_id_errors() {
    assert_eq!(derive_id_from_source(".git"), "");
    let store = GlobalSettingsStore::in_memory();
    assert!(store.install_plugin(".git").is_err());
    assert!(store.list_installed_plugins().unwrap().is_empty());
}

#[test]
fn install_plugin_is_idempotent_on_id() {
    let store = GlobalSettingsStore::in_memory();
    store.install_plugin("https://x/markdown-toolbar").unwrap();
    store.install_plugin("https://y/markdown-toolbar").unwrap();
    let plugins = store.list_installed_plugins().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].source, "https://y/markdown-toolbar");
}

#[test]
fn uninstall_plugin_cascades_disable_scoped_to_that_id_only() {
    // GSS-FR-11: uninstalling disables that plugin in the open project —
    // and ONLY that one (a "clear all" cascade would be caught here).
    let store = GlobalSettingsStore::in_memory();
    store.install_plugin("https://x/git-pr").unwrap();
    store.install_plugin("https://x/other").unwrap();
    store.set_open_project_plugin_enabled("git-pr", true).unwrap();
    store.set_open_project_plugin_enabled("other", true).unwrap();

    store.uninstall_plugin("git-pr").unwrap();

    let remaining: Vec<String> = store
        .list_installed_plugins()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(remaining, vec!["other".to_string()]);
    assert!(!store.is_open_project_plugin_enabled("git-pr").unwrap());
    assert!(
        store.is_open_project_plugin_enabled("other").unwrap(),
        "the cascade must NOT disable other plugins' enablement"
    );
}

// ---- Store: adapter registry ---------------------------------------

#[test]
fn install_then_list_adapter() {
    let store = GlobalSettingsStore::in_memory();
    assert!(store.list_agent_adapters().unwrap().is_empty());
    let a = store.install_adapter("https://x/claude-code").unwrap();
    assert_eq!(a.id, "claude-code");
    assert_eq!(store.list_agent_adapters().unwrap().len(), 1);
}

#[test]
fn install_adapter_empty_source_errors() {
    let store = GlobalSettingsStore::in_memory();
    assert!(store.install_adapter("").is_err());
}
