//! The tests of the conversational agent registry
//! (`../../specifications/core/AGR-agent-registry.md`).

use super::*;
use crate::ai_api::AiApiRecord;
use crate::ai_shared::{ModelOption, ModelReasoning, ModelsOrigin};

fn store() -> GlobalSettingsStore {
    GlobalSettingsStore::in_memory()
}

fn ladder() -> ModelReasoning {
    ModelReasoning {
        mandatory: false,
        supported_efforts: Some(vec!["high".into(), "medium".into(), "low".into()]),
        default_effort: None,
        ..Default::default()
    }
}

/// A verified provider offering `models`. `verified_at` is what separates a
/// verified record from a merely-configured one (AAP-FR-13).
fn verified(provider: &str, models: Vec<ModelOption>) -> AiApiRecord {
    AiApiRecord {
        turn_timeout_ms: None,
        provider: provider.into(),
        base_url: Some(format!("https://{provider}.example/v1")),
        masked_hint: Some("abcd".into()),
        verified_at: Some("2026-01-01T00:00:00Z".into()),
        selected_model: None,
        models_origin: ModelsOrigin::Probed,
        selected_reasoning: None,
        models,
    }
}

/// Configured — it has a base URL — but never verified.
fn unverified(provider: &str) -> AiApiRecord {
    AiApiRecord {
        provider: provider.into(),
        base_url: Some(format!("https://{provider}.example/v1")),
        models: vec![ModelOption::new("m", "M")],
        ..Default::default()
    }
}

fn with_openrouter(store: &GlobalSettingsStore) {
    store
        .save_ai_api_registry(
            vec![verified(
                "openrouter",
                vec![ModelOption::new("m", "M").with_reasoning(Some(ladder()))],
            )],
            None,
        )
        .unwrap();
}

fn draft(nickname: &str) -> AgentDraft {
    AgentDraft {
        nickname: nickname.into(),
        title: String::new(),
        model_id: "m".into(),
        instructions: String::new(),
        reasoning: None,
    }
}

// -- AGR-FR-02, AGR-FR-22 / AGR-FR-02: the empty machine -------------------------

#[test]
fn a_fresh_machine_lists_no_agent_and_raises_nothing() {
    // AGR-FR-22 / AGR-FR-02: the registry answers before any project has
    // been opened, because a definition belongs to the machine.
    let store = store();
    assert_eq!(list_agents_impl(&store).unwrap(), vec![]);
}

#[test]
fn the_project_scoped_reads_answer_with_an_empty_key_and_the_registry_answers_too() {
    // AGR-FR-02, AGR-FR-22's storage half. The *command* refusal for a closed project
    // is `require_project`'s and is asserted in the Tauri-level test below;
    // the impls take a key and are exercised with one.
    let store = store();
    assert_eq!(list_project_agents_impl(&store, "proj").unwrap(), vec![]);
    assert_eq!(list_agents_impl(&store).unwrap(), vec![]);
}

// -- AGR-FR-01, AGR-FR-02, AGR-FR-03, AGR-FR-09: create round-trips ------------------------------------

#[test]
fn a_created_agent_carries_an_id_two_timestamps_and_every_supplied_field() {
    // AGR-FR-02, AGR-FR-03, AGR-FR-09 / AGR-FR-01, FR-02, FR-03, FR-09.
    let store = store();
    with_openrouter(&store);
    let created = create_agent_impl(
        &store, "",
        &AgentDraft {
            nickname: "arch".into(),
            title: "Architect".into(),
            model_id: "m".into(),
            instructions: "Argue about structure.".into(),
            reasoning: Some(ReasoningChoice::Effort {
                effort: "high".into(),
            }),
        },
    )
    .unwrap();

    assert!(!created.id.is_empty());
    assert!(!created.created_at.is_empty());
    assert!(!created.updated_at.is_empty());
    assert_eq!(created.nickname, "arch");
    assert_eq!(created.instructions, "Argue about structure.");
    assert_eq!(
        created.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        })
    );
    // The "relaunch" half: read back out of the store rather than from the
    // returned value.
    assert_eq!(list_agents_impl(&store).unwrap(), vec![created]);
}

// -- AGR-FR-04, AGR-FR-09, AGR-FR-10 / AGR-FR-05: the nickname ------------------------------

#[test]
fn an_invalid_nickname_is_refused_by_kind_and_writes_nothing() {
    // AGR-FR-09, AGR-FR-10 / AGR-FR-04, FR-09.
    let store = store();
    with_openrouter(&store);
    for (nickname, expected) in [
        ("", ERR_NICKNAME_EMPTY),
        ("two words", ERR_NICKNAME_INVALID),
        ("a@b", ERR_NICKNAME_INVALID),
        ("tab\there", ERR_NICKNAME_INVALID),
        // The reserved handle, in each spelling the case-insensitive
        // comparison has to cover.
        ("all", ERR_NICKNAME_RESERVED),
        ("ALL", ERR_NICKNAME_RESERVED),
        ("All", ERR_NICKNAME_RESERVED),
    ] {
        assert_eq!(
            create_agent_impl(&store, "", &draft(nickname)).unwrap_err(),
            expected,
            "nickname {nickname:?}",
        );
        assert_eq!(list_agents_impl(&store).unwrap(), vec![]);
    }
}

#[test]
fn renaming_an_agent_to_the_reserved_handle_is_refused() {
    // AGR-FR-09, AGR-FR-10 / AGR-FR-04, FR-10: the reservation binds `update_agent`
    // under exactly the validation `create_agent` applies, so an agent
    // cannot be renamed into the handle after the fact.
    let store = store();
    with_openrouter(&store);
    let created = create_agent_impl(&store, "", &draft("arch")).unwrap();
    assert_eq!(
        update_agent_impl(&store, "", &created.id, &draft("All")).unwrap_err(),
        ERR_NICKNAME_RESERVED,
    );
    // AGR-FR-09: a rejected update leaves the record byte-for-byte as it was.
    assert_eq!(list_agents_impl(&store).unwrap(), vec![created]);
}

#[test]
fn a_nickname_is_unique_without_regard_to_case_on_create_and_on_update() {
    // AGR-FR-05.
    let store = store();
    with_openrouter(&store);
    create_agent_impl(&store, "", &draft("arch")).unwrap();
    assert_eq!(
        create_agent_impl(&store, "", &draft("ARCH")).unwrap_err(),
        ERR_NICKNAME_TAKEN,
    );
    assert_eq!(list_agents_impl(&store).unwrap().len(), 1);

    let sec = create_agent_impl(&store, "", &draft("sec")).unwrap();
    assert_eq!(
        update_agent_impl(&store, "", &sec.id, &draft("Arch")).unwrap_err(),
        ERR_NICKNAME_TAKEN,
    );
    let after = list_agents_impl(&store).unwrap();
    assert_eq!(after.iter().filter(|a| a.nickname == "sec").count(), 1);
    assert_eq!(after.iter().filter(|a| a.nickname == "arch").count(), 1);
}

#[test]
fn an_agent_may_keep_its_own_nickname_through_an_update() {
    // AGR-FR-05's exclusion: an agent is not a collision with itself.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    let mut d = draft("arch");
    d.instructions = "changed".into();
    let updated = update_agent_impl(&store, "", &arch.id, &d).unwrap();
    assert_eq!(updated.instructions, "changed");
}

// -- AGR-FR-06, AGR-FR-09 / AGR-FR-07: provider, model, reasoning ----------------

// AGR-FR-06, AGR-FR-09: nothing is written when no provider resolves or when
// the active provider does not serve the model.
#[test]
fn the_model_is_validated_against_the_active_provider_and_nothing_is_written_on_refusal() {
    let store = store();

    // Nothing configured at all.
    assert_eq!(
        create_agent_impl(&store, "", &draft("a")).unwrap_err(),
        ERR_PROVIDER_UNCONFIGURED,
    );

    // Providers exist, but only a configured-never-verified one.
    store
        .save_ai_api_registry(vec![unverified("openai")], None)
        .unwrap();
    assert_eq!(
        create_agent_impl(&store, "", &draft("a")).unwrap_err(),
        ERR_PROVIDER_NOT_VERIFIED,
    );

    // Two verified providers and none chosen: nothing resolves.
    store
        .save_ai_api_registry(
            vec![
                verified("anthropic", vec![ModelOption::new("claude", "Claude")]),
                verified("openai", vec![ModelOption::new("gpt", "GPT")]),
            ],
            None,
        )
        .unwrap();
    assert_eq!(
        create_agent_impl(&store, "", &draft("a")).unwrap_err(),
        ERR_PROVIDER_NOT_VERIFIED,
    );

    // The active provider does not offer the model.
    store
        .save_ai_api_registry(
            vec![
                verified("anthropic", vec![ModelOption::new("claude", "Claude")]),
                verified("openai", vec![ModelOption::new("gpt", "GPT")]),
            ],
            Some("anthropic".into()),
        )
        .unwrap();
    let mut d = draft("a");
    d.model_id = "gpt".into();
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap_err(),
        ai_api::ERR_UNKNOWN_MODEL,
    );

    assert_eq!(list_agents_impl(&store).unwrap(), vec![]);
}

// AGR-FR-06, AGR-FR-16, AGR-FR-17, AGR-FR-20: an agent follows the active
// provider; switching it neither rewrites the agent nor loses it.
#[test]
fn switching_the_active_provider_marks_an_agent_whose_model_is_not_served_and_rewrites_nothing() {
    let store = store();
    let both = |active: &str| {
        store
            .save_ai_api_registry(
                vec![
                    verified("anthropic", vec![ModelOption::new("claude", "Claude")]),
                    verified(
                        "custom",
                        vec![ModelOption::new("gw-model", "gw-model")],
                    ),
                ],
                Some(active.to_string()),
            )
            .unwrap();
    };
    both("anthropic");
    let mut d = draft("arch");
    d.model_id = "claude".into();
    let arch = create_agent_impl(&store, "", &d).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();
    assert_eq!(
        list_project_agents_impl(&store, "A").unwrap()[0].availability,
        AgentAvailability::Ready,
    );

    both("custom");
    let listed = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(listed.len(), 1, "the agent is retained");
    assert_eq!(listed[0].availability, AgentAvailability::ModelUnavailable);
    assert_eq!(listed[0].agent.model_id, "claude", "the model is not rewritten");
    assert_eq!(
        resolve_project_agent(&store, "A", "arch").unwrap_err(),
        ERR_AGENT_UNAVAILABLE,
    );

    // Selecting a model the new provider serves makes the agent available again.
    let mut fixed = draft("arch");
    fixed.model_id = "gw-model".into();
    update_agent_impl(&store, "", &arch.id, &fixed).unwrap();
    assert_eq!(
        resolve_project_agent(&store, "A", "arch").unwrap().model_id,
        "gw-model",
    );

    // Switching back returns the first agent state to unavailable for that model.
    both("anthropic");
    assert_eq!(
        list_project_agents_impl(&store, "A").unwrap()[0].availability,
        AgentAvailability::ModelUnavailable,
    );
}

// AGR-FR-06, AAP-FR-16: the project override picks the provider an agent is
// validated against and served by.
#[test]
fn the_project_override_selects_the_provider_an_agent_is_read_against() {
    let store = store();
    store
        .save_ai_api_registry(
            vec![
                verified("anthropic", vec![ModelOption::new("claude", "Claude")]),
                verified("openai", vec![ModelOption::new("gpt", "GPT")]),
            ],
            Some("anthropic".into()),
        )
        .unwrap();
    store.save_ai_api_override("proj", "openai").unwrap();

    let mut d = draft("a");
    d.model_id = "gpt".into();
    assert!(create_agent_impl(&store, "proj", &d).is_ok());
    // Without the override the same model is not served.
    d.nickname = "b".into();
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap_err(),
        ai_api::ERR_UNKNOWN_MODEL,
    );
}

// AGR-FR-20, GSS-FR-30: a stored record written with a `provider` field loads.
#[test]
fn a_stored_record_with_a_provider_field_still_loads() {
    let text = r#"
id = "a1"
nickname = "arch"
provider = "openrouter"
modelId = "m"
"#;
    let agent: Agent = toml::from_str(text).unwrap();
    assert_eq!(agent.model_id, "m");
    let written = toml::to_string(&agent).unwrap();
    assert!(!written.contains("provider"));
}

#[test]
fn the_reasoning_choice_is_validated_against_the_chosen_model() {
    // AGR-FR-07.
    let store = store();
    store
        .save_ai_api_registry(
            vec![verified(
                "openrouter",
                vec![
                    ModelOption::new("plain", "Plain"),
                    ModelOption::new("ladder", "Ladder").with_reasoning(Some(ladder())),
                    ModelOption::new("always", "Always").with_reasoning(Some(
                        ModelReasoning {
                            mandatory: true,
                            supported_efforts: None,
                            default_effort: None,
                            ..Default::default()
                        },
                    )),
                ],
            )],
            None,
        )
        .unwrap();

    let mut d = draft("a");
    d.model_id = "plain".into();
    d.reasoning = Some(ReasoningChoice::On);
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap_err(),
        ai_api::ERR_REASONING_UNSUPPORTED,
    );

    d.model_id = "ladder".into();
    d.reasoning = Some(ReasoningChoice::Effort {
        effort: "absurd".into(),
    });
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap_err(),
        ai_api::ERR_UNKNOWN_EFFORT,
    );

    d.model_id = "always".into();
    d.reasoning = Some(ReasoningChoice::Off);
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap_err(),
        ai_api::ERR_REASONING_MANDATORY,
    );

    // A null choice is the model's own default and is always accepted.
    d.reasoning = None;
    assert!(create_agent_impl(&store, "", &d).is_ok());
}

// -- AGR-FR-23, AGR-FR-10 … AGR-FR-24, GSS-FR-30: the title ---------------------------------

#[test]
fn a_title_is_trimmed_and_survives_a_round_trip_with_its_own_shape_intact() {
    // AGR-FR-23, AGR-FR-10.
    let store = store();
    with_openrouter(&store);

    let mut d = draft("arch");
    d.title = "  Lead  UI/UX   Designer  ".into();
    let created = create_agent_impl(&store, "", &d).unwrap();
    // Only the ends go: the internal run of spaces, the slash, and the
    // casing are the author's and are none of this module's business.
    assert_eq!(created.title, "Lead  UI/UX   Designer");

    // Read back from the store rather than from the return value, so the
    // trimmed form is what was *written* rather than only what was handed
    // back. This store is in-memory, so it is not a serialisation round
    // trip — the on-disk one is `global_settings`' own (GSS-FR-30).
    let reloaded = store.load_agent_registry().unwrap();
    assert_eq!(reloaded[0].title, "Lead  UI/UX   Designer");

    // AGR-FR-10: an update that changes the title alone changes the title
    // alone. `updated_at` moves because the record was written; nothing
    // else about the persona does.
    let mut u = d.clone();
    u.title = "Architect".into();
    let updated = update_agent_impl(&store, "", &created.id, &u).unwrap();
    assert_eq!(updated.title, "Architect");
    assert_eq!(updated.id, created.id);
    assert_eq!(updated.nickname, created.nickname);
    assert_eq!(updated.model_id, created.model_id);
    assert_eq!(updated.instructions, created.instructions);
    assert_eq!(updated.reasoning, created.reasoning);
    assert_eq!(updated.created_at, created.created_at);
    // AGR-FR-10 refreshes `updated_at`. Compared lexically rather than with
    // `assert_ne!`: `now_rfc3339` is second-resolution, so two writes inside
    // one second are legitimately equal and an inequality assertion would
    // fail on a fast machine and pass on a slow one.
    assert!(updated.updated_at >= created.updated_at);
}

#[test]
fn an_absent_an_empty_and_a_whitespace_only_title_all_store_as_empty() {
    // AGR-FR-23. Saving succeeds in every case and no error
    // names the title — it is optional in the sense that carrying none is
    // an ordinary state, not in the sense that it is validated leniently.
    let store = store();
    with_openrouter(&store);

    for (i, supplied) in ["", "   ", "\t\n ", "\u{00a0}"].iter().enumerate() {
        let mut d = draft(&format!("a{i}"));
        d.title = (*supplied).into();
        let created = create_agent_impl(&store, "", &d)
            .unwrap_or_else(|e| panic!("title {supplied:?} was refused: {e}"));
        assert_eq!(created.title, "", "title {supplied:?} stores as empty");
    }

    // The same holds on update: AGR-FR-23 says no error *either* command
    // returns names the title.
    let existing = create_agent_impl(&store, "", &draft("updatable")).unwrap();
    let mut u = draft("updatable");
    u.title = "   ".into();
    assert_eq!(
        update_agent_impl(&store, "", &existing.id, &u).unwrap().title,
        "",
    );

    // Braces are text. A title reading like a prompt tag is stored as
    // typed and expanded by nothing (CVL-FR-04 substitutes it once and
    // never re-scans it).
    let mut d = draft("braces");
    d.title = "{{ agent-title }}".into();
    assert_eq!(
        create_agent_impl(&store, "", &d).unwrap().title,
        "{{ agent-title }}",
    );
}

#[test]
fn a_record_written_before_the_field_existed_reads_as_an_empty_title() {
    // GSS-FR-30 / AGR-FR-24. The registry is `#[serde(default)]`, so an
    // older `synthesis.toml` loads rather than tripping GSS-FR-13's
    // repair-to-defaults — which would take the recents and every other
    // registry with it.
    let older = r#"
id = "a1"
nickname = "arch"
provider = "openrouter"
modelId = "m"
instructions = "Argue about structure."
createdAt = "2026-07-01T09:00:00Z"
updatedAt = "2026-07-01T09:00:00Z"
"#;
    let agent: Agent = toml::from_str(older).expect("an older record still loads");
    assert_eq!(agent.title, "");
    assert_eq!(agent.nickname, "arch");
    assert_eq!(agent.instructions, "Argue about structure.");
    assert_eq!(agent.created_at, "2026-07-01T09:00:00Z");
}

// -- AGR-FR-08: instructions ------------------------------------------

#[test]
fn instructions_are_stored_verbatim_and_an_empty_value_is_ordinary() {
    // AGR-FR-08.
    let store = store();
    with_openrouter(&store);
    let empty = create_agent_impl(&store, "", &draft("a")).unwrap();
    assert_eq!(empty.instructions, "");

    let mut d = draft("b");
    d.instructions = "# Heading\n\n- one\n- two\n".into();
    let rich = create_agent_impl(&store, "", &d).unwrap();
    assert_eq!(rich.instructions, "# Heading\n\n- one\n- two\n");
    let read_back = list_agents_impl(&store)
        .unwrap()
        .into_iter()
        .find(|a| a.id == rich.id)
        .unwrap();
    assert_eq!(read_back.instructions, "# Heading\n\n- one\n- two\n");
}

// -- AGR-FR-03, AGR-FR-10: update -------------------------------------------------

#[test]
fn update_replaces_every_field_keeps_created_at_and_the_id_and_refuses_a_missing_id() {
    // AGR-FR-10 / AGR-FR-03, FR-10.
    let store = store();
    store
        .save_ai_api_registry(
            vec![verified(
                "openrouter",
                vec![ModelOption::new("m", "M"), ModelOption::new("m2", "M2")],
            )],
            None,
        )
        .unwrap();
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();

    let updated = update_agent_impl(
        &store,
        "",
        &arch.id,
        &AgentDraft {
            nickname: "architect".into(),
            title: String::new(),
            model_id: "m2".into(),
            instructions: "new".into(),
            reasoning: None,
        },
    )
    .unwrap();

    assert_eq!(updated.id, arch.id);
    assert_eq!(updated.created_at, arch.created_at);
    assert_eq!(updated.nickname, "architect");
    assert_eq!(updated.model_id, "m2");
    assert_eq!(updated.instructions, "new");

    assert_eq!(
        update_agent_impl(&store, "", "nope", &draft("x")).unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );
}

// -- AGR-FR-11 through AGR-FR-12: enrolment ----------------------------

#[test]
fn deleting_an_agent_clears_it_from_every_project_and_is_idempotent() {
    // AGR-FR-11, and GSS-FR-31's prune.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();
    enrol_project_agent_impl(&store, "B", &arch.id).unwrap();

    delete_agent_impl(&store, &arch.id).unwrap();
    assert_eq!(list_agents_impl(&store).unwrap(), vec![]);
    assert_eq!(list_project_agents_impl(&store, "A").unwrap(), vec![]);
    assert_eq!(list_project_agents_impl(&store, "B").unwrap(), vec![]);
    // The slot itself was rewritten, not merely filtered on read.
    assert_eq!(store.load_project_agent_enrolment("A").unwrap(), vec![] as Vec<String>);
    assert!(delete_agent_impl(&store, &arch.id).is_ok());
}

#[test]
fn a_project_lists_only_its_enrolment_while_the_registry_lists_everything() {
    // AGR-FR-13 / AGR-FR-12, FR-13. Ordering is by nickname without regard
    // to case, which `Zed` before `arch` would break if it were byte order.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    create_agent_impl(&store, "", &draft("Zed")).unwrap();
    create_agent_impl(&store, "", &draft("bee")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();

    let project = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(project.len(), 1);
    assert_eq!(project[0].agent.nickname, "arch");

    let nicknames: Vec<String> = list_agents_impl(&store)
        .unwrap()
        .into_iter()
        .map(|a| a.nickname)
        .collect();
    assert_eq!(nicknames, vec!["arch", "bee", "Zed"]);
}

#[test]
fn enrolment_and_removal_are_idempotent_and_removal_keeps_the_definition() {
    // AGR-FR-15 / AGR-FR-14, FR-15.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();

    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();
    let twice = enrol_project_agent_impl(&store, "A", &arch.id).unwrap();
    assert_eq!(twice.len(), 1);
    assert_eq!(store.load_project_agent_enrolment("A").unwrap().len(), 1);

    assert_eq!(
        enrol_project_agent_impl(&store, "A", "nope").unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );

    remove_project_agent_impl(&store, "A", &arch.id).unwrap();
    assert!(remove_project_agent_impl(&store, "A", &arch.id).is_ok());
    assert_eq!(list_agents_impl(&store).unwrap().len(), 1);
}

#[test]
fn an_enrolment_is_one_fact_per_project_key_not_per_worktree() {
    // AGR-FR-12. The key IS the project (GSS-FR-18), so
    // "changing worktree" is "reading the same key again".
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    enrol_project_agent_impl(&store, "proj", &arch.id).unwrap();
    assert_eq!(list_project_agents_impl(&store, "proj").unwrap().len(), 1);
    assert_eq!(list_project_agents_impl(&store, "other").unwrap().len(), 0);
}

// -- AGR-FR-16, AGR-FR-17 / AGR-FR-16: availability -------------------------------

#[test]
fn availability_degrades_by_kind_and_the_agent_is_still_returned() {
    // AGR-FR-17 / AGR-FR-16, FR-17.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();

    let ready = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(ready[0].availability, AgentAvailability::Ready);

    // Provider cleared entirely.
    store.save_ai_api_registry(vec![], None).unwrap();
    let cleared = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(cleared.len(), 1, "the agent is retained, not pruned");
    assert_eq!(
        cleared[0].availability,
        AgentAvailability::ProviderUnconfigured,
    );

    // Configured but never verified.
    store
        .save_ai_api_registry(vec![unverified("openrouter")], None)
        .unwrap();
    assert_eq!(
        list_project_agents_impl(&store, "A").unwrap()[0].availability,
        AgentAvailability::ProviderUnverified,
    );

    // Verified, but the model list no longer offers the agent's model.
    store
        .save_ai_api_registry(
            vec![verified(
                "openrouter",
                vec![ModelOption::new("something-else", "Else")],
            )],
            None,
        )
        .unwrap();
    let gone = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0].availability, AgentAvailability::ModelUnavailable);
}

#[test]
fn availability_is_computed_without_a_keychain_or_a_network() {
    // AGR-FR-16. `availability_of` goes through
    // `validate_ai_api_selection`, which reads the registry alone — so a
    // roster renders on a machine that is offline with its keychain shut.
    // Asserted structurally: this store has no `AiApiIntegrations` at all,
    // so there is no prober to reach and no secret store to open, and the
    // computation still answers.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();

    let listed = list_project_agents_impl(&store, "A").unwrap();
    assert_eq!(listed[0].availability, AgentAvailability::Ready);
    assert_eq!(availability_of(&store, "", &arch), AgentAvailability::Ready);
}

// -- AGR-FR-18, AGR-FR-04: resolution ---------------------------------------------

#[test]
fn a_nickname_resolves_only_against_the_projects_own_enrolment() {
    // AGR-FR-04 / AGR-FR-18.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    create_agent_impl(&store, "", &draft("sec")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();

    assert_eq!(
        resolve_project_agent(&store, "A", "ARCH").unwrap().id,
        arch.id,
    );
    // Defined on the machine, but not enrolled here.
    assert_eq!(
        resolve_project_agent(&store, "A", "sec").unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );
    // AGR-FR-04: the reserved handle is a nickname no agent can carry, so it
    // resolves to nothing here — the expansion happens in the surface that
    // read the tag, and every dispatch reaching this module names one agent.
    assert_eq!(
        resolve_project_agent(&store, "A", "all").unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );

    store.save_ai_api_registry(vec![], None).unwrap();
    assert_eq!(
        resolve_project_agent(&store, "A", "arch").unwrap_err(),
        ERR_AGENT_UNAVAILABLE,
    );
}

// -- AGR-FR-19 / AGR-FR-02, AGR-FR-20: one definition, no secret ------------------

#[test]
fn one_definition_is_shared_by_every_project_that_enrolled_it() {
    // AGR-FR-19: there is no per-project copy to diverge.
    let store = store();
    with_openrouter(&store);
    let arch = create_agent_impl(&store, "", &draft("arch")).unwrap();
    enrol_project_agent_impl(&store, "A", &arch.id).unwrap();
    enrol_project_agent_impl(&store, "B", &arch.id).unwrap();

    let mut d = draft("arch");
    d.instructions = "edited while A was open".into();
    update_agent_impl(&store, "", &arch.id, &d).unwrap();

    assert_eq!(
        list_project_agents_impl(&store, "B").unwrap()[0]
            .agent
            .instructions,
        "edited while A was open",
    );
}

#[test]
fn the_serialised_registry_carries_descriptions_and_no_key() {
    // AGR-FR-20 / AGR-FR-02, FR-20. The record type itself has no field a
    // key could occupy; this asserts the serialised form to catch one being
    // added.
    let store = store();
    with_openrouter(&store);
    let mut d = draft("arch");
    d.instructions = "Argue about structure.".into();
    d.reasoning = Some(ReasoningChoice::Effort {
        effort: "high".into(),
    });
    create_agent_impl(&store, "", &d).unwrap();

    let text = toml::to_string(&AgentsOnly {
        agents: store.load_agent_registry().unwrap(),
    })
    .unwrap();
    assert!(text.contains("arch"));
    // AGR-FR-20: no provider id is stored in an agent.
    assert!(!text.contains("openrouter"));
    assert!(!text.to_lowercase().contains("provider"));
    assert!(text.contains("Argue about structure."));
    assert!(text.contains("high"));
    assert!(!text.to_lowercase().contains("key"));
}

#[derive(Serialize)]
struct AgentsOnly {
    agents: Vec<Agent>,
}

// -- Registration guards ------------------------------------------------

#[test]
fn the_registry_commands_are_in_scope() {
    // A rename that is not propagated to `generate_handler!` fails to
    // compile here rather than at runtime, matching the convention every
    // other domain module follows.
    let _ = list_agents;
    let _ = create_agent;
    let _ = update_agent;
    let _ = delete_agent::<tauri::test::MockRuntime>;
    let _ = list_project_agents;
    let _ = enrol_project_agent;
    let _ = remove_project_agent::<tauri::test::MockRuntime>;
}
