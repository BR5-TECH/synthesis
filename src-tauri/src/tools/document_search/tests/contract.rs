//! The contract surface: name, description, schema, output shape, refusals
//! (SDT-FR-LNNL, SDT-FR-JMSA, SDT-FR-ZWJW, SDT-FR-GGYO).

use super::*;

// SDT-FR-LNNL, SDT-FR-JMSA: the name, the fixed description, and the schema with
// the documented parameter descriptions.
#[test]
fn the_definition_is_the_documented_one() {
    let fixture = DocFixture::new();
    let tool = SearchDocumentsTool::new(fixture.fixture.handle());
    let definition = rig::tool::tool_definition(&tool);
    assert_eq!(definition.name, "search_documents");
    assert_eq!(NAME, "search_documents");
    assert_eq!(definition.description, DESCRIPTION);
    assert!(DESCRIPTION.starts_with("Search for Documents: "));
    assert!(DESCRIPTION.contains("Pass the id to Get Document to read the document."));

    let schema = definition.parameters;
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 2);
    assert_eq!(properties["query"]["type"], "string");
    assert_eq!(properties["limit"]["type"], "integer");
    assert!(properties["query"]["description"]
        .as_str()
        .unwrap()
        .contains("how the vendor API rate limits requests"));
    assert!(properties["limit"]["description"]
        .as_str()
        .unwrap()
        .contains("Defaults to 5"));
}

// SDT-FR-ZWJW: a match carries exactly `id`, `name`, `folder`, `format`,
// `snippet`, and `score`, and no filesystem path.
#[test]
fn a_match_carries_exactly_the_documented_fields_and_no_path() {
    let fixture = DocFixture::new();
    fixture.write("manuals/vendor/rate.md", "The vendor API rate limits requests per minute.");
    fixture.select(SourceKind::Folder, "manuals");
    let matches = search(&fixture, "vendor rate limits", None);
    assert_eq!(matches.len(), 1);
    let json = serde_json::to_value(&matches[0]).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["folder", "format", "id", "name", "score", "snippet"]);
    assert_eq!(json["id"], fixture.id_of("manuals/vendor/rate.md"));
    assert_eq!(json["name"], "rate.md");
    assert_eq!(json["folder"], "vendor");
    assert_eq!(json["format"], "markdown");
    let rendered = serde_json::to_string(&matches).unwrap();
    assert!(!rendered.contains(&*fixture.root.to_string_lossy()), "no filesystem path");
    let output = serde_json::to_value(SearchDocumentsOutput { documents: matches }).unwrap();
    assert!(output["documents"].is_array());
}

// SDT-FR-GGYO: an empty or blank query is the retryable `InvalidArgs` refusal, and
// no project open is the shared refusal.
#[test]
fn refusals_have_the_documented_kinds_and_messages() {
    let fixture = DocFixture::new();
    for query in ["", "   \n\t"] {
        let error = refusal(&fixture, query);
        assert_eq!(error, ToolRefusal::InvalidArguments(EMPTY_DOCUMENT_QUERY));
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert!(error.retryable());
        assert_eq!(
            error.to_string(),
            "The query must describe the topic you want reference documents about. Call again with a short plain-language description of it."
        );
    }

    let app = closed_project();
    let tool = SearchDocumentsTool::new(app.handle().clone());
    let error = block_on(tool.call(SearchDocumentsArgs {
        query: "anything".into(),
        limit: None,
    }))
    .unwrap_err();
    assert_eq!(error, ToolRefusal::NoProjectOpen);
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert!(!error.retryable());
}

// SDT-FR-GGYO: a project whose Documents collection is closed is no open project
// for this tool, even where the index is mounted.
#[test]
fn a_closed_collection_is_no_open_project() {
    let fixture = DocFixture::new();
    fixture.collection().close();
    assert_eq!(refusal(&fixture, "anything"), ToolRefusal::NoProjectOpen);
}

// SDT-FR-BTOJ: the limit is clamped into 1..=20 and defaults to 5, and a number a
// model spelled oddly still decodes.
#[test]
fn the_limit_is_clamped_and_decoded_leniently() {
    assert_eq!(normalize_limit(None), 5);
    assert_eq!(normalize_limit(Some(0)), 1);
    assert_eq!(normalize_limit(Some(-4)), 1);
    assert_eq!(normalize_limit(Some(7)), 7);
    assert_eq!(normalize_limit(Some(1000)), 20);
    for (value, expected) in [
        (serde_json::json!("3"), Some(3)),
        (serde_json::json!(4.9), Some(4)),
        (serde_json::json!(null), None),
        (serde_json::json!("soon"), None),
    ] {
        let args: SearchDocumentsArgs =
            serde_json::from_value(serde_json::json!({ "query": "q", "limit": value })).unwrap();
        assert_eq!(args.limit, expected);
    }
}
