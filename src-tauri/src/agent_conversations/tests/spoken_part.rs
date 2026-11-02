//! The spoken part of a comment (`CVL-conversation-loop.md` CVL-FR-AGNN): what
//! the prompts ask for, and what the loop does with a reply that lacks it.

use super::asking_author::{
    folded_discussion, proposes, proposes_prompt, run_draft_turn, run_prompt_turn,
};
use super::*;

const UNSPOKEN: &str = "The written answer, with no tag.";

/// The prompts of every target, as the turns of that target carry them.
fn prompts() -> Vec<String> {
    [
        OriginKind::ArtifactComment,
        OriginKind::DraftComment,
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftDiscussion,
        OriginKind::NoteDiscussion,
    ]
    .into_iter()
    .map(|kind| compile_prompt(kind, "", "Reviewer"))
    .collect()
}

// CVL-FR-HREC, CVL-FR-KWYV: every prompt states the rule, on every turn.
#[test]
fn every_prompt_states_the_rules_of_the_spoken_part() {
    for prompt in prompts() {
        assert!(prompt.contains("one `<spoken>…</spoken>` tag"), "the tag: {prompt}");
        assert!(prompt.contains("anywhere in its body"), "the place");
        assert!(prompt.contains("one or two short sentences"), "the length");
        assert!(prompt.contains("an offer to read it"), "the offer");
        assert!(prompt.contains("A comment with no tag is refused."), "the refusal");
        assert!(prompt.contains("also when the author does not use voice"), "every turn");
    }
}

// CVL-FR-MRHD
#[test]
fn every_prompt_says_how_to_answer_a_declined_offer() {
    for prompt in prompts() {
        assert!(prompt
            .contains("declines an offer to read more, answer in a few words and read nothing"));
    }
}

// CVL-FR-05: a prompt names no tool, the spoken part included.
#[test]
fn the_rule_of_the_spoken_part_names_no_tool() {
    for prompt in prompts() {
        let rule = prompt.split("## Spoken part").nth(1).expect("the section");
        let rule = rule.split("## Rules").next().expect("its end");
        for tool in ["ask_user_comment", "propose_draft_changes", "propose_prompt_changes"] {
            assert!(!rule.contains(tool), "{tool} in {rule}");
        }
    }
}

// CVL-FR-NOHO, CVL-FR-13, CVL-FR-18: a final reply with no spoken part is retried
// as the same logical call, and the next attempt delivers.
#[test]
fn a_final_reply_with_no_spoken_part_is_asked_for_again() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::unspoken(UNSPOKEN)),
        Ok(ScriptedReply::answer("The answer.")),
    ]);
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2);
    let comments = folded(&h, "spec.md", &thread.id).comments;
    assert_eq!(comments.len(), 2, "the first reply was not appended");
    assert_eq!(comments[1].body, said("The answer."));
    let ended = find(&records_where("turnId", &turn.id), "agent turn ended");
    assert_eq!(require(&ended, "modelCalls"), "1", "one logical call");
}

// CVL-FR-NOHO, CVL-FR-20: the retry repeats the exchange byte for byte.
#[test]
fn the_retry_presents_the_same_exchange() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::unspoken(UNSPOKEN)),
        Ok(ScriptedReply::answer("The answer.")),
    ]);
    h.create_agent("arch", "");

    run_one(&h, "arch");

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges[0], exchanges[1]);
}

// CVL-FR-NOHO, CVL-FR-13: with every attempt spent, the turn ends as `empty_reply`,
// it is recoverable, and nothing is appended.
#[test]
fn a_turn_with_no_spoken_part_on_every_attempt_ends_as_an_empty_reply() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken(UNSPOKEN)); MAX_ATTEMPTS]);
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Failed);
    assert_eq!(turn.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert!(turn.retry_permitted, "the failure is recoverable");
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
    assert_eq!(folded(&h, "spec.md", &thread.id).comments.len(), 1, "nothing appended");
}

// CVL-FR-AGNN, CVL-FR-NOHO: a tag quoted as code is no spoken part, and neither
// is a blank one.
#[test]
fn a_blank_tag_and_a_quoted_tag_are_no_spoken_part() {
    for text in ["Text. <spoken>  </spoken>", "Use `<spoken>x</spoken>` here."] {
        let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken(text)); MAX_ATTEMPTS]);
        h.create_agent("arch", "");
        let (turn, _) = run_one(&h, "arch");
        assert_eq!(turn.state, AgentTurnState::Failed, "{text}");
    }
}

// CVL-FR-NOHO, CVL-FR-26: the warning names the reply by size, and the stored
// comment keeps the tag.
#[test]
fn the_warning_holds_no_text_and_the_comment_keeps_its_tag() {
    let sentinel = "Zanzibaranswer";
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::unspoken(sentinel)),
        Ok(ScriptedReply::unspoken("<spoken>Short.</spoken> Written.")),
    ]);
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    let records = records_where("turnId", &turn.id);
    assert!(
        records.iter().all(|r| r.message != "model reply held nothing to deliver"),
        "the reply held text, so the record of an empty reply does not apply",
    );
    let warning = find(&records, "agent reply has no spoken part");
    assert_eq!(warning.level, LogLevel::Warn);
    assert_eq!(require(&warning, "attempt"), "1");
    assert_eq!(require(&warning, "replyChars"), sentinel.chars().count().to_string());
    let fields = serde_json::to_string(&warning.fields).expect("fields serialize");
    assert!(!fields.contains(sentinel), "the record carries no text: {fields}");
    let last = folded(&h, "spec.md", &thread.id).comments.last().expect("answer").body.clone();
    assert_eq!(last, "<spoken>Short.</spoken> Written.");
}

// CVL-FR-NOHO, CVL-FR-WQZD: the provider billed the reply that the loop asks for
// again, so the turn counts it.
#[test]
fn a_reply_with_no_spoken_part_counts_as_a_billed_call() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::unspoken(UNSPOKEN).reporting(0, 7)),
        Ok(ScriptedReply::answer("The answer.").reporting(0, 10)),
    ]);
    h.create_agent("arch", "");

    let (turn, _) = run_one(&h, "arch");

    let ended = find(&records_where("turnId", &turn.id), "agent turn ended");
    assert_eq!(require(&ended, "uncachedInputTokens"), "17");
}

// CVL-FR-NOHO, CVL-FR-13: a reply that asks for a tool below the bound is no
// final reply, whatever prose it carries.
#[test]
fn a_reply_that_asks_for_a_tool_needs_no_spoken_part() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({})).with_text("Looking.")),
        Ok(ScriptedReply::answer("Found it.")),
    ]);
    h.create_agent("arch", "");

    let (turn, _) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2, "no attempt was spent on a retry");
}

// CVL-FR-NOHO, CVL-FR-13: at the bound the prose of a reply is the answer, so it
// needs a spoken part, and its retry is a physical attempt of the same call.
#[test]
fn at_the_bound_a_reply_with_no_spoken_part_is_retried_as_the_same_call() {
    let mut script: Vec<Result<ScriptedReply, &'static str>> = (0..MAX_MODEL_CALLS - 1)
        .map(|_| Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))))
        .collect();
    script.push(Ok(
        ScriptedReply::calls("list_skills", serde_json::json!({})).with_text("No tag here.")
    ));
    script.push(Ok(ScriptedReply::answer("Concluded.")));
    let h = Harness::scripted(script);
    h.mount();
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), MAX_MODEL_CALLS + 1);
    let last = folded(&h, "spec.md", &thread.id).comments.last().expect("answer").body.clone();
    assert_eq!(last, said("Concluded."));
}

// CVL-FR-NOHO, CVL-FR-13: at the bound the attempts are spent as anywhere else, and
// the turn ends as `empty_reply`.
#[test]
fn at_the_bound_no_spoken_part_on_every_attempt_ends_as_an_empty_reply() {
    let mut script: Vec<Result<ScriptedReply, &'static str>> = (0..MAX_MODEL_CALLS - 1)
        .map(|_| Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))))
        .collect();
    for _ in 0..MAX_ATTEMPTS {
        script
            .push(Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))
                .with_text("No tag here.")));
    }
    let h = Harness::scripted(script);
    h.mount();
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Failed);
    assert_eq!(turn.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert_eq!(h.seam.call_count(), MAX_MODEL_CALLS - 1 + MAX_ATTEMPTS);
    assert_eq!(folded(&h, "spec.md", &thread.id).comments.len(), 1, "nothing appended");
}

// CVL-FR-UCTB, PDC-FR-ZQOH, CVL-FR-14, CVL-FR-15: a refused proposal ends nothing.
#[test]
fn a_draft_proposal_with_no_spoken_part_is_refused_and_the_loop_continues() {
    let mut proposal = proposes("artifact-window.md", "The rewrite.", "Reason.");
    proposal.tools[0].1["rationale"] = serde_json::json!("Reason, with no tag.");
    let h = Harness::scripted(vec![Ok(proposal), Ok(ScriptedReply::answer("Said in prose."))]);
    h.mount();
    h.create_agent("arch", "");

    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(results[0].contains(crate::tools::RATIONALE_NO_SPOKEN_PART), "{results:?}");
    assert_eq!(
        folded_discussion(&h, &draft_id, &thread.id).comments.last().expect("answer").body,
        said("Said in prose."),
    );
}

// CVL-FR-UCTB, PPC-FR-HMEH, CVL-FR-14, CVL-FR-15: a refused proposal ends nothing.
#[test]
fn a_prompt_proposal_with_no_spoken_part_is_refused_and_the_loop_continues() {
    let mut proposal = proposes_prompt("prompts/review.md", "# Better\n", "Reason.");
    proposal.tools[0].1["rationale"] = serde_json::json!("Reason, with no tag.");
    let h = Harness::scripted(vec![Ok(proposal), Ok(ScriptedReply::answer("Said in prose."))]);
    h.mount();
    h.create_agent("arch", "");

    let (turn, _) = run_prompt_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(results[0].contains(crate::tools::RATIONALE_NO_SPOKEN_PART), "{results:?}");
}

// CVL-FR-UCTB, CVL-FR-14, CVL-FR-15: a refused question ends nothing, and the turn
// goes on to an answer.
#[test]
fn a_question_with_no_spoken_part_is_refused_and_the_loop_continues() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "One spec or two?" }),
        )),
        Ok(ScriptedReply::answer("Two specs.")),
    ]);
    h.mount();
    h.create_agent("arch", "");

    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    let exchanges = h.seam.exchanges();
    let results = tool_results(&exchanges[1]);
    assert!(results[0].contains("has no spoken part"), "{results:?}");
    let comments = folded(&h, "spec.md", &thread.id).comments;
    assert_eq!(comments.len(), 2, "the question was not posted");
}

// CVL-FR-UCTB: a question set appends no comment, so it needs no spoken part.
#[test]
fn a_question_set_needs_no_spoken_part() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::calls(
        crate::tools::ask_discussion_questions::NAME,
        serde_json::json!({
            "questions": [{
                "question": "One spec or two?",
                "options": [
                    { "label": "One", "description": "One file." },
                    { "label": "Two", "description": "Two files." },
                ],
            }],
        }),
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let created =
        crate::drafts::create_draft_at_root(&h.root(), Some("Spoken")).expect("the draft");
    let thread = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
        None,
        "@arch what now?".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("the discussion");

    let turn =
        h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).expect("go");
    let terminal = wait_for_terminal(&h, &turn.id);

    assert_eq!(terminal.state, AgentTurnState::AwaitingReply);
}
