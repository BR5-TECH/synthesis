//! Tests for the `hunks` form of `propose_prompt_changes` (PPC-FR-XHHT,
//! PPC-FR-RJCA, PPC-FR-DLHS, PPC-FR-RXCS, PPC-FR-JGSH).

use super::*;

const REPEATED: &str = "# Review\n\nCheck the input.\n\n## Then\n\nCheck the input.\n";

fn hunk(before: &str, after: &str) -> PromptHunkArg {
    PromptHunkArg {
        kind: None,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        lead: None,
        trail: None,
    }
}

fn with_lead(mut h: PromptHunkArg, lead: &str) -> PromptHunkArg {
    h.lead = Some(lead.into());
    h
}

fn with_trail(mut h: PromptHunkArg, trail: &str) -> PromptHunkArg {
    h.trail = Some(trail.into());
    h
}

impl Fixture {
    fn call_hunks(
        &self,
        hunks: Vec<PromptHunkArg>,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        block_on(self.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            revises: None,
            content: None,
            hunks: Some(hunks),
            rationale: RATIONALE.into(),
        }))
    }

    fn set_artifact(&self, body: &str) {
        std::fs::write(self.path.join(PROMPT), body).expect("write");
    }

    fn candidate_of(&self, id: &str) -> String {
        crate::prompt_proposals::candidate_for_test(&self.root(), id).expect("candidate")
    }
}

/// The hunks that turn `ORIGINAL` into `PROPOSED`.
fn to_proposed() -> Vec<PromptHunkArg> {
    vec![hunk("The original three paragraphs.", "A better opening.\n\nAnd a second paragraph.")]
}

// PPC-FR-XHHT: both forms, or neither, is the retryable not-one-form refusal,
// and nothing is recorded.
#[test]
fn both_forms_or_neither_is_refused_and_records_nothing() {
    let f = Fixture::new();
    for (content, hunks) in [(Some(PROPOSED.to_string()), Some(to_proposed())), (None, None)] {
        let refusal = block_on(f.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            revises: None,
            content,
            hunks,
            rationale: RATIONALE.into(),
        }))
        .expect_err("refused");
        assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM));
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        assert!(f.proposals().is_empty());
        assert!(!f.proposed.load(Ordering::SeqCst));
    }
}

// PPC-FR-XHHT: a missing `content` decodes as absent rather than refusing the
// whole call, so a call with `hunks` alone is accepted by the decoder.
#[test]
fn a_call_with_hunks_alone_decodes() {
    let args: ProposePromptChangesArgs = serde_json::from_value(serde_json::json!({
        "path": PROMPT,
        "rationale": RATIONALE,
        "hunks": [{ "before": "a", "after": "b", "lead": "x", "trail": "y", "note": "ignored" }],
    }))
    .expect("decodes");
    assert!(args.content.is_none());
    let hunks = args.hunks.expect("hunks");
    assert_eq!(hunks[0].lead.as_deref(), Some("x"));
    assert_eq!(hunks[0].trail.as_deref(), Some("y"));
}

// PPC-FR-RJCA, PPC-FR-JGSH, PPC-FR-04: hunks record the same complete candidate,
// comment, and attachment that the whole text would, and end the turn.
#[test]
fn hunks_record_the_candidate_the_whole_text_would() {
    let by_hunks = Fixture::new();
    let output = by_hunks.call_hunks(to_proposed()).expect("recorded");
    assert!(output.proposed);
    assert!(by_hunks.proposed.load(Ordering::SeqCst));
    assert_eq!(by_hunks.candidate_of(&output.proposal_id), PROPOSED);
    assert_eq!(by_hunks.artifact_body(), ORIGINAL, "the file is not rewritten");

    let by_content = Fixture::new();
    let whole = by_content.call().expect("recorded");
    assert_eq!(
        by_hunks.candidate_of(&output.proposal_id),
        by_content.candidate_of(&whole.proposal_id),
    );
    let comment = by_hunks.discussion().comments.last().cloned().expect("comment");
    assert_eq!(comment.body, RATIONALE);
    assert!(matches!(
        comment.attachments.as_slice(),
        [crate::comments::Attachment::PromptProposal { proposal_id, .. }]
            if *proposal_id == output.proposal_id
    ));
}

// PPC-FR-RJCA: `lead` names one copy of a repeated text, and only `before`
// changes.
#[test]
fn a_lead_names_one_copy_of_a_repeated_text() {
    let f = Fixture::new();
    f.set_artifact(REPEATED);
    let output = f
        .call_hunks(vec![with_lead(hunk("Check the input.", "Check the output."), "## Then\n\n")])
        .expect("recorded");
    assert_eq!(
        f.candidate_of(&output.proposal_id),
        "# Review\n\nCheck the input.\n\n## Then\n\nCheck the output.\n",
    );
}

// PPC-FR-RXCS, PPC-FR-DLHS: a repeated text is refused with its place, and the
// message tells the model to add `lead` or `trail`, also when the context it
// gave still repeats.
#[test]
fn a_repeated_text_is_refused_with_its_place_and_the_correction() {
    let f = Fixture::new();
    f.set_artifact(REPEATED);
    for second in [hunk("Check the input.", "x"), with_lead(hunk("Check the input.", "x"), "\n\n")]
    {
        let refusal = f.call_hunks(vec![hunk("# Review", "# Read"), second]).expect_err("refused");
        assert_eq!(
            refusal,
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_UNIQUE, at: 2, of: 2 },
        );
        assert!(refusal.retryable());
        let text = refusal.to_string();
        assert!(text.starts_with("Change 2 of 2"), "{text}");
        assert!(text.contains("`lead` or `trail`"), "{text}");
        assert!(f.proposals().is_empty());
    }
}

// PPC-FR-DLHS: an empty list, text not found, overlapping hunks, and a context
// over its bound are each refused, name the place where there is one, and
// record nothing.
#[test]
fn each_hunk_refusal_names_its_place_and_records_nothing() {
    let f = Fixture::new();
    let long = "x".repeat(201);
    let cases = vec![
        (Vec::new(), ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS)),
        (
            vec![hunk("# Review", "# Read"), hunk("Not in the file.", "x")],
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_FOUND, at: 2, of: 2 },
        ),
        (
            vec![hunk("The original", "An"), hunk("original three", "x")],
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_HUNKS_OVERLAP, at: 2, of: 2 },
        ),
        (
            vec![with_lead(hunk("The original", "An"), &long)],
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_LEAD_TOO_LONG, at: 1, of: 1 },
        ),
    ];
    for (hunks, expected) in cases {
        let refusal = f.call_hunks(hunks).expect_err("refused");
        assert_eq!(refusal, expected);
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        assert!(f.proposals().is_empty());
        assert!(!f.proposed.load(Ordering::SeqCst));
    }
    assert_eq!(f.artifact_body(), ORIGINAL);
}

// PPC-FR-08: a candidate built from hunks that changes nothing is the
// no-change refusal.
#[test]
fn hunks_that_change_nothing_are_the_no_change_refusal() {
    let f = Fixture::new();
    let refusal = f.call_hunks(vec![hunk("The original", "The original")]).expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE));
    assert!(f.proposals().is_empty());
}

// PPC-FR-RJCA: hunks resolve against a CRLF file on disk after line endings are
// normalised.
#[test]
fn hunks_resolve_against_a_crlf_file() {
    let f = Fixture::new();
    f.set_artifact(&ORIGINAL.replace('\n', "\r\n"));
    let output = f.call_hunks(to_proposed()).expect("recorded");
    let built = f.candidate_of(&output.proposal_id);
    assert_eq!(built.replace("\r\n", "\n"), PROPOSED);
}

// PPC-FR-19: a hunk refusal is logged with its reason and its place, and no
// text of the hunks.
#[test]
fn a_hunk_refusal_is_logged_with_its_place_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        revises: None,
        content: None,
        hunks: Some(vec![hunk("# Review", "# Read"), hunk("Not in the file.", "secret words")]),
        rationale: RATIONALE.into(),
    }))
    .expect_err("refused");
    let records = records(&BUFFER, Domain::Ai);
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");
    let fields = serde_json::to_value(&warn.fields).expect("fields");
    assert_eq!(fields["refusal"], "text_not_found");
    assert_eq!(fields["refusalAt"], 2);
    assert_eq!(fields["form"], "hunks");
    assert_eq!(fields["hunks"], 2);
    let whole = format!("{} {}", warn.message, fields);
    for text in ["Not in the file.", "secret words", "# Read", RATIONALE] {
        assert!(!whole.contains(text), "no hunk text: {whole}");
    }
}

// PPC-FR-19: a recorded hunk proposal is logged at INFO with the path, the
// form, and the hunk count, and no text of the hunks or the rationale.
#[test]
fn a_hunk_proposal_is_logged_with_its_form_and_count_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        revises: None,
        content: None,
        hunks: Some(to_proposed()),
        rationale: RATIONALE.into(),
    }))
    .expect("recorded");
    let records = records(&BUFFER, Domain::Ai);
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("the success");
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["path"], PROMPT);
    assert_eq!(fields["form"], "hunks");
    assert_eq!(fields["hunks"], 1);
    let whole = format!("{} {}", info.message, fields);
    for text in ["The original three paragraphs.", "A better opening.", RATIONALE] {
        assert!(!whole.contains(text), "no hunk text: {whole}");
    }
}

// PPC-FR-02: the definition publishes the hunk fields with their descriptions.
#[test]
fn the_hunk_item_publishes_lead_and_trail() {
    let schema = parameters();
    let item = &schema["properties"]["hunks"]["items"]["properties"];
    let mut keys: Vec<&String> = item.as_object().expect("object").keys().collect();
    keys.sort();
    assert_eq!(keys, ["after", "after_text", "before", "kind", "lead", "trail"]);
    assert_eq!(item["lead"]["description"], LEAD_DESCRIPTION);
    assert_eq!(item["trail"]["description"], TRAIL_DESCRIPTION);
    assert!(DESCRIPTION.contains("`hunks`") && DESCRIPTION.contains("`lead`"));
}

// PPC-FR-KWQB: a `trail` alone and both contexts over 200 characters are their
// own refusals, each with its place, and nothing is recorded.
#[test]
fn a_long_trail_and_two_long_contexts_are_named_as_such() {
    let f = Fixture::new();
    let long = "x".repeat(201);
    let cases = [
        (with_trail(hunk("The original", "An"), &long), PROPOSAL_TRAIL_TOO_LONG, "Shorten `trail`"),
        (
            with_trail(with_lead(hunk("The original", "An"), &long), &long),
            PROPOSAL_LEAD_AND_TRAIL_TOO_LONG,
            "Shorten both",
        ),
    ];
    for (hunk, message, correction) in cases {
        let refusal =
            f.call_hunks(vec![self::hunk("# Review", "# Read"), hunk]).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::InvalidArgumentsAt { message, at: 2, of: 2 });
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        let text = refusal.to_string();
        assert!(text.starts_with("Change 2 of 2") && text.contains(correction), "{text}");
        assert!(f.proposals().is_empty());
        assert!(!f.proposed.load(Ordering::SeqCst));
        assert_eq!(f.artifact_body(), ORIGINAL, "the artifact is untouched");
    }
}

// PPC-FR-KWQB: the three refusal texts are the spec's own, word for word.
#[test]
fn the_three_context_refusals_are_the_spec_text() {
    const SPEC: &str =
        include_str!("../../../../../specifications/tools/PPC-propose-prompt-changes-tool.md");
    for text in [PROPOSAL_LEAD_TOO_LONG, PROPOSAL_TRAIL_TOO_LONG, PROPOSAL_LEAD_AND_TRAIL_TOO_LONG]
    {
        assert!(SPEC.contains(&format!("*\"{text}\"*")), "not in the spec: {text}");
    }
}

// PPC-FR-19, PPC-FR-KWQB: each side is logged under its own code, with the
// place and no text of the context.
#[test]
fn each_context_refusal_is_logged_under_its_own_code() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let long = "secret".repeat(40);
    let cases = [
        (with_lead(hunk("The original", "An"), &long), "lead_too_long"),
        (with_trail(hunk("The original", "An"), &long), "trail_too_long"),
        (
            with_trail(with_lead(hunk("The original", "An"), &long), &long),
            "lead_and_trail_too_long",
        ),
    ];
    for (hunk, code) in cases {
        BUFFER.clear();
        block_on(f.tool().with_buffer(&BUFFER).call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            revises: None,
            content: None,
            hunks: Some(vec![hunk]),
            rationale: RATIONALE.into(),
        }))
        .expect_err("refused");
        let records = records(&BUFFER, Domain::Ai);
        let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");
        let fields = serde_json::to_value(&warn.fields).expect("fields");
        assert_eq!(fields["refusal"], code);
        assert_eq!(fields["refusalAt"], 1);
        assert!(!format!("{} {}", warn.message, fields).contains("secret"));
    }
}
