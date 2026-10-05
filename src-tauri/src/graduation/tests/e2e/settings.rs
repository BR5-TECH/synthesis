//! The project settings a scenario runs under (GTE-FR-KAZX), and the guarded
//! filesystem handle the harness reads and writes project files through.

use std::path::{Path, PathBuf};

use super::super::Fixture;

/// GTE-FR-KAZX: a project setting a scenario runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    /// PSS-FR-TQMV, in milliseconds.
    ExecutionTimeoutMs,
    /// PSS-FR-WPKS. The graduation loop counts passes with it.
    PassBudget,
    /// PSS-FR-JRWC: how many runs may hold a project slot at once.
    GraduationConcurrencyLimit,
}

fn key_of(setting: Setting) -> &'static str {
    match setting {
        Setting::ExecutionTimeoutMs => "executionTimeoutMs",
        Setting::PassBudget => "retryBudget",
        Setting::GraduationConcurrencyLimit => "graduationConcurrencyLimit",
    }
}

pub(crate) fn write_setting(fx: &Fixture, setting: Setting, value: Option<u64>) {
    write_setting_at(&fx.root(), setting, value);
}

/// GTE-FR-GBLI: change one setting of a scenario's project after its run has
/// come to rest, so the dispatch that resumes it reads the new value.
pub(crate) fn set_setting(fx: &Fixture, setting: Setting, value: u64) {
    write_setting(fx, setting, Some(value));
}

pub(crate) fn write_setting_at(root: &Path, setting: Setting, value: Option<u64>) {
    write_raw_setting_at(root, setting, value.map(|v| v as i64));
}

pub(crate) fn write_raw_setting(fx: &Fixture, setting: Setting, value: i64) {
    write_raw_setting_at(&fx.root(), setting, Some(value));
}

/// The store is written through `toml` directly rather than through
/// `save_project_config`, so a scenario can put a value there that the payload
/// itself refuses — which is what PSS-FR-ZLCF's repair-on-read is about.
fn write_raw_setting_at(root: &Path, setting: Setting, value: Option<i64>) {
    let fs = guard(root);
    let path = root.join(".synthesis").join("project.toml");
    let mut table: toml::Table = fs
        .read_text(&path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    match value {
        Some(value) => {
            table.insert(key_of(setting).to_string(), toml::Value::Integer(value));
        }
        None => {
            table.remove(key_of(setting));
        }
    }
    // The write creates its own parents (FSA-FR-05).
    fs.write_text_atomic(&path, &toml::to_string(&table).expect("a table"))
        .expect("the settings file is writable");
}

/// A filesystem instance allowed at one root and nowhere else.
///
/// The project files this harness reads and writes go through one, on the
/// terms FSA-FR-19 binds the backend to.
pub(crate) fn guard(root: &Path) -> crate::fs::FsAccess {
    let root: PathBuf = root.to_path_buf();
    crate::fs::FsAccess::builder()
        .allow_root(&root)
        .build()
        .expect("a real directory")
}

/// The revision a working copy stands at.
pub(crate) fn head_of(worktree: &Path) -> Option<String> {
    git2::Repository::open(worktree)
        .ok()?
        .head()
        .ok()?
        .peel_to_commit()
        .ok()
        .map(|commit| commit.id().to_string())
}
