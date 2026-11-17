//! The limits that keep a turn inside the context window, end to end: the
//! result bound (CVL-FR-XMGI, CVL-FR-ZJLK, CVL-FR-AXQN), the last-call line
//! (CVL-FR-FSAY), and the recovery from a context overflow (CVL-FR-PFNV,
//! CVL-FR-ZFMU, CVL-FR-QJLU, CVL-FR-LQDM, CVL-FR-LZDN). One part of
//! `../tests/mod.rs`.

use super::compaction::sized_harness;
use super::support::*;
use super::*;
use crate::agent_conversations::scripted::SCRIPTED_STRUCTURED;

/// A file of `chars` characters in the worktree, opening and ending with a
/// mark this test alone uses.
fn big_file(h: &Harness, name: &str, chars: usize, mark: &str) {
    let fill = chars - 2 * mark.len();
    let text = format!("{mark}{}{mark}", "x".repeat(fill));
    std::fs::write(h.root().join(name), text).expect("write the file");
}

fn reads(name: &str) -> ScriptedReply {
    ScriptedReply::calls("read_file", serde_json::json!({ "path": name }))
}

/// A structured overflow refusal, with a failure value of its own so a test
/// can tell it from any other.
fn refusal(status: u16, message: &str) -> CallFailure {
    CallFailure {
        status: Some(status),
        provider_message: Some(message.to_string()),
        ..CallFailure::new(FAIL_INVALID_REQUEST, class::HTTP_STATUS)
    }
}

/// A structured refusal that the loop repeats on the usual terms.
fn repeatable_refusal(status: u16, message: &str) -> CallFailure {
    CallFailure {
        status: Some(status),
        provider_message: Some(message.to_string()),
        ..CallFailure::unreachable(class::HTTP_STATUS)
    }
}

const RECOVERY: &str = "agent turn recovering from a context overflow; set the provider's max context size to the real window of the model";

const TOO_LONG: &str = "prompt is too long: 250000 tokens > 200000 maximum";

fn records_of(turn_id: &str, message: &str) -> Vec<LogRecord> {
    all_records()
        .into_iter()
        .filter(|r| r.message == message && field(r, "turnId") == turn_id)
        .collect()
}

/// The free room the result `drop` messages from the end of `exchange` met:
/// the room of the call as it stood before that result was appended.
fn bound_before(
    request: &AgentRequest,
    exchange: &[rig::completion::Message],
    drop: usize,
) -> usize {
    let before = &exchange[..exchange.len() - drop];
    let estimate = crate::agent_conversations::compaction::estimate_input_tokens(request, before);
    crate::agent_conversations::result_cap::result_bound(60_000, estimate)
}

// CVL-FR-XMGI, CVL-FR-ZJLK, CVL-FR-AXQN: each result meets the free room of
// the next call as it stands, the results of the round already appended
// included. A result that fits stands whole; one above the room is cut there
// with the line that names the range arguments, and one record says so.
#[test]
fn each_result_meets_the_free_room_of_the_call() {
    // A 60,000-token window: the mark is 45,000 tokens.
    let h = sized_harness(
        vec![
            Ok(reads("first.md")
                .and_calls("read_file", serde_json::json!({ "path": "second.md" }))),
            Ok(ScriptedReply::answer("Done.")),
        ],
        Some(60_000),
        None,
    );
    big_file(&h, "first.md", 100_000, "MARK-FIRST");
    big_file(&h, "second.md", 100_000, "MARK-SECOND");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let exchange = h.seam.exchanges()[1].clone();
    let request = h.seam.requests()[1].0.clone();
    let results = tool_results(&exchange);
    // The first result fits the room it met and stands whole.
    assert!(bound_before(&request, &exchange, 2) >= 100_000);
    assert_eq!(results[0].chars().count(), 100_000);
    // The second met a room less the first result, and is cut there.
    let bound = bound_before(&request, &exchange, 1);
    assert!(bound < 100_000, "the room is {bound}");
    let records = records_of(&terminal.id, "agent tool result cut to the result bound");
    assert_eq!(records.len(), 1, "one record for the one cut");
    assert_eq!(records[0].level, logging::LogLevel::Debug);
    assert_eq!(field(&records[0], "tool"), "read_file");
    assert_eq!(field(&records[0], "resultChars"), "100000");
    let kept: usize = field(&records[0], "keptChars").parse().unwrap();
    let line = format!(
        "[The result was cut at {kept} of 100000 characters. Read the rest in parts with offset and limit.]"
    );
    assert!(results[1].starts_with("MARK-SECOND"));
    assert!(results[1].ends_with(&line));
    // The cut result, its line included, fills the room and no more.
    assert_eq!(results[1].chars().count(), kept + 2 + line.chars().count());
    assert!(results[1].chars().count() <= bound);
    assert!(bound - results[1].chars().count() < 10);
    // So the next call stays at or below the compaction mark: no compaction.
    assert_eq!(h.seam.call_count(), 2);
    for record in all_records() {
        let text = format!("{record:?}");
        assert!(!text.contains("MARK-FIRST") && !text.contains("MARK-SECOND"));
    }
}

// CVL-FR-XMGI: a large window leaves a large room, so a large result stands
// whole.
#[test]
fn a_large_window_takes_a_large_result_whole() {
    let h = sized_harness(
        vec![Ok(reads("big.md")), Ok(ScriptedReply::answer("Done."))],
        Some(1_000_000),
        None,
    );
    big_file(&h, "big.md", 500_000, "MARK-LARGE");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results[0].chars().count(), 500_000);
    assert!(records_of(&terminal.id, "agent tool result cut to the result bound").is_empty());
}

// CVL-FR-FSAY: the last call under the bound carries the last-call line at
// the end of its exchange, once, and with the same tools; no earlier call
// carries it.
#[test]
fn the_last_call_is_told_it_is_the_last() {
    let mut script: Vec<Result<ScriptedReply, &'static str>> =
        (0..MAX_MODEL_CALLS - 1).map(|_| Ok(reads("spec.md"))).collect();
    script.push(Ok(ScriptedReply::answer("Concluded.")));
    let h = sized_harness(script, Some(200_000), None);
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), MAX_MODEL_CALLS);
    let is_line =
        |m: &rig::completion::Message| *m == rig::completion::Message::user(LAST_CALL_LINE);
    let last = &exchanges[MAX_MODEL_CALLS - 1];
    assert!(is_line(last.last().unwrap()));
    assert_eq!(last.iter().filter(|m| is_line(m)).count(), 1);
    for exchange in &exchanges[..MAX_MODEL_CALLS - 1] {
        assert!(!exchange.iter().any(is_line));
    }
    let requests = h.seam.requests();
    let names = |i: usize| requests[i].0.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(MAX_MODEL_CALLS - 1), names(0));
}

// CVL-FR-FSAY: a turn that answers before the bound never carries the line.
#[test]
fn a_short_turn_never_carries_the_line() {
    let h = sized_harness(vec![Ok(ScriptedReply::answer("Done."))], Some(200_000), None);
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let line = rig::completion::Message::user(LAST_CALL_LINE);
    assert!(!h.seam.exchanges()[0].contains(&line));
}

// CVL-FR-ZFMU, CVL-FR-QJLU, CVL-FR-LZDN: a context overflow is not repeated.
// The loop compacts, cuts the latest result to the minimum bound, and makes
// the call one more time, and one record says so.
#[test]
fn an_overflow_compacts_cuts_and_calls_once_more() {
    let h = sized_harness(
        vec![
            Ok(reads("big.md")),
            Ok(reads("big.md")),
            Err(SCRIPTED_STRUCTURED),
            Ok(ScriptedReply::unspoken("SUMMARY-QJLU")),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(200_000),
        None,
    );
    h.seam.push_structured(refusal(400, TOO_LONG));
    big_file(&h, "big.md", 100_000, "MARK-QJLU");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    // Two rounds, the refused call once, the compaction call, the recovery.
    assert_eq!(h.seam.call_count(), 5);
    let requests = h.seam.requests();
    assert_eq!(
        requests[3].0.instructions,
        crate::prompts::instruction_text(
            crate::agent_conversations::compaction::COMPACT_PROMPT_TEMPLATE
        )
    );
    let last = h.seam.exchanges().pop().unwrap();
    assert_eq!(last.len(), 3, "the compacted input, then the latest round");
    let results = tool_results(&last);
    assert!(results[0].starts_with("MARK-QJLU"));
    assert!(results[0]
        .ends_with(" of 100000 characters. Read the rest in parts with offset and limit.]"));
    // The minimum bound of a 200,000-token window: 16,000 characters.
    assert!(results[0].chars().count() <= 16_000);
    assert!(results[0].chars().count() > 15_800);
    assert!(section_of(&requests[4].0.input, TAG_DISCUSSION_HISTORY).body.contains("SUMMARY-QJLU"));

    let recoveries = records_of(&terminal.id, RECOVERY);
    assert_eq!(recoveries.len(), 1);
    assert_eq!(recoveries[0].level, logging::LogLevel::Warn);
    assert_eq!(field(&recoveries[0], "budgetTokens"), "200000");
    assert_eq!(field(&recoveries[0], "compacted"), "true");
    assert_eq!(field(&recoveries[0], "resultsCut"), "1");
    assert_eq!(field(&recoveries[0], "changed"), "true");
    for record in all_records() {
        let text = format!("{record:?}");
        assert!(!text.contains("MARK-QJLU") && !text.contains("SUMMARY-QJLU"));
    }
}

// CVL-FR-LQDM: an overflow with nothing older and no result to cut changes
// nothing, so the loop makes no further call and the turn fails.
#[test]
fn an_overflow_with_nothing_to_change_fails_at_once() {
    let h = sized_harness(
        vec![Err(SCRIPTED_STRUCTURED), Ok(ScriptedReply::answer("never"))],
        Some(200_000),
        None,
    );
    h.seam.push_structured(refusal(400, TOO_LONG));
    let (_, thread) = seed_discussion(&h, &[]);
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    // The class of the overflow refusal itself.
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_INVALID_REQUEST));
    assert_eq!(h.seam.call_count(), 1, "no repeat, no compaction call, no recovery call");
    let recovery = wait_for_record(&terminal.id, RECOVERY);
    assert_eq!(field(&recovery, "changed"), "false");
}

// CVL-FR-LQDM: a turn recovers at most once; a second overflow fails it.
#[test]
fn a_second_overflow_fails_the_turn() {
    let h = sized_harness(
        vec![
            Ok(reads("big.md")),
            Ok(reads("big.md")),
            Err(SCRIPTED_STRUCTURED),
            Ok(ScriptedReply::unspoken("SUMMARY")),
            Err(SCRIPTED_STRUCTURED),
            Ok(ScriptedReply::answer("never")),
        ],
        Some(200_000),
        None,
    );
    h.seam.push_structured(refusal(400, TOO_LONG));
    h.seam.push_structured(refusal(400, TOO_LONG));
    big_file(&h, "big.md", 100_000, "MARK-TWICE");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_INVALID_REQUEST));
    assert_eq!(h.seam.call_count(), 5, "the spare reply is never used");
    assert_eq!(records_of(&terminal.id, RECOVERY,).len(), 1);
}

// CVL-FR-PFNV: a 400 without an overflow marker is no overflow: it is
// repeated on the usual terms and no recovery starts.
#[test]
fn another_refusal_does_not_recover() {
    let h = sized_harness(
        vec![Err(SCRIPTED_STRUCTURED), Err(SCRIPTED_STRUCTURED), Err(SCRIPTED_STRUCTURED)],
        Some(200_000),
        None,
    );
    for _ in 0..3 {
        h.seam.push_structured(repeatable_refusal(400, "Invalid value for 'messages[2].content'"));
    }
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(h.seam.call_count(), 3);
    assert!(records_of(&terminal.id, RECOVERY,).is_empty());
    assert!(records_of(&terminal.id, "conversation history compaction").is_empty());
}

// AGC-FR-NXPQ, AGC-FR-11: the question of Meta is bound like every other
// section.
#[test]
fn the_meta_question_section_is_bound() {
    let at = crate::agent_conversations::advisors::meta_question_section(&"m".repeat(40_000));
    assert!(!at.truncated);
    let above = crate::agent_conversations::advisors::meta_question_section(&"m".repeat(40_001));
    assert!(above.truncated);
    assert!(above.body.ends_with("[This material is partial: it was truncated here.]"));
}

// CVL-FR-FSAY: the line is this fixed text.
#[test]
fn the_last_call_line_is_fixed() {
    assert_eq!(
        LAST_CALL_LINE,
        "This is your last model call in this turn. No tool call will run. Give your answer now with what you have."
    );
}

// AGC-FR-NXPQ, AGC-FR-11: the group sections are bound like every other
// section.
#[test]
fn the_group_sections_are_bound() {
    // AGC-FR-GZSI: `candidates` is the section of the driver classifier.
    for tag in ["synthesis", "group_questions", "group_proposals", "candidates"] {
        let at = crate::agent_conversations::groups::group_section(tag, "g".repeat(40_000));
        assert!(!at.truncated, "{tag}");
        let above = crate::agent_conversations::groups::group_section(tag, "g".repeat(40_001));
        assert!(above.truncated, "{tag}");
        assert!(above.body.ends_with("[This material is partial: it was truncated here.]"));
    }
}

/// A turn on a draft discussion with `n` earlier comments of 2,000
/// characters, under a window of 1,000,000 tokens and this history percent.
fn history_turn(percent: i64) -> (Harness, InputSection) {
    let h = sized_harness(vec![Ok(ScriptedReply::answer("Done."))], Some(1_000_000), None);
    h.store().save_history_limit_percent(percent).unwrap();
    let mut earlier: Vec<String> = vec!["OLDEST-COMMENT".to_string()];
    earlier.extend((0..50).map(|i| format!("c{i:02} {}", "h".repeat(2_000))));
    earlier.push("@arch what now?".to_string());
    let refs: Vec<&str> = earlier.iter().map(String::as_str).collect();
    let (_, thread) = seed_discussion(&h, &refs);
    let trigger = thread.comments.last().unwrap().id.clone();
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &trigger).unwrap();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let history = section_of(&h.seam.requests()[0].0.input, TAG_DISCUSSION_HISTORY).clone();
    (h, history)
}

// AGC-FR-CLRX, AGC-FR-HOND: a turn bounds its history by the window and the
// History size limit: a large share holds a history of about 100,000 characters whole,
// and a small one keeps the newest comments within 40,000 characters.
#[test]
fn a_turn_bounds_its_history_by_the_window_and_the_setting() {
    let (_h, whole) = history_turn(20);
    assert!(!whole.truncated);
    assert!(whole.body.contains("OLDEST-COMMENT"));
    // Far above the floor of 40,000 characters, and whole.
    assert!(whole.body.chars().count() > 90_000, "{}", whole.body.chars().count());
    assert!(whole.body.contains("c49 "));

    // 1 % of 1,000,000 tokens is 40,000 characters, the floor.
    let (_h, cut) = history_turn(1);
    assert!(cut.truncated);
    assert!(cut.body.starts_with("[Earlier comments omitted:"));
    assert!(!cut.body.contains("OLDEST-COMMENT"));
    assert!(cut.body.contains("c49 "));
    assert!(cut.body.chars().count() <= 40_000);
}

const PARTIAL: &str = "[This material is partial: it was truncated here.]";

// AGC-FR-GZSI: the sections of the translation request and of the progress
// update request are bound by AGC-FR-11.
#[test]
fn the_voice_requests_bound_their_sections() {
    let long = "t".repeat(40_001);
    let translation =
        crate::agent_conversations::translation_request("English", "German", &[], &long);
    let transcript = section_of(&translation.input, "transcript");
    assert!(transcript.truncated);
    assert!(transcript.body.ends_with(PARTIAL));
    let short = crate::agent_conversations::translation_request("English", "German", &[], "hi");
    assert!(!section_of(&short.input, "transcript").truncated);

    let progress = crate::agent_conversations::progress_request(&[], &long);
    let language = section_of(&progress.input, "language");
    assert!(language.truncated);
    assert!(language.body.ends_with(PARTIAL));
}

// AGC-FR-GZSI: a speech chunk is bound by AGC-FR-11.
#[test]
fn a_speech_chunk_is_bound() {
    let text_of = |m: rig::completion::Message| match m {
        rig::completion::Message::User { content } => content
            .iter()
            .filter_map(|c| match c {
                rig::completion::message::UserContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<String>(),
        _ => panic!("a user message"),
    };
    let long = text_of(crate::agent_conversations::steering::chunk_message(&"s".repeat(40_001)));
    let preface = format!("{}\n\n", crate::agent_conversations::steering::PARTIAL_SPEECH_LINE);
    let body = long.strip_prefix(&preface).expect("the fixed line opens the chunk");
    assert_eq!(body, format!("{}\n\n{PARTIAL}", "s".repeat(40_000)));
    let short = text_of(crate::agent_conversations::steering::chunk_message("a few words"));
    assert!(short.ends_with("a few words"));
}

// AGC-FR-KLID: the summary that replaces the history is bound by the history
// bound.
#[test]
fn the_summary_is_bound_by_the_history_bound() {
    // A bound that is not the section bound, so the test sees which one cuts.
    let request = AgentRequest::default();
    let compacted = crate::agent_conversations::compaction::compacted_request(
        &request,
        &"y".repeat(100_000),
        80_000,
    );
    let history = section_of(&compacted.input, TAG_DISCUSSION_HISTORY);
    assert!(history.truncated);
    assert!(history.body.ends_with(PARTIAL));
    assert_eq!(history.body.chars().count(), 80_000 + 2 + PARTIAL.chars().count());
    let whole = crate::agent_conversations::compaction::compacted_request(
        &request,
        &"y".repeat(50_000),
        80_000,
    );
    assert!(!section_of(&whole.input, TAG_DISCUSSION_HISTORY).truncated);
}

/// A turn that compacts once, with a summary of 100,000 characters, under a
/// 200,000-token window and this history percent. Returns the history the call
/// after the compaction carried.
fn summary_turn(percent: i64) -> InputSection {
    let summary = format!("SUMMARY-START{}SUMMARY-END", "s".repeat(100_000));
    let h = sized_harness(
        vec![
            Ok(super::compaction::big_tool_reply(400_000)),
            Ok(super::compaction::big_tool_reply(400_001)),
            Ok(ScriptedReply::unspoken(summary)),
            Ok(ScriptedReply::answer("Final.")),
        ],
        Some(200_000),
        None,
    );
    h.store().save_history_limit_percent(percent).unwrap();
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let requests = h.seam.requests();
    assert_eq!(requests.len(), 4, "two rounds, the compaction call, the last call");
    section_of(&requests[3].0.input, TAG_DISCUSSION_HISTORY).clone()
}

// AGC-FR-KLID: a turn bounds the summary by its history bound: 20 % of a
// 200,000-token window (160,000 characters) holds it whole, and 1 % (the
// floor of 40,000 characters) cuts it.
#[test]
fn a_turn_bounds_the_summary_by_its_history_bound() {
    let whole = summary_turn(20);
    assert!(!whole.truncated);
    assert!(whole.body.contains("SUMMARY-END"));
    let cut = summary_turn(1);
    assert!(cut.truncated);
    assert!(cut.body.contains("SUMMARY-START") && !cut.body.contains("SUMMARY-END"));
    assert!(cut.body.ends_with(PARTIAL));
}

// AGC-FR-GZSI: the request of the driver classifier bounds its candidates
// section, also when many agents with long instructions take part.
#[test]
fn the_classifier_request_bounds_its_candidates() {
    let agents: Vec<crate::agents::Agent> = (0..25)
        .map(|i| crate::agents::Agent {
            nickname: format!("agent{i:02}"),
            instructions: "i".repeat(2_000),
            ..Default::default()
        })
        .collect();
    let request = crate::agent_conversations::groups::classifier_request(Vec::new(), &agents);
    let candidates = section_of(&request.input, "candidates");
    assert!(candidates.truncated);
    assert!(candidates.body.ends_with(PARTIAL));
    let few = crate::agent_conversations::groups::classifier_request(Vec::new(), &agents[..2]);
    assert!(!section_of(&few.input, "candidates").truncated);
}
