//! Model downgrades: the event a turn emits when a provider serves another
//! model than the request named (AGC-FR-UVKF), and the suppression rules the
//! author saves from its toast (GSS-FR-DAKR .. GSS-FR-UUET).
//!
//! The rules live in the per-project slot of the user-global store. This
//! module holds their shape, the expiry arithmetic, and the two commands. The
//! decision to raise is the main window's (NTF-FR-CYTU).

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::global_settings::GlobalSettingsStore;
use crate::logging::{log_info, log_warn, Domain, BUFFER};

/// AGC-FR-UVKF. Kebab-case because Tauri rejects an event name with a space.
/// Matches `src/events.ts` byte-for-byte.
pub const MODEL_DOWNGRADE_DETECTED: &str = "model-downgrade-detected";

/// Every command here answers only while a project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// GSS-FR-UUET: a blank model id names no pair.
pub const ERR_INVALID_MODEL_ID: &str = "invalid_model_id";

/// AGC-FR-PNHM: what the event carries. The two ids, the provider, and where
/// the work runs. No comment, no message text, no prompt, and no key.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDowngrade {
    /// `agent` or `graduation`.
    pub source: &'static str,
    pub requested_model: String,
    pub served_model: String,
    pub provider: String,
    /// The key of the per-project slot (GSS-FR-18).
    pub project_key: String,
    /// The absolute path of the worktree the work runs in.
    pub worktree: String,
    /// Source `agent`: the conversation of the turn.
    pub origin: Option<crate::agent_conversations::ConversationOrigin>,
    /// Source `graduation`: the run (GRL-FR-OPRK).
    pub run_id: Option<String>,
}

/// GSS-FR-DAKR: one suppression rule. `expires_at` is fixed-width RFC 3339
/// UTC, so two times compare as strings. `None` is Forever.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDowngradeSuppression {
    pub requested_model: String,
    pub served_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// GSS-FR-UUET: the four durations of the Suppress menu (NTF-FR-JXQL).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionDuration {
    Hour,
    Day,
    Week,
    Forever,
}

impl SuppressionDuration {
    /// The length of the rule in seconds, or `None` for Forever.
    fn seconds(self) -> Option<i64> {
        match self {
            Self::Hour => Some(3_600),
            Self::Day => Some(86_400),
            Self::Week => Some(7 * 86_400),
            Self::Forever => None,
        }
    }
}

/// GSS-FR-PCPW: whether a rule is still in force at `now`, both fixed-width
/// RFC 3339 UTC.
pub fn in_force(rule: &ModelDowngradeSuppression, now: &str) -> bool {
    rule.expires_at.as_deref().is_none_or(|expiry| expiry > now)
}

/// GSS-FR-UUET: the rule for a pair and a duration, made at `now_unix`.
pub fn rule_for(
    requested_model: &str,
    served_model: &str,
    duration: SuppressionDuration,
    now_unix: i64,
) -> Result<ModelDowngradeSuppression, String> {
    if requested_model.trim().is_empty() || served_model.trim().is_empty() {
        return Err(ERR_INVALID_MODEL_ID.to_string());
    }
    Ok(ModelDowngradeSuppression {
        requested_model: requested_model.to_string(),
        served_model: served_model.to_string(),
        expires_at: duration
            .seconds()
            .map(|seconds| crate::notes::format_rfc3339_utc(now_unix + seconds)),
    })
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `"load model downgrade suppressions"` (GSS-FR-PCPW).
#[tauri::command]
pub fn load_model_downgrade_suppressions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<ModelDowngradeSuppression>, String> {
    let project = app.state::<crate::project::ProjectState>();
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let now = crate::notes::format_rfc3339_utc(now_unix());
    let loaded = app
        .state::<GlobalSettingsStore>()
        .load_model_downgrade_suppressions(&key, &now)
        .inspect_err(|error| {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "model downgrade suppressions not read",
            crate::log_fields! { "error" => error },
        );
    })?;
    if loaded.removed > 0 {
        log_info(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "expired model downgrade suppressions removed",
            crate::log_fields! { "removed" => loaded.removed },
        );
    }
    // GSS-FR-PCPW: the read succeeded, so the rules in force still apply.
    if let Some(error) = &loaded.cleanup_error {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "expired model downgrade suppressions not written",
            crate::log_fields! { "error" => error },
        );
    }
    Ok(loaded.rules)
}

/// `"save model downgrade suppression"` (GSS-FR-UUET).
#[tauri::command]
pub fn save_model_downgrade_suppression<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    requested_model: String,
    served_model: String,
    duration: SuppressionDuration,
) -> Result<ModelDowngradeSuppression, String> {
    let project = app.state::<crate::project::ProjectState>();
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let rule = rule_for(&requested_model, &served_model, duration, now_unix())?;
    let saved = app.state::<GlobalSettingsStore>().save_model_downgrade_suppression(&key, &rule);
    match &saved {
        Ok(()) => log_info(
            &app,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "model downgrade suppression saved",
            crate::log_fields! {
                "requestedModel" => &rule.requested_model,
                "servedModel" => &rule.served_model,
                "duration" => serde_json::to_value(duration).unwrap_or_default(),
            },
        ),
        Err(error) => log_warn(
            &app,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "model downgrade suppression not saved",
            crate::log_fields! { "error" => error },
        ),
    }
    saved.map(|()| rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    // GSS-FR-UUET: each duration sets its expiry from now, and Forever sets none.
    #[test]
    fn each_duration_sets_its_expiry() {
        let at = |d| rule_for("a", "b", d, NOW).unwrap().expires_at;
        assert_eq!(
            at(SuppressionDuration::Hour),
            Some(crate::notes::format_rfc3339_utc(NOW + 3_600))
        );
        assert_eq!(
            at(SuppressionDuration::Day),
            Some(crate::notes::format_rfc3339_utc(NOW + 86_400))
        );
        assert_eq!(
            at(SuppressionDuration::Week),
            Some(crate::notes::format_rfc3339_utc(NOW + 7 * 86_400))
        );
        assert_eq!(at(SuppressionDuration::Forever), None);
    }

    // GSS-FR-UUET: a blank model id is refused.
    #[test]
    fn blank_model_id_is_refused() {
        assert_eq!(
            rule_for(" ", "b", SuppressionDuration::Hour, NOW).unwrap_err(),
            ERR_INVALID_MODEL_ID
        );
        assert_eq!(
            rule_for("a", "", SuppressionDuration::Forever, NOW).unwrap_err(),
            ERR_INVALID_MODEL_ID
        );
    }

    // GSS-FR-PCPW: a rule is in force until its expiry, and Forever always.
    #[test]
    fn a_rule_is_in_force_until_it_expires() {
        let now = crate::notes::format_rfc3339_utc(NOW);
        let rule = rule_for("a", "b", SuppressionDuration::Hour, NOW).unwrap();
        assert!(in_force(&rule, &now));
        assert!(!in_force(&rule, &crate::notes::format_rfc3339_utc(NOW + 3_600)));
        let forever = rule_for("a", "b", SuppressionDuration::Forever, NOW).unwrap();
        assert!(in_force(&forever, &crate::notes::format_rfc3339_utc(NOW + 100 * 86_400)));
    }

    // AGC-FR-PNHM: the payload carries the ids, the provider, and the place of
    // the work, in camelCase, and nothing else.
    #[test]
    fn the_event_payload_has_the_contract_shape() {
        let value = serde_json::to_value(ModelDowngrade {
            source: "graduation",
            requested_model: "a".into(),
            served_model: "b".into(),
            provider: "anthropic".into(),
            project_key: "/repo".into(),
            worktree: "/repo".into(),
            origin: None,
            run_id: Some("r1".into()),
        })
        .unwrap();
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "origin",
                "projectKey",
                "provider",
                "requestedModel",
                "runId",
                "servedModel",
                "source",
                "worktree"
            ]
        );
    }
}
