//! Placeholders: a reply that holds back for an open speech, and the retired
//! failure of an agent that answered the same comment later
//! (CVL-FR-KPOY .. CVL-FR-APIT, AGC-FR-RGIX).
//!
//! Each scenario holds the model call in a gate that the test opens, and waits
//! for a published state. Nothing here waits on time.

use super::speech_turns::*;
use super::*;

/// The steering message that ends a speech, for the hold rules.
fn speech_end() -> SteeringItem {
    SteeringItem::SpeechEnd("c-1".into())
}

fn assistant_texts(exchange: &[rig::completion::Message]) -> Vec<String> {
    use rig::completion::message::AssistantContent;
    exchange
        .iter()
        .filter_map(|message| match message {
            rig::completion::Message::Assistant(assistant) => Some(
                assistant
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        AssistantContent::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect()
}

// CVL-FR-NEXN, CVL-FR-SZKK: a chunk ends the hold of a held answer, and it
// does not end the hold of a placeholder. A comment ends both.
#[test]
fn a_chunk_ends_no_placeholder_hold() {
    let chunk = SteeringItem::Chunk("more".into());
    assert!(!HoldUntil::AnyMessage.ends_on(&[]));
    assert!(HoldUntil::AnyMessage.ends_on(std::slice::from_ref(&chunk)));
    assert!(!HoldUntil::FinalMessage.ends_on(&[]));
    assert!(!HoldUntil::FinalMessage.ends_on(&[chunk.clone(), chunk.clone()]));
    assert!(HoldUntil::FinalMessage.ends_on(&[chunk.clone(), speech_end()]));
    assert!(HoldUntil::FinalMessage.ends_on(&[SteeringItem::Comment("c-2".into())]));
}

// CVL-FR-KPOY, CVL-FR-NEXN, CVL-FR-JJRH, CVL-FR-13, CVL-FR-18, CVL-FR-NOHO:
// an empty reply to an open speech is held. It is not retried, it adds
// nothing to the exchange, and the turn answers the final comment. No failure
// is recorded.
#[test]
fn an_empty_reply_to_an_open_speech_is_held_and_answers_the_final_comment() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::unspoken("")), Ok(ScriptedReply::answer("Final."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-p", "Please look").expect("speak");
    gate.wait_entered(1);
    gate.release();
    let held = wait_for_event(&h, &turn.id, |t| t.reply_held);
    assert_eq!(held.state, AgentTurnState::Running);
    assert_eq!(h.seam.call_count(), 1, "the placeholder is not retried");

    let comment_id = post(&h, &draft_id, &thread, "Please look at the second part.");
    assert_eq!(end(&h, "s-p", Some(&comment_id)).len(), 1);
    gate.wait_entered(2);
    gate.release();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(terminal.failure, None);
    h.settle();

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 2);
    assert_eq!(exchanges[1].len(), exchanges[0].len() + 1, "only the final message is added");
    assert!(assistant_texts(&exchanges[1]).is_empty(), "an empty placeholder adds nothing");
    let last = user_texts(&exchanges[1]);
    assert!(last.last().expect("final").starts_with(FULL_SPEECH_LINE));
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Final.")]);
    assert!(h.turns().recoverable_failures(None).is_empty());
    let placeholders: Vec<String> = records_where("turnId", &turn.id)
        .iter()
        .filter(|r| r.message == "model answered")
        .map(|r| field(r, "speechPlaceholder"))
        .collect();
    assert_eq!(placeholders, vec!["true".to_string(), "false".to_string()]);
    // CVL-FR-JJRH: the round of the placeholder is the first logical call.
    let set_aside =
        find(&records_where("turnId", &turn.id), "agent placeholder set aside for a speech");
    assert_eq!(field(&set_aside, "modelCalls"), "1");
    assert_eq!(field(&set_aside, "withdrawn"), "false");
}

// CVL-FR-KPOY, CVL-FR-JJRH, CVL-FR-NOHO: a reply with text and no spoken part
// is a placeholder too. Its text goes into the exchange, and the turn never
// delivers it.
#[test]
fn an_untagged_reply_to_an_open_speech_is_not_delivered() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::unspoken("I will wait for the rest.")),
        Ok(ScriptedReply::answer("Final.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-u", "Look").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    let comment_id = post(&h, &draft_id, &thread, "Look at the second part.");
    end(&h, "s-u", Some(&comment_id));
    gate.wait_entered(2);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    assert_eq!(h.seam.call_count(), 2);
    let second = &h.seam.exchanges()[1];
    assert_eq!(assistant_texts(second), vec!["I will wait for the rest.".to_string()]);
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Final.")]);
}

// CVL-FR-NEXN, AGC-FR-FPYB: chunks that arrive while a placeholder is held do
// not wake the turn. The next call carries the final message and no chunk.
#[test]
fn chunks_do_not_wake_a_placeholder_turn() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::unspoken("")), Ok(ScriptedReply::answer("Final."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-c", "One").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    assert_eq!(steer(&h, "s-c", "chunk two").len(), 1);
    assert_eq!(steer(&h, "s-c", "chunk three").len(), 1);
    let comment_id = post(&h, &draft_id, &thread, "One, two, and three.");
    end(&h, "s-c", Some(&comment_id));
    gate.wait_entered(2);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    // CVL-FR-NEXN: the hold waits for a message that is not a chunk.
    let held = find(&records_where("turnId", &turn.id), "agent reply held for a speech");
    assert_eq!(field(&held, "until"), "final_message");
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 2, "the chunks made no call");
    let second = user_texts(&exchanges[1]);
    assert!(!second.iter().any(|t| t.contains("chunk two") || t.contains("chunk three")));
    assert!(second.last().expect("final").starts_with(FULL_SPEECH_LINE));
}

// AGC-FR-NFYK, CVL-FR-KPOY: a speech with no comment cancels the turn that it
// registered while that turn holds a placeholder. Nothing is delivered, and no
// failure is recorded.
#[test]
fn a_speech_with_no_comment_cancels_a_placeholder_turn() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::unspoken(""))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-n", "Hmm").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    end(&h, "s-n", None);
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Cancelled);
    h.settle();
    assert_eq!(h.seam.call_count(), 1);
    assert!(delivered_bodies(&h, &draft_id, &thread.id).is_empty());
    assert!(h.turns().recoverable_failures(None).is_empty());
}

// CVL-FR-APIT, AGC-FR-NFYK, CVL-FR-AEJO: a turn that ran before the first
// chunk holds a placeholder. The speech ends with no comment, so the loop
// appends the withdrawn line and calls again. The placeholder is never
// delivered.
#[test]
fn an_earlier_turn_calls_again_under_the_withdrawn_line() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::answer("First.")),
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::answer("The answer.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    let bound = speak(&h, &origin, "s-w", "also this").expect("speak");
    assert_eq!(bound.id, turn.id, "the running turn took the first chunk");
    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    let ended = end(&h, "s-w", None);
    assert_eq!(ended[0].state, AgentTurnState::Running, "the earlier turn keeps running");
    gate.wait_entered(3);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 3);
    let third = user_texts(&exchanges[2]);
    assert_eq!(third.last().expect("the withdrawn line"), SPEECH_WITHDRAWN_LINE);
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("The answer.")]);
}

// CVL-FR-WHEY, CVL-FR-KPOY, CVL-FR-13: at the logical bound no placeholder
// exists. An empty reply there is `empty_reply`, retried and then failed.
#[test]
fn an_empty_reply_at_the_bound_is_still_empty_reply() {
    let mut replies: Vec<Result<ScriptedReply, &'static str>> = (1..MAX_MODEL_CALLS)
        .map(|_| Ok(ScriptedReply::calls("search_skills", serde_json::json!({ "query": "x" }))))
        .collect();
    replies.extend((0..3).map(|_| Ok(ScriptedReply::unspoken(""))));
    let h = Harness::scripted(replies);
    h.mount();
    h.create_agent("arch", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-b", "Search").expect("speak");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert_eq!(h.seam.call_count(), MAX_MODEL_CALLS - 1 + 3, "the bound's reply is retried");
    h.settle();
    end(&h, "s-b", None);
}

// CVL-FR-QXHL, CVL-FR-UJXD: a tool call written as prose is not a
// placeholder, even while the speech is open. It is retried as `empty_reply`.
#[test]
fn a_tool_call_written_as_prose_is_no_placeholder() {
    let prose = r#"{"name":"search_skills","arguments":{"query":"x"}}"#;
    let h = Harness::scripted((0..3).map(|_| Ok(ScriptedReply::unspoken(prose))).collect());
    h.mount();
    h.create_agent("arch", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-j", "Search").expect("speak");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert_eq!(h.seam.call_count(), 3);
    assert!(!every_event(&h).iter().any(|t| t.reply_held), "nothing was held");
    h.settle();
    end(&h, "s-j", None);
}

/// A seam whose every reply the framework reports as failed (CVL-FR-UALC).
struct RefusingSeam(Arc<ScriptedCompletion>);

impl CompletionSeam for RefusingSeam {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        self.0.complete(request, exchange, endpoint, timeout).map(|reply| ModelReply {
            text: String::new(),
            refused: true,
            ..reply
        })
    }
}

// CVL-FR-QXHL, CVL-FR-UALC: a reply that the framework reports as failed is
// not a placeholder, even while the speech is open.
#[test]
fn a_refused_reply_is_no_placeholder() {
    let inner = Arc::new(ScriptedCompletion::scripted(
        (0..3).map(|_| Ok(ScriptedReply::unspoken(""))).collect(),
    ));
    let h = Harness::build_with(
        Box::new(RefusingSeam(inner.clone())),
        inner,
        DEFAULT_CONCURRENCY,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
    );
    h.mount();
    h.create_agent("arch", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-r", "Hi").expect("speak");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert!(!every_event(&h).iter().any(|t| t.reply_held), "nothing was held");
    h.settle();
    end(&h, "s-r", None);
}

// AGC-FR-RGIX, AGC-FR-31, AGC-FR-CEKN: the speech ends with a comment while
// the speech turn is still retrying. The turn fails, and its unread comment
// goes to a new turn of the same agent. That turn delivers, and the recorded
// failure for the same comment goes.
#[test]
fn a_delivery_for_the_same_comment_retires_the_agents_failure() {
    let (h, gate) = gated(vec![
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("I can hear you.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-f", "Can you hear").expect("speak");
    gate.wait_entered(1);
    let comment_id = post(&h, &draft_id, &thread, "Can you hear me now?");
    end(&h, "s-f", Some(&comment_id));
    for entered in 1..=3 {
        gate.wait_entered(entered);
        gate.release();
    }
    let failed = wait_for_terminal(&h, &turn.id);
    assert_eq!(failed.state, AgentTurnState::Failed);
    assert!(failed.retry_permitted, "the failure names its trigger comment");

    gate.wait_entered(4);
    let next = h
        .turns()
        .in_flight(Some(&origin))
        .into_iter()
        .find(|t| t.id != turn.id && t.trigger_comment_id.as_deref() == Some(comment_id.as_str()))
        .expect("a new turn for the unread comment");
    gate.release();
    assert_eq!(wait_for_terminal(&h, &next.id).state, AgentTurnState::Delivered);
    h.settle();
    assert!(h.turns().recoverable_failures(None).is_empty(), "the failure is retired");
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("I can hear you.")]);
    find(&records_where("turnId", &next.id), "agent turn failure retired by a delivery");
}

// AGC-FR-RGIX, CTA-FR-GQSP: a delivery of the same agent for another comment,
// or of another agent for the same comment, retires nothing. A delivery of
// the same agent for the same comment retires the failure.
#[test]
fn only_the_same_agent_and_comment_retire_a_failure() {
    let h = Harness::scripted(vec![
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("Other comment.")),
        Ok(asks("Which part?")),
        Ok(ScriptedReply::answer("Other agent.")),
        Ok(ScriptedReply::answer("Same agent, same comment.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    h.create_agent("bob", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch one", "@arch two"]);
    let origin = ConversationOrigin::of(&thread);
    let first = &thread.comments[0].id;
    let second = &thread.comments[1].id;

    let failed = h.dispatch("arch", origin.clone(), first).expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &failed.id).state, AgentTurnState::Failed);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);

    let other_comment = h.dispatch("arch", origin.clone(), second).expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &other_comment.id).state, AgentTurnState::Delivered);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1, "another comment");

    let asks_back = h.dispatch("arch", origin.clone(), first).expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &asks_back.id).state, AgentTurnState::AwaitingReply);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1, "a question is no delivery");

    let other_agent = h.dispatch("bob", origin.clone(), first).expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &other_agent.id).state, AgentTurnState::Delivered);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1, "another agent");

    let same = h.dispatch("arch", origin.clone(), first).expect("dispatch");
    assert_eq!(wait_for_terminal(&h, &same.id).state, AgentTurnState::Delivered);
    assert!(h.turns().recoverable_failures(None).is_empty(), "the same agent and comment");
    h.settle();
}

// CVL-FR-APIT, CVL-FR-AEJO, CVL-FR-JJRH, AGC-FR-NFYK: an earlier turn holds a
// placeholder with text. The speech ends with no comment, so the text goes
// into the exchange before the withdrawn line, and only the next answer is
// delivered.
#[test]
fn a_placeholder_with_text_is_never_delivered_when_the_speech_ends_with_no_comment() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::answer("First.")),
        Ok(ScriptedReply::unspoken("I will wait.")),
        Ok(ScriptedReply::answer("The answer.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    speak(&h, &origin, "s-t", "also this").expect("speak");
    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    end(&h, "s-t", None);
    gate.wait_entered(3);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    let third = &h.seam.exchanges()[2];
    assert_eq!(assistant_texts(&third[third.len() - 2..]), vec!["I will wait.".to_string()]);
    assert_eq!(rig_user_text(third.last().expect("the last message")), SPEECH_WITHDRAWN_LINE);
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("The answer.")]);
    assert!(h.turns().recoverable_failures(None).is_empty());
}

// CVL-FR-KPOY, CVL-FR-JJRH, AGC-FR-TCDQ: the speech ends with a comment while
// the call is in flight. The placeholder is not held, adds nothing, and the
// next call carries the final message and no withdrawn line.
#[test]
fn a_speech_that_ends_during_the_placeholder_call_goes_to_the_final_message() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::unspoken("  \n")), Ok(ScriptedReply::answer("Final."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-i", "Look").expect("speak");
    gate.wait_entered(1);
    let comment_id = post(&h, &draft_id, &thread, "Look at the second part.");
    end(&h, "s-i", Some(&comment_id));
    gate.release();
    gate.wait_entered(2);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges[1].len(), exchanges[0].len() + 1, "a blank placeholder adds nothing");
    let second = user_texts(&exchanges[1]);
    assert!(second.last().expect("final").starts_with(FULL_SPEECH_LINE));
    assert!(!second.iter().any(|t| t == SPEECH_WITHDRAWN_LINE));
    let set_aside =
        find(&records_where("turnId", &turn.id), "agent placeholder set aside for a speech");
    assert_eq!(field(&set_aside, "held"), "false");
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Final.")]);
}

// CVL-FR-NEXN, CVL-FR-EZKC: a typed comment ends the hold while the speech is
// still open. The next call carries the waiting chunk and the comment, and an
// empty reply to it is held again.
#[test]
fn a_comment_ends_the_hold_and_a_second_placeholder_is_held_again() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::answer("Final.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-m", "One").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    steer(&h, "s-m", "chunk two");
    let typed = post(&h, &draft_id, &thread, "@arch a typed note");
    let steered = h.dispatch("arch", origin.clone(), &typed).expect("steer");
    assert_eq!(steered.id, turn.id, "the running turn took the comment");
    gate.wait_entered(2);
    gate.release();
    // The second placeholder is held again: the speech is still open.
    let held_twice = || {
        records_where("turnId", &turn.id)
            .iter()
            .filter(|r| r.message == "agent reply held for a speech")
            .count()
            >= 2
    };
    for _ in 0..1000 {
        if held_twice() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(held_twice(), "the second placeholder was not held");
    let comment_id = post(&h, &draft_id, &thread, "One, two.");
    end(&h, "s-m", Some(&comment_id));
    gate.wait_entered(3);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();

    let second = user_texts(&h.seam.exchanges()[1]);
    assert!(second.iter().any(|t| t.contains("chunk two")), "the chunk is appended");
    assert!(second.iter().any(|t| t.contains("a typed note")), "the comment is appended");
    assert!(!second.iter().any(|t| t == SPEECH_WITHDRAWN_LINE));
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Final.")]);
}

// CVL-FR-KPOY, CVL-FR-APIT, CVL-FR-18: after the withdrawn line the speech is
// closed, so an empty reply is `empty_reply` again.
#[test]
fn an_empty_reply_after_the_withdrawn_line_is_empty_reply() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::answer("First.")),
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::unspoken("")),
        Ok(ScriptedReply::unspoken("")),
    ]);
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    speak(&h, &origin, "s-e2", "also").expect("speak");
    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    end(&h, "s-e2", None);
    for entered in 3..=5 {
        gate.wait_entered(entered);
        gate.release();
    }
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    h.settle();
}

// CVL-FR-NEXN, AGC-FR-20: a cancel during the hold of a placeholder ends the
// turn. Nothing is delivered, and no failure is recorded.
#[test]
fn a_cancel_during_a_placeholder_hold_ends_the_turn() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::unspoken(""))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-k", "Hmm").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);
    let handle = h.app.handle().clone();
    cancel_impl(&handle, &h.turns(), &h.progress(), &turn.id).expect("cancel");
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Cancelled);
    h.settle();
    assert_eq!(h.seam.call_count(), 1);
    assert!(delivered_bodies(&h, &draft_id, &thread.id).is_empty());
    assert!(h.turns().recoverable_failures(None).is_empty());
    end(&h, "s-k", None);
}

// CVL-FR-QXHL, CVL-FR-UALC: an OpenRouter reply that ends on filtered content
// or on an error is a failed reply, and one that stops is not.
#[test]
fn an_openrouter_reply_that_ends_on_a_filter_is_refused() {
    let reply_ending = |reason: &str| {
        let response: openrouter_rs::types::completion::CompletionsResponse =
            serde_json::from_value(serde_json::json!({
                "id": "gen-1",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "" },
                    "finish_reason": reason,
                }],
                "created": 0,
                "model": "m",
                "object": "chat.completion",
            }))
            .expect("the fixture parses as an answer");
        reply_from_openrouter(&response, "refusal-test-model", 1)
    };
    assert!(reply_ending("content_filter").refused);
    assert!(reply_ending("error").refused);
    assert!(!reply_ending("stop").refused);
}
