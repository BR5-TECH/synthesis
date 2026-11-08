//! The advisors tool of a Meta turn (AGC-FR-HVQD, AGC-FR-KRTP, AGC-FR-WBZN,
//! AGC-FR-DLXE, CVL-FR-RBQV, CVL-FR-GSNK).
//!
//! Shares the seam and the helpers of `agent_groups`. A turn that has to wait is
//! held by the state of the registry, and the test waits for a published state.
//! Nothing here waits on time.

use super::agent_groups::*;
use super::speech_turns::{end, speak_to};
use super::*;

const ADVICE: &str = "Important points:\n- a point\nKey questions:\n- a question";

fn advisor_requests(seam: &RoutedSeam) -> Vec<AgentRequest> {
    seam.requests().into_iter().filter(|r| r.instructions.contains(ADVISOR_MARK)).collect()
}

fn meta_requests(seam: &RoutedSeam) -> Vec<AgentRequest> {
    seam.requests().into_iter().filter(|r| persona_of(r) == "ARCH").collect()
}

/// Wait until the Meta turn shows the call of the advisors tool as active.
fn wait_for_call(h: &Harness, meta_id: &str) {
    loop {
        let calling = h.turns().in_flight(None).into_iter().any(|t| {
            t.id == meta_id && t.active_tool_calls.iter().any(|c| c.tool == "ask_advisors")
        });
        if calling {
            return;
        }
        std::thread::yield_now();
    }
}

// AGC-FR-HVQD, AGC-FR-MWYP: the system starts no advisor. A Meta turn that makes
// no call answers alone, and its comment carries no Advisors list.
#[test]
fn meta_can_answer_with_no_call_and_no_advisor_starts() {
    let seam =
        RoutedSeam::replying(|r, _| Ok(text_reply(said(format!("reply of {}", persona_of(r))))));
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "advisor");
    assert!(advisor_requests(&seam).is_empty());
    assert_eq!(seam.requests().len(), 1);
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.is_empty(), "no call, so no Advisors list");
}

// AGC-FR-HVQD, AGC-FR-GNMP: the advisors receive the full message of the author
// and the question of Meta, and they receive nothing else of Meta.
#[test]
fn the_advisors_receive_the_message_and_the_question_of_meta() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said(ADVICE)))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is the scope too wide?", text_reply(said("Done."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    let advisors = advisor_requests(&seam);
    assert_eq!(advisors.len(), 2);
    for request in &advisors {
        let question = request.input.iter().find(|s| s.tag == "meta_question").expect("question");
        assert_eq!(question.body, "Is the scope too wide?");
        let message = request.input.iter().find(|s| s.tag == "current_comment").expect("message");
        assert!(message.body.contains("@arch what do you think?"), "{}", message.body);
    }
    assert!(meta_requests(&seam).iter().all(|r| r.input.iter().all(|s| s.tag != "meta_question")));
}

// AGC-FR-HVQD: an empty question is refused as invalid arguments, no advisor
// starts, and Meta goes on.
#[test]
fn an_empty_question_is_refused_and_starts_no_advisor() {
    let seam = RoutedSeam::replying(|r, exchange| {
        Ok(meta_asks_advisors(r, exchange, "   ", text_reply(said("Done."))))
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert!(advisor_requests(&seam).is_empty());
    assert!(seam.exchanges().iter().any(|e| e.contains("The question is empty")));
    assert_eq!(agent_comments(&h).len(), 1);
}

// AGC-FR-HVQD, AGC-FR-VDOO: a later call is optional. Each call is its own join,
// and the list names each advisor one time, with the state of its last call.
#[test]
fn a_second_call_asks_the_advisors_again() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said(ADVICE)))
        } else if exchange <= 1 {
            Ok(tool_reply("ask_advisors", serde_json::json!({ "question": "First?" })))
        } else if exchange <= 3 {
            Ok(tool_reply("ask_advisors", serde_json::json!({ "question": "Second?" })))
        } else {
            Ok(text_reply(said("Done.")))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(advisor_requests(&seam).len(), 4);
    let comments = agent_comments(&h);
    let advisors = comments[0]
        .2
        .iter()
        .find_map(|a| match a {
            comments::Attachment::Advisors { advisors } => Some(advisors.clone()),
            _ => None,
        })
        .expect("advisors");
    assert_eq!(advisors.len(), 2);
}

// AGC-FR-HVQD: a Meta turn with no other ready agent is a regular turn that has
// no advisors tool.
#[test]
fn a_single_agent_has_no_advisors_tool() {
    let seam = RoutedSeam::replying(|_, _| Ok(text_reply(said("Alone."))));
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "regular");
    assert!(seam.requests()[0].tools.iter().all(|t| t.name.as_str() != "ask_advisors"));
}

// AGC-FR-HVQD: no other turn has the tool, and the tool is the same on every
// origin. An advisor and a regular turn do not have it.
#[test]
fn only_a_meta_turn_has_the_advisors_tool() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said(ADVICE)))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Any risk?", text_reply(said("Done."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, false, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert!(seam
        .requests()
        .iter()
        .all(|r| r.tools.iter().all(|t| t.name.as_str() != "ask_advisors")));
}

// AGC-FR-DLXE, AGC-FR-NWDA: with the switch on, the speech turn of one agent is
// a Meta turn. With the switch off it is a regular turn.
#[test]
fn a_speech_turn_is_meta_with_the_switch_on_and_regular_with_it_off() {
    for enabled in [true, false] {
        let seam = RoutedSeam::replying(|_, _| Ok(text_reply(said("Reply."))));
        let (h, thread) = group_harness(seam.clone());
        switch(&h, enabled, 60);
        let origin = ConversationOrigin::of(&thread);
        let turn = speak_to(&h, "arch", &origin, "sp-1", "What do you think?").expect("turn");
        assert_eq!(turn.group.as_ref().map(|g| g.role), enabled.then_some(GroupRole::Meta));
        end(&h, "sp-1", Some(&thread.comments[0].id));
        h.settle();
        let has_tool = seam.requests()[0].tools.iter().any(|t| t.name.as_str() == "ask_advisors");
        assert_eq!(has_tool, enabled);
    }
}

// AGC-FR-DLXE: with the switch on and no other ready agent, the speech turn is a
// regular turn.
#[test]
fn a_speech_turn_with_no_other_ready_agent_is_regular() {
    let seam = RoutedSeam::replying(|_, _| Ok(text_reply(said("Reply."))));
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    switch(&h, true, 60);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak_to(&h, "arch", &origin, "sp-1", "Hello.").expect("turn");
    assert!(turn.group.is_none());
    end(&h, "sp-1", Some(&thread.comments[0].id));
    h.settle();
}

// AGC-FR-KRTP, AGC-FR-WBZN, CVL-FR-GSNK, AGC-FR-HVQD: a call made while the speech
// is open starts no advisor before the comment posts. Then the advisors run for
// that comment, and Meta answers after them.
#[test]
fn a_call_in_an_open_speech_waits_for_the_comment() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said(ADVICE)))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta reply."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak_to(&h, "arch", &origin, "sp-1", "What do you think?").expect("turn");
    wait_for_call(&h, &turn.id);
    // The call is active and the speech is open: no advisor exists yet.
    assert!(advisor_requests(&seam).is_empty());
    assert!(h.turns().in_flight(None).iter().all(|t| t.nickname == "arch"));

    end(&h, "sp-1", Some(&thread.comments[0].id));
    h.settle();
    assert_eq!(advisor_requests(&seam).len(), 2);
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].0, "arch");
    assert!(comments[0].2.iter().any(|a| matches!(a, comments::Attachment::Advisors { .. })));
    assert_eq!(terminal_of(&h, "arch")[0].state, AgentTurnState::Delivered);
}

// AGC-FR-KRTP, AGC-FR-NFYK: a speech that ends with no comment cancels the early
// Meta turn, the wait ends, and no advisor ever starts.
#[test]
fn a_speech_that_ends_with_no_comment_ends_the_wait_and_starts_no_advisor() {
    let seam = RoutedSeam::replying(|r, exchange| {
        Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta reply."))))
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak_to(&h, "arch", &origin, "sp-1", "What do you think?").expect("turn");
    wait_for_call(&h, &turn.id);
    end(&h, "sp-1", None);
    h.settle();
    assert!(advisor_requests(&seam).is_empty());
    assert_eq!(terminal_of(&h, "arch")[0].state, AgentTurnState::Cancelled);
    assert!(agent_comments(&h).is_empty());
}

// AGC-FR-KRTP: cancelling the turn ends the wait of the call.
#[test]
fn cancelling_the_turn_ends_the_wait_of_the_call() {
    let seam = RoutedSeam::replying(|r, exchange| {
        Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta reply."))))
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak_to(&h, "arch", &origin, "sp-1", "What do you think?").expect("turn");
    wait_for_call(&h, &turn.id);
    let handle = h.app.handle().clone();
    let progress = h.app.state::<ProgressRegistry>();
    cancel_impl(&handle, &h.turns(), &progress, &turn.id).expect("cancel");
    h.settle();
    assert!(advisor_requests(&seam).is_empty());
    assert_eq!(terminal_of(&h, "arch")[0].state, AgentTurnState::Cancelled);
}

/// A harness with one call slot only (AGC-FR-25).
fn one_slot_harness(seam: Arc<RoutedSeam>) -> (Harness, Discussion) {
    let h = Harness::build_with(
        Box::new(seam),
        Arc::new(ScriptedCompletion::new(vec![])),
        1,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
    );
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    h.create_agent("ops", "OPS-PERSONA");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    (h, thread)
}

// AGC-FR-25, AGC-FR-HVQD: a Meta turn frees its call slot during the call of the
// advisors tool. With one slot only, the advisors still run and return advice,
// Meta takes the slot again, and the slot is free when every turn has ended.
#[test]
fn meta_frees_its_slot_while_the_advisors_run() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said(ADVICE)))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta reply."))))
        }
    });
    let (h, thread) = one_slot_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(advisor_requests(&seam).len(), 2);
    assert_eq!(meta_requests(&seam).len(), 2);
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.iter().any(|a| matches!(a, comments::Attachment::Advisors { .. })));
    let entries = comments[0].2.iter().find_map(|a| match a {
        comments::Attachment::Advisors { advisors } => Some(advisors),
        _ => None,
    });
    assert!(entries.expect("advisors").iter().all(|a| a.state == "finished"), "{entries:?}");
    assert_eq!(terminal_of(&h, "arch")[0].state, AgentTurnState::Delivered);
    assert_eq!(h.turns().permits_available(), 1);
}

// AGC-FR-25, AGC-FR-HVQD: a Meta turn that waits for the end of an open speech
// holds no call slot, so another turn can use the one slot.
#[test]
fn meta_holds_no_slot_while_it_waits_for_the_speech() {
    let seam = RoutedSeam::replying(|r, exchange| {
        Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta reply."))))
    });
    let (h, thread) = one_slot_harness(seam.clone());
    switch(&h, true, 60);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak_to(&h, "arch", &origin, "sp-1", "What do you think?").expect("turn");
    wait_for_call(&h, &turn.id);
    // The call is reported before the turn frees its slot, so the test waits for it.
    while h.turns().permits_available() == 0 {
        std::thread::yield_now();
    }
    end(&h, "sp-1", None);
    h.settle();
    assert_eq!(h.turns().permits_available(), 1);
}
