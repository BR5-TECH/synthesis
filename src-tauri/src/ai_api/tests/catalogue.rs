//! The keychain-free catalogue.

use super::*;

// -- The keychain-free catalogue ----------------------------------------

fn catalog<'a>(list: &'a [AiApiCatalog], provider: &str) -> &'a AiApiCatalog {
    list.iter().find(|c| c.provider == provider).unwrap()
}

#[test]
fn a_catalogue_names_every_provider_and_its_models() {
    let h = ok_harness();
    verify_openrouter(&h);

    let listed = list_catalogs_impl(&h.store).unwrap();
    assert_eq!(
        listed.len(),
        PROVIDERS.len(),
        "every provider is present, configured or not — the selector renders \
         from this",
    );

    let open = catalog(&listed, "openrouter");
    assert_eq!(open.state, IntegrationState::Verified);
    assert_eq!(open.display_name, "OpenRouter");
    assert_eq!(
        open.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["gpt-5", "gpt-4o"],
        "the probed list, so a row can render a model's label",
    );

    // An untouched provider is `unconfigured` rather than absent, which is
    // what lets a surface say "configure one" instead of showing nothing.
    assert_eq!(
        catalog(&listed, "anthropic").state,
        IntegrationState::Unconfigured,
    );
}

#[test]
fn a_catalogue_reads_no_key_and_survives_a_locked_keychain() {
    // AGR-FR-16: the agent surfaces render with the keychain
    // shut. `list_catalogs_impl` takes the store alone — there is no
    // `SecretStore` in its signature to reach — and this pins the behaviour
    // that follows from that: the same registry that reads `key_unavailable`
    // through `AiApiIntegration` still reads `verified` here.
    let h = ok_harness();
    verify_openrouter(&h);
    h.keys.lock_it();

    assert_eq!(
        find(&list_integrations_impl(&h.store, &h.ai).unwrap(), "openrouter").state,
        IntegrationState::KeyUnavailable,
        "the credential view degrades, as it should — it is about the key",
    );

    // The harm is the access itself, not the state it produces: on macOS a
    // read is what raises the prompt, so a probe that failed and was read as
    // "absent" would leave every assertion below intact and still have
    // interrupted the author. Counted, not inferred.
    let before = h.keys.reads();
    let listed = list_catalogs_impl(&h.store).unwrap();
    assert_eq!(
        h.keys.reads(),
        before,
        "building a catalogue touched the keychain",
    );
    assert_eq!(
        catalog(&listed, "openrouter").state,
        IntegrationState::Verified,
        "the catalogue does not degrade, because it never asked",
    );

    // Deleting the entry outright is the same story: an agent naming this
    // provider is still describable, and whether the call can be *made* is
    // `resolve_ai_api_endpoint`'s answer to give (AAP-FR-34).
    h.keys.wipe("openrouter");
    assert_eq!(
        catalog(&list_catalogs_impl(&h.store).unwrap(), "openrouter").state,
        IntegrationState::Verified,
    );
}

#[test]
fn a_catalogue_keeps_the_one_unreadable_state_no_key_could_rescue() {
    // `key_present = true` forces `verified` only where a key is what is
    // missing. A record naming an endpoint that never held a key, for a
    // provider that requires one, is a half-written store — no key exists to
    // assume present, so `state_of`'s last arm still answers
    // `key_unavailable` and this carries it.
    //
    // Left alone rather than collapsed, because both sides already agree
    // about it: `availability_of` maps it to `provider_unverified` and so
    // does the frontend. Calling it `unconfigured` here would be the one
    // edit that makes the two disagree about the same agent.
    let h = ok_harness();
    h.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "openrouter".into(),
                base_url: Some("https://openrouter.ai/api/v1".into()),
                masked_hint: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();

    assert_eq!(
        catalog(&list_catalogs_impl(&h.store).unwrap(), "openrouter").state,
        IntegrationState::KeyUnavailable,
    );
    // And the backend refuses the same record, by the same reading.
    assert_eq!(
        validate_ai_api_selection(&h.store, "openrouter", "gpt-5", None).unwrap_err(),
        ERR_NOT_VERIFIED,
    );

    // AAP-FR-04: `custom` requires a key as well, so a record that holds no
    // hint is condemned exactly as a named provider's is.
    h.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "custom".into(),
                base_url: Some("https://example/v1".into()),
                masked_hint: None,
                verified_at: Some("2026-01-01T00:00:00Z".into()),
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    h.keys.lock_it();
    assert_eq!(
        catalog(&list_catalogs_impl(&h.store).unwrap(), "custom").state,
        IntegrationState::KeyUnavailable,
        "a keyless Custom record is not usable either",
    );

    // Without that check having passed, though, a base URL is just a base
    // URL — AAP-FR-13 applies to `custom` exactly as it does to the rest.
    h.store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "custom".into(),
                base_url: Some("https://example/v1".into()),
                masked_hint: None,
                verified_at: None,
                ..Default::default()
            }],
            None,
        )
        .unwrap();
    assert_eq!(
        catalog(&list_catalogs_impl(&h.store).unwrap(), "custom").state,
        IntegrationState::KeyUnavailable,
    );
}

#[test]
fn a_catalogue_agrees_with_what_an_agents_availability_is_computed_from() {
    // Two derivations of one fact. `GlobalAgents` reads the catalogue and
    // decides an agent is ready, unconfigured, unverified, or on a missing
    // model; `list_project_agents` gets the same verdict from
    // `availability_of`. They render side by side, so a disagreement shows
    // up as the same agent described two ways in two tabs.
    //
    // Driven over every record shape the store can hold rather than over the
    // verified one, because agreement at the happy point is free — the
    // shapes below are where the two readings were actually found to differ.
    use crate::agents::{availability_of, Agent, AgentAvailability};

    let cases: &[(&str, Option<&str>, Option<&str>, Option<&str>, AgentAvailability)] = &[
        // label, base_url, masked_hint, verified_at, expected verdict
        ("never configured", None, None, None, AgentAvailability::ProviderUnconfigured),
        (
            "verified with a key",
            Some("https://openrouter.ai/api/v1"),
            Some("cdef"),
            Some("2026-01-01T00:00:00Z"),
            AgentAvailability::Ready,
        ),
        // AAP-FR-13: a base URL and a hint, but no check has ever passed.
        // The catalogue read this as verified while `availability_of` read
        // it as unverified, until both went through `state_assuming_key`.
        (
            "a hand-edited store that never verified",
            Some("https://openrouter.ai/api/v1"),
            Some("cdef"),
            None,
            AgentAvailability::ProviderUnverified,
        ),
        // An endpoint that never held a key, for a provider that needs one.
        (
            "a base URL with no key ever stored",
            Some("https://openrouter.ai/api/v1"),
            None,
            Some("2026-01-01T00:00:00Z"),
            AgentAvailability::ProviderUnverified,
        ),
    ];

    for (label, base_url, hint, verified_at, expected) in cases {
        let h = ok_harness();
        h.store
            .save_ai_api_registry(
                vec![AiApiRecord {
                    provider: "openrouter".into(),
                    base_url: base_url.map(str::to_string),
                    masked_hint: hint.map(str::to_string),
                    verified_at: verified_at.map(str::to_string),
                    models: vec![ModelOption::new("gpt-5", "GPT-5")],
                    ..Default::default()
                }],
                None,
            )
            .unwrap();
        // The keychain is shut throughout: neither reading may depend on it.
        h.keys.lock_it();

        let agent = Agent {
            id: "a1".into(),
            nickname: "arch".into(),
            title: String::new(),
            model_id: "gpt-5".into(),
            instructions: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            reasoning: None,
        };
        let backend = availability_of(&h.store, "", &agent);
        assert_eq!(backend, *expected, "backend verdict for {label}");

        // The frontend's derivation, transcribed from `GlobalAgents`: it reads
        // the active catalogue (AAP-FR-APRV) and nothing else.
        let active = active_catalog_impl(&h.store, "").unwrap();
        let frontend = match &active.catalog {
            None if active.resolution == ProjectResolution::NoneConfigured => {
                AgentAvailability::ProviderUnconfigured
            }
            None => AgentAvailability::ProviderUnverified,
            Some(c) if c.models.iter().any(|m| m.id == agent.model_id) => AgentAvailability::Ready,
            Some(_) => AgentAvailability::ModelUnavailable,
        };
        assert_eq!(
            frontend, backend,
            "the catalogue and `availability_of` describe {label} differently",
        );
    }
}

#[test]
fn a_catalogue_holds_the_providers_in_their_declared_order() {
    // The persona editor seeds its provider from the first verified entry,
    // so the order this returns decides which provider a fresh editor opens
    // on (AAP-FR-02's stable order).
    let listed = list_catalogs_impl(&GlobalSettingsStore::in_memory()).unwrap();
    assert_eq!(
        listed.iter().map(|c| c.provider.as_str()).collect::<Vec<_>>(),
        PROVIDERS.iter().map(|p| p.provider).collect::<Vec<_>>(),
    );
}

#[test]
fn an_untouched_provider_carries_its_bundled_catalogue() {
    // The fallback in `available_models` is what makes the model selector
    // render before anything has been verified. Observed here on a provider
    // that ships one, since `openrouter` and `custom` deliberately do not.
    let listed = list_catalogs_impl(&GlobalSettingsStore::in_memory()).unwrap();
    let anthropic = catalog(&listed, "anthropic");
    assert_eq!(anthropic.state, IntegrationState::Unconfigured);
    assert!(
        !anthropic.models.is_empty(),
        "a bundled catalogue still names models for the selector",
    );
}

