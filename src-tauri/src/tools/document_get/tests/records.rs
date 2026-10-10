//! The log records (GDT-FR-VZXF, GDT-FR-LDHA).

use super::*;

static GDT_BUFFER: LogBuffer = LogBuffer::new();
static GDT_BOUND_BUFFER: LogBuffer = LogBuffer::new();

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

// GDT-FR-VZXF, GDT-FR-LDHA: an INFO record names the tool, the id, the range form,
// and the bytes returned; a WARN names the tool, the id, and the reason. Neither
// carries a range value, a name, a path, or any part of the text.
#[test]
fn records_name_the_call_and_nothing_of_the_document() {
    let (fixture, id) = with_document("confidential words in the document");
    let tool = GetDocumentTool::with_buffer(fixture.fixture.handle(), &GDT_BUFFER);
    let mut ranged = args(&id);
    ranged.byte_offset = Some(0);
    ranged.byte_length = Some(12);
    assert_eq!(block_on(tool.call(ranged)).unwrap(), "confidential");
    block_on(tool.call(args(&id))).unwrap();
    let mut both = args(&id);
    both.byte_offset = Some(777);
    both.line_offset = Some(888);
    block_on(tool.call(both)).unwrap_err();

    let records = records_of(&GDT_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 3);
    assert_eq!(records, records_of(&GDT_BUFFER, Domain::Backend));
    let infos: Vec<_> = records.iter().filter(|r| r.level == LogLevel::Info).collect();
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].fields["tool"], serde_json::json!(NAME));
    assert_eq!(infos[0].fields["id"], serde_json::json!(id));
    assert_eq!(infos[0].fields["range"], serde_json::json!("bytes"));
    assert_eq!(infos[0].fields["bytes"], serde_json::json!(12));
    assert_eq!(infos[1].fields["range"], serde_json::json!("none"));
    assert_eq!(infos[1].fields["bytes"], serde_json::json!("confidential words in the document".len()));
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).unwrap();
    assert_eq!(warn.fields["tool"], serde_json::json!(NAME));
    assert_eq!(warn.fields["id"], serde_json::json!(id));
    assert_eq!(warn.fields["reason"], serde_json::json!("invalid_arguments"));

    let rendered = serde_json::to_string(&records).unwrap();
    assert!(!rendered.contains("confidential words"), "no part of the text");
    assert!(!rendered.contains("doc.md"), "no name");
    assert!(!rendered.contains(&*fixture.root.to_string_lossy()), "no path");
    // Read as JSON values rather than as digits anywhere in the text: a
    // timestamp or an id can hold the same digits by chance.
    let fields = serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>()).unwrap();
    for value in ["777", "888"] {
        assert!(
            !fields.contains(&format!(":{value}")) && !fields.contains(&format!("\"{value}\"")),
            "no range value: {fields}",
        );
    }
}

// GDT-FR-LDHA: the id is recorded untrimmed and bounded to 512 characters and an
// ellipsis.
#[test]
fn a_long_id_is_bounded_in_the_record() {
    let (fixture, _) = with_document("text");
    let id = format!(" {}", "é".repeat(2000));
    let tool = GetDocumentTool::with_buffer(fixture.fixture.handle(), &GDT_BOUND_BUFFER);
    block_on(tool.call(args(&id))).unwrap_err();
    let records = records_of(&GDT_BOUND_BUFFER, Domain::Ai);
    let logged = records[0].fields["id"].as_str().unwrap();
    assert_eq!(logged.chars().count(), 512 + 1);
    assert!(logged.starts_with(' '), "untrimmed");
    assert!(logged.ends_with('…'));
}
