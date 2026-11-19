//! What a replay does to the rounds of an entry (`ERP-exchange-replay.md`
//! ERP-FR-QZNV, ERP-FR-RVYX, ERP-FR-GVHU, ERP-FR-UGII, ERP-FR-LXFF,
//! ERP-FR-XJGI). Every function here is pure.

use super::super::*;
use rig::completion::message::{AssistantContent, ToolResultContent, UserContent};
use rig::completion::Message;
use std::collections::{BTreeSet, HashMap};

/// ERP-FR-GVHU: the text of a result that a replay omits to save room.
pub(in crate::agent_conversations) const RESULT_OMITTED_LINE: &str =
    "[Result omitted to save room. Read it again if you need it.]";

/// ERP-FR-QZNV: the result of a call that a contribution left undispatched.
pub(in crate::agent_conversations) fn not_run_line(tool: &str) -> String {
    format!("Not run: the turn ended on {tool}.")
}

/// The calls of an assistant message, where it is one.
fn calls_of(message: &Message) -> Vec<&rig::completion::message::ToolCall> {
    match message {
        Message::Assistant(assistant) => assistant
            .content
            .iter()
            .filter_map(|c| match c {
                AssistantContent::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// ERP-FR-RVYX: replace the text of the result of the call `call_id` with
/// `text`. Returns whether a result was found.
pub(in crate::agent_conversations) fn replace_result(
    rounds: &mut [Message],
    call_id: &str,
    text: &str,
) -> bool {
    let mut found = false;
    for message in rounds.iter_mut() {
        let Message::User { content } = message else { continue };
        for part in content.iter_mut() {
            if let UserContent::ToolResult(result) = part {
                if result.call.wire() == call_id {
                    result.content = vec![ToolResultContent::text(text.to_string())];
                    found = true;
                }
            }
        }
    }
    found
}

/// ERP-FR-GVHU: replace every result of `rounds` with the omitted line.
/// Returns how many results changed.
pub(in crate::agent_conversations) fn omit_results(rounds: &mut [Message]) -> usize {
    let mut changed = 0;
    for message in rounds.iter_mut() {
        let Message::User { content } = message else { continue };
        for part in content.iter_mut() {
            let UserContent::ToolResult(result) = part else { continue };
            let already = matches!(result.content.as_slice(),
                [ToolResultContent::Text(t)] if t.text == RESULT_OMITTED_LINE);
            if !already {
                result.content = vec![ToolResultContent::text(RESULT_OMITTED_LINE.to_string())];
                changed += 1;
            }
        }
    }
    changed
}

/// ERP-FR-UGII: the rounds with every round that calls a tool not in `offered`
/// replayed as text: its reply as one assistant message, and its results as
/// one user message.
fn with_text_rounds(rounds: &[Message], offered: &BTreeSet<String>) -> Vec<Message> {
    let mut out = Vec::with_capacity(rounds.len());
    let mut texted: BTreeSet<String> = BTreeSet::new();
    let mut pending: Vec<String> = Vec::new();
    let flush = |out: &mut Vec<Message>, pending: &mut Vec<String>| {
        if !pending.is_empty() {
            out.push(Message::user(pending.join("\n\n")));
            pending.clear();
        }
    };
    for message in rounds {
        let calls = calls_of(message);
        if !calls.is_empty() {
            flush(&mut out, &mut pending);
            if calls.iter().any(|call| !offered.contains(call.function.name.as_str())) {
                texted.extend(calls.iter().map(|call| call.id.wire().into_owned()));
                out.push(Message::assistant(compaction::message_text(message)));
                continue;
            }
        }
        let is_texted_result = matches!(message, Message::User { content }
            if !content.is_empty() && content.iter().all(|part| matches!(part,
                UserContent::ToolResult(r) if texted.contains(r.call.wire().as_ref()))));
        if is_texted_result {
            pending.push(compaction::message_text(message));
            continue;
        }
        flush(&mut out, &mut pending);
        out.push(message.clone());
    }
    flush(&mut out, &mut pending);
    out
}

/// ERP-FR-XJGI: the id of call `n` of entry `seq`: 9 base-36 characters, the
/// same on every turn.
pub(in crate::agent_conversations) fn derived_call_id(seq: u64, n: usize) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{seq}:{n}").as_bytes());
    let mut value = u64::from_le_bytes(digest[..8].try_into().unwrap_or_default());
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut id = String::with_capacity(9);
    for _ in 0..9 {
        id.push(DIGITS[(value % 36) as usize] as char);
        value /= 36;
    }
    id
}

/// ERP-FR-GAXX: the rounds with a derived id for each call and its result, and
/// the content as stored.
fn renamed_rounds(rounds: &[Message], seq: u64) -> Vec<Message> {
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut next = 0usize;
    let mut out = rounds.to_vec();
    for message in out.iter_mut() {
        match message {
            Message::Assistant(assistant) => {
                for block in assistant.content.iter_mut() {
                    if let AssistantContent::ToolCall(call) = block {
                        let fresh = derived_call_id(seq, next);
                        next += 1;
                        ids.insert(call.id.wire().into_owned(), fresh.clone());
                        call.id = rig::completion::message::CallId::from_wire(fresh);
                    }
                }
            }
            Message::User { content } => {
                for part in content.iter_mut() {
                    if let UserContent::ToolResult(result) = part {
                        if let Some(fresh) = ids.get(result.call.wire().as_ref()) {
                            result.call =
                                rig::completion::message::CallId::from_wire(fresh.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The wire ids of the calls of `rounds`.
pub(in crate::agent_conversations) fn call_ids(rounds: &[Message]) -> Vec<String> {
    rounds.iter().flat_map(calls_of).map(|call| call.id.wire().into_owned()).collect()
}

/// ERP-FR-XJGI: the rounds for another provider or model: no reasoning, no
/// provider-native item, and a derived id for each call and its result.
fn canonical_rounds(rounds: &[Message], seq: u64) -> Vec<Message> {
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut next = 0usize;
    let mut out = Vec::with_capacity(rounds.len());
    for message in rounds {
        match message {
            Message::Assistant(assistant) => {
                let mut copy = assistant.clone();
                copy.origin = None;
                copy.content = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        AssistantContent::Text(_) => Some(block.canonical()),
                        AssistantContent::ToolCall(call) => {
                            let fresh = derived_call_id(seq, next);
                            next += 1;
                            ids.insert(call.id.wire().into_owned(), fresh.clone());
                            let mut call = call.clone();
                            call.native = None;
                            call.id = rig::completion::message::CallId::from_wire(fresh);
                            Some(AssistantContent::ToolCall(call))
                        }
                        _ => None,
                    })
                    .collect();
                if !copy.content.is_empty() {
                    out.push(Message::Assistant(copy));
                }
            }
            Message::User { content } => {
                let content = content
                    .iter()
                    .map(|part| match part {
                        UserContent::ToolResult(result) => {
                            let mut result = result.clone();
                            if let Some(fresh) = ids.get(result.call.wire().as_ref()) {
                                result.call =
                                    rig::completion::message::CallId::from_wire(fresh.clone());
                            }
                            UserContent::ToolResult(result)
                        }
                        other => other.clone(),
                    })
                    .collect();
                out.push(Message::User { content });
            }
            other => out.push(other.clone()),
        }
    }
    out
}

/// How a replay treats the ids and the provider items of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::agent_conversations) enum Identity {
    /// ERP-FR-LXFF: as stored.
    Stored,
    /// ERP-FR-GAXX: derived ids, the content as stored.
    Renamed,
    /// ERP-FR-XJGI: derived ids, and no reasoning or provider item.
    Canonical,
}

/// ERP-FR-UGII, ERP-FR-LXFF, ERP-FR-XJGI, ERP-FR-GAXX: the rounds of an entry
/// as the current turn replays them.
pub(in crate::agent_conversations) fn rounds_for_turn(
    rounds: &[Message],
    offered: &BTreeSet<String>,
    identity: Identity,
    seq: u64,
) -> Vec<Message> {
    let texted = with_text_rounds(rounds, offered);
    match identity {
        Identity::Stored => texted,
        Identity::Renamed => renamed_rounds(&texted, seq),
        Identity::Canonical => canonical_rounds(&texted, seq),
    }
}

/// ERP-FR-QZNV: the result messages of a reply that ended the turn on the
/// call at `ending`: its real result, and the not-run line for every other
/// call.
pub(in crate::agent_conversations) fn ending_results(
    calls: &[rig::completion::message::ToolCall],
    ending: usize,
    result: &str,
) -> Vec<Message> {
    let tool = calls.get(ending).map_or("", |call| call.function.name.as_str()).to_string();
    calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            let text = if index == ending { result.to_string() } else { not_run_line(&tool) };
            Message::tool_result(call.id.clone(), call.function.name.clone(), text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::message::{AssistantMessage, CallId, ToolCall, ToolFunction};

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall::from_wire(
            id,
            ToolFunction::new(
                rig::completion::message::ToolName::new(name).expect("a tool name"),
                serde_json::json!({ "path": "a" }),
            ),
        )
    }

    fn round(id: &str, name: &str, result: &str) -> Vec<Message> {
        let c = call(id, name);
        vec![
            Message::Assistant(AssistantMessage::new(vec![
                AssistantContent::text("Looking."),
                AssistantContent::ToolCall(c.clone()),
            ])),
            Message::tool_result(c.id.clone(), c.function.name.clone(), result),
        ]
    }

    fn result_texts(rounds: &[Message]) -> Vec<String> {
        rounds
            .iter()
            .flat_map(|m| match m {
                Message::User { content } => content
                    .iter()
                    .filter_map(|p| match p {
                        UserContent::ToolResult(r) => {
                            Some(compaction::message_text(&Message::User {
                                content: vec![UserContent::ToolResult(r.clone())],
                            }))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect()
    }

    fn offered(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    // ERP-FR-RVYX: one result is replaced by its call id; ERP-FR-GVHU: every
    // result is replaced by the omitted line, once.
    #[test]
    fn results_are_replaced_and_omitted() {
        let mut rounds = round("c1", "read_file", "TEXT");
        rounds.extend(round("c2", "read_file", "MORE"));
        assert!(replace_result(&mut rounds, "c1", "[Outdated: a]"));
        assert!(!replace_result(&mut rounds, "c9", "x"));
        let texts = result_texts(&rounds);
        assert!(texts[0].ends_with("[Outdated: a]") && texts[1].ends_with("MORE"), "{texts:?}");
        assert_eq!(omit_results(&mut rounds), 2);
        assert_eq!(omit_results(&mut rounds), 0, "already omitted");
        assert!(result_texts(&rounds).iter().all(|t| t.ends_with(RESULT_OMITTED_LINE)));
    }

    // ERP-FR-UGII: a round that calls a tool the turn is not offered becomes
    // one assistant text and one user text; other rounds stay native.
    #[test]
    fn a_round_of_a_tool_not_offered_becomes_text() {
        let mut rounds = round("c1", "list_plan_issues", "ISSUES");
        rounds.extend(round("c2", "read_file", "FILE"));
        let out = rounds_for_turn(&rounds, &offered(&["read_file"]), Identity::Stored, 1);
        assert_eq!(out.len(), 4);
        assert!(calls_of(&out[0]).is_empty(), "the first round is text");
        let Message::Assistant(text) = &out[0] else { panic!("assistant") };
        assert!(
            matches!(&text.content[0], AssistantContent::Text(t) if t.text.contains("list_plan_issues"))
        );
        assert!(matches!(&out[1], Message::User { content }
            if matches!(&content[0], UserContent::Text(t) if t.text.contains("ISSUES"))));
        assert_eq!(calls_of(&out[2]).len(), 1, "the second round stays native");
        assert_eq!(out[3], rounds[3]);
    }

    // ERP-FR-LXFF: under the same provider and model, the rounds stay as
    // stored. ERP-FR-XJGI: under another, reasoning goes and ids are derived,
    // the same on every turn.
    #[test]
    fn another_model_gets_canonical_rounds_with_derived_ids() {
        let mut rounds = round("c1", "read_file", "FILE");
        if let Message::Assistant(a) = &mut rounds[0] {
            a.content.insert(
                0,
                AssistantContent::Reasoning(rig::completion::message::Reasoning::new("think")),
            );
        }
        let all = offered(&["read_file"]);
        assert_eq!(rounds_for_turn(&rounds, &all, Identity::Stored, 7), rounds);
        let out = rounds_for_turn(&rounds, &all, Identity::Canonical, 7);
        assert_eq!(out, rounds_for_turn(&rounds, &all, Identity::Canonical, 7), "stable");
        let Message::Assistant(a) = &out[0] else { panic!("assistant") };
        assert!(!a.content.iter().any(|c| matches!(c, AssistantContent::Reasoning(_))));
        let id = calls_of(&out[0])[0].id.wire().into_owned();
        assert_eq!(id, derived_call_id(7, 0));
        assert_eq!(id.len(), 9);
        assert!(id.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        let Message::User { content } = &out[1] else { panic!("user") };
        let UserContent::ToolResult(r) = &content[0] else { panic!("result") };
        assert_eq!(r.call.wire(), id.as_str(), "the result follows its call");
        assert_ne!(derived_call_id(7, 0), derived_call_id(7, 1));
        assert_ne!(derived_call_id(7, 0), derived_call_id(8, 0));
    }

    // ERP-FR-GAXX: renamed rounds keep their reasoning and get derived ids
    // that their results follow.
    #[test]
    fn renamed_rounds_keep_their_content() {
        let mut rounds = round("call_0", "read_file", "FILE");
        if let Message::Assistant(a) = &mut rounds[0] {
            a.content.insert(
                0,
                AssistantContent::Reasoning(rig::completion::message::Reasoning::new("think")),
            );
        }
        let out = rounds_for_turn(&rounds, &offered(&["read_file"]), Identity::Renamed, 3);
        let Message::Assistant(a) = &out[0] else { panic!("assistant") };
        assert!(a.content.iter().any(|c| matches!(c, AssistantContent::Reasoning(_))));
        assert_eq!(call_ids(&out), vec![derived_call_id(3, 0)]);
        let Message::User { content } = &out[1] else { panic!("user") };
        let UserContent::ToolResult(r) = &content[0] else { panic!("result") };
        assert_eq!(r.call.wire(), derived_call_id(3, 0).as_str());
        assert_eq!(call_ids(&rounds), vec!["call_0".to_string()]);
    }

    // ERP-FR-QZNV: the ending call keeps its result, and every other call gets
    // the not-run line naming the ending tool.
    #[test]
    fn the_ending_call_keeps_its_result() {
        let calls = vec![call("c1", "read_file"), call("c2", "ask_user_comment")];
        let out = ending_results(&calls, 1, "posted");
        let texts = result_texts(&out);
        assert!(texts[0].ends_with("Not run: the turn ended on ask_user_comment."), "{texts:?}");
        assert!(texts[1].ends_with("posted"), "{texts:?}");
        let Message::User { content } = &out[0] else { panic!("user") };
        let UserContent::ToolResult(r) = &content[0] else { panic!("result") };
        assert_eq!(r.call, CallId::from_wire("c1"));
    }
}
