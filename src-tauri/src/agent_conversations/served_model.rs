//! The model a response says served it (CVL-FR-XBMT), and the downgrade check
//! a turn makes on it (AGC-FR-BXOS, AGC-FR-UVKF).

use super::*;
use crate::model_downgrades::{ModelDowngrade, MODEL_DOWNGRADE_DETECTED};

/// AGC-FR-UVKF: the requested→served pairs a turn has emitted, so a turn of
/// many rounds emits each pair once.
#[derive(Default)]
pub(super) struct ReportedDowngrades(Mutex<std::collections::HashSet<(String, String)>>);

impl ReportedDowngrades {
    /// True the first time the turn sees `pair`, false after.
    fn first(&self, requested: &str, served: &str) -> bool {
        self.0
            .lock()
            .map(|mut seen| seen.insert((requested.to_string(), served.to_string())))
            .unwrap_or(false)
    }
}

/// AGC-FR-BXOS: a downgrade is any difference of the two ids as exact strings.
/// No requested id, or no served id, is no downgrade.
pub(super) fn downgrade_pair<'a>(
    requested: Option<&'a str>,
    served: Option<&'a str>,
) -> Option<(&'a str, &'a str)> {
    let requested = requested.filter(|id| !id.is_empty())?;
    let served = served.filter(|id| !id.is_empty())?;
    (requested != served).then_some((requested, served))
}

/// CVL-FR-XBMT: the loop reports the model id of each answered attempt, and
/// AGC-FR-BXOS / AGC-FR-UVKF: the turn compares it with the requested id and
/// emits `"model downgrade detected"` once for each new pair.
///
/// The turn continues and ends as it would without this (AGC-FR-PNHM). The
/// `WARN` record of the downgrade is the main window's (NTF-FR-DYHQ); the
/// served id is already on the record of each answered call (CVL-FR-28).
pub(super) fn report_served_model<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    served: Option<&str>,
) {
    let Some((requested, served)) = downgrade_pair(endpoint.model_id.as_deref(), served) else {
        return;
    };
    if !plan.reported_downgrades.first(requested, served) {
        return;
    }
    let payload = ModelDowngrade {
        source: "agent",
        requested_model: requested.to_string(),
        served_model: served.to_string(),
        provider: endpoint.provider.clone(),
        project_key: plan.project_key.clone(),
        worktree: plan.roots.worktree.path().to_string_lossy().into_owned(),
        // AGC-FR-PNHM: the discussion and its owner alone. The fragment target
        // holds a quote of the document, so it stays out of the payload.
        origin: Some(ConversationOrigin { fragment_target: None, ..plan.origin.clone() }),
        run_id: None,
    };
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "model downgrade detected",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "provider" => &endpoint.provider,
            // The ids come from the provider, so a record keeps a bounded part.
            "requestedModel" => truncated(requested, IDENTIFIER_LIMIT),
            "servedModel" => truncated(served, IDENTIFIER_LIMIT),
        },
    );
    if let Err(error) = app.emit(MODEL_DOWNGRADE_DETECTED, payload) {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "model downgrade not emitted",
            log_fields! { "turnId" => &plan.turn_id, "error" => error.to_string() },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AGC-FR-BXOS: exact strings, with no normalisation; a missing id on either
    // side is no downgrade.
    #[test]
    fn a_downgrade_is_an_exact_string_difference() {
        assert_eq!(downgrade_pair(Some("opus-5"), Some("opus-4")), Some(("opus-5", "opus-4")));
        assert_eq!(downgrade_pair(Some("opus-5"), Some("Opus-5")), Some(("opus-5", "Opus-5")));
        assert_eq!(downgrade_pair(Some("opus-5"), Some("opus-5")), None);
        assert_eq!(downgrade_pair(Some("opus-5"), None), None);
        assert_eq!(downgrade_pair(Some("opus-5"), Some("")), None);
        assert_eq!(downgrade_pair(None, Some("opus-4")), None);
    }

    // AGC-FR-UVKF: one pair is reported once per turn, and a second pair once more.
    #[test]
    fn each_pair_is_reported_once() {
        let seen = ReportedDowngrades::default();
        assert!(seen.first("a", "b"));
        assert!(!seen.first("a", "b"));
        assert!(seen.first("a", "c"));
    }
}
