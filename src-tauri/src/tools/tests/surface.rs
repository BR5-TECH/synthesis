//! Read-only behaviour, credentials, the loop-facing tool, and the absent registry.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-17 / TLC-FR-18 — read-only, and no credential anywhere
// ---------------------------------------------------------------------------

#[test]
fn calling_the_tools_changes_nothing_on_disk() {
    // TLC-FR-17.
    let fixture = demo_project();
    let root = {
        let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.root().expect("mounted")
    };
    let before = tree_snapshot(&root);

    let handle = fixture.handle();
    for query in ["review a specification", "deploy", "nothing at all matches"] {
        let _ = block_on(skill_search::SkillSearchTool::new(handle.clone()).call(
            skill_search::SkillSearchArgs {
                query: query.to_string(),
                limit: None,
            },
        ));
    }
    for _ in 0..5 {
        let _ = block_on(
            skill_list::SkillListTool::new(handle.clone())
                .call(skill_list::SkillListArgs::default()),
        );
    }
    // LSK-FR-18: the one tool that opens a file opens it for reading alone —
    // no mtime change, no truncation, nothing created beside it.
    for name in ["analyst", "deployer", "no such skill", ""] {
        let _ = block_on(skill_load::SkillLoadTool::new(handle.clone()).call(
            skill_load::LoadSkillArgs {
                name: name.to_string(),
                ecosystem: None,
            },
        ));
    }

    assert_eq!(
        before,
        tree_snapshot(&root),
        "TLC-FR-17: a read-only tool creates, modifies, and deletes nothing",
    );
}

#[test]
fn no_tool_reads_holds_or_returns_a_credential() {
    // TLC-FR-18. The group's whole source is searched: a tool that never names
    // a credential store cannot return one.
    for (name, source) in GROUP_SOURCES {
        // The module names matter more than the English words: a tool calling
        // `crate::github_tokens::…` would sail past a scan for "password".
        for forbidden in [
            "keyring",
            "api_key",
            "secret",
            "password",
            "token",
            "credential",
            "github_tokens",
            "global_settings",
            "ai_api",
            "agentic",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} must not reach for {forbidden:?} (TLC-FR-18)",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// TLC-FR-20 — the loop-facing tool (TLC-FR-20)
// ---------------------------------------------------------------------------

/// The group's one loop-facing member is reached from backend loop code through
/// a typed Rust API, and by no model.
///
/// All three limbs of TLC-FR-20: no `ToolDefinition` names it and no agent can
/// be constructed with it attached; its request type rejects an unknown field
/// and an out-of-range value rather than ignoring or clamping either; and its
/// failures are variants of its own enumeration rather than `ToolExecutionError`.
#[test]
fn the_loop_facing_tool_is_unreachable_by_any_model() {
    use crate::tools::agent_exec::protocol::{
        AgentTaskRequest, TaskInvalid, MAX_TIMEOUT_MS, MIN_TIMEOUT_MS,
    };

    // Limb 1 — no model-facing surface exists to put in a `ToolDefinition`.
    // The whole module is scanned, not just its entry point.
    const LOOP_FACING: [(&str, &str); 4] = [
        ("agent_exec.rs", include_str!("../agent_exec.rs")),
        ("agent_exec/protocol.rs", include_str!("../agent_exec/protocol.rs")),
        (
            "agent_exec/descriptor.rs",
            include_str!("../agent_exec/descriptor.rs"),
        ),
        ("agent_exec/runtime.rs", include_str!("../agent_exec/runtime.rs")),
    ];
    for (name, source) in LOOP_FACING {
        for declaration in [
            "impl rig::tool::PortableTool",
            "impl PortableTool",
            "fn parameters(",
            "fn description(",
            "const NAME",
            "#[tauri::command]",
        ] {
            assert!(
                !source
                    .lines()
                    .any(|line| line.trim_start().starts_with(declaration)),
                "{name} declares {declaration}, which would make it model-facing (TLC-FR-20)",
            );
        }
    }
    // And it is absent from the set of tools a caller attaches to an agent.
    for (name, _) in GROUP_SOURCES {
        assert_ne!(name, "agent_exec.rs", "the loop-facing tool is attachable");
    }

    // Limb 2 — strict decoding. A model-facing tool ignores an unknown field
    // and clamps an out-of-range number (TLC-FR-07); this one does neither.
    let with_unknown_field = r#"{
        "protocol_version": 1, "instruction": "go", "surprise": true,
        "execution": {"timeout_ms": 60000, "cancellation": "caller_controlled"}
    }"#;
    assert!(
        serde_json::from_str::<AgentTaskRequest>(with_unknown_field).is_err(),
        "an unknown field must be rejected, not ignored",
    );

    let out_of_range = format!(
        r#"{{"protocol_version": 1, "instruction": "go",
             "execution": {{"timeout_ms": {}, "cancellation": "caller_controlled"}}}}"#,
        MAX_TIMEOUT_MS + 1
    );
    let decoded: AgentTaskRequest =
        serde_json::from_str(&out_of_range).expect("it decodes; the range is a validation rule");
    match decoded.validate() {
        Err(TaskInvalid::TimeoutOutOfRange(value)) => {
            assert_eq!(value, MAX_TIMEOUT_MS + 1, "refused, and not clamped");
        }
        other => panic!("an out-of-range deadline must be refused, got {other:?}"),
    }
    // The bound itself is accepted, so the refusal discriminates.
    let mut ok = decoded;
    ok.execution.timeout_ms = MIN_TIMEOUT_MS;
    assert!(ok.validate().is_ok());

    // Limb 3 — failures are its own enumeration. `ToolExecutionError` and
    // `ToolErrorKind` are the model-facing refusal types and appear nowhere in
    // the module.
    for (name, source) in LOOP_FACING {
        for model_facing in ["ToolExecutionError", "ToolErrorKind", "map_error"] {
            assert!(
                !source.contains(model_facing),
                "{name} refuses through {model_facing}, which only a model reads",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// TLC-FR-19 — no registry (TLC-FR-19)
// ---------------------------------------------------------------------------

#[test]
fn the_group_publishes_no_registry() {
    // TLC-FR-19: a capability reaches a model only where a caller named it, so
    // adding a tool cannot silently widen an existing agent.
    // Comments are stripped first: this module documents the registry it
    // deliberately does not publish, and a naive scan would match that prose
    // and fail for the opposite of the reason this test exists.
    for (name, source) in GROUP_SOURCES {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["builtin_tools", "all_tools", "ALL_TOOLS", "fn registry"] {
            assert!(
                !code.contains(forbidden),
                "{name} must publish no registry, found {forbidden:?} (TLC-FR-19)",
            );
        }
    }
}

