//! The AI integration registries: the agentic and API records, the agent
//! roster, per-project enrolment, and the per-project overrides.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// -- AI integration registries (GSS-FR-14 / 26 / 27 / 28) --------------

#[test]
fn gss_ts23_the_agentic_registry_survives_a_round_trip_through_disk() {
    // GSS-FR-14. `with_path` rather than `in_memory` for the reason above:
    // the registry is an array of tables and the active vendor is a scalar,
    // so a declaration order that emitted the scalar second would fail to
    // serialise — and only a real persist proves it does not. Both kinds are
    // stored here because a CLI record and an API record are the same array.
    use crate::agentic::{AgenticRecord, PathOrigin};
    use crate::ai_shared::{ModelOption, ModelsOrigin};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_agentic_registry(
                vec![
                    AgenticRecord {
                        vendor: "claude_code".into(),
                        binary_path: Some("/opt/homebrew/bin/claude".into()),
                        path_origin: PathOrigin::Detected,
                        version: Some("2.1.4".into()),
                        verified_at: Some("2026-07-01T09:00:00Z".into()),
                        models: vec![ModelOption::new("opus", "Opus")],
                        models_origin: ModelsOrigin::Catalog,
                        selected_model: Some("opus".into()),
                        // GSS-FR-14: one model override under one turn kind
                        // and one effort override under another, so a map
                        // that failed to round-trip could not pass by
                        // sharing the other's key.
                        model_overrides: [("review".to_string(), "opus".to_string())]
                            .into_iter()
                            .collect(),
                        selected_effort: Some("high".into()),
                        effort_overrides: [("implementation".to_string(), "low".to_string())]
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    },
                    AgenticRecord {
                        vendor: "claude_agent_api".into(),
                        base_url: Some("https://api.anthropic.com/v1".into()),
                        masked_hint: Some("a71c".into()),
                        version: Some("2026-01-01".into()),
                        ..Default::default()
                    },
                ],
                Some("claude_code".into()),
            )
            .unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path.clone());
    let (records, active) = reloaded.load_agentic_registry().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].binary_path.as_deref(),
        Some("/opt/homebrew/bin/claude")
    );
    assert_eq!(records[0].version.as_deref(), Some("2.1.4"));
    assert_eq!(records[0].selected_model.as_deref(), Some("opus"));
    assert_eq!(records[0].selected_effort.as_deref(), Some("high"));
    // Both maps read back with those entries alone, so every other turn
    // kind still resolves the record's defaults (GSS-FR-14).
    assert_eq!(
        records[0].model_overrides.get("review").map(String::as_str),
        Some("opus"),
    );
    assert_eq!(records[0].model_overrides.len(), 1);
    assert_eq!(
        records[0].effort_overrides.get("implementation").map(String::as_str),
        Some("low"),
    );
    assert_eq!(records[0].effort_overrides.len(), 1);
    assert!(
        records[1].model_overrides.is_empty() && records[1].effort_overrides.is_empty(),
        "a record configured before any kind was distinguished reads back empty",
    );
    assert_eq!(records[0].path_origin, PathOrigin::Detected);
    assert_eq!(
        records[1].base_url.as_deref(),
        Some("https://api.anthropic.com/v1")
    );
    assert_eq!(records[1].masked_hint.as_deref(), Some("a71c"));
    assert_eq!(
        active.as_deref(),
        Some("claude_code"),
        "one active vendor across both kinds, restored alongside the records"
    );

    // The file carries the API agent's masked hint and nothing more of its
    // key (GSS-FR-14).
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("a71c"), "the masked hint is the exception");
    let lowered = text.to_lowercase();
    for forbidden in ["secret", "password", "bearer"] {
        assert!(
            !lowered.contains(forbidden),
            "the store must stay attachable to a bug report: found {forbidden}"
        );
    }
}

#[test]
fn gss_ts26_the_ai_api_registry_survives_a_round_trip_through_disk() {
    // GSS-FR-27: descriptions round-trip; no key is part of this store, and
    // the only key-derived text present is the masked hint (GSS-FR-27).
    use crate::ai_api::AiApiRecord;
    use crate::ai_shared::{ModelOption, ModelsOrigin};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_ai_api_registry(
                vec![
                    AiApiRecord {
                        turn_timeout_ms: None,
                        provider: "openrouter".into(),
                        base_url: Some("https://openrouter.ai/api/v1".into()),
                        masked_hint: Some("3f9a".into()),
                        verified_at: Some("2026-07-01T09:00:00Z".into()),
                        models: vec![ModelOption::new(
                            "anthropic/claude-opus-5",
                            "Claude Opus 5",
                        )],
                        models_origin: ModelsOrigin::Probed,
                        selected_model: Some("anthropic/claude-opus-5".into()),
                        selected_reasoning: None,
                    },
                    AiApiRecord {
                        provider: "custom".into(),
                        base_url: Some("http://localhost:11434/v1".into()),
                        // A local endpoint that authenticates nobody: no key,
                        // so no hint (AAP-FR-04).
                        masked_hint: None,
                        ..Default::default()
                    },
                ],
                Some("openrouter".into()),
            )
            .unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path.clone());
    let (records, active) = reloaded.load_ai_api_registry().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].base_url.as_deref(),
        Some("https://openrouter.ai/api/v1")
    );
    assert_eq!(records[0].masked_hint.as_deref(), Some("3f9a"));
    assert_eq!(records[0].models_origin, ModelsOrigin::Probed);
    assert_eq!(
        records[0].selected_model.as_deref(),
        Some("anthropic/claude-opus-5")
    );
    assert_eq!(records[1].masked_hint, None);
    assert_eq!(active.as_deref(), Some("openrouter"));

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("3f9a"), "the masked hint is the exception");
    let lowered = text.to_lowercase();
    for forbidden in ["secret", "password", "bearer", "sk-"] {
        assert!(
            !lowered.contains(forbidden),
            "no key material may reach the store: found {forbidden}"
        );
    }
}

#[test]
fn gss_ts23_both_active_scalars_serialise_alongside_each_other() {
    // The two `active_*` fields are the only scalars at the top level, and
    // TOML requires scalars before sub-tables. A regression that declared
    // one of them after `app_preferences` would fail to serialise at all —
    // and would do so silently, because the store's writers log and carry on.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_agentic_registry(vec![], Some("codex".into()))
            .unwrap();
        store
            .save_ai_api_registry(vec![], Some("anthropic".into()))
            .unwrap();
    }
    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reloaded.load_agentic_registry().unwrap().1.as_deref(),
        Some("codex")
    );
    assert_eq!(
        reloaded.load_ai_api_registry().unwrap().1.as_deref(),
        Some("anthropic")
    );
}

#[test]
fn gss_ts24_an_agentic_override_is_keyed_by_project_and_never_validated() {
    // GSS-FR-26, GSS-FR-18: one override per repository however many worktrees it has,
    // and this module stores whatever vendor it is given — whether the
    // integration is still usable is `crate::agentic`'s question (AIC-FR-16).
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store.save_agentic_override("/dev/acme", "codex").unwrap();
        store
            .save_agentic_override("/dev/other", "claude_code")
            .unwrap();
        // The registry is empty: nothing here checks that `codex` exists.
        assert!(store.load_agentic_registry().unwrap().0.is_empty());
    }

    let reloaded = GlobalSettingsStore::with_path(path.clone());
    assert_eq!(
        reloaded
            .load_agentic_override("/dev/acme")
            .unwrap()
            .as_deref(),
        Some("codex"),
        "the key is the project, so a change of active worktree keeps it"
    );
    assert_eq!(
        reloaded
            .load_agentic_override("/dev/other")
            .unwrap()
            .as_deref(),
        Some("claude_code"),
        "and one project's override never leaks into another"
    );

    reloaded.clear_agentic_override("/dev/acme").unwrap();
    assert_eq!(reloaded.load_agentic_override("/dev/acme").unwrap(), None);
    assert_eq!(
        reloaded
            .load_agentic_override("/dev/other")
            .unwrap()
            .as_deref(),
        Some("claude_code"),
    );
}

#[test]
fn gss_ts29_the_agent_registry_survives_a_round_trip_and_carries_no_key() {
    // GSS-FR-30. `with_path` rather than `in_memory` for the
    // reason GSS-FR-14 uses it: the registry is an array of tables and every
    // reasoning choice inside a record is a nested one, so a declaration
    // order that emitted a scalar after either would fail to serialise —
    // and only a real persist proves it does not.
    use crate::agents::Agent;
    use crate::ai_api::ReasoningChoice;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_agent_registry(vec![
                Agent {
                    id: "a1".into(),
                    nickname: "arch".into(),
                    title: "Architect".into(),
                    model_id: "anthropic/claude-opus-5".into(),
                    instructions: "Argue about structure.".into(),
                    created_at: "2026-07-01T09:00:00Z".into(),
                    updated_at: "2026-07-01T09:00:00Z".into(),
                    reasoning: Some(ReasoningChoice::Effort {
                        effort: "high".into(),
                    }),
                },
                Agent {
                    id: "a2".into(),
                    nickname: "scribe".into(),
                    title: String::new(),
                    model_id: "gpt-5".into(),
                    instructions: String::new(),
                    created_at: "2026-07-02T09:00:00Z".into(),
                    updated_at: "2026-07-02T09:00:00Z".into(),
                    reasoning: None,
                },
            ])
            .unwrap();
    }

    let text = std::fs::read_to_string(&path).unwrap();
    for expected in [
        "arch",
        "anthropic/claude-opus-5",
        "high",
        "Argue about structure.",
        "scribe",
        "gpt-5",
        // GSS-FR-30: the title is persisted beside the rest of the record.
        "Architect",
    ] {
        assert!(text.contains(expected), "{expected:?} is missing from {text}");
    }
    // GSS-FR-30 / AGR-FR-20: an agent names no provider, and the active
    // provider's key lives in the application secret vault — so the file gains
    // no credential by gaining an agent.
    assert!(!text.contains("sk-"));

    let reloaded = GlobalSettingsStore::with_path(path);
    let agents = reloaded.load_agent_registry().unwrap();
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].nickname, "arch");
    assert_eq!(agents[0].instructions, "Argue about structure.");
    assert_eq!(agents[0].title, "Architect");
    // GSS-FR-30: an agent that carries no title reads back as `""` rather
    // than as anything else — the same value AGR-FR-24 gives a record that
    // predates the field.
    assert_eq!(agents[1].title, "");
    assert_eq!(
        agents[0].reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
    );
    assert_eq!(agents[1].reasoning, None);
}

#[test]
fn gss_ts29_an_agent_record_predating_the_title_field_loads_with_an_empty_one() {
    // GSS-FR-30, per AGR-FR-24. The whole point is that this
    // does *not* go through GSS-FR-13's repair-to-defaults: a store written
    // by an older build has to keep its recents, its tokens, and every
    // other registry while the new field defaults quietly.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(
        &path,
        r#"
[appPreferences]
theme = "dark"

[[agents]]
id = "a1"
nickname = "arch"
provider = "openrouter"
modelId = "anthropic/claude-opus-5"
instructions = "Argue about structure."
createdAt = "2026-07-01T09:00:00Z"
updatedAt = "2026-07-01T09:00:00Z"
"#,
    )
    .unwrap();

    let store = GlobalSettingsStore::with_path(path.clone());
    let agents = store.load_agent_registry().unwrap();
    assert_eq!(agents.len(), 1, "the record loaded rather than being repaired");
    assert_eq!(agents[0].title, "");
    assert_eq!(agents[0].nickname, "arch");
    assert_eq!(agents[0].instructions, "Argue about structure.");
    // And nothing else in the store was reset on the way past.
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::Dark);
    // Reading does not rewrite: the file is untouched until something saves.
    assert!(!std::fs::read_to_string(&path).unwrap().contains("title"));
}

#[test]
fn gss_ts30_an_enrolment_is_per_project_and_pruned_when_its_agent_goes() {
    // GSS-FR-31, GSS-FR-18.
    use crate::agents::Agent;

    let agent = |id: &str, nickname: &str| Agent {
        id: id.into(),
        nickname: nickname.into(),
        title: String::new(),
        model_id: "m".into(),
        instructions: String::new(),
        created_at: "2026-07-01T09:00:00Z".into(),
        updated_at: "2026-07-01T09:00:00Z".into(),
        reasoning: None,
    };

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_agent_registry(vec![
                agent("a1", "arch"),
                agent("a2", "sec"),
                agent("a3", "scribe"),
            ])
            .unwrap();
        store
            .save_project_agent_enrolment("/dev/acme", vec!["a1".into()])
            .unwrap();
        store
            .save_project_agent_enrolment("/dev/other", vec!["a2".into(), "a3".into()])
            .unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reloaded.load_project_agent_enrolment("/dev/acme").unwrap(),
        vec!["a1".to_string()],
        "the key is the project, so a change of active worktree keeps it",
    );
    assert_eq!(
        reloaded.load_project_agent_enrolment("/dev/other").unwrap(),
        vec!["a2".to_string(), "a3".to_string()],
        "and one project's enrolment never leaks into another",
    );
    // A project that has enrolled nothing reads as an empty set rather than
    // an error.
    assert!(reloaded
        .load_project_agent_enrolment("/dev/fresh")
        .unwrap()
        .is_empty());

    // GSS-FR-31: an id naming a record the registry no longer holds is not
    // written. Deleting `a2` clears it from `/dev/other` in the same
    // operation, leaving `a3` alone.
    reloaded
        .save_agent_registry(vec![agent("a1", "arch"), agent("a3", "scribe")])
        .unwrap();
    assert_eq!(
        reloaded.load_project_agent_enrolment("/dev/other").unwrap(),
        vec!["a3".to_string()],
    );
    assert_eq!(
        reloaded.load_project_agent_enrolment("/dev/acme").unwrap(),
        vec!["a1".to_string()],
    );
}

#[test]
fn gss_ts27_the_two_project_overrides_are_independent_of_each_other() {
    // GSS-FR-26, GSS-FR-28: a project may override one level, both, or neither, so
    // clearing one must leave the other exactly as it was.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store.save_agentic_override("/dev/acme", "codex").unwrap();
        store.save_ai_api_override("/dev/acme", "openai").unwrap();
        // A project that overrides only the API level.
        store.save_ai_api_override("/dev/solo", "anthropic").unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reloaded
            .load_agentic_override("/dev/acme")
            .unwrap()
            .as_deref(),
        Some("codex")
    );
    assert_eq!(
        reloaded.load_ai_api_override("/dev/acme").unwrap().as_deref(),
        Some("openai")
    );

    reloaded.clear_agentic_override("/dev/acme").unwrap();
    assert_eq!(reloaded.load_agentic_override("/dev/acme").unwrap(), None);
    assert_eq!(
        reloaded.load_ai_api_override("/dev/acme").unwrap().as_deref(),
        Some("openai"),
        "clearing one level's override must not disturb the other"
    );

    assert_eq!(
        reloaded.load_agentic_override("/dev/solo").unwrap(),
        None,
        "a project that overrides only the API level reads back unset here"
    );
    assert_eq!(
        reloaded.load_ai_api_override("/dev/solo").unwrap().as_deref(),
        Some("anthropic")
    );
}

#[test]
fn gss_ts25_a_fresh_machine_has_no_integration_no_active_and_no_override() {
    // GSS-FR-14, GSS-FR-26, GSS-FR-27, GSS-FR-28: none of those absences is an error, in either level.
    let store = GlobalSettingsStore::in_memory();
    let (agentic, active_agentic) = store.load_agentic_registry().unwrap();
    assert!(agentic.is_empty());
    assert_eq!(active_agentic, None);
    let (api, active_api) = store.load_ai_api_registry().unwrap();
    assert!(api.is_empty());
    assert_eq!(active_api, None);
    assert_eq!(store.load_agentic_override("/dev/acme").unwrap(), None);
    assert_eq!(store.load_ai_api_override("/dev/acme").unwrap(), None);
}
