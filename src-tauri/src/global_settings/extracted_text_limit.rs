//! The extracted text limit of the user-global store (GSS-FR-IRYZ,
//! GSS-FR-JGZY, GSS-FR-GRWY).
//!
//! One part of `global_settings.rs`. The value is a whole number of
//! kilobytes. `CMS-comments-storage.md` CMS-FR-YSRD reads it at each append
//! and decides what it refuses; this module persists it and interprets nothing.

use serde::{Deserialize, Deserializer};
use tauri::State;

use super::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain};

/// GSS-FR-IRYZ: the kilobytes where nothing is stored.
pub const EXTRACTED_TEXT_LIMIT_KB_DEFAULT: u32 = 1024;
/// GSS-FR-IRYZ: the smallest limit.
pub const EXTRACTED_TEXT_LIMIT_KB_MIN: u32 = 1;
/// GSS-FR-IRYZ: the largest limit, the attachment size bound of 10 MiB.
pub const EXTRACTED_TEXT_LIMIT_KB_MAX: u32 = 10240;
/// GSS-FR-JGZY: a save outside 1 to 10240.
pub const ERR_KILOBYTES_OUT_OF_RANGE: &str = "kilobytes_out_of_range";

fn in_range(kb: i64) -> bool {
    (i64::from(EXTRACTED_TEXT_LIMIT_KB_MIN)..=i64::from(EXTRACTED_TEXT_LIMIT_KB_MAX)).contains(&kb)
}

/// GSS-FR-GRWY: a stored value that is not a whole number from 1 to 10240
/// reads as unset, so the rest of the file still loads.
pub(super) fn lenient_kilobytes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Whole(i64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Whole(kb) if in_range(kb) => Some(kb as u32),
        Stored::Whole(_) | Stored::Other(_) => None,
    })
}

impl GlobalSettingsStore {
    /// GSS-FR-IRYZ: the stored kilobytes, or 1024 where nothing is stored.
    pub fn load_extracted_text_limit_kb(&self) -> Result<u32, String> {
        Ok(self.lock()?.extracted_text_limit_kb.unwrap_or(EXTRACTED_TEXT_LIMIT_KB_DEFAULT))
    }

    /// GSS-FR-IRYZ, GSS-FR-JGZY: persist kilobytes from 1 to 10240. A value
    /// outside that range is refused and nothing is written.
    pub fn save_extracted_text_limit_kb(&self, kb: i64) -> Result<u32, String> {
        if !in_range(kb) {
            return Err(ERR_KILOBYTES_OUT_OF_RANGE.to_string());
        }
        let kb = kb as u32;
        let mut state = self.lock()?;
        state.extracted_text_limit_kb = Some(kb);
        self.persist(&state)?;
        Ok(kb)
    }
}

/// GSS contract: `"load extracted text limit kb"`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn load_extracted_text_limit_kb(store: State<'_, GlobalSettingsStore>) -> Result<u32, String> {
    store.load_extracted_text_limit_kb()
}

/// GSS contract: `"save extracted text limit kb"`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn save_extracted_text_limit_kb(
    kb: i64,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<u32, String> {
    let result = store.save_extracted_text_limit_kb(kb);
    match &result {
        Ok(stored) => logging::log_info(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "extracted text limit saved",
            log_fields! { "kilobytes" => *stored },
        ),
        Err(error) => logging::log_warn(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "extracted text limit refused",
            log_fields! { "kilobytes" => kb, "reason" => error.as_str() },
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-IRYZ: 1024 where nothing is stored, and the saved value
    // afterwards, also after a reload of the store.
    #[test]
    fn the_limit_is_1024_until_one_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(store.load_extracted_text_limit_kb().unwrap(), 1024);
        assert_eq!(store.save_extracted_text_limit_kb(2048).unwrap(), 2048);
        assert_eq!(store.load_extracted_text_limit_kb().unwrap(), 2048);
        let reloaded = GlobalSettingsStore::with_path(path);
        assert_eq!(reloaded.load_extracted_text_limit_kb().unwrap(), 2048);
    }

    // GSS-FR-JGZY: a value outside 1 to 10240 is refused and writes nothing;
    // both ends of the range are accepted.
    #[test]
    fn a_limit_out_of_range_is_refused_and_writes_nothing() {
        let store = GlobalSettingsStore::in_memory();
        store.save_extracted_text_limit_kb(512).unwrap();
        for bad in [0, -1, 10241, 1_000_000] {
            assert_eq!(
                store.save_extracted_text_limit_kb(bad),
                Err(ERR_KILOBYTES_OUT_OF_RANGE.to_string())
            );
            assert_eq!(store.load_extracted_text_limit_kb().unwrap(), 512);
        }
        assert_eq!(store.save_extracted_text_limit_kb(1).unwrap(), 1);
        assert_eq!(store.save_extracted_text_limit_kb(10240).unwrap(), 10240);
    }

    // GSS-FR-GRWY: a stored value out of range or of another type reads as
    // 1024, and every other setting of the file survives the load.
    #[test]
    fn a_stored_limit_out_of_range_reads_as_1024_and_keeps_the_file() {
        for stored in ["0", "10241", "-5", "\"x\"", "12.5", "true"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("synthesis.toml");
            std::fs::write(
                &path,
                format!("activeAiApi = \"custom\"\nextractedTextLimitKb = {stored}\n"),
            )
            .unwrap();
            let store = GlobalSettingsStore::with_path(path);
            assert_eq!(store.load_extracted_text_limit_kb().unwrap(), 1024, "stored {stored}");
            let (_, active) = store.load_ai_api_registry().unwrap();
            assert_eq!(active.as_deref(), Some("custom"), "stored {stored}: the file kept");
        }
    }

    // GSS-FR-IRYZ: the limit is a value of its own, apart from the other
    // limits and from the app-preferences record, in the global file alone.
    #[test]
    fn the_limit_is_apart_from_the_other_limits_and_the_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        store.save_extracted_text_limit_kb(300).unwrap();
        store.save_history_limit_percent(12).unwrap();
        let prefs = store.load_app_preferences().unwrap();
        store.save_app_preferences(prefs).unwrap();
        assert_eq!(store.load_extracted_text_limit_kb().unwrap(), 300);
        assert_eq!(store.load_history_limit_percent().unwrap(), 12);
        assert!(std::fs::read_to_string(&path).unwrap().contains("extractedTextLimitKb = 300"));
    }
}
