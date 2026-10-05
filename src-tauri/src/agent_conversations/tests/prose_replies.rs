//! A tool call written as prose (CVL-FR-JPHT, CVL-FR-UJXD, CVL-FR-ARTC).
//!
//! The recognition rule is exercised against tool definitions directly; what
//! the loop does with such a reply is exercised through a turn.

use super::*;

use super::super::prose_tool_calls::tool_written_as_prose;

fn tool(name: &str, parameters: serde_json::Value) -> rig::completion::ToolDefinition {
    rig::completion::ToolDefinition {
        name: name.into(),
        description: String::new(),
        parameters,
    }
}

/// The real question tool beside two shaped like the gathering tools.
fn tools() -> Vec<rig::completion::ToolDefinition> {
    vec![
        tool(
            "search_specifications",
            serde_json::json!({
                "type": "object",
                "properties": { "query": {}, "limit": {} },
                "required": ["query"],
            }),
        ),
        tool(
            "read_file",
            serde_json::json!({
                "type": "object",
                "properties": { "path": {}, "limit": {} },
                "required": ["path"],
            }),
        ),
        tool(
            crate::tools::ask_discussion_questions::NAME,
            crate::tools::ask_discussion_questions::parameters(),
        ),
        tool(
            "list_skills",
            serde_json::json!({ "type": "object", "properties": {}, "required": [] }),
        ),
    ]
}

const QUESTIONS: &str = r#"{"questions":[{"question":"One spec or two?","options":["one","two"]}]}"#;

// CVL-FR-JPHT: a question set written out, bare or in one enclosing fence, is
// the questions tool's call.
#[test]
fn a_question_set_written_out_is_recognised_bare_or_fenced() {
    let tools = tools();
    for text in [
        QUESTIONS.to_string(),
        format!("  \n{QUESTIONS}\n  "),
        format!("```json\n{QUESTIONS}\n```"),
        format!("```\n{QUESTIONS}\n```"),
        format!("```JSON  \n{QUESTIONS}\n```\n"),
    ] {
        assert_eq!(
            tool_written_as_prose(&text, &tools),
            Some(crate::tools::ask_discussion_questions::NAME),
            "{text:?}",
        );
    }
}

// CVL-FR-JPHT: an envelope naming an attached tool beside an `arguments` or
// `parameters` object is that tool's call, whatever else it holds.
#[test]
fn an_envelope_naming_an_attached_tool_is_recognised() {
    let tools = tools();
    for text in [
        r#"{"name":"read_file","arguments":{"path":"a.md"}}"#,
        r#"{"name":"read_file","parameters":{"path":"a.md"}}"#,
        r#"{"name":"read_file","arguments":{}}"#,
        r#"{"type":"function","name":"read_file","arguments":{"path":"a.md"}}"#,
    ] {
        assert_eq!(tool_written_as_prose(text, &tools), Some("read_file"), "{text}");
    }
}

// CVL-FR-JPHT: an envelope naming a tool that is not attached, or carrying its
// arguments as a string, is not recognised as an envelope.
#[test]
fn an_envelope_that_names_nothing_attached_is_not_recognised() {
    let tools = tools();
    for text in [
        r#"{"name":"delete_everything","arguments":{"path":"a.md"}}"#,
        r#"{"name":"read_file","arguments":"{\"path\":\"a.md\"}"}"#,
    ] {
        assert_eq!(tool_written_as_prose(text, &tools), None, "{text}");
    }
}

// CVL-FR-JPHT: an object must hold every required key and no key the schema
// does not name. The first attached tool it fits is the one named.
#[test]
fn an_object_is_recognised_only_where_it_fits_a_schema() {
    let tools = tools();
    assert_eq!(tool_written_as_prose(r#"{"path":"x"}"#, &tools), Some("read_file"));
    assert_eq!(
        tool_written_as_prose(r#"{"query":"x"}"#, &tools),
        Some("search_specifications"),
    );
    // An empty schema requiring nothing fits the empty object alone.
    assert_eq!(tool_written_as_prose("{}", &tools), Some("list_skills"));
    for text in [r#"{"path":"x","line":3}"#, r#"{"limit":5}"#, r#"{"timeout":30}"#] {
        assert_eq!(tool_written_as_prose(text, &tools), None, "{text}");
    }
}

// CVL-FR-JPHT: anything other than exactly one JSON object is prose.
#[test]
fn anything_but_exactly_one_object_is_not_recognised() {
    let tools = tools();
    for text in [
        "Plain prose.".to_string(),
        "[1, 2]".to_string(),
        format!("{QUESTIONS}{QUESTIONS}"),
        format!("{QUESTIONS} and more"),
        format!("Here: {QUESTIONS}"),
        r#"{"questions":[{"question":"x""#.to_string(),
        // One fence alone is removed, and only one that encloses the whole.
        format!("```{QUESTIONS}```"),
        format!("````json\n{QUESTIONS}\n````"),
        format!("~~~json\n{QUESTIONS}\n~~~"),
        format!("```json\n{QUESTIONS}"),
        format!("```json\n```json\n{QUESTIONS}\n```\n```"),
        format!("{}{}", "[".repeat(10_000), "]".repeat(10_000)),
    ] {
        assert_eq!(tool_written_as_prose(&text, &tools), None, "{:.60}", text);
    }
    assert_eq!(tool_written_as_prose(QUESTIONS, &[]), None, "no tool is attached");
}

fn asks_questions() -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::ask_discussion_questions::NAME,
        serde_json::from_str(QUESTIONS).expect("valid"),
    )
}

/// A discussion turn on a fresh draft, run to its end.
fn run_discussion(h: &Harness) -> (AgentTurn, String, Discussion) {
    h.mount();
    h.create_agent("arch", "");
    let (draft_id, thread) = seed_discussion(h, &["@arch what would you change?"]);
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    (wait_for_terminal(h, &turn.id), draft_id, thread)
}

fn comments_now(h: &Harness, draft_id: &str, thread_id: &str) -> usize {
    crate::comments::fold_discussion(&h.root(), draft_id)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("thread")
        .comments
        .len()
}

// CVL-FR-UJXD, CVL-FR-18, CVL-FR-20: a question set written as prose is not
// delivered. The same logical call is retried, and the real call that follows
// records the set and ends the turn.
#[test]
fn a_question_set_written_as_prose_is_retried_and_the_real_call_records_it() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::answer(format!("```json\n{QUESTIONS}\n```"))),
        Ok(asks_questions()),
    ]);
    let (turn, draft_id, thread) = run_discussion(&h);

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.call_count(), 2);
    assert_eq!(comments_now(&h, &draft_id, &thread.id), 1, "no JSON comment was appended");
    let set = crate::comments::read_question_set(&h.root(), &h.root(), &thread.id)
        .expect("the set was recorded");
    assert_eq!(set.questions[0].text, "One spec or two?");

    // CVL-FR-20: the retry is the same logical call, so the second attempt
    // presents exactly the exchange the first did.
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges[0], exchanges[1]);
}

// CVL-FR-UJXD: prose on every attempt ends the turn as `empty_reply`. Nothing
// is appended, nothing is recorded, and each attempt is warned with the tool's
// name and none of the text.
#[test]
fn a_tool_call_written_as_prose_on_every_attempt_ends_as_an_empty_reply() {
    let sentinel = "Zanzibarquestion";
    let prose = QUESTIONS.replace("One spec or two?", sentinel);
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer(prose)); MAX_ATTEMPTS]);
    let (turn, draft_id, thread) = run_discussion(&h);

    assert_eq!(turn.state, AgentTurnState::Failed);
    assert_eq!(turn.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert!(turn.retry_permitted);
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
    assert_eq!(comments_now(&h, &draft_id, &thread.id), 1);
    assert!(crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).is_none());

    let warnings: Vec<_> = records_where("turnId", &turn.id)
        .into_iter()
        .filter(|r| r.message == "agent wrote a tool call as prose")
        .collect();
    assert_eq!(warnings.len(), MAX_ATTEMPTS);
    for (i, warning) in warnings.iter().enumerate() {
        assert_eq!(warning.level, LogLevel::Warn);
        assert_eq!(require(warning, "tool"), crate::tools::ask_discussion_questions::NAME);
        assert_eq!(require(warning, "attempt"), (i + 1).to_string());
        let fields = serde_json::to_string(&warning.fields).expect("fields serialize");
        assert!(!fields.contains(sentinel), "the record carries no text: {fields}");
    }
}

// CVL-FR-VSSU before CVL-FR-JPHT: markers after the object are removed first,
// so the object is still recognised.
#[test]
fn a_tool_call_followed_by_a_citation_marker_is_still_recognised() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::answer(format!(
            "{QUESTIONS}\u{E200}cite\u{E202}turn0search1\u{E201}"
        ))),
        Ok(asks_questions()),
    ]);
    let (turn, _, _) = run_discussion(&h);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.call_count(), 2);
}

// CVL-FR-JPHT: on an anchored comment the questions tool is not attached
// (CVL-FR-08), so the same text is an ordinary answer and is delivered.
#[test]
fn the_same_text_is_an_answer_where_the_questions_tool_is_not_attached() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer(QUESTIONS))]);
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 1);
    let folded = folded(&h, "spec.md", &thread.id);
    assert_eq!(folded.comments.last().expect("answer").body, QUESTIONS);
    assert!(
        records_where("turnId", &turn.id)
            .iter()
            .all(|r| r.message != "agent wrote a tool call as prose"),
    );
}

// CVL-FR-12: prose that only holds JSON, or is JSON fitting no schema, is the
// answer.
#[test]
fn prose_that_merely_holds_json_is_delivered() {
    for text in [r#"Use this: {"query":"x"}"#, r#"{"timeout":30}"#] {
        let h = Harness::scripted(vec![Ok(ScriptedReply::answer(text))]);
        h.create_agent("arch", "");
        let (turn, thread) = run_one(&h, "arch");
        assert_eq!(turn.state, AgentTurnState::Delivered, "{text}");
        assert_eq!(h.seam.call_count(), 1, "{text}");
        assert_eq!(folded(&h, "spec.md", &thread.id).comments.last().expect("a").body, text);
    }
}

// CVL-FR-UJXD: a reply that also asks for a real tool is no tool call written
// as prose, whatever its text.
#[test]
fn a_reply_with_a_real_call_is_dispatched_whatever_its_text() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))
            .with_text(r#"{"path":"spec.md"}"#)),
        Ok(ScriptedReply::answer("Read it.")),
    ]);
    h.create_agent("arch", "");
    let (turn, _) = run_one(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2);
}

// CVL-FR-ARTC: each discussion prompt tells the agent to ask through its tool
// and never to write questions or a tool's arguments as text.
#[test]
fn every_discussion_prompt_says_to_ask_through_the_tool_and_not_as_text() {
    for (name, template) in [
        ("discuss-draft", DISCUSS_DRAFT_PROMPT_TEMPLATE),
        ("discuss-artifact", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
        ("discuss-note", DISCUSS_NOTE_PROMPT_TEMPLATE),
    ] {
        let lower = template.to_lowercase();
        assert!(
            lower.contains("ask by calling your tool for asking the author"),
            "{name} does not say to ask through the tool",
        );
        assert!(
            lower.contains("never write the questions, or the arguments of any tool, into the text of your reply"),
            "{name} does not forbid writing a call as text",
        );
    }
}
