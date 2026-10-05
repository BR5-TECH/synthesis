//! AAP-FR-27, AAP-FR-29, AAP-FR-31: choosing a reasoning level and what happens when it stops applying.

use super::*;

// -- AAP-FR-27 / TS-26 / TS-27: choosing a level ------------------------

#[test]
fn aap_ts25_a_reasoning_choice_persists_and_null_returns_the_model_default() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder(
        "a/model",
        &["high", "medium", "low"],
        Some("medium"),
        false,
    )]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();

    let rec = set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "medium".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        rec.selected_reasoning,
        Some(ReasoningChoice::Effort {
            effort: "medium".into()
        })
    );
    // It survives a relaunch — the registry is the only thing holding it.
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "openrouter").selected_reasoning,
        Some(ReasoningChoice::Effort {
            effort: "medium".into()
        })
    );

    let cleared = set_reasoning_impl(&h.store, &h.ai, "openrouter", None).unwrap();
    assert_eq!(cleared.selected_reasoning, None);
}

#[test]
fn aap_ts26_every_incompatible_choice_is_refused_with_its_own_reason() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("ladder/model", &["high", "low"], Some("high"), false),
        ladderless("mandatory/model", true),
        ModelOption::new("plain/model", "Plain"),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);

    let effort = |e: &str| ReasoningChoice::Effort { effort: e.into() };

    // No model selected: the provider is on its own default, so no model's
    // capability is known.
    assert_eq!(
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(effort("high"))).unwrap_err(),
        ERR_NO_MODEL_SELECTED
    );

    // A model that declares no reasoning at all.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("plain/model")).unwrap();
    assert_eq!(
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::On))
            .unwrap_err(),
        ERR_REASONING_UNSUPPORTED
    );

    // A level outside the selected model's ladder.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("ladder/model")).unwrap();
    assert_eq!(
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(effort("medium")))
            .unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );

    // Switching off a model that cannot stop reasoning.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("mandatory/model")).unwrap();
    assert_eq!(
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::Off))
            .unwrap_err(),
        ERR_REASONING_MANDATORY
    );
    // ...while on is fine for the same model.
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::On)).unwrap();

    // A refusal leaves the stored choice exactly as it was.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("ladder/model")).unwrap();
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(effort("high"))).unwrap();
    assert!(
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(effort("nope"))).is_err()
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "openrouter").selected_reasoning,
        Some(effort("high")),
        "a refused choice leaves the previous one alone"
    );
}

#[test]
fn aap_ts27_a_level_this_application_never_heard_of_is_still_selectable() {
    // AAP-FR-26: the model's declared ladder is authoritative over any level
    // this application knows by name, so a provider adding one needs no
    // change here for it to become choosable.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder(
        "a/model",
        &["ludicrous", "high"],
        Some("high"),
        false,
    )]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();

    let rec = set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "ludicrous".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        rec.selected_reasoning,
        Some(ReasoningChoice::Effort {
            effort: "ludicrous".into()
        })
    );

    // And a level this application *does* know, that the model does not
    // list, is still refused.
    assert_eq!(
        set_reasoning_impl(
            &h.store,
            &h.ai,
            "openrouter",
            Some(ReasoningChoice::Effort {
                effort: "medium".into()
            })
        )
        .unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
}

// -- AAP-FR-29 / TS-29: a choice that stops applying --------------------

#[test]
fn aap_ts28_a_choice_the_new_model_cannot_honour_falls_back_to_its_default() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("rich/model", &["max", "high"], Some("high"), false),
        with_ladder("poor/model", &["high"], Some("high"), false),
        ModelOption::new("plain/model", "Plain"),
    ]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("rich/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "max".into(),
        }),
    )
    .unwrap();

    // A model whose ladder lacks that level.
    let rec = set_model_impl(&h.store, &h.ai, "openrouter", Some("poor/model")).unwrap();
    assert_eq!(
        rec.selected_reasoning, None,
        "the level did not survive a model that does not offer it"
    );

    // A model that reasons at a level the new one shares keeps the choice.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("rich/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();
    let rec = set_model_impl(&h.store, &h.ai, "openrouter", Some("poor/model")).unwrap();
    assert_eq!(
        rec.selected_reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
        "a level both models offer survives the change"
    );

    // Returning to the provider's own model default leaves no model whose
    // capability is known, so the choice cannot stand either.
    let rec = set_model_impl(&h.store, &h.ai, "openrouter", None).unwrap();
    assert_eq!(rec.selected_reasoning, None);

    // AAP-FR-29: and a re-verification whose refreshed descriptor drops the
    // level clears it just the same.
    set_model_impl(&h.store, &h.ai, "openrouter", Some("rich/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "max".into(),
        }),
    )
    .unwrap();
    prober.set_rich_models(vec![with_ladder(
        "rich/model",
        &["high"],
        Some("high"),
        false,
    )]);
    let rec = verify_openrouter(&h);
    assert_eq!(rec.selected_model.as_deref(), Some("rich/model"));
    assert_eq!(
        rec.selected_reasoning, None,
        "a level the refreshed descriptor stopped offering does not survive"
    );
}

#[test]
fn aap_ts28_every_way_a_refreshed_descriptor_can_invalidate_a_choice() {
    // AAP-FR-29 composed with AAP-FR-12: a re-verification can invalidate a
    // stored choice through each arm of `reasoning_error`, not just the
    // effort one. Each case starts from a working configuration and asserts
    // the choice does not survive the refresh.
    let effort_choice = ReasoningChoice::Effort {
        effort: "high".into(),
    };

    // 1. The model itself is gone from the refreshed list. `selected_model`
    //    is cleared first, which leaves no capability known at all — the
    //    interaction between AAP-FR-12 and AAP-FR-29.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder("a/model", &["high"], Some("high"), false)]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(effort_choice.clone()))
        .unwrap();
    prober.set_rich_models(vec![with_ladder("other/model", &["high"], Some("high"), false)]);
    let rec = verify_openrouter(&h);
    assert_eq!(rec.selected_model, None, "the model went with the refresh");
    assert_eq!(
        rec.selected_reasoning, None,
        "and no choice outlives the model it was made against"
    );

    // 2. The model becomes one that cannot stop reasoning, while the stored
    //    choice is `off`.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![ladderless("a/model", false)]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::Off))
        .unwrap();
    prober.set_rich_models(vec![ladderless("a/model", true)]);
    let rec = verify_openrouter(&h);
    assert_eq!(rec.selected_model.as_deref(), Some("a/model"));
    assert_eq!(
        rec.selected_reasoning, None,
        "off cannot stand against a model that always reasons"
    );

    // 3. The model stops declaring reasoning at all, while a choice stands.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![ladderless("a/model", false)]);
    let h = harness(prober.clone(), FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::On))
        .unwrap();
    prober.set_rich_models(vec![ModelOption::new("a/model", "A model")]);
    let rec = verify_openrouter(&h);
    assert_eq!(rec.selected_model.as_deref(), Some("a/model"));
    assert_eq!(
        rec.selected_reasoning, None,
        "a model that no longer reasons carries no depth"
    );
}

#[test]
fn a_choice_a_model_can_honour_is_accepted_in_each_permissive_shape() {
    // The arms `reasoning_error` falls through, which the refusal tests
    // above cannot reach: `off` against a model that permits it, and `on`
    // against one that declares a ladder.
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![
        with_ladder("ladder/model", &["high"], Some("high"), false),
        ladderless("plain/model", false),
    ]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);

    set_model_impl(&h.store, &h.ai, "openrouter", Some("ladder/model")).unwrap();
    let rec =
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::On))
            .unwrap();
    assert_eq!(rec.selected_reasoning, Some(ReasoningChoice::On));

    set_model_impl(&h.store, &h.ai, "openrouter", Some("plain/model")).unwrap();
    let rec =
        set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::Off))
            .unwrap();
    assert_eq!(rec.selected_reasoning, Some(ReasoningChoice::Off));

    // AAP-FR-30 / the store: each unit-variant shape survives a round trip
    // too, which the effort shape alone would not prove.
    for choice in [ReasoningChoice::Off, ReasoningChoice::On] {
        let record = AiApiRecord {
            provider: "openrouter".into(),
            base_url: Some("https://openrouter.ai/api/v1".into()),
            selected_model: Some("a/model".into()),
            selected_reasoning: Some(choice.clone()),
            models: vec![ladderless("a/model", false)],
            ..Default::default()
        };
        let text = toml::to_string(&record).unwrap();
        assert_eq!(toml::from_str::<AiApiRecord>(&text).unwrap(), record, "{text}");
    }
}

#[test]
fn aap_ts29_a_reasoning_choice_is_per_provider_and_clearing_removes_it() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder(
        "a/model",
        &["high", "low"],
        Some("high"),
        false,
    )]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "low".into(),
        }),
    )
    .unwrap();

    // A second provider carrying a reasoning choice of its *own* — an
    // assertion that the other one is merely absent would hold however the
    // records were keyed, and would survive dropping the keying entirely.
    verify_integration_impl(
        &h.store,
        &h.ai,
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("sk-ant-1234"),
    )
    .unwrap();
    // The same probe answered for both, so both offer the same model —
    // which is exactly what makes this a test of per-provider isolation
    // rather than of two providers that happen to differ.
    set_model_impl(&h.store, &h.ai, "anthropic", Some("a/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "anthropic",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();

    // Changing one leaves the other's own value exactly where it was.
    set_reasoning_impl(&h.store, &h.ai, "openrouter", Some(ReasoningChoice::On))
        .unwrap();
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "anthropic").selected_reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
        "one provider's choice is not the other's"
    );
    assert_eq!(
        find(&list, "openrouter").selected_reasoning,
        Some(ReasoningChoice::On)
    );

    // AAP-FR-30: clearing takes the choice with the rest of the record.
    let list = clear_integration_impl(&h.store, &h.ai, "openrouter").unwrap();
    assert_eq!(find(&list, "openrouter").selected_reasoning, None);
    assert_eq!(find(&list, "openrouter").selected_model, None);
}

// -- AAP-FR-31: what an actual call carries -----------------------------

#[test]
fn aap_ts30_resolve_carries_the_reasoning_choice_alongside_the_endpoint() {
    let prober = FakeProber::returning(&[]);
    prober.set_rich_models(vec![with_ladder(
        "a/model",
        &["high", "low"],
        Some("high"),
        false,
    )]);
    let h = harness(prober, FakeKeychain::new());
    verify_openrouter(&h);
    set_model_impl(&h.store, &h.ai, "openrouter", Some("a/model")).unwrap();
    set_reasoning_impl(
        &h.store,
        &h.ai,
        "openrouter",
        Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    )
    .unwrap();

    let call = resolve_ai_api_call(&h.store, &h.ai, "some-project").unwrap();
    assert_eq!(call.provider, "openrouter");
    assert_eq!(call.model_id.as_deref(), Some("a/model"));
    assert_eq!(
        call.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        })
    );

    // AAP-FR-31: a null choice sends nothing at all.
    set_reasoning_impl(&h.store, &h.ai, "openrouter", None).unwrap();
    let call = resolve_ai_api_call(&h.store, &h.ai, "some-project").unwrap();
    assert_eq!(call.reasoning, None);

    // And the key still never reaches a debug line (AAP-FR-07).
    assert!(!format!("{call:?}").contains("sk-or-1234"));
}

