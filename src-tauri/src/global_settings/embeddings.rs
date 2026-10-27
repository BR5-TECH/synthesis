//! The embeddings settings of the user-global store (GSS-FR-MBJV, GSS-FR-DSUG).
//!
//! One part of `global_settings.rs`. `EMB-embeddings-index.md` reads and
//! writes these; this module persists them and interprets neither.

use super::GlobalSettingsStore;
use crate::embeddings::{EmbeddingsRecord, ProjectEmbeddingsMode};

impl GlobalSettingsStore {
    /// GSS-FR-MBJV: the Global switch and the model, off and unset on a fresh
    /// install.
    pub fn load_embeddings_config(&self) -> Result<EmbeddingsRecord, String> {
        Ok(self.lock()?.embeddings.clone())
    }

    /// GSS-FR-MBJV: persist the Global switch and the model.
    pub fn save_embeddings_config(&self, record: &EmbeddingsRecord) -> Result<(), String> {
        let mut state = self.lock()?;
        state.embeddings = record.clone();
        self.persist(&state)
    }

    /// GSS-FR-DSUG: the project's embeddings mode, `use_global` when unset.
    pub fn load_embeddings_mode(&self, key: &str) -> Result<ProjectEmbeddingsMode, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).embeddings_mode.unwrap_or_default())
    }

    /// GSS-FR-DSUG: persist the project's embeddings mode in its slot, keyed by
    /// project (GSS-FR-18), so every worktree of the repository reads it.
    pub fn save_embeddings_mode(
        &self,
        key: &str,
        mode: ProjectEmbeddingsMode,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| slot.embeddings_mode = Some(mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-MBJV, GSS-FR-DSUG: the defaults, and both values survive a
    // reload of `synthesis.toml`, keyed by project.
    #[test]
    fn embeddings_settings_round_trip_and_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(store.load_embeddings_config().unwrap(), EmbeddingsRecord::default());
        assert_eq!(store.load_embeddings_mode("/repo").unwrap(), ProjectEmbeddingsMode::UseGlobal);

        store
            .save_embeddings_config(&EmbeddingsRecord {
                enabled: true,
                model: Some("BGEM3".into()),
            })
            .unwrap();
        store.save_embeddings_mode("/repo", ProjectEmbeddingsMode::Off).unwrap();

        let reloaded = GlobalSettingsStore::with_path(path.clone());
        let record = reloaded.load_embeddings_config().unwrap();
        assert!(record.enabled);
        assert_eq!(record.model.as_deref(), Some("BGEM3"));
        assert_eq!(reloaded.load_embeddings_mode("/repo").unwrap(), ProjectEmbeddingsMode::Off);
        assert_eq!(
            reloaded.load_embeddings_mode("/other").unwrap(),
            ProjectEmbeddingsMode::UseGlobal
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("embeddingsMode = \"off\""), "{text}");
    }
}
