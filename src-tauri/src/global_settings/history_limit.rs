//! The history size limit of the user-global store (GSS-FR-USGD,
//! GSS-FR-RMJH).
//!
//! One part of `global_settings.rs`. The value is a whole percent of a model's
//! context window. `AGC-agent-conversations.md` reads it and decides what it
//! limits; this module persists it and interprets nothing.

use tauri::State;

use super::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain};

/// GSS-FR-USGD: the percent where nothing is stored.
pub const HISTORY_LIMIT_PERCENT_DEFAULT: u8 = 20;

impl GlobalSettingsStore {
    /// GSS-FR-USGD: the stored percent, or 20 where nothing is stored.
    pub fn load_history_limit_percent(&self) -> Result<u8, String> {
        Ok(self.lock()?.history_limit_percent.unwrap_or(HISTORY_LIMIT_PERCENT_DEFAULT))
    }

    /// GSS-FR-USGD, GSS-FR-RMJH: persist a percent from 1 to 100. A value
    /// outside that range is refused and nothing is written.
    pub fn save_history_limit_percent(&self, percent: i64) -> Result<u8, String> {
        if !(1..=100).contains(&percent) {
            return Err(super::ERR_PERCENT_OUT_OF_RANGE.to_string());
        }
        let percent = percent as u8;
        let mut state = self.lock()?;
        state.history_limit_percent = Some(percent);
        self.persist(&state)?;
        Ok(percent)
    }
}

/// GSS contract: `"load history limit percent"`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_history_limit_percent(store: State<'_, GlobalSettingsStore>) -> Result<u8, String> {
    store.load_history_limit_percent()
}

/// GSS contract: `"save history limit percent"`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn save_history_limit_percent(
    percent: i64,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<u8, String> {
    let result = store.save_history_limit_percent(percent);
    match &result {
        Ok(stored) => logging::log_info(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "history size limit saved",
            log_fields! { "percent" => *stored },
        ),
        Err(error) => logging::log_warn(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "history size limit refused",
            log_fields! { "percent" => percent, "reason" => error.as_str() },
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-USGD: 20 where nothing is stored, and the saved value afterwards,
    // also after a reload of the store.
    #[test]
    fn the_percent_is_twenty_until_one_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(store.load_history_limit_percent().unwrap(), 20);
        assert_eq!(store.save_history_limit_percent(45).unwrap(), 45);
        assert_eq!(store.load_history_limit_percent().unwrap(), 45);
        let reloaded = GlobalSettingsStore::with_path(path);
        assert_eq!(reloaded.load_history_limit_percent().unwrap(), 45);
    }

    // GSS-FR-RMJH: a value outside 1 to 100 is refused and writes nothing.
    #[test]
    fn a_percent_out_of_range_is_refused_and_writes_nothing() {
        let store = GlobalSettingsStore::in_memory();
        store.save_history_limit_percent(60).unwrap();
        for bad in [0, -1, 101, 1_000] {
            assert_eq!(
                store.save_history_limit_percent(bad),
                Err(super::super::ERR_PERCENT_OUT_OF_RANGE.to_string())
            );
            assert_eq!(store.load_history_limit_percent().unwrap(), 60);
        }
        assert_eq!(store.save_history_limit_percent(1).unwrap(), 1);
        assert_eq!(store.save_history_limit_percent(100).unwrap(), 100);
    }

    // GSS-FR-USGD: a stored value outside 1 to 100, negative, or of another type
    // reads as 20, and every other setting of the file survives the load.
    #[test]
    fn a_stored_percent_out_of_range_reads_as_twenty_and_keeps_the_file() {
        for stored in ["0", "101", "-5", "\"x\"", "12.5", "true"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("synthesis.toml");
            std::fs::write(
                &path,
                format!("activeAiApi = \"custom\"\nhistoryLimitPercent = {stored}\n"),
            )
            .unwrap();
            let store = GlobalSettingsStore::with_path(path);
            assert_eq!(store.load_history_limit_percent().unwrap(), 20, "stored {stored}");
            let (_, active) = store.load_ai_api_registry().unwrap();
            assert_eq!(active.as_deref(), Some("custom"), "stored {stored}: the file kept");
        }
    }

    // GSS-FR-USGD: the percent is a value of its own, apart from the document
    // size limit and from the app-preferences record.
    #[test]
    fn the_percent_is_apart_from_the_document_limit_and_the_preferences() {
        let store = GlobalSettingsStore::in_memory();
        store.save_history_limit_percent(12).unwrap();
        store.save_document_limit_percent(40).unwrap();
        let prefs = store.load_app_preferences().unwrap();
        store.save_app_preferences(prefs).unwrap();
        assert_eq!(store.load_history_limit_percent().unwrap(), 12);
        assert_eq!(store.load_document_limit_percent().unwrap(), 40);
    }

    // GSS-FR-USGD: the percent is written to the user-global file alone.
    #[test]
    fn the_percent_is_written_to_the_global_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        store.save_history_limit_percent(33).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("historyLimitPercent = 33"));
    }
}
