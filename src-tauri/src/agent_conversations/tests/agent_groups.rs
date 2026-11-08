//! Agent groups: the modes, the join, the advisors, and the Synthesis
//! (AGC-FR-KOHJ and the requirements around it).
//!
//! Every scenario runs against a seam that answers by the persona of the turn it
//! is asked for, so parallel turns get their replies without a race. A turn that
//! has to wait is held in a latch the test opens, and nothing waits on time.

use super::asking_author::folded_discussion;
use super::*;
use crate::agent_groups::AgentGroupSettings;

pub(super) type Route =
    dyn Fn(&AgentRequest, usize) -> Result<ModelReply, &'static str> + Send + Sync;

/// A seam that answers a request from a function of the request and of the
/// length of its exchange (one on the first call of a turn).
pub(super) struct RoutedSeam {
    route: Box<Route>,
    requests: Mutex<Vec<AgentRequest>>,
    /// The exchange of each call as text, in the order of the calls.
    exchanges: Mutex<Vec<String>>,
}

pub(super) fn text_reply(text: String) -> ModelReply {
    ModelReply { text, ..Default::default() }
}

/// A reply that calls one tool and says nothing.
pub(super) fn tool_reply(name: &str, arguments: serde_json::Value) -> ModelReply {
    ModelReply {
        tool_calls: vec![rig::completion::message::ToolCall::from_wire(
            "call-0".to_string(),
            rig::completion::message::ToolFunction::new(
                rig::completion::message::ToolName::new(name.to_string()).expect("a tool name"),
                arguments,
            ),
        )],
        ..Default::default()
    }
}

/// A reply that calls one tool and also carries text.
pub(super) fn tool_reply_with_text(
    name: &str,
    arguments: serde_json::Value,
    text: String,
) -> ModelReply {
    ModelReply { text, ..tool_reply(name, arguments) }
}

impl RoutedSeam {
    pub(super) fn new(
        route: impl Fn(&AgentRequest) -> Result<String, &'static str> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Self::replying(move |request, _| route(request).map(text_reply))
    }

    pub(super) fn replying(
        route: impl Fn(&AgentRequest, usize) -> Result<ModelReply, &'static str> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            route: Box::new(route),
            requests: Mutex::new(Vec::new()),
            exchanges: Mutex::new(Vec::new()),
        })
    }

    pub(super) fn requests(&self) -> Vec<AgentRequest> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(super) fn exchanges(&self) -> Vec<String> {
        self.exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl CompletionSeam for RoutedSeam {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).push(request.clone());
        self.exchanges.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{exchange:?}"));
        (self.route)(request, exchange.len())
            .map_err(|class| CallFailure::new(FAIL_UNREACHABLE, class))
    }
}

/// A gate a seam call waits at until the test opens it.
#[derive(Default)]
pub(super) struct Latch {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Latch {
    pub(super) fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        while !*open {
            open = self.changed.wait(open).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub(super) fn open(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.changed.notify_all();
    }
}

pub(super) const ADVISOR_MARK: &str = "For this message you are an advisor";
pub(super) const SYNTHESIS_MARK: &str = "You write the Synthesis of these replies";
pub(super) const CLASSIFIER_MARK: &str = "You choose the one agent that must lead";

/// The reply of a Meta turn: its first call asks the advisors (AGC-FR-HVQD), and
/// each later call gives `then`. An advisor and a classifier get `other`.
pub(super) fn meta_asks_advisors(
    request: &AgentRequest,
    exchange: usize,
    question: &str,
    then: ModelReply,
) -> ModelReply {
    if exchange <= 1 && request.tools.iter().any(|t| t.name.as_str() == "ask_advisors") {
        tool_reply("ask_advisors", serde_json::json!({ "question": question }))
    } else {
        then
    }
}

pub(super) fn persona_of(request: &AgentRequest) -> &'static str {
    ["ARCH", "SEC", "OPS"]
        .into_iter()
        .find(|p| request.instructions.contains(&format!("{p}-PERSONA")))
        .unwrap_or("NONE")
}

pub(super) fn group_harness(seam: Arc<RoutedSeam>) -> (Harness, Discussion) {
    let h = Harness::with_patient_seam(Box::new(seam));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    h.create_agent("ops", "OPS-PERSONA");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    (h, thread)
}

pub(super) fn switch(h: &Harness, enabled: bool, join_timeout_secs: u32) {
    h.store()
        .save_agent_group_settings(PROJECT_KEY, &AgentGroupSettings { enabled, join_timeout_secs })
        .expect("settings");
}

pub(super) fn dispatch_group(
    h: &Harness,
    nicknames: &[&str],
    thread: &Discussion,
) -> Result<AgentGroup, String> {
    dispatch_group_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        &nicknames.iter().map(|n| n.to_string()).collect::<Vec<_>>(),
        ConversationOrigin::of(thread),
        &thread.comments[0].id,
    )
}

pub(super) fn agent_comments(h: &Harness) -> Vec<(String, String, Vec<comments::Attachment>)> {
    h.threads("spec.md")[0]
        .comments
        .iter()
        .filter_map(|c| match &c.author {
            Participant::Agent { handle, .. } => {
                Some((handle.clone(), c.body.clone(), c.attachments.clone()))
            }
            _ => None,
        })
        .collect()
}

pub(super) fn terminal_of(h: &Harness, nickname: &str) -> Vec<AgentTurn> {
    h.terminal.lock().unwrap().iter().filter(|t| t.nickname == nickname).cloned().collect()
}

// AGC-FR-MRAR: with the switch off, a group call is one dispatch per nickname.
#[test]
fn the_switch_off_dispatches_each_agent_as_before() {
    let seam = RoutedSeam::new(|r| Ok(said(format!("reply of {}", persona_of(r)))));
    let (h, thread) = group_harness(seam.clone());
    let group = dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "regular");
    assert!(group.group_id.is_none() && group.driver.is_none());
    assert_eq!(group.turns.len(), 2);
    assert!(group.turns.iter().all(|t| t.group.is_none()));
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 2);
    assert!(comments.iter().all(|(_, _, attachments)| attachments.is_empty()));
    // No classifier call and no advisor call.
    assert!(seam.requests().iter().all(
        |r| !r.instructions.contains(CLASSIFIER_MARK) && !r.instructions.contains(ADVISOR_MARK)
    ));
}

// AGC-FR-LJVM: one dispatched agent is a regular turn.
#[test]
fn one_dispatched_agent_is_a_regular_turn() {
    let seam = RoutedSeam::new(|r| Ok(said(format!("reply of {}", persona_of(r)))));
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "regular");
    assert!(group.turns[0].group.is_none());
    assert_eq!(seam.requests().len(), 1);
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.is_empty());
}

// AGC-FR-YVNC, AGC-FR-HVQD: a shadowed agent is not ready, so a nickname that a
// Project agent shadows advises once and not twice.
#[test]
fn a_shadowed_agent_is_not_an_advisor() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said("Important points:\n- p\nKey questions:\n- q")))
        } else {
            Ok(meta_asks_advisors(r, exchange, "What matters?", text_reply(said("answer"))))
        }
    });
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    h.create_project_agent_with_skills("sec", "SEC-PERSONA", &[]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "advisor");
    let advisor_calls =
        seam.requests().iter().filter(|r| r.instructions.contains(ADVISOR_MARK)).count();
    assert_eq!(advisor_calls, 1);
    assert_eq!(terminal_of(&h, "sec").len(), 1);
}

// AGC-FR-GNMP, AGC-FR-UFAY, AGC-FR-MWYP, AGC-FR-HVQD, CVL-FR-RBQV, AGC-FR-VDOO, AGC-FR-BIPE:
// one tagged agent is Meta and starts no advisor at dispatch. Its call of the
// advisors tool starts the other ready agents, and its reply carries the list.
#[test]
fn a_single_tag_makes_the_agent_meta_and_its_call_starts_the_advisors() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            let who = persona_of(r).to_lowercase();
            Ok(text_reply(said(format!(
                "Important points:\n- {who} point\nKey questions:\n- {who} question"
            ))))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta answer."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();

    assert_eq!(group.mode, "advisor");
    assert_eq!(group.driver.as_deref(), Some("arch"));
    // Only the Meta turn is registered at dispatch.
    assert_eq!(group.turns.len(), 1);
    assert_eq!(group.turns[0].group.as_ref().expect("group").role, GroupRole::Meta);

    // One comment: Meta's, with the Advisors list.
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    let (handle, _, attachments) = &comments[0];
    assert_eq!(handle, "arch");
    let advisors = attachments
        .iter()
        .find_map(|a| match a {
            comments::Attachment::Advisors { advisors } => Some(advisors.clone()),
            _ => None,
        })
        .expect("an advisors attachment");
    assert_eq!(advisors.len(), 2);
    assert!(advisors.iter().all(|a| a.state == "finished"), "{advisors:?}");
    let sec = advisors.iter().find(|a| a.handle == "sec").expect("sec advised");
    assert_eq!(sec.points, vec!["sec point".to_string()]);
    assert_eq!(sec.questions, vec!["sec question".to_string()]);

    // Meta has the tool and the Meta prompt from its first call. Its second call
    // reads the advice as the result of the tool. An advisor's request holds the
    // question of Meta and none of the tools that end a turn on a contribution.
    let requests = seam.requests();
    let meta: Vec<_> = requests.iter().filter(|r| persona_of(r) == "ARCH").collect();
    assert_eq!(meta.len(), 2);
    assert!(meta[0].tools.iter().any(|t| t.name.as_str() == "ask_advisors"));
    assert!(meta[0].instructions.contains("ask_advisors"));
    assert!(meta.iter().all(|r| r.input.iter().all(|s| s.tag != "advisors")));
    let exchanges = seam.exchanges();
    assert!(exchanges.iter().any(|e| e.contains("@sec") && e.contains("sec point")));
    let advisor = requests.iter().find(|r| r.instructions.contains(ADVISOR_MARK)).expect("advisor");
    assert!(advisor.tools.iter().all(|t| !ADVISOR_WITHHELD_TOOLS.contains(&t.name.as_str())));
    assert!(advisor.tools.iter().all(|t| t.name.as_str() != "ask_advisors"));
    assert!(!advisor.tools.is_empty());

    // The advisor turns ended delivered and appended no comment.
    for nickname in ["sec", "ops"] {
        let ended = terminal_of(&h, nickname);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].state, AgentTurnState::Delivered);
        assert_eq!(ended[0].group.as_ref().map(|g| g.role), Some(GroupRole::Advisor));
    }
}

// AGC-FR-ZOCI, AGC-FR-ZLWN, AGC-FR-ANSK: the join ends at the timeout; the
// advisor that has not finished is cancelled, marked skipped, and listed with the
// reason. A zero timeout ends the join at the quorum.
#[test]
fn the_timeout_skips_an_advisor_that_has_not_finished() {
    let hold = Arc::new(Latch::default());
    let held = hold.clone();
    let seam = RoutedSeam::replying(move |r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            if persona_of(r) == "OPS" {
                held.wait();
            }
            Ok(text_reply(said("Important points:\n- a point")))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Any risk?", text_reply(said("Meta answer."))))
        }
    });
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    for (nick, persona) in [("arch", "ARCH"), ("sec", "SEC"), ("ops", "OPS"), ("dev", "DEV")] {
        h.create_agent(nick, &format!("{persona}-PERSONA"));
    }
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    // Three advisors: the quorum is two. Zero seconds: the join ends at the quorum.
    switch(&h, true, 0);
    let group = dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    // The Meta turn answers after the call joined, while `ops` is still held.
    let meta_id = group.turns[0].id.clone();
    let ended = loop {
        if let Some(t) = h.turns().terminated(&meta_id) {
            break t;
        }
        std::thread::yield_now();
    };
    assert_eq!(ended.state, AgentTurnState::Delivered);
    let advisors = agent_comments(&h)[0]
        .2
        .iter()
        .find_map(|a| match a {
            comments::Attachment::Advisors { advisors } => Some(advisors.clone()),
            _ => None,
        })
        .expect("advisors");
    let ops = advisors.iter().find(|a| a.handle == "ops").expect("ops listed");
    assert_eq!(ops.state, "skipped");
    assert_eq!(ops.reason.as_deref(), Some("timed_out"));
    assert_eq!(advisors.iter().filter(|a| a.state == "finished").count(), 2);
    let skipped = terminal_of(&h, "ops");
    assert_eq!(skipped[0].state, AgentTurnState::Cancelled);
    assert_eq!(skipped[0].group.as_ref().and_then(|g| g.skipped), Some(SkipReason::TimedOut));
    hold.open();
    h.settle();
}

// AGC-FR-ANSK, AGC-FR-ZOCI, AGC-FR-HVQD: a failed advisor is skipped, and with no
// advice at all the call returns a result that says so and Meta answers alone.
#[test]
fn failed_advisors_are_skipped_and_meta_answers_alone() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Err("unreachable")
        } else {
            Ok(meta_asks_advisors(r, exchange, "Any risk?", text_reply(said("Meta answer."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 1);
    let advisors = comments[0]
        .2
        .iter()
        .find_map(|a| match a {
            comments::Attachment::Advisors { advisors } => Some(advisors.clone()),
            _ => None,
        })
        .expect("advisors");
    assert!(advisors.iter().all(|a| a.state == "skipped" && a.reason.as_deref() == Some("failed")));
    assert!(seam.exchanges().iter().any(|e| e.contains("No advice came")));
}

// AGC-FR-WPCZ, AGC-FR-ZOCI, AGC-FR-CZCU, AGC-FR-WZUL, CTA-FR-JPKE: the classifier picks the
// driver, the tagged agents answer in parallel, and the driver writes the
// Synthesis after the join.
#[test]
fn several_tags_are_answered_in_parallel_and_the_driver_writes_the_synthesis() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("@SEC".into())
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            Ok(said("The synthesis."))
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();

    assert_eq!(group.mode, "synthesis");
    assert_eq!(group.driver.as_deref(), Some("sec"));
    assert_eq!(group.turns.len(), 2);
    assert!(group.turns.iter().all(|t| t.group.as_ref().unwrap().role == GroupRole::Participant));

    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 3, "two replies and one Synthesis");
    let (handle, body, attachments) = comments.last().unwrap();
    assert_eq!(handle, "sec");
    assert!(body.contains("The synthesis."));
    assert!(attachments.contains(&comments::Attachment::Synthesis));
    assert!(comments[..2].iter().all(|(_, _, a)| a.is_empty()));

    // The classifier read the instructions of the tagged agents, and it was not
    // an agent turn: it carried no tool.
    let requests = seam.requests();
    let classifier = requests.iter().find(|r| r.instructions.contains(CLASSIFIER_MARK)).unwrap();
    assert!(classifier.tools.is_empty());
    let candidates = classifier.input.iter().find(|s| s.tag == "candidates").unwrap();
    assert!(candidates.body.contains("ARCH-PERSONA") && candidates.body.contains("SEC-PERSONA"));
    assert!(!candidates.body.contains("OPS-PERSONA"));
    // The synthesis turn sees the replies in the discussion.
    let synthesis = requests.iter().find(|r| r.instructions.contains(SYNTHESIS_MARK)).unwrap();
    let section = synthesis.input.iter().find(|s| s.tag == "synthesis").unwrap();
    assert!(section.body.contains("@arch replied:\nreply of ARCH"), "{}", section.body);
    assert!(section.body.contains("@sec replied:\nreply of SEC"), "{}", section.body);
    assert!(!section.body.contains("<spoken>"));
}

// AGC-FR-RMUQ: a classifier that cannot run picks no driver; the message follows
// the parallel flow of today and no Synthesis is posted.
#[test]
fn a_classifier_failure_falls_back_to_the_parallel_flow() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Err("unreachable")
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "regular");
    assert!(group.turns.iter().all(|t| t.group.is_none()));
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 2);
    assert!(comments.iter().all(|(_, _, a)| a.is_empty()));
}

// AGC-FR-RMUQ: a reply that names no tagged agent picks no driver either.
#[test]
fn a_classifier_naming_nobody_picks_no_driver() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("ops".into())
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(seam);
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();
    assert_eq!(group.mode, "regular");
    assert_eq!(agent_comments(&h).len(), 2);
}

// AGC-FR-ANSK, AGC-FR-ZOCI: a failed member is skipped and does not count; the
// Synthesis combines the replies that arrived.
#[test]
fn a_failed_member_is_skipped_and_the_synthesis_uses_the_rest() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("sec".into())
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            Ok(said("The synthesis."))
        } else if persona_of(r) == "ARCH" {
            Err("unreachable")
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();
    let failed = terminal_of(&h, "arch");
    assert_eq!(failed[0].state, AgentTurnState::Failed);
    assert_eq!(failed[0].group.as_ref().and_then(|g| g.skipped), Some(SkipReason::Failed));
    let comments = agent_comments(&h);
    assert_eq!(comments.len(), 2, "one reply and the Synthesis");
    let synthesis =
        seam.requests().into_iter().find(|r| r.instructions.contains(SYNTHESIS_MARK)).unwrap();
    assert!(synthesis
        .input
        .iter()
        .any(|s| s.tag == "synthesis" && s.body.contains("@arch: skipped (failed)")));
}

// AGC-FR-WZUL, AGC-FR-ANSK: when no member finished there is nothing to combine.
#[test]
fn no_synthesis_is_posted_when_no_agent_finished() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("sec".into())
        } else {
            Err("unreachable")
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.settle();
    assert!(agent_comments(&h).is_empty());
    assert!(seam.requests().iter().all(|r| !r.instructions.contains(SYNTHESIS_MARK)));
}

// AGC-FR-BIPE: a tool call that an advisor makes is recorded on the Meta turn
// under the advisor's name.
#[test]
fn an_advisor_tool_call_is_recorded_on_the_meta_turn_with_its_name() {
    let seam = RoutedSeam::new(|_| Ok(String::new()));
    let (h, thread) = group_harness(seam);
    let turns = h.turns();
    let origin = ConversationOrigin::of(&thread);
    let agent =
        |nick: &str| agents::Agent { id: nick.into(), nickname: nick.into(), ..Default::default() };
    let (meta, _) =
        turns.register(&agent("arch"), origin.clone(), Some("c1"), PROJECT_KEY, None, None);
    let (advisor, _) = turns.register(&agent("sec"), origin, Some("c1"), PROJECT_KEY, None, None);
    let state = Arc::new(crate::agent_groups::GroupState::new(
        "group-t",
        crate::agent_groups::GroupMode::Advisor,
    ));
    turns.attach_group(&meta.id, state.clone(), GroupRole::Meta, None);
    turns.attach_group(
        &advisor.id,
        state,
        GroupRole::Advisor,
        Some((meta.id.clone(), "sec".into())),
    );

    let handle = h.app.handle().clone();
    let (call, _) = turns
        .begin_tool_call(&handle, &advisor.id, "search_drafts", Some("q".into()))
        .expect("call");
    let on_meta = turns.in_flight(None).into_iter().find(|t| t.id == meta.id).expect("meta");
    assert_eq!(on_meta.active_tool_calls.len(), 1);
    assert_eq!(on_meta.active_tool_calls[0].advisor.as_deref(), Some("sec"));
    assert_eq!(on_meta.tool_calls[0].advisor.as_deref(), Some("sec"));
    assert_eq!(on_meta.tool_calls[0].tool, "search_drafts");
    // The advisor's own record names no advisor.
    let own = turns.in_flight(None).into_iter().find(|t| t.id == advisor.id).expect("advisor");
    assert_eq!(own.tool_calls[0].advisor, None);

    turns.finish_tool_call(&handle, &advisor.id, &call, ToolCallState::Done);
    let on_meta = turns.in_flight(None).into_iter().find(|t| t.id == meta.id).expect("meta");
    assert!(on_meta.active_tool_calls.is_empty());
    assert_eq!(on_meta.tool_calls[0].state, ToolCallState::Done);
}

pub(super) fn question_set_args(question: &str) -> serde_json::Value {
    serde_json::json!({ "questions": [{
        "question": question,
        "options": [
            { "label": "yes", "description": "Go on." },
            { "label": "no", "description": "Stop." },
        ],
    }] })
}

// AGC-FR-DIZM, ADQ-FR-CXVH, ADQ-FR-HNAV, DQA-FR-WNCM: an addressed agent of a
// synthesis group that asks questions records no set. The group holds the
// questions, and the driver's synthesis turn records one set.
#[test]
fn the_questions_of_the_agents_are_merged_into_one_set_by_the_driver() {
    let seam = RoutedSeam::replying(|r, exchange| {
        let first_call = exchange <= 1;
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok(text_reply("sec".into()))
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            let held = r
                .input
                .iter()
                .any(|s| s.tag == "group_questions" && s.body.contains("Does arch ask?"));
            if first_call && held {
                Ok(tool_reply_with_text(
                    crate::tools::ask_discussion_questions::NAME,
                    question_set_args("One merged question?"),
                    said("The synthesis."),
                ))
            } else {
                Ok(text_reply(said("The synthesis.")))
            }
        } else if persona_of(r) == "ARCH" && first_call {
            Ok(tool_reply(
                crate::tools::ask_discussion_questions::NAME,
                question_set_args("Does arch ask?"),
            ))
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
    let group = dispatch_group_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        &["arch".to_string(), "sec".to_string()],
        ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    )
    .expect("dispatch");
    h.join_turns();
    assert_eq!(group.mode, "synthesis");

    // The agent that asked recorded nothing of its own: its turn ended awaiting a
    // reply, and the only set is the driver's, with the merged question.
    let arch = h.terminal.lock().unwrap().iter().find(|t| t.nickname == "arch").cloned().unwrap();
    assert_eq!(arch.state, AgentTurnState::AwaitingReply);
    let set =
        crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).expect("one set");
    assert_eq!(set.questions.len(), 1);
    assert_eq!(set.questions[0].text, "One merged question?");
    match &set.asked_by {
        Participant::Agent { handle, .. } => assert_eq!(handle, "sec"),
        other => panic!("the set is the driver's: {other:?}"),
    }
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    assert!(folded
        .comments
        .iter()
        .all(|c| !matches!(&c.author, Participant::Agent { handle, .. } if handle == "arch")));
    // AGC-FR-WZUL: the driver posts its Synthesis beside the merged set.
    let synthesis: Vec<_> = folded
        .comments
        .iter()
        .filter(|c| c.attachments.contains(&comments::Attachment::Synthesis))
        .collect();
    assert_eq!(synthesis.len(), 1, "one Synthesis comment");
    assert!(synthesis[0].body.contains("The synthesis."));
}

pub(super) fn proposal_args(content: &str) -> serde_json::Value {
    serde_json::json!({
        "path": "artifact-window.md",
        "rationale": said("A reason."),
        "hunks": [{ "kind": "replace", "before": "Loose notes.", "after": content }],
    })
}

// AGC-FR-DIZM, DCP-FR-ATAH, DCR-FR-LKGK: an addressed agent of a synthesis group
// that proposes a change records no proposal. The group holds the offer, and the
// driver's synthesis turn records the one proposal the author reviews.
#[test]
fn the_proposals_of_the_agents_are_merged_into_one_proposal_by_the_driver() {
    let seam = RoutedSeam::replying(|r, exchange| {
        let first_call = exchange <= 1;
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok(text_reply("sec".into()))
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            let held = r
                .input
                .iter()
                .any(|s| s.tag == "group_proposals" && s.body.contains("Arch rewrite."));
            if first_call && held {
                Ok(tool_reply_with_text(
                    crate::tools::propose_draft_changes::NAME,
                    proposal_args("Merged rewrite."),
                    said("The synthesis."),
                ))
            } else {
                Ok(text_reply(said("The synthesis.")))
            }
        } else if persona_of(r) == "ARCH" && first_call {
            Ok(tool_reply(
                crate::tools::propose_draft_changes::NAME,
                proposal_args("Arch rewrite."),
            ))
        } else {
            Ok(text_reply(said(format!("reply of {}", persona_of(r)))))
        }
    });
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    let (draft_id, thread) = seed_discussion(&h, &["@arch @sec what would you change?"]);
    switch(&h, true, 60);
    dispatch_group_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        &["arch".to_string(), "sec".to_string()],
        ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    )
    .expect("dispatch");
    h.join_turns();

    let arch = h.terminal.lock().unwrap().iter().find(|t| t.nickname == "arch").cloned().unwrap();
    assert_eq!(arch.state, AgentTurnState::AwaitingReply);
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    let proposers: Vec<String> = folded
        .comments
        .iter()
        .filter(|c| {
            c.attachments.iter().any(|a| matches!(a, comments::Attachment::Proposal { .. }))
        })
        .filter_map(|c| match &c.author {
            Participant::Agent { handle, .. } => Some(handle.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(proposers, vec!["sec".to_string()], "one proposal, and it is the driver's");
    // AGC-FR-WZUL: the driver posts its Synthesis beside the merged proposal.
    let synthesis: Vec<_> = folded
        .comments
        .iter()
        .filter(|c| c.attachments.contains(&comments::Attachment::Synthesis))
        .collect();
    assert_eq!(synthesis.len(), 1, "one Synthesis comment");
    assert!(synthesis[0].body.contains("The synthesis."));
}
