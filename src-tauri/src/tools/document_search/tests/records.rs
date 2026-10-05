//! The log records (SDT-FR-QSKI, SDT-FR-KRWA).

use super::*;

static SDT_BUFFER: LogBuffer = LogBuffer::new();
static SDT_BOUND_BUFFER: LogBuffer = LogBuffer::new();

fn records_of(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records
}

// SDT-FR-QSKI, SDT-FR-KRWA: an INFO record on success and a WARN on refusal, under
// the `ai` and `backend` domains, carrying the tool and the query and nothing
// returned.
#[test]
fn records_name_the_arguments_and_nothing_returned() {
    let fixture = DocFixture::new();
    fixture.write("refs/secret-name.md", "Platypus venom is confidential material.");
    fixture.select(SourceKind::Folder, "refs");
    let id = fixture.id_of("refs/secret-name.md");
    let tool = SearchDocumentsTool::with_buffer(fixture.fixture.handle(), &SDT_BUFFER);
    let output = block_on(tool.call(SearchDocumentsArgs {
        query: "platypus venom".into(),
        limit: Some(3),
    }))
    .unwrap();
    assert_eq!(output.documents.len(), 1);
    let _ = block_on(tool.call(SearchDocumentsArgs {
        query: " ".into(),
        limit: None,
    }))
    .unwrap_err();

    let ai = records_of(&SDT_BUFFER, Domain::Ai);
    assert_eq!(ai.len(), 2);
    assert_eq!(ai, records_of(&SDT_BUFFER, Domain::Backend));
    let success = ai.iter().find(|r| r.level == LogLevel::Info).unwrap();
    assert_eq!(success.fields["tool"], serde_json::json!(NAME));
    assert_eq!(success.fields["query"], serde_json::json!("platypus venom"));
    assert_eq!(success.fields["limit"], serde_json::json!(3));
    assert_eq!(success.fields["limitApplied"], serde_json::json!(3));
    assert_eq!(success.fields["documents"], serde_json::json!(1));
    let refusal = ai.iter().find(|r| r.level == LogLevel::Warn).unwrap();
    assert_eq!(refusal.fields["tool"], serde_json::json!(NAME));
    assert_eq!(refusal.fields["query"], serde_json::json!(" "));
    assert!(refusal.fields.contains_key("reason"));
    assert!(!refusal.fields.contains_key("limit"));

    let rendered = serde_json::to_string(&ai).unwrap();
    assert!(!rendered.contains(&id), "no returned id");
    assert!(!rendered.contains("secret-name"), "no returned name");
    assert!(!rendered.contains("confidential"), "no part of any document");
    assert!(!rendered.contains("refs"), "no folder");
}

// SDT-FR-KRWA: a long query is recorded as its first 512 characters and an
// ellipsis.
#[test]
fn a_long_query_is_bounded_in_the_record() {
    let fixture = DocFixture::new();
    fixture.write("refs/a.md", "Wombat burrows are deep.");
    fixture.select(SourceKind::Folder, "refs");
    let query = "w\u{f6}mbat ".repeat(3000);
    let tool = SearchDocumentsTool::with_buffer(fixture.fixture.handle(), &SDT_BOUND_BUFFER);
    block_on(tool.call(SearchDocumentsArgs {
        query: query.clone(),
        limit: None,
    }))
    .unwrap();
    let records = records_of(&SDT_BOUND_BUFFER, Domain::Ai);
    let logged = records[0].fields["query"].as_str().unwrap();
    assert_eq!(logged.chars().count(), 512 + 1);
    assert!(logged.ends_with('…'));
    assert_eq!(
        logged.trim_end_matches('…'),
        query.chars().take(512).collect::<String>()
    );
}
