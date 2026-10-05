//! NST-FR-01, NST-FR-02: the contract surface.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-01, NST-FR-02: the contract surface
// ---------------------------------------------------------------------------

/// NST-FR-01: the tool is a `rig` portable tool under the documented name, and
/// nothing about it is a Tauri command or an event.
#[test]
fn is_a_portable_tool_named_search_notes() {
    let fixture = NoteFixture::new();
    let tool = fixture.tool();
    assert_eq!(NAME, "search_notes");
    assert_eq!(
        <NoteSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        NAME
    );
    assert_eq!(rig::tool::portable_tool_definition(&tool).name, NAME);

    // Absent from `invoke_handler`, and no event belongs to it. The claim rests
    // on an ABSENCE, so it is asserted against the handler's own text.
    const LIB: &str = include_str!("../../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    assert!(!handler.contains(NAME));
    const SOURCE: &str = include_str!("../../note_search.rs");
    let code: String = SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!code.contains("#[tauri::command]"));
    assert!(!code.contains(".emit("));
}

/// NST-FR-02: the description and the schema are the documented ones.
#[test]
fn the_description_and_schema_are_the_documented_ones() {
    let fixture = NoteFixture::new();
    let tool = fixture.tool();
    assert_eq!(tool.description(), DESCRIPTION);
    assert!(DESCRIPTION.starts_with("Find the notes most relevant to a topic"));

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    let query = &schema["properties"]["query"];
    assert_eq!(query["type"], "string");
    assert_eq!(query["description"], QUERY_DESCRIPTION);
    assert!(!QUERY_DESCRIPTION.is_empty());
    let limit = &schema["properties"]["limit"];
    assert_eq!(limit["type"], "integer");
    assert_eq!(limit["description"], LIMIT_DESCRIPTION);
    // TLC-FR-06: a parameter with a default and a bounded range states both.
    // Derived from the constants rather than spelled out, so changing
    // `DEFAULT_LIMIT` without changing the sentence a model reads turns this
    // red rather than leaving the two quietly disagreeing.
    assert!(
        LIMIT_DESCRIPTION.contains(&format!("Defaults to {DEFAULT_LIMIT}")),
        "the default is stated: {LIMIT_DESCRIPTION:?}",
    );
    assert!(
        LIMIT_DESCRIPTION.contains(&format!("below {MIN_LIMIT} or above {MAX_LIMIT}")),
        "both bounds are stated: {LIMIT_DESCRIPTION:?}",
    );

    // TLC-FR-05: fixed, so two instances answer identically whatever the state.
    let closed = NoteSearchTool::new(crate::tools::tests::closed_project().handle().clone(), "x");
    assert_eq!(closed.description(), tool.description());
    assert_eq!(closed.parameters(), tool.parameters());
}
