//! The token budget of a turn and the compaction of its older history
//! (`CVL-conversation-loop.md` CVL-FR-LCXM, CVL-FR-PGII, CVL-FR-XCOY,
//! CVL-FR-EBJG, CVL-FR-ZLSM, CVL-FR-BSMY, CVL-FR-HFRL, CVL-FR-CSLX).

use super::*;
use crate::agent_conversations::compaction::*;
use rig::completion::Message;

fn section(tag: &str, body: &str) -> InputSection {
    InputSection {
        tag: tag.into(),
        attributes: Vec::new(),
        body: body.into(),
        truncated: false,
        parts: Vec::new(),
    }
}

fn input_with_history(history: &str) -> AgentRequest {
    AgentRequest {
        instructions: "abcd".into(),
        input: vec![
            section(TAG_ARTIFACT, "the artifact"),
            section(TAG_DISCUSSION_HISTORY, history),
            section(TAG_CURRENT_COMMENT, "the question"),
        ],
        stable_head_sections: 1,
        ..Default::default()
    }
}

fn tool_round(id: &str, result: &str) -> Vec<Message> {
    vec![
        Message::Assistant(rig::completion::AssistantMessage::new(vec![
            rig::completion::AssistantContent::ToolCall(rig_tool_call(
                id,
                "read_file",
                serde_json::json!({ "path": "spec.md" }),
            )),
        ])),
        tool_result_message(id, "read_file", result),
    ]
}

// CVL-FR-PGII: one token for each four characters, rounded up, plus a fixed
// amount for each image.
#[test]
fn the_estimate_counts_four_characters_a_token_and_a_fixed_amount_an_image() {
    let request = AgentRequest { instructions: "a".repeat(7), ..Default::default() };
    let exchange = vec![Message::user("b".repeat(6))];
    assert_eq!(estimate_input_tokens(&request, &exchange), 4);

    // An image counts a fixed amount beside the text of its message.
    let mut pictured = input_with_history("");
    pictured.input[0].parts = vec![InputPart::Image(InputImage {
        data: "AAAA".into(),
        media_type: "image/png".into(),
        context: "c".into(),
        metadata: "m".into(),
    })];
    let exchange = opening_exchange(&pictured, true);
    let text_chars: u64 = match &exchange[0] {
        Message::User { content } => content
            .iter()
            .filter_map(|part| match part {
                rig::completion::message::UserContent::Text(t) => {
                    Some(t.text.chars().count() as u64)
                }
                _ => None,
            })
            .sum(),
        _ => unreachable!(),
    };
    assert_eq!(
        estimate_input_tokens(&AgentRequest::default(), &exchange),
        text_chars.div_ceil(CHARS_PER_TOKEN) + IMAGE_TOKENS
    );
    // The tool definitions count too.
    let tool = rig::completion::ToolDefinition {
        name: rig::completion::message::ToolName::new("t").unwrap(),
        description: "d".repeat(40),
        parameters: serde_json::json!({}),
    };
    let with_tools = AgentRequest { tools: vec![tool], ..Default::default() };
    assert!(estimate_input_tokens(&with_tools, &[]) >= 10);
}

// CVL-FR-EBJG: compaction starts above three quarters of the budget.
#[test]
fn the_threshold_is_three_quarters_of_the_budget() {
    assert!(!over_budget(750, 1000));
    assert!(over_budget(751, 1000));
    assert!(!over_budget(0, 0));
}

// CVL-FR-XCOY: the latest round is the last reply that asked for tools and its
// results; everything between it and the input is older history.
#[test]
fn the_latest_round_is_the_last_reply_that_asked_for_tools() {
    let input = vec![Message::user("input")];
    assert_eq!(latest_round_start(&input), 1);

    let mut exchange = input.clone();
    exchange.extend(tool_round("c1", "one"));
    assert_eq!(latest_round_start(&exchange), 1);
    exchange.extend(tool_round("c2", "two"));
    assert_eq!(latest_round_start(&exchange), 3);

    let request = input_with_history("");
    assert!(!has_older_history(&request, &exchange[..3]));
    assert!(has_older_history(&request, &exchange));
    assert!(has_older_history(&input_with_history("ada: earlier"), &input));
}

// CVL-FR-ZLSM: the older history reaches the compaction call as text, with an
// image replaced by its context text, and the latest round left out.
#[test]
fn the_older_history_is_text_with_images_as_their_context() {
    let mut request = input_with_history("ada: look [image metadata]");
    request.input[1].parts = vec![
        InputPart::Text("ada: look".into()),
        InputPart::Image(InputImage {
            data: "AAAA".into(),
            media_type: "image/png".into(),
            context: "diagram.png from ada".into(),
            metadata: "image/png diagram.png".into(),
        }),
    ];
    let mut exchange = vec![Message::user("input")];
    exchange.extend(tool_round("c1", "OLDER-RESULT"));
    exchange.extend(tool_round("c2", "LATEST-RESULT"));

    let text = older_history_text(&request, &exchange);
    assert!(text.contains("<discussion_history>"), "{text}");
    assert!(text.contains("[image: diagram.png from ada]"), "{text}");
    assert!(!text.contains("AAAA"), "no image bytes: {text}");
    assert!(text.contains("OLDER-RESULT"), "{text}");
    assert!(text.contains("[agent called read_file]"), "{text}");
    assert!(!text.contains("LATEST-RESULT"), "the latest round is kept: {text}");

    let compaction = compaction_request();
    assert!(compaction.tools.is_empty());
    assert!(compaction.input.is_empty());
    assert_eq!(compaction.instructions, crate::prompts::instruction_text(COMPACT_PROMPT_TEMPLATE));
}

// CVL-FR-BSMY: the summary replaces the discussion history and every older
// message; the latest round stays as it was.
#[test]
fn the_summary_replaces_the_older_history_and_the_latest_round_stays() {
    let request = input_with_history("ada: earlier");
    let compacted = compacted_request(&request, "THE SUMMARY");
    assert_eq!(
        tags_of(&compacted.input),
        [TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]
    );
    assert!(compacted.input[1].body.contains("THE SUMMARY"));
    assert!(!compacted.input[1].body.contains("ada: earlier"));
    assert_eq!(compacted.input[0], request.input[0]);

    // An input with no discussion history gains one before the current comment.
    let mut bare = request.clone();
    bare.input.remove(1);
    let compacted_bare = compacted_request(&bare, "S");
    assert_eq!(
        tags_of(&compacted_bare.input),
        [TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]
    );

    let mut exchange = opening_exchange(&request, false);
    exchange.extend(tool_round("c1", "OLDER"));
    let latest = tool_round("c2", "LATEST");
    exchange.extend(latest.clone());
    let out = compacted_exchange(&compacted, false, &exchange);
    assert_eq!(out.len(), 3);
    assert_eq!(&out[1..], &latest[..]);
    assert_eq!(out[0], opening_exchange(&compacted, false)[0]);
}

/// A Custom provider whose model `m` has the given detected context size, and
/// an agent on it.
fn custom_budget_harness(
    replies: Vec<Result<ScriptedReply, &'static str>>,
    budget: Option<u64>,
) -> Harness {
    sized_harness(replies, budget, None)
}

/// A Custom provider whose model `m` has the given detected context size and
/// whose record has the given max context size, and an agent on it.
fn sized_harness(
    replies: Vec<Result<ScriptedReply, &'static str>>,
    detected: Option<u64>,
    max_context_size: Option<u64>,
) -> Harness {
    let h = Harness::scripted(replies);
    h.mount();
    h.seed_second_provider("custom");
    let mut records = h.store().load_ai_api_registry().unwrap().0;
    let custom = records.iter_mut().find(|r| r.provider == "custom").unwrap();
    custom.models =
        vec![ModelOption { detected_context_size: detected, ..ModelOption::new("m", "m") }];
    custom.max_context_size = max_context_size;
    h.store().save_ai_api_registry(records, Some("custom".into())).unwrap();
    h.create_plain_agent("arch");
    h
}

/// The length of the text of an assistant message, or `None` for any other
/// message.
fn assistant_text_len(message: &Message) -> Option<usize> {
    let Message::Assistant(assistant) = message else { return None };
    assistant.content.iter().find_map(|c| match c {
        rig::completion::AssistantContent::Text(text) => Some(text.text.len()),
        _ => None,
    })
}

/// A reply that asks for `read_file` and carries `chars` characters of prose.
fn big_tool_reply(chars: usize) -> ScriptedReply {
    ScriptedReply {
        text: "x".repeat(chars),
        ..ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))
    }
}

// CVL-FR-LCXM, CVL-FR-EBJG, CVL-FR-ZLSM, CVL-FR-BSMY, CVL-FR-CSLX: a call over
// the budget is preceded by one compaction call, and the turn goes on with the
// summary in place of the older history.
#[test]
fn a_call_over_the_budget_is_preceded_by_a_compaction_call() {
    let h = custom_budget_harness(
        vec![
            Ok(big_tool_reply(400_000)),
            Ok(big_tool_reply(400_001)),
            Ok(ScriptedReply::answer("SUMMARY-OF-OLDER-ROUNDS")),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(200_000),
    );

    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let exchanges = h.seam.exchanges();
    let requests = h.seam.requests();
    assert_eq!(exchanges.len(), 4, "two rounds, one compaction call, one final round");
    // The compaction call: its own instruction, no tool, the older history.
    assert!(requests[2].0.tools.is_empty());
    assert_eq!(
        requests[2].0.instructions,
        crate::prompts::instruction_text(COMPACT_PROMPT_TEMPLATE)
    );
    assert_eq!(exchanges[2].len(), 1);
    // CVL-FR-ZLSM: the same endpoint, model, and reasoning as the turn's own
    // calls.
    assert_eq!(requests[2].1, requests[3].1);
    // The final round: the compacted input, then the latest round alone. The
    // latest round is the second reply, which is the longer one.
    assert_eq!(exchanges[3].len(), 3);
    assert_eq!(assistant_text_len(&exchanges[3][1]), Some(400_001));
    assert!(matches!(&exchanges[3][2], Message::User { .. }));
    let input = &requests[3].0.input;
    assert!(section_of(input, TAG_DISCUSSION_HISTORY).body.contains("SUMMARY-OF-OLDER-ROUNDS"));

    let record = wait_for_record(&terminal.id, "conversation history compaction");
    assert_eq!(field(&record, "outcome"), "compacted");
    assert_eq!(field(&record, "budgetTokens"), "200000");
    // The anchored discussion history and the first round.
    assert_eq!(field(&record, "compactedMessages"), "3");
    let estimate: u64 = field(&record, "estimatedTokens").parse().unwrap();
    assert!(estimate * 4 > 200_000 * 3);
    // CVL-FR-CSLX: the record of the compaction call says what it is, so it is
    // not read as a second call of the same ordinal.
    let calls: Vec<String> = all_records()
        .iter()
        .filter(|r| r.message == "calling model" && field(r, "turnId") == terminal.id)
        .map(|r| format!("{}:{}", field(r, "modelCall"), field(r, "compaction")))
        .collect();
    assert_eq!(calls, ["1:false", "2:false", "3:true", "3:false"]);
    for record in all_records() {
        assert!(
            !format!("{record:?}").contains("SUMMARY-OF-OLDER-ROUNDS"),
            "no summary in a record"
        );
    }
}

// CVL-FR-LCXM, AAP-FR-RANS: a model with no detected size and no override
// has the default budget of 1,000,000 tokens. Below three quarters of it the
// turn compacts nothing.
#[test]
fn a_turn_below_the_default_budget_compacts_nothing() {
    let h = custom_budget_harness(
        vec![
            Ok(big_tool_reply(400_000)),
            Ok(big_tool_reply(400_000)),
            Ok(ScriptedReply::answer("Final.")),
        ],
        None,
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 3);
    assert_eq!(h.seam.exchanges()[2].len(), 5, "the exchange only grew");
}

// CVL-FR-LCXM, AAP-FR-RANS: with no detected size and no override, a call
// over three quarters of the default budget is compacted all the same.
#[test]
fn a_turn_over_the_default_budget_is_compacted() {
    let h = custom_budget_harness(
        vec![
            Ok(big_tool_reply(1_600_000)),
            Ok(big_tool_reply(1_600_001)),
            Ok(ScriptedReply::answer("SUMMARY")),
            Ok(ScriptedReply::answer("Final.")),
        ],
        None,
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 4, "two rounds, one compaction call, one final round");
    let record = wait_for_record(&terminal.id, "conversation history compaction");
    assert_eq!(field(&record, "outcome"), "compacted");
    assert_eq!(field(&record, "budgetTokens"), "1000000");
}

// CVL-FR-LCXM, AAP-FR-RANS: the provider's max context size wins over the
// detected size, so it is the budget.
#[test]
fn the_max_context_size_is_the_budget_over_a_detected_size() {
    let h = sized_harness(
        vec![
            Ok(big_tool_reply(400_000)),
            Ok(big_tool_reply(400_001)),
            Ok(ScriptedReply::answer("SUMMARY")),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(10_000_000),
        Some(200_000),
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 4);
    let record = wait_for_record(&terminal.id, "conversation history compaction");
    assert_eq!(field(&record, "budgetTokens"), "200000");
}

// CVL-FR-HFRL: a compaction call that fails leaves the exchange as it was, and
// the turn goes on; it does not count against the logical bound.
#[test]
fn a_failed_compaction_call_leaves_the_exchange_unchanged() {
    let h = custom_budget_harness(
        vec![
            Ok(big_tool_reply(400_000)),
            Ok(big_tool_reply(400_000)),
            Err("rejected"),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(200_000),
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 4);
    assert_eq!(exchanges[3].len(), 5, "nothing was compacted");

    let record = wait_for_record(&terminal.id, "conversation history compaction");
    assert_eq!(field(&record, "outcome"), "failed");
}

// CVL-FR-HFRL, CVL-FR-ZLSM: a compaction call that gives no text, after the
// retries of its own budget, leaves the exchange as it was, and the turn goes
// on.
#[test]
fn a_compaction_reply_with_no_text_leaves_the_exchange_unchanged() {
    let mut replies = vec![Ok(big_tool_reply(400_000)), Ok(big_tool_reply(400_000))];
    replies.extend(vec![Ok(ScriptedReply::answer("   ")); MAX_ATTEMPTS]);
    replies.push(Ok(ScriptedReply::answer("Final.")));
    let h = custom_budget_harness(replies, Some(200_000));
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 3 + MAX_ATTEMPTS);
    assert_eq!(exchanges.last().unwrap().len(), 5, "nothing was compacted");
    let record = wait_for_record(&terminal.id, "conversation history compaction");
    assert_eq!(field(&record, "outcome"), "failed");
}

// CVL-FR-BSMY: a later compaction treats the earlier summary as older history.
#[test]
fn a_later_compaction_takes_the_earlier_summary_as_older_history() {
    let h = custom_budget_harness(
        vec![
            Ok(big_tool_reply(400_000)),
            Ok(big_tool_reply(400_000)),
            Ok(ScriptedReply::answer("FIRST-SUMMARY")),
            Ok(big_tool_reply(400_000)),
            Ok(ScriptedReply::answer("SECOND-SUMMARY")),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(200_000),
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let requests = h.seam.requests();
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 6);
    let compact = crate::prompts::instruction_text(COMPACT_PROMPT_TEMPLATE);
    assert_eq!(requests[4].0.instructions, compact);
    let older = format!("{:?}", exchanges[4]);
    assert!(older.contains("FIRST-SUMMARY"), "the first summary is older history now");
    let input = &requests[5].0.input;
    let history = &section_of(input, TAG_DISCUSSION_HISTORY).body;
    assert!(history.contains("SECOND-SUMMARY"));
    assert!(!history.contains("FIRST-SUMMARY"));
}
