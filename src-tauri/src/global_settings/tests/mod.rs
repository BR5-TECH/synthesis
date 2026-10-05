//! The unit tests of `specifications/core/GSS-global-settings-storage.md`.
//!
//! This module holds the imports and the shared helper that every topic file
//! uses. Each topic file below covers one part of the store.

use super::*;
use tempfile::TempDir;

fn entry(name: &str, ts: &str, pinned: bool, missing: bool) -> RecentProjectEntry {
    RecentProjectEntry {
        name: name.to_string(),
        path: format!("~/dev/{name}"),
        last_opened_at: ts.to_string(),
        pinned,
        missing,
    }
}

mod app_preferences;
mod font_settings;
mod persistence;
mod plugin_registry;
mod project_slot;
mod recents;
mod registries;
mod switches;
mod viewer_modes;
mod wire_shape;
