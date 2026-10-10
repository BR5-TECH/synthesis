//! DST-FR-01, DST-FR-02: the contract surface.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-01, DST-FR-02: the contract surface
// ---------------------------------------------------------------------------

/// DST-FR-01: the tool is a `rig` portable tool under the documented name, and
/// nothing about it is a Tauri command.
#[test]
fn is_a_portable_tool_named_search_drafts() {
    let fixture = DraftFixture::new();
    let tool = fixture.tool();
    assert_eq!(NAME, "search_drafts");
    assert_eq!(
        <DraftSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        NAME
    );
    assert_eq!(rig::tool::tool_definition(&tool).name, NAME);

    // DST-FR-01: `crate::drafts`' Tauri command of the same name is the panel's
    // own substring filter and answers on its own terms. Two operations, two
    // surfaces, and no path from either to the other — asserted against a
    // worktree that actually holds a draft, so the two answers can differ.
    let id = fixture.draft("panel filter", "# Plan\n\nteardown\n");
    fixture.reindex();

    // The command matches on the *name* as a substring, which the tool cannot do
    // at all, and answers with its own shape.
    let by_name = crate::drafts::search_drafts_impl(&fixture.root(), "panel");
    assert_eq!(by_name.len(), 1, "the command matches a draft's name");
    assert_eq!(by_name[0].draft_id, id);
    assert_eq!(by_name[0].matched_in, crate::drafts::DraftMatchedIn::Name);
    let command_shape = serde_json::to_value(&by_name[0]).expect("serialises");
    for absent in ["score", "excerpt", "status", "name"] {
        assert!(
            command_shape.get(absent).is_none(),
            "the command's DraftMatch carries no {absent}; the tool's match does",
        );
    }
    assert!(
        fixture.search("panel", None).is_empty(),
        "the tool ranks prompt text and never a draft's name",
    );
}

/// DST-FR-03: the result is drawn from exactly what the delegated `search`
/// returned, and this tool ranks nothing of its own.
///
/// Asserted by running the same delegated call the tool makes and checking every
/// returned triple came out of it. A hand-rolled scan or a second, widened query
/// added tomorrow would produce a row with no counterpart here.
#[test]
fn every_result_comes_from_the_delegated_search_and_nothing_else() {
    let fixture = DraftFixture::new();
    for i in 0..6 {
        fixture.draft(
            &format!("draft {i}"),
            &format!("# A{i}\n\nteardown\n\n# B{i}\n\nteardown ordering\n"),
        );
    }
    fixture.reindex();

    let limit = 4usize;
    let matches = fixture.search("teardown", Some(limit as i64));
    let indexer = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let raw = crate::bm25_index::search(
        &indexer,
        &[crate::bm25_index::IndexId::Drafts],
        "teardown",
        limit * OVERFETCH_FACTOR,
    );

    assert_eq!(matches.len(), limit);
    for m in &matches {
        assert!(
            raw.iter().any(|hit| {
                hit.draft_id.as_deref() == Some(m.draft_id.as_str())
                    && hit.text == m.excerpt
                    && hit.score == m.score
            }),
            "every match is a hit the delegated search produced: {m:?}",
        );
        // DST-FR-05: and every hit came from the drafts index alone.
        assert!(raw.iter().all(|hit| hit.index == crate::bm25_index::IndexId::Drafts));
    }
}

/// DST-FR-02: the description is the fixed text, and the schema declares the two
/// documented arguments with their documented descriptions.
#[test]
fn definition_is_the_fixed_contract_surface() {
    let fixture = DraftFixture::new();
    let definition = rig::tool::tool_definition(&fixture.tool());
    assert_eq!(definition.description, DESCRIPTION);
    assert!(DESCRIPTION.contains("Only current live prompts are searched"));

    let schema = definition.parameters;
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    assert_eq!(schema["properties"]["query"]["description"], QUERY_DESCRIPTION);
    assert_eq!(schema["properties"]["limit"]["description"], LIMIT_DESCRIPTION);
    // TLC-FR-06: a bounded parameter states its default and its range in its own
    // description, a model not reliably inferring either from schema keywords.
    // Cross-checked against the constants `normalize_limit` applies, so changing
    // a bound without rewording the description fails here.
    assert!(LIMIT_DESCRIPTION.contains(&DEFAULT_LIMIT.to_string()));
    assert!(LIMIT_DESCRIPTION.contains(&MIN_LIMIT.to_string()));
    assert!(LIMIT_DESCRIPTION.contains(&MAX_LIMIT.to_string()));
    assert_eq!(normalize_limit(None), DEFAULT_LIMIT as usize);
    assert_eq!(normalize_limit(Some(i64::MAX)), MAX_LIMIT as usize);
}
