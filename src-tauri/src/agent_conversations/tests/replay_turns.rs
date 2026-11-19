//! A later turn replays the agent's own earlier exchange
//! (`ERP-exchange-replay.md`), end to end through the scripted seam.

use super::*;
use crate::agent_conversations::replay::ReplayMemory;
use crate::comments::{Stamp, ThreadRef};

const SOURCE: &str = "Some artifact source here.";

/// A harness with a replay memory, an agent, and a notes file to read.
fn harness(replies: Vec<Result<ScriptedReply, &'static str>>) -> Harness {
    let h = Harness::scripted(replies);
    h.app.manage(ReplayMemory::default());
    h.mount();
    h.create_agent("arch", "");
    std::fs::write(h.root().path().join("notes.md"), "NOTES-V1").expect("notes");
    h
}

/// Turn 1 on a new artifact thread; returns the thread and the terminal turn.
fn first_turn(h: &Harness) -> (Discussion, AgentTurn) {
    let thread = h.seed_artifact_thread("spec.md", SOURCE);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let terminal = wait_for_terminal(h, &turn.id);
    h.join_turns();
    (thread, terminal)
}

/// A human comment on `thread` and the turn it dispatches.
fn next_turn(h: &Harness, thread: &Discussion, body: &str, _minute: u32) -> AgentTurn {
    let after = crate::comments::add_comment_to(
        &h.root(),
        ThreadRef::artifact("spec.md"),
        &thread.id,
        body.to_string(),
        Vec::new(),
        Vec::new(),
        // The real time, as an agent answer is stamped, so the thread holds
        // the comments in the order the test wrote them.
        Stamp { by: &human("ada"), at: &now_rfc3339() },
    )
    .expect("comment");
    let trigger = after.comments.last().expect("the comment").id.clone();
    let turn = h.dispatch("arch", ConversationOrigin::of(&after), &trigger).expect("dispatch");
    let terminal = wait_for_terminal(h, &turn.id);
    h.join_turns();
    terminal
}

fn text_of(message: &rig::completion::Message) -> String {
    crate::agent_conversations::compaction::message_text(message)
}

/// The two-turn script: turn 1 reads a file and answers; turn 2 answers.
fn two_turns() -> Vec<Result<ScriptedReply, &'static str>> {
    vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))),
        Ok(ScriptedReply::answer("FIRST-ANSWER")),
        Ok(ScriptedReply::answer("SECOND-ANSWER")),
    ]
}

// ERP-FR-FEAD, ERP-FR-OPRO, ERP-FR-ISTB, ERP-FR-LKUP, ERP-FR-LXFF, CVL-FR-12:
// turn 2 opens with the opening of turn 1, its read and its result as native
// messages, and its answer, then an input with no material and only the new
// comment.
#[test]
fn a_later_turn_replays_the_earlier_read_natively() {
    let h = harness(two_turns());
    let (thread, first) = first_turn(&h);
    assert_eq!(first.state, AgentTurnState::Delivered);
    let second = next_turn(&h, &thread, "@arch and now?", 5);
    assert_eq!(second.state, AgentTurnState::Delivered);

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 3);
    let turn2 = &exchanges[2];
    let results = tool_results(turn2);
    assert_eq!(results.len(), 1, "the result of turn 1 is replayed: {results:?}");
    assert!(results[0].contains("NOTES-V1"), "{results:?}");
    assert!(
        turn2.iter().any(|m| matches!(m, rig::completion::Message::Assistant(a)
            if a.content.iter().any(|c| matches!(c, rig::completion::message::AssistantContent::ToolCall(_))))),
        "the call is a native call"
    );
    let input = text_of(turn2.last().expect("the input"));
    assert!(input.contains("and now?"), "{input}");
    assert!(!input.contains(SOURCE), "the material is in the first opening: {input}");
    assert!(!input.contains("FIRST-ANSWER"), "the answer is an assistant message: {input}");
    let answer = text_of(&turn2[turn2.len() - 2]);
    assert!(answer.contains("FIRST-ANSWER"), "{answer}");
    assert!(text_of(&turn2[0]).contains(SOURCE), "the first opening holds the material");
}

// ERP-FR-IVGL, CVL-FR-23: the last request of turn 1 is a byte-for-byte
// prefix of the first request of turn 2.
#[test]
fn the_last_request_of_a_turn_is_a_prefix_of_the_next() {
    let h = harness(two_turns());
    let (thread, _) = first_turn(&h);
    next_turn(&h, &thread, "@arch and now?", 5);
    let exchanges = h.seam.exchanges();
    let last_of_first = &exchanges[1];
    let first_of_second = &exchanges[2];
    assert!(first_of_second.len() > last_of_first.len());
    assert_eq!(&first_of_second[..last_of_first.len()], last_of_first.as_slice());
    let requests = h.seam.requests();
    assert_eq!(requests[1].0.instructions, requests[2].0.instructions);
    assert_eq!(requests[1].0.tools, requests[2].0.tools);
}

// ERP-FR-NKLF, ERP-FR-RVYX: a file that changed between the turns gives the
// outdated line in place of the old text.
#[test]
fn a_changed_file_gives_the_outdated_line() {
    let h = harness(two_turns());
    let (thread, _) = first_turn(&h);
    std::fs::write(h.root().path().join("notes.md"), "NOTES-V2").expect("edit");
    next_turn(&h, &thread, "@arch and now?", 5);
    let results = tool_results(&h.seam.exchanges()[2]);
    assert_eq!(results.len(), 1);
    assert!(
        results[0].contains("[Outdated: notes.md changed after this result was read."),
        "{results:?}"
    );
    assert!(!results[0].contains("NOTES-V1"), "{results:?}");
}

// ERP-FR-CRJL: a turn that fails is not captured, so the next turn opens
// with its input alone.
#[test]
fn a_failed_turn_is_not_replayed() {
    let h = harness(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))),
        Err(FAIL_REJECTED),
        Ok(ScriptedReply::answer("SECOND-ANSWER")),
    ]);
    let (thread, first) = first_turn(&h);
    assert_eq!(first.state, AgentTurnState::Failed);
    next_turn(&h, &thread, "@arch and now?", 5);
    let last = h.seam.exchanges().pop().expect("turn 2");
    assert_eq!(last.len(), 1, "no replay: the input alone");
    assert!(tool_results(&last).is_empty());
}

// ERP-FR-XTQI, CVL-FR-AAPA: the context figures of turn 2 count the replay as
// `replayed`, and the results of turn 2 alone as `tool_results`.
#[test]
fn the_figures_count_the_replay_apart() {
    let h = harness(two_turns());
    let (thread, first) = first_turn(&h);
    let first_split = first.context.as_ref().expect("figures").split;
    assert_eq!(first_split.replayed, 0);
    let second = next_turn(&h, &thread, "@arch and now?", 5);
    let split = second.context.as_ref().expect("figures").split;
    assert!(split.replayed > 0, "{split:?}");
    assert_eq!(split.tool_results, 0, "turn 2 read nothing itself: {split:?}");
    assert!(split.document > 0, "the material of the first opening: {split:?}");
}

// ERP-FR-GDLV, AGC-FR-JFBZ: the estimate selects the replay of its agent and
// counts it, and changes no entry.
#[test]
fn the_estimate_counts_the_replay() {
    let h = harness(two_turns());
    let (thread, first) = first_turn(&h);
    let estimate = |agent: Option<&str>| {
        estimate_impl(
            &h.app.handle().clone(),
            Roots::same(&h.root()),
            PROJECT_KEY,
            thread.target.clone(),
            Some(&thread.id),
            agent,
        )
    };
    let with_agent = estimate(Some(&first.agent_id));
    assert!(with_agent.split.replayed > 0, "{:?}", with_agent.split);
    assert_eq!(with_agent.estimated_tokens, with_agent.split.total());
    let memory = h.app.state::<ReplayMemory>();
    let key = crate::agent_conversations::replay::ReplayKey {
        discussion_id: thread.id.clone(),
        agent_id: first.agent_id.clone(),
    };
    let entries = memory.entries_of(&key);
    let start = memory.history_start(&key);
    let again = estimate(Some(&first.agent_id));
    assert_eq!(again.split, with_agent.split);
    assert_eq!(memory.entries_of(&key), entries, "the estimate changes no entry");
    assert_eq!(memory.history_start(&key), start);
    // AGC-FR-OBUA: an absent agent gives no replay.
    assert_eq!(estimate(None).split.replayed, 0);
}

// ERP-FR-GOXP, CVL-FR-MOTF: the turn emits its replay record and its prefix
// records, with counts and hashes and no part of the exchange.
#[test]
fn the_records_hold_counts_and_hashes_alone() {
    let h = harness(two_turns());
    let (thread, _) = first_turn(&h);
    let second = next_turn(&h, &thread, "@arch and now?", 5);
    let planned = wait_for_record(&second.id, "agent turn replay planned");
    assert_eq!(field(&planned, "selected"), "1");
    let prefix = wait_for_record(&second.id, "agent call prefix");
    assert_eq!(field(&prefix, "divergesFromPreviousTurnAt"), "none");
    let text = format!("{planned:?}{prefix:?}");
    for word in ["NOTES-V1", "FIRST-ANSWER", "and now?", SOURCE] {
        assert!(!text.contains(word), "a record holds the exchange word {word:?}");
    }
}

// ERP-FR-IVGL, ERP-FR-VMRY: over three turns, each last request is a prefix of
// the next first request, and turn 3 replays two blocks.
#[test]
fn three_turns_keep_the_prefix() {
    let h = harness(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))
            .with_call_ids("t1-")),
        Ok(ScriptedReply::answer("FIRST-ANSWER")),
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))
            .with_call_ids("t2-")),
        Ok(ScriptedReply::answer("SECOND-ANSWER")),
        Ok(ScriptedReply::answer("THIRD-ANSWER")),
    ]);
    let (thread, _) = first_turn(&h);
    next_turn(&h, &thread, "@arch and now?", 5);
    next_turn(&h, &thread, "@arch last one?", 9);
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 5);
    for (last, first) in [(1usize, 2usize), (3, 4)] {
        let prefix = &exchanges[last];
        assert_eq!(&exchanges[first][..prefix.len()], prefix.as_slice(), "{last} -> {first}");
    }
    let results = tool_results(&exchanges[4]);
    assert_eq!(results.len(), 2, "both reads are replayed: {results:?}");
    let input = text_of(exchanges[4].last().expect("input"));
    assert!(input.contains("last one?") && !input.contains("and now?"), "{input}");
}

// ERP-FR-FEAD, ERP-FR-QZNV: a turn that ends on a question is replayed with
// its ending call and result, and its question comment stays in the history.
#[test]
fn a_turn_that_asked_is_replayed_with_its_question() {
    let h = harness(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))
            .with_call_ids("t1-")),
        Ok(asks("WHICH-ONE?").with_call_ids("t1b-")),
        Ok(ScriptedReply::answer("SECOND-ANSWER")),
    ]);
    let (thread, first) = first_turn(&h);
    assert_eq!(first.state, AgentTurnState::AwaitingReply);
    next_turn(&h, &thread, "@arch the first one", 5);
    let turn2 = h.seam.exchanges().pop().expect("turn 2");
    let results = tool_results(&turn2);
    assert_eq!(results.len(), 2, "the read and the question: {results:?}");
    let ask_at = turn2
        .iter()
        .position(|m| text_of(m).contains(crate::tools::ask_user_comment::NAME))
        .expect("the ending call");
    assert!(ask_at < turn2.len() - 1);
    let input = text_of(turn2.last().expect("input"));
    assert!(input.contains("WHICH-ONE?"), "the question comment stays in the history: {input}");
    assert!(input.contains("the first one"), "{input}");
}

// ERP-FR-BXWA, ERP-FR-CMMA: another agent in the same discussion gets no
// replay, reads the first answer as a comment, and never the first result.
#[test]
fn another_agent_gets_no_replay() {
    let h = harness(two_turns());
    h.create_agent("eve", "");
    let (thread, _) = first_turn(&h);
    let after = crate::comments::add_comment_to(
        &h.root(),
        ThreadRef::artifact("spec.md"),
        &thread.id,
        "@eve your view?".into(),
        Vec::new(),
        Vec::new(),
        Stamp { by: &human("ada"), at: &now_rfc3339() },
    )
    .expect("comment");
    let trigger = after.comments.last().expect("comment").id.clone();
    let turn = h.dispatch("eve", ConversationOrigin::of(&after), &trigger).expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    h.join_turns();
    let eve = h.seam.exchanges().pop().expect("eve's turn");
    assert_eq!(eve.len(), 1, "the input alone");
    let input = text_of(&eve[0]);
    assert!(input.contains("FIRST-ANSWER"), "the answer is a comment: {input}");
    assert!(!input.contains("NOTES-V1"), "the result of another agent is not read: {input}");
}

// CVL-FR-GWTI, ERP-FR-FVVU: every prompt says that an earlier tool result
// shows the state at the time it was made, and the current sections the state
// now.
#[test]
fn every_prompt_says_how_old_an_earlier_result_is() {
    for template in [
        COMMENT_PROMPT_TEMPLATE,
        DISCUSS_ARTIFACT_PROMPT_TEMPLATE,
        DISCUSS_DRAFT_PROMPT_TEMPLATE,
        DISCUSS_NOTE_PROMPT_TEMPLATE,
        DISCUSS_MAP_PROMPT_TEMPLATE,
        DISCUSS_CHANGE_PROMPT_TEMPLATE,
    ] {
        assert!(
            template.contains(
                "Tool results from an earlier turn of this conversation show the state at the time they were made."
            ),
            "a prompt lacks the line"
        );
        assert!(template.contains("This section and the material sections show the state now."));
        assert!(template.contains("<discussion_state>"));
    }
}

// ERP-FR-KIZX: the tool list saved on a comment is no source of replay: with
// the memory empty, the next turn opens with its input alone.
#[test]
fn the_saved_tool_list_is_not_replayed() {
    let h = harness(two_turns());
    let (thread, first) = first_turn(&h);
    let saved = folded(&h, "spec.md", &thread.id);
    assert!(saved.comments.iter().any(|c| !c.tool_calls.is_empty()), "turn 1 saved its list");
    let memory = h.app.state::<ReplayMemory>();
    let key = crate::agent_conversations::replay::ReplayKey {
        discussion_id: thread.id.clone(),
        agent_id: first.agent_id.clone(),
    };
    for entry in memory.entries_of(&key) {
        memory.remove(&key, entry.seq);
    }
    next_turn(&h, &thread, "@arch and now?", 5);
    let turn2 = h.seam.exchanges().pop().expect("turn 2");
    assert_eq!(turn2.len(), 1, "the input alone");
    assert!(!text_of(&turn2[0]).contains("NOTES-V1"));
}

// ERP-FR-PBCM: a search that the provider ran for itself in turn 1 is in no
// replayed message.
#[test]
fn a_native_search_is_not_replayed() {
    let h = harness(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))
            .and_native("web_search", "{\"query\":\"q\"}", "NATIVE-RESULT")),
        Ok(ScriptedReply::answer("FIRST-ANSWER")),
        Ok(ScriptedReply::answer("SECOND-ANSWER")),
    ]);
    let (thread, _) = first_turn(&h);
    next_turn(&h, &thread, "@arch and now?", 5);
    let turn2 = h.seam.exchanges().pop().expect("turn 2");
    assert_eq!(tool_results(&turn2).len(), 1, "the read is replayed");
    assert!(!mentions_a_provider_entry(&turn2));
    assert!(turn2.iter().all(|m| !text_of(m).contains("NATIVE-RESULT")));
}

// ERP-FR-GWPG, CVL-FR-FSAY: the line of the last call is not replayed.
#[test]
fn the_last_call_line_is_not_replayed() {
    let mut replies: Vec<Result<ScriptedReply, &'static str>> = (0..7)
        .map(|i| {
            Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "notes.md" }))
                .with_call_ids(&format!("r{i}-")))
        })
        .collect();
    replies.push(Ok(ScriptedReply::answer("FIRST-ANSWER")));
    replies.push(Ok(ScriptedReply::answer("SECOND-ANSWER")));
    let h = harness(replies);
    let (thread, first) = first_turn(&h);
    assert_eq!(first.state, AgentTurnState::Delivered);
    let at_bound = h.seam.exchanges()[7].clone();
    assert!(at_bound.iter().any(|m| text_of(m).contains(LAST_CALL_LINE)), "turn 1 got the line");
    next_turn(&h, &thread, "@arch and now?", 5);
    let turn2 = h.seam.exchanges().pop().expect("turn 2");
    assert_eq!(tool_results(&turn2).len(), 7, "every read is replayed");
    assert!(turn2.iter().all(|m| !text_of(m).contains(LAST_CALL_LINE)));
}
