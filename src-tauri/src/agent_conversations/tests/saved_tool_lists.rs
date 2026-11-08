//! The tool list that a delivered answer saves with its comment (AGC-FR-WNQE,
//! AGC-FR-PXLV, AGC-FR-YBJC).

use super::*;

fn tool_call(seq: u64, state: ToolCallState, advisor: Option<&str>) -> TurnToolCall {
    TurnToolCall {
        id: format!("call-{seq}"),
        tool: format!("tool_{seq}"),
        activation_seq: seq,
        state,
        key_input: Some(format!("input {seq}")),
        advisor: advisor.map(str::to_string),
    }
}

// AGC-FR-WNQE: the list is in activation order, and a call that has not finished
// is saved as failed.
#[test]
fn a_saved_list_follows_activation_order_and_holds_no_running_call() {
    let saved = delivery::saved_tool_calls(&[
        tool_call(3, ToolCallState::Running, None),
        tool_call(1, ToolCallState::Done, None),
        tool_call(2, ToolCallState::Failed, None),
    ]);
    let tools: Vec<&str> = saved.iter().map(|c| c.tool.as_str()).collect();
    assert_eq!(tools, ["tool_1", "tool_2", "tool_3"]);
    assert_eq!(saved[0].state, comments::SavedToolState::Done);
    assert_eq!(saved[1].state, comments::SavedToolState::Failed);
    assert_eq!(saved[2].state, comments::SavedToolState::Failed);
    assert_eq!(saved[0].key_input.as_deref(), Some("input 1"));
}

// AGC-FR-PXLV: an advisor's call keeps the advisor's name, and a call of the
// answering agent has none.
#[test]
fn a_saved_list_keeps_the_advisor_name_of_an_advisor_call() {
    let saved = delivery::saved_tool_calls(&[
        tool_call(1, ToolCallState::Done, Some("sec")),
        tool_call(2, ToolCallState::Done, None),
    ]);
    assert_eq!(saved[0].advisor.as_deref(), Some("sec"));
    assert_eq!(saved[1].advisor, None);
}

// AGC-FR-WNQE: a delivered answer carries the calls of its turn.
#[test]
fn a_delivered_answer_carries_the_tool_list_of_its_turn() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))),
        Ok(ScriptedReply::answer("I read it.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let folded = folded(&h, "spec.md", &thread.id);
    let answer = folded.comments.last().expect("the answer");
    assert_eq!(answer.tool_calls.len(), 1);
    assert_eq!(answer.tool_calls[0].tool, "read_file");
    assert_eq!(answer.tool_calls[0].state, comments::SavedToolState::Done);
    assert_eq!(answer.tool_calls[0].advisor, None);
    assert!(
        folded.comments[0].tool_calls.is_empty(),
        "the author's comment carries no tool list",
    );
}

// AGC-FR-WNQE: a turn with no tool call writes no list.
#[test]
fn a_delivered_answer_without_tool_calls_carries_no_list() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Nothing to look up."))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let folded = folded(&h, "spec.md", &thread.id);
    assert!(folded.comments.last().expect("the answer").tool_calls.is_empty());
}

// AGC-FR-YBJC: the comment a turn ends on by a question carries no list.
#[test]
fn a_question_comment_carries_no_tool_list() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))),
        Ok(ScriptedReply::calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": said("Which reading?") }),
        )),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::AwaitingReply);
    let folded = folded(&h, "spec.md", &thread.id);
    let question = folded.comments.last().expect("the question");
    assert!(question.body.contains("Which reading?"));
    assert!(question.tool_calls.is_empty());
}
