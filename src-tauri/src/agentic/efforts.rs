//! Which reasoning efforts a selection offers, and which stored effort still
//! applies (`AIC-agentic-integrations.md` AIC-FR-SFMS, AIC-FR-TPKB).
//!
//! A Codex model declares its own ladder, and the levels one model takes are
//! not the levels another takes. The CLI accepts an undeclared level without an
//! error (`../infra/CDX-codex-cli-protocol.md` CDX-FR-OOCJ), so this module is
//! the only place that keeps such a level out of a run. A vendor whose models
//! declare no ladder — Claude Code — offers its vendor list as before.

use super::*;

/// The model list a record offers: the stored one after a verification, the
/// bundled catalog before it. The same list the selectors render.
pub(super) fn record_models(
    descriptor: &AgenticVendor,
    record: &AgenticRecord,
) -> Vec<ModelOption> {
    if record.models.is_empty() {
        catalog_models(descriptor)
    } else {
        record.models.clone()
    }
}

/// AIC-FR-SFMS: the effective model of a selection — the kind's entry in
/// `model_overrides`, else `selected_model`. A `None` kind is the default
/// selection.
pub(super) fn effective_model(record: &AgenticRecord, turn_kind: Option<&str>) -> Option<String> {
    model_for(record, turn_kind)
}

/// A model's own ladder, where it declares one.
fn ladder_of(model: &ModelOption) -> Option<&[String]> {
    model.reasoning.as_ref().and_then(|r| r.supported_efforts.as_deref())
}

/// AIC-FR-SFMS: the efforts offered for a selection whose effective model is
/// `model_id`.
///
/// The effective model's ladder where the model is in the list and declares
/// one. With no effective model, the levels every ladder of the list shares.
/// Where no model declares a ladder, the vendor list. Each level takes its
/// label from the vendor list, and an id the vendor list does not name is its
/// own label.
pub fn offered_efforts(
    descriptor: &AgenticVendor,
    models: &[ModelOption],
    model_id: Option<&str>,
) -> Vec<EffortOption> {
    let ladders: Vec<&[String]> = models.iter().filter_map(ladder_of).collect();
    if ladders.is_empty() {
        return declared_efforts(descriptor);
    }
    let label = |id: &str| {
        descriptor
            .reasoning_efforts
            .iter()
            .find(|(e, _)| *e == id)
            .map_or_else(|| id.to_string(), |(_, l)| l.to_string())
    };
    let chosen = model_id.and_then(|id| models.iter().find(|m| m.id == id));
    let levels: Vec<String> = match chosen {
        Some(model) => match ladder_of(model) {
            Some(ladder) => ladder.to_vec(),
            // A model with no ladder in a list where others carry one offers
            // the vendor list, as a vendor with no ladders at all does.
            None => return declared_efforts(descriptor),
        },
        // No effective model, or one the list no longer holds: the backend
        // default decides the model, so only a level every ladder holds is
        // safe to send with it.
        None => ladders[0]
            .iter()
            .filter(|level| ladders.iter().all(|l| l.contains(level)))
            .cloned()
            .collect(),
    };
    levels.iter().map(|id| EffortOption::new(id, &label(id))).collect()
}

/// Whether `effort` is offered for the selection of `turn_kind`.
pub(super) fn effort_offered(
    descriptor: &AgenticVendor,
    record: &AgenticRecord,
    turn_kind: Option<&str>,
    effort: &str,
) -> bool {
    let models = record_models(descriptor, record);
    let model = effective_model(record, turn_kind);
    offered_efforts(descriptor, &models, model.as_deref()).iter().any(|e| e.id == effort)
}

/// AIC-FR-10 / AIC-FR-TPKB: the effort a record resolves for one turn kind.
///
/// The kind's override where it holds one that is offered for the kind;
/// otherwise `selected_effort` where that is offered for the kind; otherwise
/// the backend default (`None`). Nothing is written: a stored effort that is
/// not offered is read as absent.
pub(super) fn effort_for(
    descriptor: &AgenticVendor,
    record: &AgenticRecord,
    turn_kind: Option<&str>,
) -> Option<String> {
    let offered = |effort: &String| effort_offered(descriptor, record, turn_kind, effort);
    turn_kind
        .and_then(|kind| record.effort_overrides.get(kind))
        .filter(|effort| offered(effort))
        .or_else(|| record.selected_effort.as_ref().filter(|effort| offered(effort)))
        .cloned()
}

/// AIC-FR-TPKB: the stored default effort as the wire reads it — absent where
/// the default selection does not offer it.
pub(super) fn readable_selected_effort(
    descriptor: &AgenticVendor,
    record: &AgenticRecord,
) -> Option<String> {
    record.selected_effort.clone().filter(|effort| effort_offered(descriptor, record, None, effort))
}

/// AIC-FR-TPKB / AIC-FR-QVWX: the stored per-kind efforts as the wire reads
/// them — only the kinds this build offers, and only the levels each kind's
/// selection offers.
pub(super) fn readable_effort_overrides(
    descriptor: &AgenticVendor,
    record: &AgenticRecord,
) -> std::collections::BTreeMap<String, String> {
    offered_kinds_only(&record.effort_overrides)
        .into_iter()
        .filter(|(kind, effort)| effort_offered(descriptor, record, Some(kind), effort))
        .collect()
}
