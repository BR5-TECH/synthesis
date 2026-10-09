//! AAP-FR-21, AAP-FR-24, AAP-FR-34, AAP-FR-35: the capabilities a model declares.

use super::*;

// -- AAP-FR-21, AAP-FR-35: no modality --------------------------------------------

#[test]
fn aap_ts21_no_descriptor_or_record_carries_a_modality() {
    // AAP-FR-21: this module cannot describe a voice, embedding, or image
    // endpoint. The check is on the serialised shape because that is what a
    // caller could reach for.
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let as_text = serde_json::to_string(&list).unwrap().to_lowercase();
    for absent in ["modality", "voice", "embedding"] {
        assert!(
            !as_text.contains(absent),
            "no record may carry a {absent} field"
        );
    }
    // AAP-FR-21 is about the **kind of endpoint** this module can describe,
    // and AAP-FR-35 puts one image-related field on a model: whether the
    // conversational endpoint takes a picture in its input. The two do not
    // conflict — a model that reads an image is still a conversational
    // model — so the assertion names the endpoint kinds rather than the
    // substring, which would now forbid the capability the later
    // requirement adds.
    for absent in ["image_endpoint", "imagegeneration", "\"image\":"] {
        assert!(
            !as_text.contains(absent),
            "no record may describe an image endpoint ({absent})"
        );
    }
    assert!(
        as_text.contains("acceptsimageinput"),
        "AAP-FR-35: every model option carries the capability"
    );
}

// -- AAP-FR-34: image-input capability (AAP-FR-35) -----------------------

/// A model whose probe declares its own input modalities.
fn with_modalities(id: &str, modalities: Option<&[&str]>) -> ModelOption {
    let declared = modalities.map(|m| m.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    ModelOption::new(id, id).with_declared_image_input(
        crate::ai_shared::image_input_declared(declared.as_deref()),
    )
}

#[test]
fn aap_ts36_a_probe_that_declares_input_modalities_settles_the_capability() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_modalities("sees/pictures", Some(&["text", "image"])),
        with_modalities("text/only", Some(&["text"])),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);

    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let models = &find(&listed, "openrouter").models;
    let by = |id: &str| models.iter().find(|m| m.id == id).unwrap().accepts_image_input;
    assert!(by("sees/pictures"), "the probe declared images");
    assert!(!by("text/only"), "the probe declared text alone");
}

#[test]
fn aap_ts36_the_shipped_catalog_settles_what_the_probe_left_unsaid() {
    // A provider whose probe reports no input modalities at all. The shipped
    // descriptor's own catalog entry settles the capability for the model it
    // names, and a model neither declares anything about carries false —
    // undeclared being absent rather than guessed at.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_modalities("gpt-5", None),
        with_modalities("some/unknown-model", None),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-1234"),
    )
    .unwrap();

    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let models = &find(&listed, "openai").models;
    let by = |id: &str| models.iter().find(|m| m.id == id).unwrap().accepts_image_input;
    assert!(by("gpt-5"), "the catalog declares this one image-capable");
    assert!(!by("some/unknown-model"), "neither declared anything about this one");
}

// AAP-FR-35: the Anthropic probe declares no input modalities, so the shipped
// catalog settles them. It names the current generation and also the earlier
// one, which a probe still lists and a stored selection can still name.
#[test]
fn the_anthropic_catalog_settles_image_input_for_both_generations() {
    let ids = [
        "claude-opus-5-5",
        "claude-sonnet-5-5",
        "claude-haiku-5-5",
        "claude-opus-5",
        "claude-sonnet-5",
        "claude-haiku-4-5",
    ];
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(ids.iter().map(|id| with_modalities(id, None)).collect());
    let h = harness(prober, FakeKeychain::new());
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();

    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let models = &find(&listed, "anthropic").models;
    for id in ids {
        let model = models.iter().find(|m| m.id == id).unwrap();
        assert!(model.accepts_image_input, "{id}");
    }
}

#[test]
fn aap_ts36_resolve_returns_the_flag_of_the_model_it_resolved() {
    // AAP-FR-35: the caller about to build a request learns the capability of
    // the exact endpoint that will serve it rather than of the provider in
    // general — two models of one provider differ in it freely.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_modalities("sees/pictures", Some(&["text", "image"])),
        with_modalities("text/only", Some(&["text"])),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);

    for (model, expected) in [("sees/pictures", true), ("text/only", false)] {
        let call =
            resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", model, None).unwrap();
        assert_eq!(call.accepts_image_input, expected, "{model}");
        assert_eq!(call.model_id.as_deref(), Some(model));
        assert_eq!(call.base_url, "https://openrouter.ai/api/v1");
        assert!(call.api_key.is_some());
        assert!(call.reasoning.is_none());
    }
}

#[test]
fn aap_ts36_validation_neither_reads_nor_reports_the_capability() {
    // AAP-FR-35: an agent is never refused for the pictures a conversation
    // might later carry.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_modalities("text/only", Some(&["text"]))]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    assert!(validate_ai_api_selection(&h.store, "openrouter", "text/only", None).is_ok());
}

// -- AAP-FR-10, AAP-FR-24 / TS-24: reasoning capability reaching the record ---------

#[test]
fn aap_ts23_each_model_carries_the_reasoning_it_declares_and_no_more() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("with/ladder", &["high", "medium", "low"], Some("medium"), false),
        ladderless("without/ladder", false),
        ModelOption::new("no/reasoning", "No reasoning"),
    ]);
    let h = harness(prober, FakeKeychain::new());
    let rec = verify_openrouter(&h);

    let ladder = rec.models.iter().find(|m| m.id == "with/ladder").unwrap();
    let r = ladder.reasoning.as_ref().unwrap();
    assert_eq!(
        r.supported_efforts.as_deref(),
        Some(["high", "medium", "low"].map(String::from).as_slice()),
        "the ladder is carried in the order the provider declared it"
    );
    assert_eq!(r.default_effort.as_deref(), Some("medium"));

    let plain = rec.models.iter().find(|m| m.id == "without/ladder").unwrap();
    let r = plain.reasoning.as_ref().unwrap();
    assert_eq!(r.supported_efforts, None, "reasoning, but no levels");

    let none = rec.models.iter().find(|m| m.id == "no/reasoning").unwrap();
    assert_eq!(none.reasoning, None);
}

#[test]
fn aap_ts24_a_bundled_catalog_carries_no_reasoning_at_all() {
    // AAP-FR-24: reasoning is something only the provider can report about a
    // model, so a list that never came from one declares nothing.
    let h = harness(FakeProber::returning(&[]), FakeKeychain::new());
    let rec = verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    assert_eq!(rec.models_origin, ModelsOrigin::Catalog);
    assert!(
        rec.models.iter().all(|m| m.reasoning.is_none()),
        "a bundled catalogue declares no reasoning"
    );

    // AAP-FR-25: and the shipped catalogues themselves carry none either.
    for descriptor in PROVIDERS {
        assert!(
            catalog_models(descriptor).iter().all(|m| m.reasoning.is_none()),
            "{}'s bundled catalogue declares no reasoning",
            descriptor.provider
        );
    }
}

