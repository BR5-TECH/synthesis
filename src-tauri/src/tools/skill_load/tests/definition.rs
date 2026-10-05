//! LSK-FR-01, LSK-FR-02: the tool and its definition.

use super::*;

// ---------------------------------------------------------------------------
// LSK-FR-01 / LSK-FR-02 — the tool and its definition
// ---------------------------------------------------------------------------

#[test]
fn the_tool_is_a_portable_tool_named_load_skill() {
    // LSK-FR-01. The absence from `invoke_handler` is asserted for the whole
    // group in `tools/tests.rs`; what is this tool's own is its name.
    let app = closed_project();
    let definition = rig::tool::portable_tool_definition(&SkillLoadTool::new(app.handle().clone()));

    assert_eq!(definition.name, "load_skill");
    assert_eq!(
        <SkillLoadTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        definition.name,
        "LSK-FR-01: NAME and ToolDefinition.name are the same string",
    );
}

#[test]
fn the_definition_is_the_specs_text_and_documents_both_parameters() {
    let app = closed_project();
    let definition = rig::tool::portable_tool_definition(&SkillLoadTool::new(app.handle().clone()));
    assert_eq!(definition.description, DESCRIPTION);

    // Asserting `description() == DESCRIPTION` alone proves only that the
    // constant equals itself. TLC-FR-05 makes the text the contract, so it is
    // checked against the spec that defines it.
    const SPEC: &str = include_str!("../../../../../specifications/tools/LSK-load-skill-tool.md");
    assert!(
        SPEC.lines()
            .any(|line| line.trim().strip_prefix("> ") == Some(DESCRIPTION)),
        "LSK-FR-02: the description must be the spec's contract-surface text",
    );

    let schema = parameters();
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["required"],
        serde_json::json!(["name"]),
        "LSK-FR-02: `name` is required and `ecosystem` is not",
    );

    // LSK-FR-02: the `ecosystem` description names all four accepted values,
    // because a model reads the description and does not reliably infer a set
    // from a schema keyword (TLC-FR-06).
    let documented = schema["properties"]["ecosystem"]["description"]
        .as_str()
        .unwrap();
    for value in ["claude", "codex", "github", "opencode"] {
        assert!(
            documented.contains(value),
            "LSK-FR-02: {value:?} is named in the parameter's own description",
        );
        assert!(
            schema["properties"]["ecosystem"]["enum"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(value)),
            "and declared in the schema a provider constrains against",
        );
    }

    // Both parameter descriptions are pinned to the spec, not merely probed
    // for a phrase: TLC-FR-05 makes the text the model reads the contract, and
    // a reword away from the spec would otherwise go unnoticed.
    for pointer in [
        "/properties/name/description",
        "/properties/ecosystem/description",
    ] {
        let text = schema.pointer(pointer).and_then(|d| d.as_str()).unwrap();
        assert!(
            SPEC.contains(text),
            "LSK-FR-02: {pointer} must be the spec's text, got {text:?}",
        );
    }
}

