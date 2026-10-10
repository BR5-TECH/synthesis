//! RFT-FR-01, RFT-FR-02: the tool's identity, description and schema.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-01 — the tool's identity (RFT-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn the_tool_is_a_portable_tool_named_for_the_contract() {
    let app = closed_project();
    let tool = FileReadTool::new(app.handle().clone(), "identity");
    let definition = rig::tool::tool_definition(&tool);

    assert_eq!(
        <FileReadTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "read_file",
    );
    assert_eq!(definition.name, "read_file");
}

// ---------------------------------------------------------------------------
// RFT-FR-02 — description and schema (RFT-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn the_description_and_schema_are_the_contract_surfaces_own_text() {
    let app = closed_project();
    let tool = FileReadTool::new(app.handle().clone(), "schema");

    assert_eq!(tool.description(), DESCRIPTION);

    let schema = tool.parameters();
    assert_eq!(schema["required"], serde_json::json!(["path"]));
    assert_eq!(schema["properties"]["path"]["description"], PATH_DESCRIPTION);
    assert_eq!(schema["properties"]["offset"]["description"], OFFSET_DESCRIPTION);
    assert_eq!(schema["properties"]["limit"]["description"], LIMIT_DESCRIPTION);

    // TLC-FR-06: each bounded parameter states its own default and what an
    // out-of-range value does, where the model will actually read it.
    assert!(OFFSET_DESCRIPTION.contains('0'), "offset states its default");
    assert!(
        OFFSET_DESCRIPTION.contains("past the end"),
        "offset states what reading past the end does",
    );
    assert!(
        LIMIT_DESCRIPTION.contains("rest of the file"),
        "limit states its default",
    );
    assert!(
        LIMIT_DESCRIPTION.contains("below 1"),
        "limit states how an out-of-range value is treated",
    );

    const SPEC: &str = include_str!("../../../../../specifications/tools/RFT-read-file-tool.md");
    assert!(SPEC.contains(DESCRIPTION), "the description is the spec's");
    assert!(SPEC.contains(PATH_DESCRIPTION));
    assert!(SPEC.contains(OFFSET_DESCRIPTION));
    assert!(SPEC.contains(LIMIT_DESCRIPTION));
}

