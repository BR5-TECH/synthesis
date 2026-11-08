//! Agent groups: the marks on the comments a Meta turn or a synthesis turn ends
//! on, the end of the calls an advisor made, and the cancellation of advisors.
//!
//! Shares the seam and the helpers of `agent_groups`. A turn that has to wait is
//! held in a latch the test opens, and nothing waits on time.

use super::agent_groups::*;
use super::asking_author::folded_discussion;
use super::*;

fn advisors_of(attachments: &[comments::Attachment]) -> Option<Vec<comments::AdvisorEntry>> {
    attachments.iter().find_map(|a| match a {
        comments::Attachment::Advisors { advisors } => Some(advisors.clone()),
        _ => None,
    })
}

fn advice_or(r: &AgentRequest, meta: ModelReply) -> Result<ModelReply, &'static str> {
    if r.instructions.contains(ADVISOR_MARK) {
        Ok(text_reply(said("Important points:\n- a point\nKey questions:\n- a question")))
    } else {
        Ok(meta)
    }
}

// AGC-FR-VDOO: a Meta turn that ends on a question carries the Advisors list on
// the comment that the question is.
#[test]
fn a_meta_turn_that_asks_the_author_carries_the_advisors_list() {
    let seam = RoutedSeam::replying(|r, _| {
        advice_or(
            r,
            tool_reply(
                crate::tools::ask_user_comment::NAME,
                serde_json::json!({ "question": said("Which reading?") }),
            ),
        )
    });
    let (h, thread) = group_harness(seam);
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.join_turns();
    assert_eq!(group.mode, "advisor");
    let meta = terminal_of(&h, "arch");
    assert_eq!(meta[0].state, AgentTurnState::AwaitingReply);
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1, "the question is the only comment");
    assert!(comments[0].1.contains("Which reading?"));
    let advisors = advisors_of(&comments[0].2).expect("the question carries the list");
    assert_eq!(advisors.iter().filter(|a| a.state == "finished").count(), 2);
}

// AGC-FR-VDOO, DCP-FR-ATAH: a Meta turn that ends on a proposal carries the
// Advisors list on the comment that announces the proposal.
#[test]
fn a_meta_turn_that_proposes_a_change_carries_the_advisors_list() {
    let seam = RoutedSeam::replying(|r, _| {
        advice_or(
            r,
            tool_reply(crate::tools::propose_draft_changes::NAME, proposal_args("Meta rewrite.")),
        )
    });
    let h = Harness::with_patient_seam(Box::new(seam));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    let (draft_id, thread) = seed_discussion(&h, &["@arch what would you change?"]);
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.join_turns();
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    let proposal = folded
        .comments
        .iter()
        .find(|c| c.attachments.iter().any(|a| matches!(a, comments::Attachment::Proposal { .. })))
        .expect("a proposal comment");
    let advisors = advisors_of(&proposal.attachments).expect("the proposal carries the list");
    assert_eq!(advisors.len(), 1);
    assert_eq!(advisors[0].handle, "sec");
}

// AGC-FR-WZUL: a call that ends a synthesis turn waits for the text of the
// Synthesis. Nothing is recorded until the reply holds both.
#[test]
fn a_synthesis_call_without_text_is_not_carried_out() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok(text_reply("sec".into()))
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            if exchange <= 1 {
                Ok(tool_reply(
                    crate::tools::ask_discussion_questions::NAME,
                    question_set_args("One merged question?"),
                ))
            } else {
                Ok(tool_reply_with_text(
                    crate::tools::ask_discussion_questions::NAME,
                    question_set_args("One merged question?"),
                    said("The synthesis."),
                ))
            }
        } else {
            Ok(text_reply(said(format!("reply of {}", persona_of(r)))))
        }
    });
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    let (draft_id, thread) = seed_discussion(&h, &["@arch @sec what now?"]);
    switch(&h, true, 60);
    dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.join_turns();
    let synthesis_calls =
        seam.requests().iter().filter(|r| r.instructions.contains(SYNTHESIS_MARK)).count();
    assert_eq!(synthesis_calls, 2, "the first call was told to add the text");
    let set = crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).expect("a set");
    assert_eq!(set.questions.len(), 1);
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    let synthesis = folded
        .comments
        .iter()
        .filter(|c| c.attachments.contains(&comments::Attachment::Synthesis))
        .count();
    assert_eq!(synthesis, 1);
}

// AGC-FR-DIZM, CVL-FR-08: a synthesis turn on an artifact origin gets the
// prompt proposal that an agent held, and the section names the proposal tool
// that an artifact turn has.
#[test]
fn a_synthesis_turn_on_an_artifact_is_told_the_prompt_proposal_tool() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok(text_reply("sec".into()))
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            Ok(text_reply(said("The synthesis.")))
        } else if persona_of(r) == "ARCH" && exchange <= 1 {
            Ok(tool_reply(
                crate::tools::propose_prompt_changes::NAME,
                proposal_args("Arch rewrite."),
            ))
        } else {
            Ok(text_reply(said(format!("reply of {}", persona_of(r)))))
        }
    });
    let (h, thread) = group_harness(Arc::clone(&seam));
    switch(&h, true, 60);
    dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.join_turns();

    let requests = seam.requests();
    let synthesis = requests
        .iter()
        .find(|r| r.instructions.contains(SYNTHESIS_MARK))
        .expect("a synthesis turn ran");
    let held = synthesis
        .input
        .iter()
        .find(|s| s.tag == "group_proposals")
        .expect("the held proposal reaches the synthesis turn");
    assert!(held.body.contains("Arch rewrite."), "{}", held.body);
    assert!(
        held.body.contains(&format!("`{}`", crate::tools::propose_prompt_changes::NAME)),
        "the section names the prompt proposal tool: {}",
        held.body
    );
    assert!(
        !held.body.contains(crate::tools::propose_draft_changes::NAME),
        "an artifact turn has no draft proposal tool: {}",
        held.body
    );
    let offered: Vec<String> = synthesis.tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        offered.iter().any(|n| n == crate::tools::propose_prompt_changes::NAME),
        "the tool that the section names is the tool that the turn has: {offered:?}"
    );
}

fn agent(nickname: &str) -> agents::Agent {
    agents::Agent { id: nickname.into(), nickname: nickname.into(), ..Default::default() }
}

// AGC-FR-BIPE, AGC-FR-ANSK: when the join cancels an advisor that is inside a
// tool call, the call ends on the Meta turn as failed, and the Meta turn shows no
// active call of the advisor.
#[test]
fn the_join_ends_the_calls_of_an_advisor_it_cancels() {
    let seam = RoutedSeam::new(|_| Ok(String::new()));
    let (h, thread) = group_harness(seam);
    let turns = h.turns();
    let origin = ConversationOrigin::of(&thread);
    let (meta, _) =
        turns.register(&agent("arch"), origin.clone(), Some("c1"), PROJECT_KEY, None, None);
    let (advisor, _) = turns.register(&agent("sec"), origin, Some("c1"), PROJECT_KEY, None, None);
    let state = Arc::new(crate::agent_groups::GroupState::new(
        "group-t",
        crate::agent_groups::GroupMode::Advisor,
    ));
    state.add_member(&advisor.id, "sec");
    turns.attach_group(&meta.id, state.clone(), GroupRole::Meta, None);
    turns.attach_group(
        &advisor.id,
        state.clone(),
        GroupRole::Advisor,
        Some((meta.id.clone(), "sec".into())),
    );
    let handle = h.app.handle().clone();
    turns.begin_tool_call(&handle, &advisor.id, "search_drafts", None).expect("call");
    let on_meta = turns.in_flight(None).into_iter().find(|t| t.id == meta.id).expect("meta");
    assert_eq!(on_meta.active_tool_calls.len(), 1);

    let pending = state.skip_pending(SkipReason::TimedOut);
    skip_pending(&handle, &pending, SkipReason::TimedOut);

    let on_meta = turns.in_flight(None).into_iter().find(|t| t.id == meta.id).expect("meta");
    assert!(on_meta.active_tool_calls.is_empty(), "no advisor call stays active");
    assert_eq!(on_meta.tool_calls[0].state, ToolCallState::Failed);
    let ended = turns.terminated(&advisor.id).expect("the advisor ended");
    assert_eq!(ended.state, AgentTurnState::Cancelled);
    assert_eq!(ended.group.as_ref().and_then(|g| g.skipped), Some(SkipReason::TimedOut));
}

// AGC-FR-ANSK: cancelling a Meta turn ends the advisors that still run, as
// skipped for `cancelled`.
#[test]
fn cancelling_the_meta_turn_cancels_its_advisors() {
    let hold = Arc::new(Latch::default());
    let held = hold.clone();
    let seam = RoutedSeam::new(move |r| {
        if r.instructions.contains(ADVISOR_MARK) {
            if persona_of(r) == "SEC" {
                held.wait();
            }
            Ok(said("Important points:\n- a point"))
        } else {
            Ok(said("Meta answer."))
        }
    });
    let (h, thread) = group_harness(seam);
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    let meta_id = group.turns[0].id.clone();
    let sec_id = group.turns.iter().find(|t| t.nickname == "sec").expect("sec").id.clone();

    let handle = h.app.handle().clone();
    let progress = h.app.state::<ProgressRegistry>();
    cancel_impl(&handle, &h.turns(), &progress, &meta_id).expect("cancel");
    let ended = loop {
        if let Some(t) = h.turns().terminated(&sec_id) {
            break t;
        }
        std::thread::yield_now();
    };
    assert_eq!(ended.state, AgentTurnState::Cancelled);
    assert_eq!(ended.group.as_ref().and_then(|g| g.skipped), Some(SkipReason::Cancelled));
    hold.open();
    h.settle();
    assert!(agent_comments(&h).is_empty(), "a cancelled Meta turn appends nothing");
}

// AGC-FR-KOHJ: an addressed agent that is not ready takes no part in the group
// and gets the refusal that a regular dispatch gives it.
#[test]
fn a_tag_that_is_not_ready_is_refused_beside_the_group() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("sec".into())
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            Ok(said("The synthesis."))
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    // The agent is created while its model is served, and loses the model after.
    let set_models = |extra: bool| {
        let (mut records, active) = h.store().load_ai_api_registry().expect("registry");
        for record in records.iter_mut() {
            record.models.retain(|m| m.id != "not-served");
            if extra {
                record.models.push(ModelOption::new("not-served", "Not served"));
            }
        }
        h.store().save_ai_api_registry(records, active).expect("registry");
    };
    set_models(true);
    agents::create_agent_impl(
        &h.store(),
        "",
        None,
        agents::AgentScope::Personal,
        &AgentDraft {
            skills: Vec::new(),
            disabled_skills: Vec::new(),
            voice: String::new(),
            voice_speed_percent: None,
            voice_style: String::new(),
            nickname: "ghost".into(),
            title: String::new(),
            model_id: "not-served".into(),
            instructions: "GHOST-PERSONA".into(),
            reasoning: None,
        },
    )
    .expect("agent");
    set_models(false);
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch", "sec", "ghost"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "synthesis");
    assert_eq!(group.turns.len(), 2);
    assert!(group.turns.iter().all(|t| t.nickname != "ghost"));
    assert_eq!(group.refusals.len(), 1);
    assert_eq!(group.refusals[0].nickname, "ghost");
    assert_eq!(group.refusals[0].failure, FAIL_AGENT_UNAVAILABLE);
    let refused = find(&records_where("discussionId", &thread.id), "agent turn refused");
    assert_eq!(require(&refused, "agent"), "ghost");
    assert_eq!(require(&refused, "failure"), FAIL_AGENT_UNAVAILABLE);
    // The classifier chose among the ready agents alone.
    let requests = seam.requests();
    let classifier = requests.iter().find(|r| r.instructions.contains(CLASSIFIER_MARK)).unwrap();
    let candidates = classifier.input.iter().find(|s| s.tag == "candidates").unwrap();
    assert!(!candidates.body.contains("GHOST-PERSONA"));
}
