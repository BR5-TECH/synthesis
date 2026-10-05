//! The provider-native entries.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-21 — a provider-native tool is one entry and nothing else (TLC-FR-21)
// ---------------------------------------------------------------------------

#[test]
fn a_provider_native_tool_is_a_definition_entry_and_nothing_else() {
    // WST-FR-01 / WFT-FR-01: neither is present in the application's code as a
    // tool at all — no `PortableTool`, no constructor, no name, no description,
    // and no parameter schema — because the provider owns every one of them.
    for (name, source) in NATIVE_SOURCES {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "impl PortableTool",
            "PortableTool for",
            "const NAME",
            "fn description",
            "fn parameters",
            "fn call",
            "fn map_error",
        ] {
            assert!(
                !code.contains(forbidden),
                "{name} must hold no implementation, found {forbidden:?} (TLC-FR-21)",
            );
        }
    }

    // TLC-FR-25 / WST-FR-09 / WFT-FR-09: no client of the application's own
    // carries any part of this out. Searched where such a client could actually
    // be added — the whole tool group and the loop that offers the entries —
    // rather than only in two files that hold no code to begin with.
    const CLIENT_SOURCES: [(&str, &str); 3] = [
        ("web_search.rs", include_str!("../web_search.rs")),
        ("web_fetch.rs", include_str!("../web_fetch.rs")),
        (
            "agent_conversations.rs",
            include_str!("../../agent_conversations.rs"),
        ),
    ];
    for (name, source) in CLIENT_SOURCES {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["reqwest", "hyper::", "ureq", "HttpClient", "http_client"] {
            assert!(
                !code.contains(forbidden),
                "{name} performs no web request of its own, found {forbidden:?} (TLC-FR-25)",
            );
        }
    }

    // And neither reaches the frontend, on the terms every other member of the
    // group is held to (TLC-FR-21).
    const LIB: &str = include_str!("../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    for entry in [
        crate::tools::web_search::ENTRY_TYPE,
        crate::tools::web_fetch::ENTRY_TYPE,
    ] {
        let bare = entry.split(':').next_back().expect("a qualified type");
        assert!(
            !handler.contains(bare),
            "{entry} is registered as no Tauri command (TLC-FR-21)",
        );
    }

    // Nor is either of them among the definitions a provider receives as tools
    // of this application's own: they are the provider's (TLC-FR-23).
    for definition in definitions() {
        assert!(
            !definition.name.contains("web_search") && !definition.name.contains("web_fetch"),
            "a provider-native capability is no portable tool of this group",
        );
    }
}

// ---------------------------------------------------------------------------
// TLC-FR-22 — the entry is fixed, and settable nowhere (TLC-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn a_provider_native_entry_is_fixed_text_carrying_its_type_and_nothing_else() {
    // WST-FR-02, WST-FR-03 / WFT-FR-02, WFT-FR-03: the entry declares a `type` and no other field, so
    // OpenRouter's own default engine selection serves every call — there is
    // nothing here for an author to have chosen.
    for (entry, expected) in [
        (crate::tools::web_search::entry(), "openrouter:web_search"),
        (crate::tools::web_fetch::entry(), "openrouter:web_fetch"),
    ] {
        let value = serde_json::to_value(&entry).expect("an entry serialises");
        assert_eq!(
            value,
            serde_json::json!({ "type": expected }),
            "the entry is exactly its type",
        );
        let object = value.as_object().expect("an entry is an object");
        assert_eq!(object.len(), 1, "no field beside `type` (TLC-FR-22)");
    }

    // Constructed twice, and from two callers, it is the same text: fixed at
    // build time rather than assembled per call (TLC-FR-22).
    assert_eq!(
        crate::tools::web_search::entry(),
        crate::tools::web_search::entry(),
    );
    assert_ne!(
        crate::tools::web_search::entry(),
        crate::tools::web_fetch::entry(),
        "two tools the model sees separately (WST-FR-06, WFT-FR-06)",
    );
}

#[test]
fn no_configuration_surface_carries_a_provider_native_field() {
    // WST-FR-12 / WFT-FR-12 / TLC-FR-22: no control, no stored field, no
    // command, no validation, and no configuration response carries anything
    // belonging to either tool — the OpenRouter record is what it is without
    // them.
    // The stored record is what a field would have to be added to, so its shape
    // is asserted directly: a key added anywhere — in this module, in the store,
    // or in a command's payload — turns this red wherever it was declared.
    let record = crate::ai_api::AiApiRecord {
        turn_timeout_ms: None,
        provider: "openrouter".into(),
        base_url: Some("https://openrouter.example/api/v1".into()),
        masked_hint: Some("cdef".into()),
        verified_at: Some("2026-01-01T00:00:00Z".into()),
        selected_model: Some("m".into()),
        models_origin: crate::ai_shared::ModelsOrigin::Probed,
        selected_reasoning: None,
        models: Vec::new(),
    };
    let stored = serde_json::to_value(&record).expect("a record serialises");
    let mut keys: Vec<&str> = stored
        .as_object()
        .expect("a record is an object")
        .keys()
        .map(|key| key.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "baseUrl",
            "maskedHint",
            "models",
            "modelsOrigin",
            "provider",
            "selectedModel",
            "verifiedAt",
        ],
        "the OpenRouter record is what it is without these tools (TLC-FR-22)",
    );

    const CONFIGURATION_SOURCES: [(&str, &str); 2] = [
        ("ai_api.rs", include_str!("../../ai_api.rs")),
        ("global_settings.rs", include_str!("../../global_settings.rs")),
    ];
    for (name, source) in CONFIGURATION_SOURCES {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "web_search",
            "web_fetch",
            "web_engine",
            "ProviderNativeTool",
        ] {
            assert!(
                !code.contains(forbidden),
                "{name} carries no web-tool configuration, found {forbidden:?} (TLC-FR-22)",
            );
        }
    }
}
