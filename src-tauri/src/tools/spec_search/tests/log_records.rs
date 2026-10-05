//! SPS-FR-19: logging.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-19 — logging (SPS-FR-19)
// ---------------------------------------------------------------------------

static SPS_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_call_logs_what_was_asked_and_what_came_back_but_no_result_text() {
    let fixture = spec_project();
    let tool = SpecSearchTool::with_buffer(fixture.handle(), &SPS_BUFFER);

    let output = block_on(tool.call(SpecificationSearchArgs {
        query: "teardown".to_string(),
        limit: Some(2),
    }))
    .expect("succeeds");
    let _ = block_on(tool.call(SpecificationSearchArgs {
        // Untrimmed, so the blank behind the refusal is visible as the blank it
        // was rather than as a missing field.
        query: "   ".to_string(),
        limit: None,
    }))
    .expect_err("refuses");
    assert_eq!(
        output.specifications.len(),
        2,
        "the fixture matches, so the count below is a real number",
    );

    assert_eq!(
        records_of(&SPS_BUFFER, Domain::Ai),
        records_of(&SPS_BUFFER, Domain::Backend),
        "both domains return the same records (SPS-FR-19)",
    );
    for domain in [Domain::Ai, Domain::Backend] {
        let records = records_of(&SPS_BUFFER, domain);
        assert_eq!(records.len(), 2, "one INFO and one WARN under {domain:?}");

        let info = records
            .iter()
            .find(|r| r.level == LogLevel::Info)
            .expect("the success");
        assert_eq!(info.fields["tool"], serde_json::json!(NAME));
        assert_eq!(
            info.fields["matches"],
            serde_json::json!(output.specifications.len()),
        );
        assert_eq!(
            info.fields["query"],
            serde_json::json!("teardown"),
            "the query the model asked with (SPS-FR-19)",
        );
        assert_eq!(
            info.fields["limit"],
            serde_json::json!(2),
            "the limit the model asked for (SPS-FR-19)",
        );
        assert_eq!(
            info.fields["limitApplied"],
            serde_json::json!(2),
            "and what SPS-FR-06 made of it",
        );

        let warn = records
            .iter()
            .find(|r| r.level == LogLevel::Warn)
            .expect("the refusal");
        assert_eq!(warn.fields["tool"], serde_json::json!(NAME));
        assert_eq!(warn.fields["reason"], serde_json::json!("invalid_arguments"));
        assert_eq!(
            warn.fields["query"],
            serde_json::json!("   "),
            "the argument that caused the refusal, untrimmed (SPS-FR-19)",
        );
        assert!(
            !warn.fields.contains_key("limit"),
            "the model sent no limit, so the record names none (SPS-FR-19)",
        );
        assert!(
            !warn.fields.contains_key("limitApplied"),
            "nothing was applied — the call never reached the index (SPS-FR-19)",
        );

        // Exactly these, so a field carrying a returned path or score cannot
        // arrive under a name this test never thought to scan for.
        assert_eq!(
            sorted_keys(info),
            vec!["limit", "limitApplied", "matches", "query", "tool"],
            "the success record carries these and nothing else (SPS-FR-19)",
        );
        assert_eq!(
            sorted_keys(warn),
            vec!["query", "reason", "retryable", "tool"],
            "the refusal record carries these and nothing else (SPS-FR-19)",
        );
    }

    // The arguments are loggable; what came back is the project's own material
    // and is not. The query term itself is excluded from this scan — it is now
    // a field by design — so the paths are checked whole and the excerpt by its
    // body text.
    let rendered = rendered_records(&SPS_BUFFER);
    for leak in [
        "specifications/core/teardown.md",
        "preamble",
        // Single-line literals: the excerpt is multi-line and
        // `rendered_records` escapes its newlines, so scanning for the whole of
        // it could never match whatever the code did. The section heading and
        // the body opening are what actually stand for it here, and the exact
        // key sets below are what rule out a field carrying either.
        "This section covers",
        "## Section 0",
        &output.specifications[0].path.clone(),
    ] {
        assert!(
            !rendered.contains(leak),
            "a log record carried {leak:?} (SPS-FR-19)",
        );
    }
}

static SPS_CLOSED_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_refusal_raised_before_the_search_still_names_what_was_asked() {
    let app = closed_project();
    let tool = SpecSearchTool::with_buffer(app.handle().clone(), &SPS_CLOSED_BUFFER);

    let refusal = block_on(tool.call(SpecificationSearchArgs {
        query: "teardown".to_string(),
        limit: Some(1000),
    }))
    .expect_err("no project is open");
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);

    let records = records_of(&SPS_CLOSED_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 1, "one WARN for the one refusal");
    let warn = &records[0];
    assert_eq!(
        sorted_keys(warn),
        vec!["limit", "query", "reason", "retryable", "tool"],
        "the shared refusal still names both arguments (SPS-FR-19)",
    );
    assert_eq!(warn.fields["reason"], serde_json::json!("no_project_open"));
    assert_eq!(warn.fields["retryable"], serde_json::json!(false));
    assert_eq!(warn.fields["query"], serde_json::json!("teardown"));
    assert_eq!(
        warn.fields["limit"],
        serde_json::json!(1000),
        "as the model sent it, not as SPS-FR-06 would have clamped it (SPS-FR-19)",
    );
    assert!(
        !warn.fields.contains_key("limitApplied"),
        "no limit was applied — the call never reached the index (SPS-FR-19)",
    );
}

static SPS_BOUND_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_pathological_query_is_recorded_bounded_and_costs_the_record_nothing_else() {
    let fixture = spec_project();
    let tool = SpecSearchTool::with_buffer(fixture.handle(), &SPS_BOUND_BUFFER);

    // Long enough that an unbounded field would push the record past
    // LGC-FR-08's ceiling and cost it the tool and the count.
    let huge = format!("{}xx", "teardown ".repeat(19_998 / 9));
    assert_eq!(huge.chars().count(), 20_000, "SPS-FR-19's query");
    assert!(
        huge.len() > crate::logging::MAX_RECORD_BYTES,
        "the premise: unbounded, this argument alone oversizes the record",
    );
    let output = block_on(tool.call(SpecificationSearchArgs {
        query: huge.clone(),
        limit: Some(1000),
    }))
    .expect("succeeds");
    assert!(
        !output.specifications.is_empty(),
        "the fixture matches, so the count below is a real number",
    );

    let records = records_of(&SPS_BOUND_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 1, "one INFO for the one call");
    let info = &records[0];
    assert_eq!(
        sorted_keys(info),
        vec!["limit", "limitApplied", "matches", "query", "tool"],
        "the record survives whole (SPS-FR-19)",
    );
    assert!(
        !info.fields.contains_key(crate::logging::OMITTED_FIELD),
        "no field of this record was given up to the ceiling (LGC-FR-08)",
    );
    assert_eq!(
        info.fields["tool"],
        serde_json::json!(NAME),
        "the record still names the tool (SPS-FR-19)",
    );
    assert_eq!(
        info.fields["matches"],
        serde_json::json!(output.specifications.len()),
        "and still names the count (SPS-FR-19)",
    );
    let logged = info.fields["query"].as_str().expect("a string");
    assert_eq!(
        logged.chars().count(),
        513,
        "512 characters and one ellipsis (SPS-FR-19)",
    );
    assert_eq!(
        logged,
        format!("{}…", huge.chars().take(512).collect::<String>()),
        "the argument's first 512 characters (SPS-FR-19)",
    );

    // The clamp is readable rather than looking like a contradiction of the
    // count: 1000 asked for, 20 applied (SPS-FR-06).
    assert_eq!(info.fields["limit"], serde_json::json!(1000));
    assert_eq!(info.fields["limitApplied"], serde_json::json!(MAX_LIMIT));
}

#[test]
fn bounding_a_query_never_cuts_a_character_in_half() {
    let bound = |text: &str| bounded_argument(text, LOGGED_QUERY_LIMIT);

    // Under the bound, and exactly at it, verbatim — no ellipsis for a query
    // that fits (SPS-FR-19).
    assert_eq!(bound(""), "");
    let plain = "how the file tree classifies artifacts";
    assert_eq!(bound(plain), plain);
    let exact = "α".repeat(LOGGED_QUERY_LIMIT);
    assert_eq!(bound(&exact), exact);

    // One character past it, whatever that character's width in bytes.
    for unit in ["x", "α", "🧪"] {
        let over = unit.repeat(LOGGED_QUERY_LIMIT + 1);
        let bounded = bound(&over);
        assert_eq!(
            bounded.chars().count(),
            LOGGED_QUERY_LIMIT + 1,
            "512 characters and one ellipsis, not 512 bytes (SPS-FR-19)",
        );
        assert_eq!(
            bounded,
            format!("{}…", unit.repeat(LOGGED_QUERY_LIMIT)),
            "cut on a character boundary (SPS-FR-19)",
        );
    }
}

static SPS_LENIENT_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_limit_spelled_oddly_is_recorded_as_it_decoded_and_a_missing_one_is_absent() {
    let fixture = spec_project();
    let tool = SpecSearchTool::with_buffer(fixture.handle(), &SPS_LENIENT_BUFFER);

    // The forms TLC-FR-07's leniency accepts, and one it cannot make a number
    // of. The record carries the decoded value rather than the JSON literal:
    // that is what the tool was called with, the string and the fraction having
    // been resolved before it ever saw them.
    for raw in [
        serde_json::json!({ "query": "teardown", "limit": "3" }),
        serde_json::json!({ "query": "teardown", "limit": 3.7 }),
        serde_json::json!({ "query": "teardown", "limit": "not a number" }),
    ] {
        let args: SpecificationSearchArgs = serde_json::from_value(raw).unwrap();
        block_on(tool.call(args)).expect("succeeds");
    }

    let records = records_of(&SPS_LENIENT_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 3, "one INFO per call, oldest first");
    assert_eq!(records[0].fields["limit"], serde_json::json!(3));
    assert_eq!(records[1].fields["limit"], serde_json::json!(3));
    assert!(
        !records[2].fields.contains_key("limit"),
        "an unparseable limit reached the tool as none, and reads as none (SPS-FR-19)",
    );
    assert_eq!(
        records[2].fields["limitApplied"],
        serde_json::json!(DEFAULT_LIMIT),
        "so the default is what produced the result (SPS-FR-06)",
    );
}
