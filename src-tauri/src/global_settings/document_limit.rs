//! The document size limit of the user-global store (GSS-FR-PCTL,
//! GSS-FR-RVQK).
//!
//! One part of `global_settings.rs`. The value is a whole percent of a model's
//! context window. `AGC-agent-conversations.md` reads it and decides what it
//! limits; this module persists it and interprets nothing.

use serde::{Deserialize, Deserializer};
use tauri::State;

use super::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain};

/// GSS-FR-PCTL: the percent where nothing is stored.
pub const DOCUMENT_LIMIT_PERCENT_DEFAULT: u8 = 30;

/// GSS-FR-RVQK: the typed error of a value outside 1 to 100.
pub const ERR_PERCENT_OUT_OF_RANGE: &str = "percent_out_of_range";

/// GSS-FR-PCTL: read the stored percent so that no stored value can fail the
/// load. A value outside 1 to 100, a negative value, and a value of another
/// type each read as nothing stored, which is 30. A failed field would make the
/// whole file unreadable, and the store then repairs every setting to its
/// default.
pub(super) fn lenient_percent<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u8>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Whole(i64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Whole(percent) if (1..=100).contains(&percent) => Some(percent as u8),
        Stored::Whole(_) | Stored::Other(_) => None,
    })
}

impl GlobalSettingsStore {
    /// GSS-FR-PCTL: the stored percent, or 30 where nothing is stored.
    pub fn load_document_limit_percent(&self) -> Result<u8, String> {
        Ok(self.lock()?.document_limit_percent.unwrap_or(DOCUMENT_LIMIT_PERCENT_DEFAULT))
    }

    /// GSS-FR-PCTL, GSS-FR-RVQK: persist a percent from 1 to 100. A value
    /// outside that range is refused and nothing is written.
    pub fn save_document_limit_percent(&self, percent: i64) -> Result<u8, String> {
        if !(1..=100).contains(&percent) {
            return Err(ERR_PERCENT_OUT_OF_RANGE.to_string());
        }
        let percent = percent as u8;
        let mut state = self.lock()?;
        state.document_limit_percent = Some(percent);
        self.persist(&state)?;
        Ok(percent)
    }
}

/// GSS contract: `"load document limit percent"`.
#[tauri::command]
pub fn load_document_limit_percent(store: State<'_, GlobalSettingsStore>) -> Result<u8, String> {
    store.load_document_limit_percent()
}

/// GSS contract: `"save document limit percent"`.
#[tauri::command]
pub fn save_document_limit_percent(
    percent: i64,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<u8, String> {
    let result = store.save_document_limit_percent(percent);
    match &result {
        Ok(stored) => logging::log_info(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "document size limit saved",
            log_fields! { "percent" => *stored },
        ),
        Err(error) => logging::log_warn(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "document size limit refused",
            log_fields! { "percent" => percent, "reason" => error.as_str() },
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-PCTL: 30 where nothing is stored, and the saved value afterwards,
    // also after a reload of the store.
    #[test]
    fn the_percent_is_thirty_until_one_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(store.load_document_limit_percent().unwrap(), 30);
        assert_eq!(store.save_document_limit_percent(45).unwrap(), 45);
        assert_eq!(store.load_document_limit_percent().unwrap(), 45);
        let reloaded = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(reloaded.load_document_limit_percent().unwrap(), 45);
    }

    // GSS-FR-RVQK: a value outside 1 to 100 is refused and writes nothing.
    #[test]
    fn a_percent_out_of_range_is_refused_and_writes_nothing() {
        let store = GlobalSettingsStore::in_memory();
        store.save_document_limit_percent(60).unwrap();
        for bad in [0, -1, 101, 1_000] {
            assert_eq!(
                store.save_document_limit_percent(bad),
                Err(ERR_PERCENT_OUT_OF_RANGE.to_string())
            );
            assert_eq!(store.load_document_limit_percent().unwrap(), 60);
        }
        assert_eq!(store.save_document_limit_percent(1).unwrap(), 1);
        assert_eq!(store.save_document_limit_percent(100).unwrap(), 100);
    }

    // GSS-FR-PCTL: a stored value outside 1 to 100, negative, or of another type
    // reads as 30, and every other setting of the file survives the load.
    #[test]
    fn a_stored_percent_out_of_range_reads_as_thirty_and_keeps_the_file() {
        for stored in ["0", "101", "300", "-5", "\"x\"", "12.5", "true"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("synthesis.toml");
            std::fs::write(
                &path,
                format!("activeAiApi = \"custom\"\ndocumentLimitPercent = {stored}\n"),
            )
            .unwrap();
            let store = GlobalSettingsStore::with_path(path.clone());
            assert_eq!(store.load_document_limit_percent().unwrap(), 30, "stored {stored}");
            let (_, active) = store.load_ai_api_registry().unwrap();
            assert_eq!(active.as_deref(), Some("custom"), "stored {stored}: the file kept");
        }
    }

    // GSS-FR-PCTL: a stored value inside 1 to 100 reads back as stored.
    #[test]
    fn a_stored_percent_in_range_reads_as_stored() {
        for (stored, read) in [("1", 1), ("30", 30), ("100", 100)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("synthesis.toml");
            std::fs::write(&path, format!("documentLimitPercent = {stored}\n")).unwrap();
            let store = GlobalSettingsStore::with_path(path);
            assert_eq!(store.load_document_limit_percent().unwrap(), read);
        }
    }

    // GSS-FR-PCTL: the percent is written to the user-global file alone and to
    // nothing beside it, so no project gains it.
    #[test]
    fn the_percent_is_written_to_the_global_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        store.save_document_limit_percent(55).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("documentLimitPercent = 55"));
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
        let written: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n != "project")
            .collect();
        assert_eq!(written, vec!["synthesis.toml".to_string()]);
    }

    // GSS-FR-PCTL: the percent is not part of the app-preferences record, so a
    // preferences save leaves it alone.
    #[test]
    fn a_preferences_save_keeps_the_percent() {
        let store = GlobalSettingsStore::in_memory();
        store.save_document_limit_percent(12).unwrap();
        let prefs = store.load_app_preferences().unwrap();
        store.save_app_preferences(prefs).unwrap();
        assert_eq!(store.load_document_limit_percent().unwrap(), 12);
    }
}
