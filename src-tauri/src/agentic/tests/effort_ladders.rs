//! Which efforts a selection offers, and which stored effort still applies
//! (AIC-FR-09, AIC-FR-10, AIC-FR-11, AIC-FR-SFMS, AIC-FR-TPKB).

use super::*;

/// A harness whose Codex verifies against the bundled catalog, and whose
/// Claude Code verifies too.
fn verified() -> Harness {
    let h = all_clis_harness();
    for (vendor, path) in CLI_BINARIES.iter().take(2) {
        verify_integration_impl(&h.store, &h.ai, vendor, &cli_config_for(vendor, path)).unwrap();
    }
    h
}

fn offered(h: &Harness, vendor: &str, turn_kind: Option<&str>) -> Vec<String> {
    let (records, _) = h.store.load_agentic_registry().unwrap();
    let record = records.iter().find(|r| r.vendor == vendor).unwrap();
    let descriptor = vendor_descriptor(vendor).unwrap();
    let models = if record.models.is_empty() { Vec::new() } else { record.models.clone() };
    let model = turn_kind
        .and_then(|k| record.model_overrides.get(k).cloned())
        .or_else(|| record.selected_model.clone());
    offered_efforts(descriptor, &models, model.as_deref()).into_iter().map(|e| e.id).collect()
}

fn resolved_effort(h: &Harness, turn_kind: Option<&str>) -> Option<String> {
    match resolve_agentic_invocation(&h.store, &h.ai, "", turn_kind).unwrap() {
        AgenticInvocation::Cli { effort_id, .. } => effort_id,
        other => panic!("{other:?}"),
    }
}

fn stored_codex(h: &Harness) -> AgenticRecord {
    let (records, _) = h.store.load_agentic_registry().unwrap();
    records.into_iter().find(|r| r.vendor == "codex").unwrap()
}

// AIC-FR-SFMS: a selection offers its effective model's ladder, each level
// labelled from the vendor list.
#[test]
fn a_codex_selection_offers_its_models_ladder() {
    let h = verified();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-luna")).unwrap();
    assert_eq!(offered(&h, "codex", None), ["low", "medium", "high", "xhigh", "max"]);

    let descriptor = vendor_descriptor("codex").unwrap();
    let models = stored_codex(&h).models;
    let labels: Vec<String> = offered_efforts(descriptor, &models, Some("gpt-5.5"))
        .into_iter()
        .map(|e| e.label)
        .collect();
    assert_eq!(labels, ["Low", "Medium", "High", "Extra high"]);

    // A kind's own model decides that kind's levels.
    set_model_impl(&h.store, &h.ai, "codex", Some("review"), Some("gpt-6-sol")).unwrap();
    assert_eq!(
        offered(&h, "codex", Some("review")),
        ["low", "medium", "high", "xhigh", "max", "ultra"]
    );
    assert_eq!(offered(&h, "codex", Some("work")), ["low", "medium", "high", "xhigh", "max"]);
}

// AIC-FR-SFMS: with no effective model, a selection offers only the levels
// every ladder shares; a vendor whose models declare no ladder offers its
// vendor list.
#[test]
fn no_model_offers_the_shared_levels_and_claude_offers_its_list() {
    let h = verified();
    assert_eq!(offered(&h, "codex", None), ["low", "medium", "high", "xhigh"]);

    let claude = vendor_descriptor("claude_code").unwrap();
    let models = catalog_models(claude);
    for model in [None, Some("opus"), Some("haiku")] {
        let ids: Vec<String> =
            offered_efforts(claude, &models, model).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["low", "medium", "high", "xhigh", "max"], "{model:?}");
    }
}

// AIC-FR-10 / AIC-FR-SFMS: a level the effective model does not offer is
// refused as `unknown_effort`, and nothing is stored.
#[test]
fn an_effort_the_model_does_not_offer_is_refused() {
    let h = verified();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-luna")).unwrap();
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "codex", None, Some("ultra")).unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
    assert_eq!(stored_codex(&h).selected_effort, None);
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("max")).unwrap();

    // A kind is checked against its own effective model.
    set_model_impl(&h.store, &h.ai, "codex", Some("work"), Some("gpt-6-sol")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", Some("work"), Some("ultra")).unwrap();
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "codex", Some("review"), Some("ultra")).unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
    // With no model selected, only a shared level is accepted.
    set_model_impl(&h.store, &h.ai, "codex", None, None).unwrap();
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "codex", Some("review"), Some("max")).unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
    set_effort_impl(&h.store, &h.ai, "codex", Some("review"), Some("xhigh")).unwrap();
}

// AIC-FR-TPKB: a stored effort the effective model stops offering reads as
// absent, on the wire and in the invocation, and nothing is written. A model
// that offers it again makes it read again.
#[test]
fn an_effort_the_model_stops_offering_is_dropped_on_read() {
    let h = verified();
    set_active_impl(&h.store, &h.ai, "codex").unwrap();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-sol")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("ultra")).unwrap();
    assert_eq!(resolved_effort(&h, None).as_deref(), Some("ultra"));

    let after = set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-luna")).unwrap();
    assert_eq!(after.selected_effort, None, "the wire drops it");
    assert_eq!(resolved_effort(&h, None), None, "the invocation takes the backend default");
    assert_eq!(stored_codex(&h).selected_effort.as_deref(), Some("ultra"), "nothing is written");

    let back = set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-sol")).unwrap();
    assert_eq!(back.selected_effort.as_deref(), Some("ultra"));
}

// AIC-FR-TPKB: a dropped override falls back to `selected_effort` where the
// kind offers it, and to the backend default otherwise.
#[test]
fn a_dropped_override_falls_back_to_the_default_effort() {
    let h = verified();
    set_active_impl(&h.store, &h.ai, "codex").unwrap();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-sol")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("high")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", Some("review"), Some("ultra")).unwrap();
    assert_eq!(resolved_effort(&h, Some("review")).as_deref(), Some("ultra"));

    let after =
        set_model_impl(&h.store, &h.ai, "codex", Some("review"), Some("gpt-6-luna")).unwrap();
    assert!(!after.effort_overrides.contains_key("review"), "{:?}", after.effort_overrides);
    assert_eq!(resolved_effort(&h, Some("review")).as_deref(), Some("high"));
    assert_eq!(stored_codex(&h).effort_overrides.get("review").map(String::as_str), Some("ultra"));

    // The default level is not offered for the kind either: the backend
    // default applies.
    set_model_impl(&h.store, &h.ai, "codex", Some("review"), Some("gpt-5.5")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("max")).unwrap();
    assert_eq!(resolved_effort(&h, Some("review")), None);
    assert_eq!(resolved_effort(&h, None).as_deref(), Some("max"));
}

// AIC-FR-11 / AIC-FR-TPKB: a re-verification writes no effort selection, so
// a level the refreshed models still offer reads again as it was.
#[test]
fn a_reverification_writes_no_effort() {
    let h = verified();
    set_model_impl(&h.store, &h.ai, "codex", None, Some("gpt-6-sol")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", None, Some("ultra")).unwrap();
    set_effort_impl(&h.store, &h.ai, "codex", Some("work"), Some("low")).unwrap();

    let again =
        verify_integration_impl(&h.store, &h.ai, "codex", &cli_config("/usr/bin/codex")).unwrap();
    assert_eq!(again.selected_effort.as_deref(), Some("ultra"));
    assert_eq!(again.effort_overrides.get("work").map(String::as_str), Some("low"));
}

// AIC-FR-09 / AIC-FR-SFMS: Claude Code is unchanged — every declared level
// is accepted whatever the model.
#[test]
fn claude_code_accepts_every_declared_level() {
    let h = verified();
    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("haiku")).unwrap();
    for level in ["low", "medium", "high", "xhigh", "max"] {
        set_effort_impl(&h.store, &h.ai, "claude_code", None, Some(level)).unwrap();
    }
    assert_eq!(
        set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("ultra")).unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
}

fn laddered(id: &str, efforts: &[&str]) -> ModelOption {
    ModelOption {
        id: id.into(),
        label: id.into(),
        reasoning: Some(crate::ai_shared::ModelReasoning {
            mandatory: true,
            supported_efforts: Some(efforts.iter().map(|e| e.to_string()).collect()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// AIC-FR-SFMS — a listed model that declares no ladder, in a list where other
// models carry one, offers the vendor list; with no effective model the list
// still offers the levels every ladder shares. The same fixture as
// `src/components/agenticEfforts.test.ts`, so both sides agree.
#[test]
fn a_listed_model_without_a_ladder_offers_the_vendor_list() {
    let descriptor = vendor_descriptor("codex").unwrap();
    let models = vec![
        laddered("sol", &["low", "medium", "high", "xhigh", "max", "ultra"]),
        laddered("luna", &["low", "medium", "high", "xhigh", "max"]),
        ModelOption { id: "bare".into(), label: "bare".into(), ..Default::default() },
    ];
    let ids = |model: Option<&str>| -> Vec<String> {
        offered_efforts(descriptor, &models, model).into_iter().map(|e| e.id).collect()
    };
    assert_eq!(ids(Some("bare")), ["low", "medium", "high", "xhigh", "max", "ultra"]);
    assert_eq!(ids(None), ["low", "medium", "high", "xhigh", "max"]);
    assert_eq!(ids(Some("gone")), ["low", "medium", "high", "xhigh", "max"]);
}
