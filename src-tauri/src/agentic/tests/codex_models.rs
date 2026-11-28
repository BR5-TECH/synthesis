//! The Codex model probe and the bundled Codex catalog (AIC-FR-08,
//! AIC-FR-09, AIC-FR-TBTH).

use super::*;

/// A trimmed `codex debug models` answer, in the shape `codex-cli 0.172.0`
/// prints: fields this module does not read are present, as in the real
/// document, so the parse must skip them.
const PROBE_DOCUMENT: &str = r#"{"models":[
  {"slug":"gpt-6.2-sol","display_name":"GPT-6.2-Sol","description":"Fast.",
   "default_reasoning_level":"low",
   "supported_reasoning_levels":[{"effort":"low","description":"x"},{"effort":"medium","description":"x"},
     {"effort":"high","description":"x"},{"effort":"xhigh","description":"x"},
     {"effort":"max","description":"x"},{"effort":"ultra","description":"x"}],
   "visibility":"list","priority":1,
   "model_messages":{"instructions_template":"long text","instructions_variables":{}}},
  {"slug":"gpt-6-luna","display_name":"GPT-6-Luna","default_reasoning_level":"medium",
   "supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"},{"effort":"high"},
     {"effort":"xhigh"},{"effort":"max"}],
   "visibility":"list"},
  {"slug":"gpt-daybreak-blue-latest","display_name":"Daybreak Blue",
   "supported_reasoning_levels":[{"effort":"low"}],"visibility":"hide"},
  {"slug":"codex-auto-review","display_name":"Codex Auto Review","visibility":"hide"}
]}"#;

fn codex_output(stdout: &str) -> CliOutput {
    CliOutput { stdout: stdout.to_string(), stderr: String::new(), success: true }
}

fn ladder(model: &ModelOption) -> Vec<&str> {
    model
        .reasoning
        .as_ref()
        .and_then(|r| r.supported_efforts.as_ref())
        .map(|l| l.iter().map(String::as_str).collect())
        .unwrap_or_default()
}

fn codex_harness() -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "codex-cli 0.172.0"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    )
}

// AIC-FR-TBTH: only `list` models are kept, with their slug, display name,
// default level, and ladder; every other field of the document is skipped.
#[test]
fn the_probe_keeps_listed_models_with_their_ladders() {
    let models = parse_codex_models(&codex_output(PROBE_DOCUMENT));
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-6.2-sol", "gpt-6-luna"], "hidden models are not offered");

    assert_eq!(models[0].label, "GPT-6.2-Sol");
    assert_eq!(ladder(&models[0]), ["low", "medium", "high", "xhigh", "max", "ultra"]);
    assert_eq!(models[0].reasoning.as_ref().and_then(|r| r.default_effort.as_deref()), Some("low"));
    assert_eq!(ladder(&models[1]), ["low", "medium", "high", "xhigh", "max"]);
    assert_eq!(
        models[1].reasoning.as_ref().and_then(|r| r.default_effort.as_deref()),
        Some("medium")
    );
}

// AIC-FR-TBTH / AIC-FR-08: the parse is tolerant. A repeated slug is kept
// once, a missing display name falls back to the slug, a default the ladder
// does not hold is dropped, and an id that is not a plain identifier is not
// carried onward.
#[test]
fn the_probe_tolerates_partial_and_unsafe_entries() {
    let document = r#"{"models":[
      {"slug":"gpt-a","visibility":"list",
       "default_reasoning_level":"ultra",
       "supported_reasoning_levels":[{"effort":"low"},{"effort":"hi\"gh"},{"effort":"low"}]},
      {"slug":"gpt-a","display_name":"Second copy","visibility":"list"},
      {"slug":"gpt b","display_name":"Spaced","visibility":"list"},
      {"slug":"gpt-c","display_name":"   ","visibility":"list"},
      {"display_name":"No slug","visibility":"list"},
      {"slug":"gpt-d","display_name":"No visibility"}
    ]}"#;
    let models = parse_codex_models(&codex_output(document));
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-a", "gpt-c"]);
    assert_eq!(models[0].label, "gpt-a", "no display name: the slug is the label");
    assert_eq!(ladder(&models[0]), ["low"], "unsafe and repeated levels are dropped");
    assert_eq!(
        models[0].reasoning.as_ref().and_then(|r| r.default_effort.clone()),
        None,
        "a default outside the ladder is dropped"
    );
    assert_eq!(models[1].label, "gpt-c", "a blank display name is no label");
    assert!(models[1].reasoning.is_none(), "no levels: no ladder");
}

// CDX-FR-SDOV, CDX-FR-OOCJ, CDX-FR-BQNL, AIC-FR-TBTH: a level outside the six
// the CLI accepts is not carried onward, so it is never offered; a model whose
// ladder holds only such levels declares no ladder.
#[test]
fn the_probe_keeps_only_the_six_codex_levels() {
    let document = r#"{"models":[
      {"slug":"gpt-a","visibility":"list","default_reasoning_level":"minimal",
       "supported_reasoning_levels":[{"effort":"minimal"},{"effort":"low"},{"effort":"ultra"},
         {"effort":"persistent"}]},
      {"slug":"gpt-b","visibility":"list",
       "supported_reasoning_levels":[{"effort":"none"},{"effort":"minimal"}]}
    ]}"#;
    let models = parse_codex_models(&codex_output(document));
    assert_eq!(ladder(&models[0]), ["low", "ultra"]);
    assert_eq!(
        models[0].reasoning.as_ref().and_then(|r| r.default_effort.clone()),
        None,
        "a default outside the six is dropped with its level"
    );
    assert!(models[1].reasoning.is_none(), "no known level: no ladder");
}

// AIC-FR-08 / AIC-FR-TBTH: a document that does not parse offers nothing,
// which the caller reads as a failed probe.
#[test]
fn a_probe_answer_that_is_not_the_document_offers_no_model() {
    for stdout in ["", "codex-cli 0.172.0", "{\"models\": 3}", "[1,2,3]", "{not json"] {
        assert!(parse_codex_models(&codex_output(stdout)).is_empty(), "{stdout:?}");
    }
}

// AIC-FR-TBTH / AIC-FR-08: a Codex verification runs `codex debug models` on
// the verified binary, and a usable answer is the record's probed list.
#[test]
fn a_codex_verification_records_the_probed_models() {
    let h = codex_harness();
    h.runner.answer("/usr/bin/codex", &["debug", "models"], PROBE_DOCUMENT, true);

    let record =
        verify_integration_impl(&h.store, &h.ai, "codex", &cli_config("/usr/bin/codex")).unwrap();

    assert_eq!(record.models_origin, ModelsOrigin::Probed);
    let ids: Vec<&str> = record.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-6.2-sol", "gpt-6-luna"]);
    let calls = h.runner.calls.lock().unwrap().clone();
    assert!(
        calls.iter().any(|(path, args)| path == "/usr/bin/codex" && args == &["debug", "models"]),
        "{calls:?}"
    );
}

// AIC-FR-08 / AIC-FR-09: a probe that fails, or answers with nothing usable,
// leaves the bundled catalog in place, with each model's own ladder.
#[test]
fn a_failed_codex_probe_falls_back_to_the_bundled_ladders() {
    for (stdout, success) in
        [(PROBE_DOCUMENT, false), ("not json", true), ("{\"models\":[]}", true)]
    {
        let h = codex_harness();
        h.runner.answer("/usr/bin/codex", &["debug", "models"], stdout, success);
        let record =
            verify_integration_impl(&h.store, &h.ai, "codex", &cli_config("/usr/bin/codex"))
                .unwrap();
        assert_eq!(record.models_origin, ModelsOrigin::Catalog, "{stdout:?} / {success}");
        assert_eq!(record.models.len(), 11);
        let by_id = |id: &str| record.models.iter().find(|m| m.id == id).unwrap();
        assert_eq!(
            ladder(by_id("gpt-6.2-sol")),
            ["low", "medium", "high", "xhigh", "max", "ultra"]
        );
        assert_eq!(ladder(by_id("gpt-6-luna")), ["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(ladder(by_id("gpt-5.6-luna")), ["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(ladder(by_id("gpt-5.5")), ["low", "medium", "high", "xhigh"]);
    }
}

// AIC-FR-09: the Codex vendor list is the six levels, with their labels, and
// a Claude Code model carries no ladder of its own.
#[test]
fn the_codex_levels_are_declared_and_claude_models_carry_no_ladder() {
    let list =
        list_integrations_impl(&GlobalSettingsStore::in_memory(), &codex_harness().ai).unwrap();
    let codex = find(&list, "codex");
    let levels: Vec<(&str, &str)> =
        codex.reasoning_efforts.iter().map(|e| (e.id.as_str(), e.label.as_str())).collect();
    assert_eq!(
        levels,
        [
            ("low", "Low"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Extra high"),
            ("max", "Max"),
            ("ultra", "Ultra"),
        ]
    );
    let claude = find(&list, "claude_code");
    assert!(claude.models.iter().all(|m| m.reasoning.is_none()));
}

// AIC-FR-TBTH: the probe carries no credential in its arguments, even in
// `access_token` mode with a token supplied. It carries none in its
// environment by construction: `CliRunner::run` takes no environment, so no
// caller can hand the probe one.
#[test]
fn the_codex_probe_carries_no_credential() {
    let h = codex_harness();
    h.runner.answer("/usr/bin/codex", &["debug", "models"], PROBE_DOCUMENT, true);
    let token = "codex-access-token-0123456789";
    let config = VerifyConfig {
        path: Some("/usr/bin/codex".into()),
        auth_mode: Some("access_token".into()),
        access_token: Some(token.into()),
        ..Default::default()
    };
    verify_integration_impl(&h.store, &h.ai, "codex", &config).unwrap();
    for (_, args) in h.runner.calls.lock().unwrap().iter() {
        assert!(args.iter().all(|a| !a.contains(token)), "{args:?}");
    }
}

// AIC-FR-08, AIC-FR-09, CDX-FR-OOCJ — the bundled Codex catalog is the model
// table of the CDX contract surface, row for row: slug, label, default level,
// and ladder.
#[test]
fn the_bundled_codex_catalog_is_the_cdx_table() {
    const FULL: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
    let to_max = &FULL[..5];
    let to_xhigh = &FULL[..4];
    let table: &[(&str, &str, &str, &[&str])] = &[
        ("gpt-6.2-sol", "GPT-6.2-Sol", "low", FULL),
        ("gpt-6.2-astra", "GPT-6.2-Astra", "low", FULL),
        ("gpt-6-astra", "GPT-6-Astra", "low", FULL),
        ("gpt-6.1-sol", "GPT-6.1-Sol", "low", FULL),
        ("gpt-6.1-terra", "GPT-6.1 Terra", "low", FULL),
        ("gpt-6-sol", "GPT-6-Sol", "medium", FULL),
        ("gpt-6-luna", "GPT-6-Luna", "medium", to_max),
        ("gpt-5.6-sol", "GPT-5.6-Sol", "low", FULL),
        ("gpt-5.6-terra", "GPT-5.6-Terra", "medium", FULL),
        ("gpt-5.6-luna", "GPT-5.6-Luna", "medium", to_max),
        ("gpt-5.5", "GPT-5.5", "medium", to_xhigh),
    ];
    let models = catalog_models(vendor_descriptor("codex").unwrap());
    assert_eq!(models.len(), table.len());
    for (model, (slug, label, default, ladder_of_row)) in models.iter().zip(table) {
        assert_eq!(model.id, *slug);
        assert_eq!(model.label, *label, "{slug}");
        let reasoning = model.reasoning.as_ref().expect("a ladder");
        assert_eq!(reasoning.default_effort.as_deref(), Some(*default), "{slug}");
        assert_eq!(ladder(model), *ladder_of_row, "{slug}");
    }
}
