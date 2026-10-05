//! NST-FR-20: the log records.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-20: the log records (NST-FR-20)
// ---------------------------------------------------------------------------

static NST_BUFFER: LogBuffer = LogBuffer::new();

/// NST-FR-20: a success and a refusal, under both domains, carrying the named
/// arguments and nothing returned.
#[test]
fn the_success_and_refusal_records_name_the_arguments_and_nothing_returned() {
    let fixture = NoteFixture::new();
    let id = fixture.note("the teardown of the overlay is unhandled");
    fixture.reindex();

    let tool = fixture.logged(&NST_BUFFER);
    let output = block_on(tool.call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(2),
    }))
    .expect("the call succeeds");
    assert_eq!(output.notes.len(), 1);
    let _ = block_on(tool.call(NoteSearchArgs {
        query: String::new(),
        limit: None,
    }))
    .expect_err("the call refuses");

    // Querying either domain returns the same two records — a tool call is work
    // the backend performs because a model asked for it (TLC-FR-14).
    let ai = records_of(&NST_BUFFER, Domain::Ai);
    let backend = records_of(&NST_BUFFER, Domain::Backend);
    assert_eq!(ai.len(), 2, "one success and one refusal");
    assert_eq!(ai, backend, "both domains return the same two records");

    let success = ai
        .iter()
        .find(|r| r.level == LogLevel::Info)
        .expect("an INFO record");
    assert_eq!(success.fields["tool"], serde_json::json!(NAME));
    assert_eq!(success.fields["query"], serde_json::json!("teardown"));
    assert_eq!(success.fields["limit"], serde_json::json!(2));
    assert_eq!(success.fields["limitApplied"], serde_json::json!(2));
    assert_eq!(success.fields["notes"], serde_json::json!(1));

    let refusal = ai
        .iter()
        .find(|r| r.level == LogLevel::Warn)
        .expect("a WARN record");
    assert_eq!(refusal.fields["tool"], serde_json::json!(NAME));
    assert_eq!(refusal.fields["query"], serde_json::json!(""));
    assert!(refusal.fields.contains_key("reason"));
    assert!(
        !refusal.fields.contains_key("limit"),
        "no `limit` at all when the model sent none",
    );
    assert!(
        !refusal.fields.contains_key("limitApplied"),
        "the applied limit appears on a success alone",
    );

    // Nothing returned is recorded.
    let rendered = serde_json::to_string(&ai).unwrap();
    assert!(!rendered.contains(&id), "no returned note id");
    assert!(!rendered.contains("overlay"), "no part of any body");
    assert!(!rendered.contains("unhandled"));
    assert!(!rendered.contains("score"));
    assert!(!rendered.contains("scope"));
}

static NST_BOUND_BUFFER: LogBuffer = LogBuffer::new();

/// NST-FR-20: a very long query is recorded as its first 512 characters and an
/// ellipsis.
#[test]
fn a_very_long_query_is_bounded_in_the_record() {
    let fixture = NoteFixture::new();
    fixture.note("the teardown of the overlay");
    fixture.reindex();

    // Multibyte on purpose: `bounded_argument` cuts at a **character** boundary,
    // and a byte slice would panic here rather than merely truncate badly.
    let query = "t\u{e9}ardown ".repeat(20_000 / 10);
    assert!(query.len() >= 20_000 - 10);
    assert!(query.chars().count() < query.len(), "multibyte, as intended");
    let tool = fixture.logged(&NST_BOUND_BUFFER);
    let output = block_on(tool.call(NoteSearchArgs {
        query: query.clone(),
        limit: None,
    }))
    .expect("the call succeeds");

    let records = records_of(&NST_BOUND_BUFFER, Domain::Ai);
    let record = records.first().expect("a record");
    assert_eq!(record.fields["tool"], serde_json::json!(NAME));
    assert_eq!(
        record.fields["notes"],
        serde_json::json!(output.notes.len()),
        "the record still names the note count",
    );
    let logged = record.fields["query"].as_str().expect("a string");
    assert_eq!(logged.chars().count(), LOGGED_QUERY_LIMIT + 1);
    assert!(logged.ends_with('…'));
    assert_eq!(
        logged.trim_end_matches('…'),
        &query.chars().take(LOGGED_QUERY_LIMIT).collect::<String>(),
    );
}

static NST_LIMIT_BUFFER: LogBuffer = LogBuffer::new();

/// NST-FR-20: the requested limit beside the applied one, an absent limit when
/// the model sent text no number could be made of, and a refusal record naming
/// both arguments and no applied limit.
#[test]
fn the_record_says_what_was_asked_for_and_what_was_applied() {
    let fixture = NoteFixture::new();
    fixture.note("the teardown of the overlay");
    fixture.reindex();

    let tool = fixture.logged(&NST_LIMIT_BUFFER);
    let _ = block_on(tool.call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(1000),
    }))
    .expect("the call succeeds");

    // A `limit` no number could be made of decodes to `None` (TLC-FR-07), and
    // the record then names none.
    let args: NoteSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "teardown", "limit": "soon" }))
            .expect("the query survives an unusable limit");
    assert_eq!(args.limit, None);
    let _ = block_on(tool.call(args)).expect("the call succeeds");

    let records = records_of(&NST_LIMIT_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 2);
    let clamped = &records[0];
    assert_eq!(clamped.fields["limit"], serde_json::json!(1000));
    assert_eq!(clamped.fields["limitApplied"], serde_json::json!(20));
    let unusable = &records[1];
    assert!(
        !unusable.fields.contains_key("limit"),
        "text no number could be made of names no limit",
    );
    assert_eq!(unusable.fields["limitApplied"], serde_json::json!(5));
}

static NST_CLOSED_BUFFER: LogBuffer = LogBuffer::new();

/// NST-FR-20: a no-project refusal names the query and the limit, and no
/// applied limit.
#[test]
fn a_no_project_refusal_names_both_arguments_and_no_applied_limit() {
    let app = crate::tools::tests::closed_project();
    let tool = NoteSearchTool::with_buffer(app.handle().clone(), "closed", &NST_CLOSED_BUFFER);
    let _ = block_on(tool.call(NoteSearchArgs {
        query: "teardown".into(),
        limit: Some(3),
    }))
    .expect_err("no project is open");

    let records = records_of(&NST_CLOSED_BUFFER, Domain::Backend);
    let record = records.first().expect("a record");
    assert_eq!(record.level, LogLevel::Warn);
    assert_eq!(record.fields["tool"], serde_json::json!(NAME));
    assert_eq!(record.fields["query"], serde_json::json!("teardown"));
    assert_eq!(record.fields["limit"], serde_json::json!(3));
    assert!(!record.fields.contains_key("limitApplied"));
}
