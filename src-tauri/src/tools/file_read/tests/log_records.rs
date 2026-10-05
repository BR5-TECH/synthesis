//! RFT-FR-20: logging.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-20 — logging (RFT-FR-20)
// ---------------------------------------------------------------------------

static RFT_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_call_logs_the_path_asked_for_and_its_byte_count_and_leaks_no_contents() {
    let project = project();
    let tool = project.logged_tool(&RFT_BUFFER);

    let text = block_on(tool.call(ReadFileArgs {
        path: "multibyte.txt".to_string(),
        offset: None,
        limit: None,
    }))
    .expect("succeeds");
    // Values that cannot be confused with a byte count, a sequence number, or a
    // timestamp, so finding either digit string anywhere means it came from the
    // arguments the model composed.
    let _ = block_on(tool.call(ReadFileArgs {
        path: "src/gone.ts".to_string(),
        offset: Some(424_242),
        limit: Some(31_337),
    }))
    .expect_err("refuses");

    for domain in [Domain::Ai, Domain::Backend] {
        let records = records_of(&RFT_BUFFER, domain);
        assert_eq!(records.len(), 2, "one INFO and one WARN under {domain:?}");

        let info = records
            .iter()
            .find(|r| r.level == LogLevel::Info)
            .expect("the success");
        assert_eq!(info.fields["tool"], serde_json::json!(NAME));
        assert_eq!(info.fields["bytes"], serde_json::json!(text.len()));
        // The argument the call carried, unchanged (RFT-FR-20).
        assert_eq!(info.fields["path"], serde_json::json!("multibyte.txt"));
        // A byte count, not a character count — the fixture is multi-byte, so
        // the two differ and this pins which one is reported.
        assert_ne!(text.len(), text.chars().count());

        let warn = records
            .iter()
            .find(|r| r.level == LogLevel::Warn)
            .expect("the refusal");
        assert_eq!(warn.fields["tool"], serde_json::json!(NAME));
        assert_eq!(warn.fields["reason"], serde_json::json!("file_not_found"));
        // The path that was refused, which is what a model has to correct.
        assert_eq!(warn.fields["path"], serde_json::json!("src/gone.ts"));

        // The exact key sets. A scan for known leak strings only catches a field
        // someone anticipated; this catches any field at all that should not be
        // there, whatever it is called (RFT-FR-20).
        assert_eq!(field_keys(info), vec!["bytes", "path", "tool"]);
        assert_eq!(field_keys(warn), vec!["path", "reason", "retryable", "tool"]);
    }

    // The path is recorded; the file's characters and the other two arguments
    // are not (RFT-FR-20).
    let rendered = rendered_records(&RFT_BUFFER);
    for leak in ["α∫∂", "β√ç", "424242", "31337", "424_242"] {
        assert!(
            !rendered.contains(leak),
            "a log record carried {leak:?} (RFT-FR-20)",
        );
    }
}

static RFT_SPELLING_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn the_path_is_recorded_as_the_model_spelled_it() {
    let project = project();
    let tool = project.logged_tool(&RFT_SPELLING_BUFFER);

    // Three spellings that all reach the same file. The record is to say what
    // the model asked for, so it carries the argument rather than the trimmed
    // form `resolve` works on or the absolute path the read ran against.
    for path in ["src/a.ts", "  src/a.ts  ", "src/./nested/../a.ts"] {
        block_on(tool.call(ReadFileArgs {
            path: path.to_string(),
            offset: None,
            limit: None,
        }))
        .expect("all three reach src/a.ts");
    }

    let recorded: Vec<_> = records_of(&RFT_SPELLING_BUFFER, Domain::Ai)
        .iter()
        .map(|record| record.fields["path"].clone())
        .collect();
    assert_eq!(
        recorded,
        vec![
            serde_json::json!("src/a.ts"),
            serde_json::json!("  src/a.ts  "),
            serde_json::json!("src/./nested/../a.ts"),
        ],
        "the argument unchanged, padding and all (RFT-FR-20)",
    );
    assert!(
        !rendered_records(&RFT_SPELLING_BUFFER).contains(&project.root.to_string_lossy().to_string()),
        "the resolved absolute path is not what a record carries (RFT-FR-20)",
    );
}

static RFT_CLOSED_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_refusal_raised_before_the_path_is_resolved_still_names_it() {
    let app = closed_project();
    let tool = FileReadTool::with_buffer(app.handle().clone(), "closed", &RFT_CLOSED_BUFFER);

    // No project, so `run` refuses before `read` is entered and before any path
    // is resolved. The record still says which file was asked for, that being
    // the whole of what makes it followable (RFT-FR-20).
    block_on(tool.call(ReadFileArgs {
        path: "src/a.ts".to_string(),
        offset: None,
        limit: None,
    }))
    .expect_err("no project is open (RFT-FR-17)");

    for domain in [Domain::Ai, Domain::Backend] {
        let records = records_of(&RFT_CLOSED_BUFFER, domain);
        assert_eq!(records.len(), 1, "one WARN under {domain:?}");
        assert_eq!(field_keys(&records[0]), vec!["path", "reason", "retryable", "tool"]);
        assert_eq!(records[0].fields["path"], serde_json::json!("src/a.ts"));
        assert_eq!(records[0].fields["reason"], serde_json::json!("no_project_open"));
        assert_eq!(records[0].fields["retryable"], serde_json::json!(false));
    }
}

static RFT_OVERSIZE_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_pathological_path_costs_the_record_none_of_its_other_fields() {
    let project = project();
    let tool = project.logged_tool(&RFT_OVERSIZE_BUFFER);

    // Far past `LGC-FR-08`'s per-record ceiling: unbounded, this one argument
    // would replace every field of the record with the omitted-size marker,
    // leaving it naming neither the tool nor the reason.
    let huge = "../".repeat(crate::logging::MAX_RECORD_BYTES);
    assert!(huge.len() > crate::logging::MAX_RECORD_BYTES);
    let refusal = block_on(tool.call(ReadFileArgs {
        path: huge.clone(),
        offset: None,
        limit: None,
    }))
    .expect_err("a path of nothing but `..` leaves the root");
    assert_eq!(refusal, ToolRefusal::PathOutsideProject);

    let records = records_of(&RFT_OVERSIZE_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 1);
    assert_eq!(
        field_keys(&records[0]),
        vec!["path", "reason", "retryable", "tool"],
        "the record survives whole (RFT-FR-20)",
    );
    assert!(
        !records[0].fields.contains_key(crate::logging::OMITTED_FIELD),
        "no field of this record was given up to the ceiling (LGC-FR-08)",
    );
    assert_eq!(records[0].fields["tool"], serde_json::json!(NAME));
    assert_eq!(records[0].fields["reason"], serde_json::json!("path_outside_project"));

    let recorded = records[0].fields["path"].as_str().expect("a string");
    assert_eq!(
        recorded,
        format!("{}…", &huge[..LOGGED_PATH_LIMIT]),
        "the first 512 characters and an ellipsis (RFT-FR-20)",
    );
    assert_eq!(recorded.chars().count(), LOGGED_PATH_LIMIT + 1);
}

#[test]
fn bounding_a_path_never_cuts_a_character_in_half() {
    // Under the limit, verbatim, whatever the bytes.
    assert_eq!(logged_path(""), "");
    assert_eq!(logged_path("src/a.ts"), "src/a.ts");
    let exactly = "α".repeat(LOGGED_PATH_LIMIT);
    assert_eq!(logged_path(&exactly), exactly, "the limit itself is not over it");

    // Over it, cut on a character boundary rather than a byte one: `α` is two
    // bytes, so a byte-counting cut would land inside one and panic.
    let over = "α".repeat(LOGGED_PATH_LIMIT + 1);
    assert_eq!(logged_path(&over), format!("{exactly}…"));
    assert_eq!(logged_path(&over).chars().count(), LOGGED_PATH_LIMIT + 1);
}

