//! The two switches of the embeddings index (EMB-FR-WQGF, EMB-FR-DWJB) and the
//! records that persist them in `GSS-global-settings-storage.md`.

use serde::{Deserialize, Serialize};

/// GSS-FR-MBJV: the user-global embeddings record of `synthesis.toml`.
///
/// `enabled` is false and `model` unset on a fresh install (EMB-FR-VXIS,
/// EMB-FR-XTLI). The record holds no credential.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct EmbeddingsRecord {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl EmbeddingsRecord {
    /// The selected model id, or the default model when none was chosen
    /// (EMB-FR-XTLI).
    pub fn model_id(&self) -> String {
        self.model.clone().unwrap_or_else(|| super::catalog::DEFAULT_MODEL_ID.to_string())
    }
}

/// GSS-FR-DSUG: the per-project embeddings mode. `use_global` when unset.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectEmbeddingsMode {
    #[default]
    UseGlobal,
    On,
    Off,
}

/// EMB-FR-WQGF: whether embeddings work for a project. The Global switch is a
/// master switch, so `On` does nothing while it is off.
pub fn embeddings_work(global_enabled: bool, mode: ProjectEmbeddingsMode) -> bool {
    global_enabled && mode != ProjectEmbeddingsMode::Off
}

/// The settings as the Global Indexing section reads and writes them: the
/// model is always named, the default standing in for an unset one.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingsSettings {
    pub enabled: bool,
    pub model: String,
}

impl From<&EmbeddingsRecord> for EmbeddingsSettings {
    fn from(record: &EmbeddingsRecord) -> Self {
        EmbeddingsSettings { enabled: record.enabled, model: record.model_id() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // EMB-FR-WQGF: the three project modes under the Global master switch.
    #[test]
    fn global_switch_is_a_master_switch() {
        use ProjectEmbeddingsMode::*;
        assert!(!embeddings_work(false, On));
        assert!(!embeddings_work(false, UseGlobal));
        assert!(!embeddings_work(false, Off));
        assert!(embeddings_work(true, On));
        assert!(embeddings_work(true, UseGlobal));
        assert!(!embeddings_work(true, Off));
    }

    // EMB-FR-VXIS, EMB-FR-XTLI, GSS-FR-MBJV, GSS-FR-DSUG: the defaults of a
    // fresh install.
    #[test]
    fn a_fresh_record_is_off_with_the_default_model() {
        let record = EmbeddingsRecord::default();
        assert!(!record.enabled);
        assert_eq!(record.model_id(), "BGESmallENV15");
        assert_eq!(ProjectEmbeddingsMode::default(), ProjectEmbeddingsMode::UseGlobal);
        let wire = serde_json::to_value(ProjectEmbeddingsMode::UseGlobal).unwrap();
        assert_eq!(wire, serde_json::json!("use_global"));
    }
}
