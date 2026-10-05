//! Provider citation markers (CVL-FR-VSSU, CVL-FR-POAR, AGC-FR-INFL).
//!
//! The rule itself is exercised directly; what the loop does with it is
//! exercised through a turn, against the scripted seam.

use super::*;
use std::borrow::Cow;

use super::super::citation_markers::strip_reply_citation_markers;

/// The span an OpenAI model writes for two web-search sources.
const SPAN: &str = "\u{E200}cite\u{E202}turn0search1\u{E202}turn0search2\u{E201}";

// CVL-FR-VSSU: text holding no character of the block is returned as it is,
// without a copy.
#[test]
fn text_without_markers_is_returned_borrowed() {
    for text in ["", "plain prose", "日本語 😀", "\u{E1FF} and \u{E300}"] {
        assert!(
            matches!(strip_citation_markers(text), Cow::Borrowed(t) if t == text),
            "{text:?} came back changed or copied",
        );
    }
}

// CVL-FR-VSSU: the span and the spaces and tabs before it go; nothing else does.
#[test]
fn a_marker_span_is_removed_with_the_spaces_before_it() {
    let cases: &[(String, &str)] = &[
        (format!("Paris is the capital {SPAN}."), "Paris is the capital."),
        (format!("route. {SPAN} Next"), "route. Next"),
        (format!("tabs \t \t{SPAN}!"), "tabs!"),
        (format!("{SPAN} leading"), " leading"),
        (format!("kept line\n{SPAN}"), "kept line\n"),
        (format!("nbsp\u{A0}{SPAN}"), "nbsp\u{A0}"),
        (format!("a {SPAN} {SPAN}."), "a."),
        (format!("日本語 {SPAN}。😀"), "日本語。😀"),
        (format!("  \t{SPAN}"), ""),
        (SPAN.to_string(), ""),
    ];
    for (input, expected) in cases {
        assert_eq!(strip_citation_markers(input), *expected, "input {input:?}");
    }
}

// CVL-FR-VSSU: a span that never closes, or that another opener interrupts, is
// no span. Its opening character goes alone, as any lone character of the block
// does, and the text around it stays.
#[test]
fn a_lone_marker_character_is_removed_alone() {
    let cases: &[(&str, &str)] = &[
        ("a \u{E200}b \u{E200}c\u{E201} d", "a b d"),
        ("a \u{E200}cite\u{E202}turn0", "a citeturn0"),
        ("stray\u{E201} close", "stray close"),
        ("\u{E201}first", "first"),
        ("edges \u{E1FF}\u{E2FF}\u{E300}", "edges \u{E1FF}\u{E300}"),
    ];
    for (input, expected) in cases {
        assert_eq!(strip_citation_markers(input), *expected, "input {input:?}");
    }
}

// CVL-FR-VSSU: removing twice removes nothing more.
#[test]
fn removing_markers_is_idempotent() {
    let once = strip_citation_markers(&format!("x {SPAN} y \u{E200}z")).into_owned();
    assert_eq!(strip_citation_markers(&once), once);
}

// CVL-FR-VSSU, CVL-FR-POAR: the reply's text and every string of every
// argument lose their markers, keys and other values stay, and the count is in
// characters.
#[test]
fn a_reply_loses_its_markers_in_text_and_arguments_and_nothing_else() {
    let mut reply = ModelReply {
        text: format!("Text {SPAN}."),
        tool_calls: vec![rig_tool_call(
            "call-1",
            "ask_discussion_questions",
            serde_json::json!({
            "questions": [{
                "question": format!("Which route? {SPAN}"),
                "options": ["Converse", format!("Mantle{SPAN}")],
            }],
            "key\u{E200}": 3,
            "flag": true,
            "none": null,
            }),
        )],
        ..Default::default()
    };
    let before = reply.clone();

    let removed = strip_reply_citation_markers(&mut reply);

    let span_chars = SPAN.chars().count();
    // The text's span and its space, the question's span and its space, and the
    // option's span with no space before it.
    assert_eq!(removed, (span_chars + 1) * 2 + span_chars);
    assert_eq!(reply.text, "Text.");
    let arguments = &reply.tool_calls[0].function.arguments;
    assert_eq!(arguments["questions"][0]["question"], "Which route?");
    assert_eq!(arguments["questions"][0]["options"][1], "Mantle");
    assert_eq!(arguments["key\u{E200}"], 3, "a key is left alone");
    assert_eq!(arguments["flag"], true);
    assert!(arguments["none"].is_null());
    assert_eq!(reply.tool_calls[0].id, before.tool_calls[0].id);
    assert_eq!(reply.tool_calls[0].function.name, before.tool_calls[0].function.name);
    assert_eq!(reply.native_calls, before.native_calls);
    assert_eq!(reply.input_tokens, before.input_tokens);
}

// CVL-FR-POAR: a clean reply is unchanged and reports nothing removed.
#[test]
fn a_clean_reply_is_left_alone() {
    let mut reply = ModelReply {
        text: "Nothing to remove.".into(),
        ..Default::default()
    };
    let before = reply.clone();
    assert_eq!(strip_reply_citation_markers(&mut reply), 0);
    assert_eq!(reply, before);
}

// CVL-FR-VSSU, CVL-FR-26: the comment a turn appends carries the answer without
// its markers, and the debug record says how many characters went and nothing
// of the text.
#[test]
fn a_delivered_answer_carries_no_marker_and_the_record_holds_only_a_count() {
    let sentinel = "zebracitation";
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer(format!(
        "The {sentinel} answer {SPAN}."
    )))]);
    h.create_agent("arch", "");

    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let folded = folded(&h, "spec.md", &thread.id);
    let body = &folded.comments.last().expect("an answer").body;
    assert_eq!(body, &format!("The {sentinel} answer."));

    let record = wait_for_record(&terminal.id, "removed citation markers from model reply");
    assert_eq!(
        require(&record, "markerCharsRemoved"),
        (SPAN.chars().count() + 1).to_string(),
    );
    let fields = serde_json::to_string(&record.fields).expect("fields serialize");
    assert!(!fields.contains(sentinel), "the record carries no text: {fields}");
}

// CVL-FR-POAR: a clean reply emits no removal record.
#[test]
fn a_clean_answer_emits_no_removal_record() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Clean."))]);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(
        records_where("turnId", &terminal.id)
            .iter()
            .all(|r| r.message != "removed citation markers from model reply"),
    );
}

// CVL-FR-POAR, CVL-FR-18: a reply that was markers alone is empty, and is
// retried as the same logical call.
#[test]
fn a_reply_of_markers_alone_is_empty_and_retried() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::answer(format!("  {SPAN}"))),
        Ok(ScriptedReply::answer("A real answer.")),
    ]);
    h.create_agent("arch", "");

    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2);
    let folded = folded(&h, "spec.md", &thread.id);
    assert_eq!(folded.comments.last().expect("an answer").body, "A real answer.");
}

// CVL-FR-POAR, CVL-FR-18: markers alone on every attempt end the turn as
// `empty_reply`, appending nothing.
#[test]
fn markers_alone_on_every_attempt_end_as_an_empty_reply() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer(SPAN)); MAX_ATTEMPTS]);
    h.create_agent("arch", "");

    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert!(terminal.retry_permitted);
    assert_eq!(folded(&h, "spec.md", &thread.id).comments.len(), thread.comments.len());
}

// AUC-FR-05, CVL-FR-VSSU: a posted question carries no marker, because the
// tool's arguments were cleaned before it read them.
#[test]
fn a_posted_question_carries_no_marker() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::calls(
        crate::tools::ask_user_comment::NAME,
        serde_json::json!({
            "question": format!("Which route? {SPAN}"),
            "options": [format!("Converse {SPAN}"), "Mantle"],
        }),
    ))]);
    h.mount();
    h.create_agent("arch", "");

    let (terminal, thread) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::AwaitingReply);
    let body = folded(&h, "spec.md", &thread.id)
        .comments
        .last()
        .expect("the question")
        .body
        .clone();
    let mut lines = body.lines();
    assert_eq!(lines.next(), Some("Which route?"), "{body:?}");
    assert!(body.contains("- Converse\n"), "{body:?}");
    assert!(body.contains("- Mantle"), "{body:?}");
    assert!(!body.chars().any(|c| ('\u{E200}'..='\u{E2FF}').contains(&c)), "{body:?}");
}

// PDC-FR-12, CVL-FR-VSSU: a draft proposal's comment is its rationale less its
// citation markers, exactly.
#[test]
fn a_draft_proposal_rationale_carries_no_marker() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::calls(
        crate::tools::propose_draft_changes::NAME,
        serde_json::json!({
            "path": "artifact-window.md",
            "rationale": format!("The half buries its point {SPAN}."),
            "hunks": [{ "kind": "replace", "before": "Loose notes.", "after": "Better." }],
        }),
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (draft_id, thread) = seed_discussion(&h, &["@arch what would you change?"]);
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::AwaitingReply);

    let stored = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(stored.comments[1].body, "The half buries its point.");
}

// PPC-FR-13, CVL-FR-VSSU: a prompt proposal's comment is its rationale less its
// citation markers, exactly.
#[test]
fn a_prompt_proposal_rationale_carries_no_marker() {
    const PATH: &str = "prompts/review.md";
    let h = Harness::scripted(vec![Ok(ScriptedReply::calls(
        crate::tools::propose_prompt_changes::NAME,
        serde_json::json!({
            "path": PATH,
            "content": "# Review\n\nRead it.\n",
            "rationale": format!("The step is buried {SPAN}."),
        }),
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let root = h.root();
    std::fs::create_dir_all(root.path().join("prompts")).expect("dirs");
    std::fs::write(root.path().join(PATH), "# Review\n\nRead the file.\n").expect("prompt");
    std::fs::create_dir_all(root.path().join(".synthesis")).expect("dirs");
    std::fs::write(
        root.path().join(".synthesis/library.toml"),
        format!("[assignments.\"{PATH}\"]\ntype = \"prompt\"\nscope = \"file\"\n"),
    )
    .expect("assignment");
    let thread = crate::comments::open_discussion_in(
        &root,
        &root,
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: PATH.into(),
        },
        None,
        "@arch what would you change?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::AwaitingReply);

    let stored = crate::comments::fold_artifact_discussion(&h.root(), PATH)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(stored.comments[1].body, "The step is buried.");
}

// CVL-FR-VSSU: the reply that goes back into the exchange for the next round is
// the cleaned one, text and arguments both.
#[test]
fn the_next_round_reads_the_cleaned_reply() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": format!("spec.md{SPAN}") }),
        )
        .with_text(format!("Reading it {SPAN}."))),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");

    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let second = h.seam.exchanges().into_iter().nth(1).expect("a second round");
    let wire = serde_json::to_string(&second).expect("the exchange serializes");
    assert!(wire.contains("Reading it."), "{wire}");
    assert!(!wire.contains("turn0search1"), "{wire}");
    assert!(!wire.contains("\\ue200") && !wire.contains('\u{E200}'), "{wire}");
}

// AGC-FR-INFL: a stored body sent back to the model, as history and as the
// current comment, loses its markers. The stored body keeps them.
#[test]
fn bodies_sent_back_to_the_model_lose_their_markers_and_the_store_keeps_them() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("ok"))]);
    h.mount();
    h.create_agent("arch", "");
    let earlier = format!("An earlier remark {SPAN}.");
    let current = format!("@arch and now? {SPAN}");
    let (draft_id, thread) = seed_discussion(&h, &[&earlier, &current]);

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[1].id,
        )
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);

    let (request, _) = h.seam.requests().into_iter().next().expect("one request");
    let text_of = |tag: &str| -> String { section_of(&request.input, tag).body.clone() };
    let history = text_of("discussion_history");
    let current_section = text_of("current_comment");
    assert!(history.contains("An earlier remark."), "{history}");
    assert!(current_section.contains("@arch and now?"), "{current_section}");
    for text in [&history, &current_section] {
        assert!(!text.chars().any(|c| ('\u{E200}'..='\u{E2FF}').contains(&c)), "{text}");
    }

    let stored = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(stored.comments[0].body, earlier, "the stored body is not changed");
}
