//! The Voice AI provider registry of the user-global store (GSS-FR-CKQG).
//!
//! One part of `global_settings.rs`. `VOP-voice-ai-providers.md` reads and
//! writes these records, and this module interprets none of them.

use super::GlobalSettingsStore;
use crate::voice_ai::VoiceAiRecord;

impl GlobalSettingsStore {
    /// GSS-FR-CKQG: the records and the active provider.
    pub fn load_voice_ai_registry(&self) -> Result<(Vec<VoiceAiRecord>, Option<String>), String> {
        let state = self.lock()?;
        Ok((state.voice_ai_integrations.clone(), state.active_voice_ai.clone()))
    }

    /// GSS-FR-CKQG: replace the records and the active provider.
    pub fn save_voice_ai_registry(
        &self,
        records: Vec<VoiceAiRecord>,
        active: Option<String>,
    ) -> Result<(), String> {
        let mut state = self.lock()?;
        state.voice_ai_integrations = records;
        state.active_voice_ai = active;
        self.persist(&state)
    }
}
