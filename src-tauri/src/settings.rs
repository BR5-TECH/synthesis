//! User-global settings commands (GSS-global-settings-storage.md).
//!
//! Thin wrappers over `GlobalSettingsStore` (managed Tauri state). The store and
//! its pure ordering/pruning/capping logic live in `global_settings.rs` and are
//! unit-tested there without a Tauri runtime; these wrappers only thread the
//! managed state through. Operation names match the GSS spec's Contract surface
//! byte-for-byte (modulo Tauri's snake/camel auto-conversion).

use tauri::State;

use crate::global_settings::{
    AppPreferences, GlobalSettingsStore, InstalledAdapter, InstalledPlugin, RecentProjectEntry,
};

#[tauri::command]
pub fn load_app_preferences(
    store: State<'_, GlobalSettingsStore>,
) -> Result<AppPreferences, String> {
    store.load_app_preferences()
}

#[tauri::command]
pub fn save_app_preferences(
    preferences: AppPreferences,
    store: State<'_, GlobalSettingsStore>,
) -> Result<(), String> {
    store.save_app_preferences(preferences)
}

#[tauri::command]
pub fn list_recent_projects(
    store: State<'_, GlobalSettingsStore>,
) -> Result<Vec<RecentProjectEntry>, String> {
    store.list_recent_projects()
}

#[tauri::command]
pub fn remove_recent_project(
    path: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<(), String> {
    store.remove_recent_project(&path)
}

#[tauri::command]
pub fn clear_recent_projects(store: State<'_, GlobalSettingsStore>) -> Result<(), String> {
    store.clear_recent_projects()
}

#[tauri::command]
pub fn pin_recent_project(
    path: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<(), String> {
    store.pin_recent_project(&path)
}

#[tauri::command]
pub fn unpin_recent_project(
    path: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<(), String> {
    store.unpin_recent_project(&path)
}

#[tauri::command]
pub fn list_installed_plugins(
    store: State<'_, GlobalSettingsStore>,
) -> Result<Vec<InstalledPlugin>, String> {
    store.list_installed_plugins()
}

#[tauri::command]
pub fn install_plugin(
    source: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<InstalledPlugin, String> {
    store.install_plugin(&source)
}

#[tauri::command]
pub fn uninstall_plugin(id: String, store: State<'_, GlobalSettingsStore>) -> Result<(), String> {
    store.uninstall_plugin(&id)
}

#[tauri::command]
pub fn list_agent_adapters(
    store: State<'_, GlobalSettingsStore>,
) -> Result<Vec<InstalledAdapter>, String> {
    store.list_agent_adapters()
}

#[tauri::command]
pub fn install_adapter(
    source: String,
    store: State<'_, GlobalSettingsStore>,
) -> Result<InstalledAdapter, String> {
    store.install_adapter(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_settings_command_functions_are_in_scope() {
        // Compile-time references: renaming/removing any GSS command without
        // updating `generate_handler!` / `COMMAND_NAMES` fails to compile here.
        let _ = load_app_preferences;
        let _ = save_app_preferences;
        let _ = list_recent_projects;
        let _ = remove_recent_project;
        let _ = clear_recent_projects;
        let _ = pin_recent_project;
        let _ = unpin_recent_project;
        let _ = list_installed_plugins;
        let _ = install_plugin;
        let _ = uninstall_plugin;
        let _ = list_agent_adapters;
        let _ = install_adapter;
    }
}
