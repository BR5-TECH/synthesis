//! The model downgrade suppressions of the per-project slot (GSS-FR-DAKR ..
//! GSS-FR-UUET).
//!
//! One part of `global_settings.rs`. `crate::model_downgrades` makes the rules
//! and owns the commands; this module persists them and drops the expired ones.

use super::GlobalSettingsStore;
use crate::model_downgrades::{in_force, ModelDowngradeSuppression};

/// GSS-FR-PCPW: what one read of the rules gives.
#[derive(Debug, Default)]
pub struct LoadedSuppressions {
    /// The rules in force.
    pub rules: Vec<ModelDowngradeSuppression>,
    /// How many expired rules the read removed.
    pub removed: usize,
    /// Why the store could not be written after the removal, if it could not.
    pub cleanup_error: Option<String>,
}

impl GlobalSettingsStore {
    /// GSS-FR-PCPW: the rules of the project that are in force at `now` (fixed-width
    /// RFC 3339 UTC). The expired rules are removed under the same lock as the
    /// read, so a save made at the same time is never lost. The store is written
    /// only when a rule was removed. A failed write does not fail the read: the
    /// rules in force still mute their pairs, and the error is returned beside them.
    pub fn load_model_downgrade_suppressions(
        &self,
        key: &str,
        now: &str,
    ) -> Result<LoadedSuppressions, String> {
        let mut state = self.lock()?;
        let rules = if key.is_empty() {
            &mut state.default_project.model_downgrade_suppressions
        } else {
            match state.projects.get_mut(key) {
                Some(slot) => &mut slot.model_downgrade_suppressions,
                None => return Ok(LoadedSuppressions::default()),
            }
        };
        let before = rules.len();
        rules.retain(|rule| in_force(rule, now));
        let loaded = LoadedSuppressions {
            rules: rules.clone(),
            removed: before - rules.len(),
            cleanup_error: None,
        };
        if loaded.removed == 0 {
            return Ok(loaded);
        }
        let cleanup_error = self.persist(&state).err();
        Ok(LoadedSuppressions { cleanup_error, ..loaded })
    }

    /// GSS-FR-DAKR / GSS-FR-UUET: save a rule, in place of a rule of the same
    /// pair. Keyed by project (GSS-FR-18), so every worktree reads it.
    pub fn save_model_downgrade_suppression(
        &self,
        key: &str,
        rule: &ModelDowngradeSuppression,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| {
            let rules = &mut slot.model_downgrade_suppressions;
            // GSS-FR-HECI: the pair is matched as two exact strings.
            rules.retain(|r| {
                r.requested_model != rule.requested_model || r.served_model != rule.served_model
            });
            rules.push(rule.clone());
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_downgrades::{rule_for, SuppressionDuration};

    const NOW: i64 = 1_800_000_000;

    fn at(offset: i64) -> String {
        crate::notes::format_rfc3339_utc(NOW + offset)
    }

    // GSS-FR-DAKR, GSS-FR-UUET: a rule survives a reload of `synthesis.toml`,
    // keyed by project, and a second rule of the same pair replaces the first.
    #[test]
    fn rules_round_trip_per_project_and_replace_by_pair() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        let hour = rule_for("opus-5", "opus-4", SuppressionDuration::Hour, NOW).unwrap();
        let forever = rule_for("opus-5", "opus-4", SuppressionDuration::Forever, NOW).unwrap();
        store.save_model_downgrade_suppression("/repo", &hour).unwrap();
        store.save_model_downgrade_suppression("/repo", &forever).unwrap();

        let reloaded = GlobalSettingsStore::with_path(path);
        let loaded = reloaded.load_model_downgrade_suppressions("/repo", &at(0)).unwrap();
        assert_eq!(loaded.rules, vec![forever]);
        assert_eq!(loaded.removed, 0);
        let other = reloaded.load_model_downgrade_suppressions("/other", &at(0)).unwrap();
        assert!(other.rules.is_empty());
    }

    // GSS-FR-HECI: two pairs that differ only in case or a suffix are two rules.
    #[test]
    fn pairs_are_exact_strings() {
        let dir = tempfile::tempdir().unwrap();
        let store = GlobalSettingsStore::with_path(dir.path().join("synthesis.toml"));
        for served in ["Opus-4", "opus-4", "opus-4-20250101"] {
            let rule = rule_for("opus-5", served, SuppressionDuration::Day, NOW).unwrap();
            store.save_model_downgrade_suppression("/repo", &rule).unwrap();
        }
        let loaded = store.load_model_downgrade_suppressions("/repo", &at(0)).unwrap();
        assert_eq!(loaded.rules.len(), 3);
    }

    // GSS-FR-PCPW: an expired rule is not returned, and the read removes it from
    // the store.
    #[test]
    fn expired_rules_are_ignored_and_removed_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        let hour = rule_for("a", "b", SuppressionDuration::Hour, NOW).unwrap();
        let week = rule_for("a", "c", SuppressionDuration::Week, NOW).unwrap();
        store.save_model_downgrade_suppression("/repo", &hour).unwrap();
        store.save_model_downgrade_suppression("/repo", &week).unwrap();

        let loaded = store.load_model_downgrade_suppressions("/repo", &at(3_600)).unwrap();
        assert_eq!(loaded.rules, vec![week.clone()]);
        assert_eq!(loaded.removed, 1);
        assert_eq!(loaded.cleanup_error, None);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("servedModel = \"c\""), "{text}");
        assert!(!text.contains("servedModel = \"b\""), "{text}");

        let later = GlobalSettingsStore::with_path(path)
            .load_model_downgrade_suppressions("/repo", &at(3_601))
            .unwrap();
        assert_eq!(later.rules, vec![week]);
    }
}
