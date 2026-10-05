//! More tests for `PDC-propose-draft-changes-tool.md`.
//!
//! One part of `mod.rs`, which holds the fixtures these all run against.

use super::*;

// ---------------------------------------------------------------------------
// The arguments as a model actually sends them (TLC-FR-06, TLC-FR-08)
// ---------------------------------------------------------------------------
//
// Every other test in this file builds `ProposeDraftChangesArgs` in Rust, which
// exercises the tool and **not** the one thing between the model and the tool:
// the JSON. A schema that says one thing and a struct that reads another is a
// tool that refuses every real call while every test passes.

/// The arguments, deserialised from the JSON a model sends.
fn from_json(payload: serde_json::Value) -> ProposeDraftChangesArgs {
    serde_json::from_value(payload).expect("the schema's own shape deserialises")
}

// TLC-FR-06 / TLC-FR-08: a call composed exactly as the published schema
// describes it reaches the tool with its changes intact.
#[test]
fn the_json_a_model_sends_deserialises_into_the_arguments() {
    let parsed = from_json(serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        "hunks": [
            { "kind": "replace", "before": ORIGINAL, "after": PROPOSED },
            { "kind": "add", "after": "\n\nA new closing line.", "after_text": ORIGINAL },
            { "kind": "del", "before": "gone" },
        ],
    }));

    assert_eq!(parsed.path, FILE);
    assert_eq!(parsed.hunks.len(), 3, "every change survives the JSON");
    assert_eq!(parsed.hunks[0].before.as_deref(), Some(ORIGINAL));
    assert_eq!(parsed.hunks[1].after_text.as_deref(), Some(ORIGINAL));
    assert_eq!(parsed.hunks[2].kind.as_deref(), Some("del"));
    assert!(parsed.revises.is_none());
}

// TLC-FR-06: every property the schema publishes is one the arguments read, and
// under the same name. A property the model is told to send and the struct
// ignores is a change silently dropped on the way in.
#[test]
fn every_published_property_is_read_under_its_own_name() {
    let schema = parameters();
    let top = schema_keys(&schema["properties"]);
    let sent: serde_json::Value = serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        "revises": "h-1",
        "hunks": [{ "kind": "replace", "before": ORIGINAL, "after": PROPOSED, "note": "why" }],
    });
    for key in &top {
        assert!(sent.get(key).is_some(), "the schema publishes `{key}` and nothing sends it");
    }
    let parsed = from_json(sent);
    assert_eq!(parsed.revises.as_deref(), Some("h-1"));
    assert_eq!(parsed.hunks[0].note.as_deref(), Some("why"));

    // And the same for each change's own properties: a property the schema
    // publishes and the argument ignores is a change silently dropped.
    let item = schema_keys(&schema["properties"]["hunks"]["items"]["properties"]);
    assert_eq!(item, ["after", "after_text", "before", "kind", "note"]);
    let sent_item = serde_json::json!({
        "kind": "replace",
        "before": ORIGINAL,
        "after": PROPOSED,
        "after_text": ORIGINAL,
        "note": "why",
    });
    for key in &item {
        assert!(sent_item.get(key).is_some(), "the schema publishes `{key}` on a change");
    }
}

// TLC-FR-07: a model that sends a field this tool does not know keeps the call.
#[test]
fn an_unknown_field_is_ignored_rather_than_refusing_the_call() {
    let parsed = from_json(serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        "confidence": 0.9,
        "hunks": [{ "before": ORIGINAL, "after": PROPOSED, "reason": "tighter" }],
    }));
    assert_eq!(parsed.hunks.len(), 1);
}

// PDC-FR-04: the call a model composes from the schema records a proposal —
// the whole way through, from JSON to the record on disk.
#[test]
fn a_call_composed_from_the_schema_records_a_proposal() {
    let f = Fixture::new();
    let out = f
        .call(from_json(serde_json::json!({
            "path": FILE,
            "rationale": RATIONALE,
            "hunks": [{ "kind": "replace", "before": ORIGINAL, "after": PROPOSED }],
        })))
        .expect("the model's own call is recorded");
    assert!(out.proposed);
}

// PDC-FR-18 / TLC-FR-14: a refused call is followable — the record says which
// correction the model was asked to make, and the shape of what it sent.
//
// Without this the eight invalid-argument refusals this tool can give all
// arrive under one word, and a call that keeps failing cannot be told apart
// from any other.
static PDC_REFUSAL_LOG: LogBuffer = LogBuffer::new();
static PDC_LEAK_LOG: LogBuffer = LogBuffer::new();
// The later refusal tests keep buffers of their own: two tests that clear one
// shared buffer race each other when the suite runs in parallel.
pub(super) static PDC_SEVERAL_REFUSAL_LOG: LogBuffer = LogBuffer::new();
pub(super) static PDC_SEVERAL_LEAK_LOG: LogBuffer = LogBuffer::new();

#[test]
fn a_refused_call_records_which_refusal_and_the_shape_of_the_arguments() {
    let f = Fixture::new();
    PDC_REFUSAL_LOG.clear();
    let refused = block_on(f.tool().with_buffer(&PDC_REFUSAL_LOG).call(
        ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: None,
            hunks: vec![],
        },
    ))
    .unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_NO_HUNKS);

    let record = records(&PDC_REFUSAL_LOG, Domain::Ai)
        .into_iter()
        .rev()
        .find(|r| r.message == "tool call refused")
        .expect("the refusal is recorded");
    assert_eq!(record.fields["reason"], serde_json::json!("invalid_arguments"));
    // Which one, as a code — so a reader filtering a column finds it, and the
    // wording the model reads can change without the filter going quiet.
    assert_eq!(record.fields["refusal"], serde_json::json!("no_changes"));
    // And the shape that produced it.
    assert_eq!(record.fields["hunks"], serde_json::json!(0));
    assert_eq!(record.fields["revising"], serde_json::json!(false));
}

// PDC-FR-18: the record carries the shape and never the text — not the prompt,
// not the proposed replacement, not the rationale.
#[test]
fn a_refused_call_records_no_text_the_model_or_the_author_wrote() {
    let f = Fixture::new();
    // PDC-FR-GMWR: a name is only wrong where a proposal stands to be wrong
    // about, so one is recorded first and the refused call names a change of it
    // that is not there.
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some(ORIGINAL.into()),
            after: Some(PROPOSED.into()),
            after_text: None,
            note: None,
        }],
    })
    .expect("a proposal stands");
    PDC_LEAK_LOG.clear();
    let secret_before = "The original three paragraphs.";
    let secret_after = "A REPLACEMENT NOBODY SHOULD SEE IN A LOG";
    let _ = block_on(f.tool().with_buffer(&PDC_LEAK_LOG).call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: "A RATIONALE NOBODY SHOULD SEE IN A LOG".into(),
        revises: Some("no-such-change".into()),
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some(secret_before.into()),
            after: Some(secret_after.into()),
            after_text: None,
            note: Some("A NOTE NOBODY SHOULD SEE IN A LOG".into()),
        }],
    }));

    let rendered = serde_json::to_string(
        &records(&PDC_LEAK_LOG, Domain::Ai)
            .into_iter()
            .map(|r| (r.message, r.fields))
            .collect::<Vec<_>>(),
    )
    .expect("records render");
    for secret in [
        secret_after,
        "A RATIONALE NOBODY SHOULD SEE IN A LOG",
        "A NOTE NOBODY SHOULD SEE IN A LOG",
    ] {
        assert!(!rendered.contains(secret), "a log record carried `{secret}`");
    }
    // The shape did reach it, which is the whole point of the pair — and so did
    // the code, which is a term this file chose rather than anything composed.
    assert!(rendered.contains("hunksNamingText"));
    assert!(rendered.contains("no_such_change"));
    // And never the sentence the model was shown, which is what `reason` and
    // `refusal` exist to stand in for.
    assert!(!rendered.contains(PROPOSAL_NO_SUCH_CHANGE));
}

// ---------------------------------------------------------------------------
// A call the decoder used to refuse before the tool ever saw it (TLC-FR-07)
// ---------------------------------------------------------------------------
//
// A field the model left out, or filled with an explicit `null`, used to fail
// the decode and reach the model as "the arguments were not in the shape it
// expects" — the one sentence that is true of every mistake there is, carrying
// no record of its own because the tool never ran. Each of these now reaches
// the refusal this tool wrote for that exact mistake.
//
// In each, the `expect` is as much of the assertion as the refusal is: a call
// that no longer decodes never reaches the tool at all.

// TLC-FR-07 / PDC-FR-07: a call that left out the rationale, or sent `null` for
// it, reads the blank-rationale refusal, which says what to write.
#[test]
fn a_call_with_no_rationale_reads_the_refusal_that_asks_for_one() {
    let f = Fixture::new();
    for rationale in [None, Some(serde_json::Value::Null)] {
        let mut payload = serde_json::json!({
            "path": FILE,
            "hunks": [{ "kind": "replace", "before": ORIGINAL, "after": PROPOSED }],
        });
        if let Some(null) = rationale {
            payload["rationale"] = null;
        }
        let args: ProposeDraftChangesArgs =
            serde_json::from_value(payload).expect("a rationale that is not there decodes");
        assert!(args.rationale.is_empty());
        let refused = block_on(f.tool().call(args)).unwrap_err();
        assert_eq!(refused.to_string(), PROPOSAL_RATIONALE_BLANK);
    }
}

// TLC-FR-07 / PDC-FR-05: a call that left out the path, or sent `null` for it,
// reads the refusal that names the one file a draft holds.
#[test]
fn a_call_with_no_path_reads_the_refusal_that_names_the_file() {
    let f = Fixture::new();
    for path in [None, Some(serde_json::Value::Null)] {
        let mut payload = serde_json::json!({
            "rationale": RATIONALE,
            "hunks": [{ "kind": "replace", "before": ORIGINAL, "after": PROPOSED }],
        });
        if let Some(null) = path {
            payload["path"] = null;
        }
        let args: ProposeDraftChangesArgs =
            serde_json::from_value(payload).expect("a path that is not there decodes");
        assert!(args.path.is_empty());
        let refused = block_on(f.tool().call(args)).unwrap_err();
        assert_eq!(
            refused.to_string(),
            ToolRefusal::ProposalPathMissing.to_string(),
        );
    }
}

// TLC-FR-07 / PDC-FR-07: `hunks: null` is no changes, which is a refusal the
// model can act on, and not a malformed call.
#[test]
fn a_call_whose_change_list_is_null_reads_the_no_changes_refusal() {
    let f = Fixture::new();
    let args: ProposeDraftChangesArgs = serde_json::from_value(serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        "hunks": serde_json::Value::Null,
    }))
    .expect("a null change list decodes");
    assert!(args.hunks.is_empty());
    let refused = block_on(f.tool().call(args)).unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_NO_HUNKS);
}

// TLC-FR-07 / PDC-FR-07: an argument set carrying nothing at all is the
// no-changes refusal, which is the **first** of the three the tool checks.
//
// Pinned because all three of its fields now default, so an empty object is a
// call the tool answers rather than one the decoder refuses — and which of the
// three corrections the model is asked to make is then a choice this file makes
// rather than an accident of the order the checks happen to stand in.
#[test]
fn a_call_carrying_nothing_at_all_is_asked_for_changes_first() {
    let f = Fixture::new();
    let args: ProposeDraftChangesArgs =
        serde_json::from_value(serde_json::json!({})).expect("an empty argument set decodes");
    let refused = block_on(f.tool().call(args)).unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_NO_HUNKS);
}

// TLC-FR-07: forgiveness reaches the list and stops there. A change that is
// itself `null` is not an empty change the tool can describe — there is no
// field of it to name in a refusal — so the argument set does not decode, and
// the model reads the boundary's own sentence.
#[test]
fn a_change_that_is_itself_null_does_not_decode() {
    let outcome: Result<ProposeDraftChangesArgs, _> = serde_json::from_value(serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        "hunks": [serde_json::Value::Null],
    }));
    assert!(outcome.is_err(), "a null change decoded as something");
}

// TLC-FR-07 / PDC-FR-04: a change names the text it changes and nothing about
// where in the file it stands. A model that sent a line number beside the text
// has that field ignored, and the proposal it gets is the one its text names.
#[test]
fn a_change_carrying_a_line_number_is_placed_by_its_text_and_not_by_the_number() {
    let f = Fixture::new();
    let args: ProposeDraftChangesArgs = serde_json::from_value(serde_json::json!({
        "path": FILE,
        "rationale": RATIONALE,
        // A line the prompt does not have, beside text it does.
        "hunks": [{ "before": ORIGINAL, "after": PROPOSED, "line": 900 }],
    }))
    .expect("a change carrying a line number decodes");
    let out = block_on(f.tool().call(args)).expect("the number is ignored, not refused");
    assert!(out.proposed);
    let hunks = f.hunks_of_the_one_proposal();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].before_text(), ORIGINAL);
    assert_eq!(hunks[0].after_text(), PROPOSED);
}
