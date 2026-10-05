//! The contract surface and the refusals (GDT-FR-LITM, GDT-FR-GNCR, GDT-FR-FZHF,
//! GDT-FR-MVHY, GDT-FR-IPJG, GDT-FR-IFCA, GDT-FR-HREW).

use super::*;

// GDT-FR-LITM, GDT-FR-GNCR: the name, the fixed description, and the schema with
// the five documented arguments.
#[test]
fn the_definition_is_the_documented_one() {
    let fixture = DocFixture::new();
    let definition =
        rig::tool::portable_tool_definition(&GetDocumentTool::new(fixture.fixture.handle()));
    assert_eq!(definition.name, "get_document");
    assert_eq!(NAME, "get_document");
    assert_eq!(definition.description, DESCRIPTION);
    assert!(DESCRIPTION.starts_with("Get Document: "));
    let schema = definition.parameters;
    assert_eq!(schema["required"], serde_json::json!(["id"]));
    let properties = schema["properties"].as_object().unwrap();
    let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(names, ["byte_length", "byte_offset", "id", "line_limit", "line_offset"]);
    for (name, property) in properties {
        assert_eq!(property["type"], if name == "id" { "string" } else { "integer" });
        assert!(!property["description"].as_str().unwrap().is_empty());
    }
    assert!(properties["id"]["description"].as_str().unwrap().contains("A path cannot be used here."));
}

// GDT-FR-IPJG: a blank id is the retryable `InvalidArgs` refusal.
#[test]
fn a_blank_id_is_refused() {
    let fixture = DocFixture::new();
    for id in ["", "  \t"] {
        let error = get(&fixture, args(id)).unwrap_err();
        assert_eq!(error, ToolRefusal::InvalidArguments(DOCUMENT_ID_BLANK));
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert!(error.retryable());
        assert_eq!(
            error.to_string(),
            "The id must name a document. Give the id exactly as a search for documents reported it."
        );
    }
}

// GDT-FR-IPJG: with no project open the shared refusal.
#[test]
fn no_open_project_is_the_shared_refusal() {
    let app = closed_project();
    let error =
        block_on(GetDocumentTool::new(app.handle().clone()).call(args("doc-1"))).unwrap_err();
    assert_eq!(error, ToolRefusal::NoProjectOpen);
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert!(!error.retryable());
}

// GDT-FR-FZHF: an id outside the collection, including any string that looks like
// a path, is the retryable unknown-document refusal.
#[test]
fn an_unknown_id_or_a_path_is_refused_as_unknown() {
    let (fixture, _) = with_document("text");
    let outside = fixture.write("outside/secret.md", "not selected");
    for id in [
        "doc-0123456789abcdef0123456789abcdef".to_string(),
        outside.to_string_lossy().into_owned(),
        fixture.root.join("refs/doc.md").to_string_lossy().into_owned(),
        "../../etc/passwd".to_string(),
        fixture.id_of("outside/secret.md"),
    ] {
        let error = get(&fixture, args(&id)).unwrap_err();
        assert_eq!(error, ToolRefusal::DocumentNotFound, "{id}");
        assert_eq!(error.kind(), ToolErrorKind::NotFound);
        assert!(error.retryable());
        assert_eq!(
            error.to_string(),
            "No document with that id is in the user's selected documents. Search for documents again to get a current id."
        );
    }
}

// GDT-FR-IFCA: a document that became unavailable since the last refresh is the
// non-retryable `Other` refusal.
#[test]
fn an_unavailable_document_is_refused() {
    let (fixture, id) = with_document("text");
    std::fs::remove_file(fixture.root.join("refs/doc.md")).unwrap();
    // Before any refresh: the read fails.
    let error = get(&fixture, args(&id)).unwrap_err();
    assert_eq!(error, ToolRefusal::DocumentUnavailable);
    assert_eq!(error.kind(), ToolErrorKind::Other);
    assert!(!error.retryable());
    assert_eq!(
        error.to_string(),
        "That document is unavailable. Its file was moved, deleted, or cannot be read."
    );

    // A file that is not UTF-8 is unavailable too.
    std::fs::write(fixture.root.join("refs/doc.md"), [0xff, 0xfe]).unwrap();
    assert_eq!(get(&fixture, args(&id)).unwrap_err(), ToolRefusal::DocumentUnavailable);
}

// GDT-FR-HREW: a PDF with no extractable text is the non-retryable `Other`
// refusal.
#[test]
fn a_pdf_without_text_is_refused() {
    let fixture = DocFixture::new();
    fixture.write("refs/scan.pdf", "   ");
    fixture.select(SourceKind::Folder, "refs");
    let error = get(&fixture, args(&fixture.id_of("refs/scan.pdf"))).unwrap_err();
    assert_eq!(error, ToolRefusal::DocumentNoText);
    assert_eq!(error.kind(), ToolErrorKind::Other);
    assert!(!error.retryable());
    assert_eq!(
        error.to_string(),
        "That document has no text that can be extracted. It may be a scanned PDF made of images."
    );
}

// GDT-FR-MVHY: both range forms together is the retryable `InvalidArgs` refusal,
// made before the id is resolved or any text is read.
#[test]
fn both_range_forms_are_refused_before_anything_is_read() {
    let (fixture, id) = with_document("one\ntwo\nthree\n");
    let combos = [
        (Some(0), None, Some(0), None),
        (None, Some(5), None, Some(1)),
        (Some(1), Some(1), Some(1), Some(1)),
        (Some(0), None, None, Some(2)),
    ];
    for (byte_offset, byte_length, line_offset, line_limit) in combos {
        let mut call = args(&id);
        call.byte_offset = byte_offset;
        call.byte_length = byte_length;
        call.line_offset = line_offset;
        call.line_limit = line_limit;
        let error = get(&fixture, call).unwrap_err();
        assert_eq!(error, ToolRefusal::InvalidArguments(DOCUMENT_BOTH_RANGE_FORMS));
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert!(error.retryable());
        assert_eq!(
            error.to_string(),
            "Use either a byte range or a line range, not both. Call again with byte_offset and byte_length, or with line_offset and line_limit."
        );
    }
    // The refusal does not wait for the id: an unknown id with both forms is the
    // range refusal.
    let mut call = args("doc-unknown");
    call.byte_offset = Some(0);
    call.line_offset = Some(0);
    assert_eq!(
        get(&fixture, call).unwrap_err(),
        ToolRefusal::InvalidArguments(DOCUMENT_BOTH_RANGE_FORMS)
    );
    // And the pure check agrees.
    let mut call = args(&id);
    call.line_limit = Some(1);
    call.byte_length = Some(1);
    assert!(range_form(&call).is_err());
}

// TLC-FR-07, GDT-FR-QTAA: a range argument spelled as text or a float decodes, and
// an unusable one falls back to the default.
#[test]
fn range_arguments_decode_leniently() {
    let decoded: GetDocumentArgs = serde_json::from_value(serde_json::json!({
        "id": "doc-1", "byte_offset": "4", "byte_length": 2.9, "line_offset": null, "line_limit": "many"
    }))
    .unwrap();
    assert_eq!(decoded.byte_offset, Some(4));
    assert_eq!(decoded.byte_length, Some(2));
    assert_eq!(decoded.line_offset, None);
    assert_eq!(decoded.line_limit, None);
}
