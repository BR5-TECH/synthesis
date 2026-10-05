//! SPS-FR-01, SPS-FR-02: the tool's identity, description and schema.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-01 — the tool's identity (SPS-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn the_tool_is_a_portable_tool_named_for_the_contract() {
    let app = closed_project();
    let tool = SpecSearchTool::new(app.handle().clone());
    let definition = rig::tool::portable_tool_definition(&tool);

    assert_eq!(
        <SpecSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "search_specifications",
    );
    assert_eq!(definition.name, "search_specifications");
}

// ---------------------------------------------------------------------------
// SPS-FR-02 — description and schema (SPS-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn the_description_and_schema_are_the_contract_surfaces_own_text() {
    let app = closed_project();
    let tool = SpecSearchTool::new(app.handle().clone());

    assert_eq!(tool.description(), DESCRIPTION);

    let schema = tool.parameters();
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    assert_eq!(schema["properties"]["query"]["description"], QUERY_DESCRIPTION);
    assert_eq!(schema["properties"]["limit"]["description"], LIMIT_DESCRIPTION);

    // TLC-FR-06: a bounded parameter states its bound where a model will read
    // it, rather than leaving it to be inferred from the schema.
    assert!(LIMIT_DESCRIPTION.contains('5'), "the default is stated");
    assert!(LIMIT_DESCRIPTION.contains("20"), "the ceiling is stated");

    // Pinned to the spec so a reworded description here fails rather than
    // silently drifting from the file the model's behaviour was designed against.
    const SPEC: &str = include_str!(
        "../../../../../specifications/tools/SPS-specification-search-tool.md"
    );
    assert!(SPEC.contains(DESCRIPTION), "the description is the spec's");
    assert!(SPEC.contains(QUERY_DESCRIPTION));
    assert!(SPEC.contains(LIMIT_DESCRIPTION));
}

