//! Steering a running turn, the tool calls a turn keeps, and the deadline it
//! carries (AGC-FR-ISKZ, AGC-FR-CEKN, AGC-FR-HMUV, AGC-FR-URXV, CVL-FR-EZKC).
//!
//! Each scenario holds the model call in a gate the test opens, so a steering
//! message is posted while the turn is known to be inside a call. Nothing here
//! waits on time.

use super::*;

/// A seam that holds every call until the test lets one through, and then
/// answers it from the scripted queue.
struct GatedScript {
    inner: Arc<ScriptedCompletion>,
    gate: Arc<Gate>,
}

#[derive(Default)]
struct Gate {
    /// How many calls have entered the seam, and how many may leave it.
    counts: Mutex<(usize, usize)>,
    changed: Condvar,
}

impl Gate {
    fn wait_entered(&self, calls: usize) {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        while counts.0 < calls {
            counts = self.changed.wait(counts).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn release(&self) {
        self.counts.lock().unwrap_or_else(|e| e.into_inner()).1 += 1;
        self.changed.notify_all();
    }
}

impl CompletionSeam for GatedScript {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        {
            let mut counts = self.gate.counts.lock().unwrap_or_else(|e| e.into_inner());
            counts.0 += 1;
            self.gate.changed.notify_all();
            while counts.1 == 0 {
                counts = self.gate.changed.wait(counts).unwrap_or_else(|e| e.into_inner());
            }
            counts.1 -= 1;
        }
        self.inner.complete(request, exchange, endpoint, timeout)
    }
}

fn gated(
    replies: Vec<Result<ScriptedReply, &'static str>>,
    concurrency: usize,
) -> (Harness, Arc<Gate>) {
    let inner = Arc::new(ScriptedCompletion::scripted(replies));
    let gate = Arc::new(Gate::default());
    let seam = GatedScript { inner: inner.clone(), gate: gate.clone() };
    let h = Harness::build_with(
        Box::new(seam),
        inner,
        concurrency,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
    );
    h.mount();
    h.create_agent("arch", "");
    (h, gate)
}

/// Append a human comment to the seeded draft discussion, and answer its id.
fn post(h: &Harness, draft_id: &str, thread: &Discussion, body: &str) -> String {
    let after = crate::comments::add_comment_to(
        &h.root(),
        crate::comments::ThreadRef::discussion(draft_id),
        &thread.id,
        body.to_string(),
        Vec::new(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:09:00Z" },
    )
    .expect("reply");
    after.comments.last().expect("the reply").id.clone()
}

fn user_texts(exchange: &[rig::completion::Message]) -> Vec<String> {
    exchange.iter().map(rig_user_text).filter(|t| !t.is_empty()).collect()
}

fn delivered_bodies(h: &Harness, draft_id: &str, thread_id: &str) -> Vec<String> {
    let target = crate::comments::DiscussionTarget::Draft { draft_id: draft_id.to_string() };
    crate::comments::list_discussions_in(&h.root(), &target)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("thread")
        .comments
        .iter()
        .filter(|c| matches!(c.author, Participant::Agent { .. }))
        .map(|c| c.body.clone())
        .collect()
}

// AGC-FR-ISKZ, AGC-FR-AUGL, CVL-FR-EZKC, CVL-FR-QPEL, CVL-FR-XXKL: a comment
// for the agent of a running turn goes into that turn. The reply composed
// before it is set aside, the next call reads the comment after a fixed line,
// and the turn delivers one answer.
#[test]
fn a_comment_for_a_running_turn_steers_it_before_its_next_call() {
    let (h, gate) = gated(
        vec![
            Ok(ScriptedReply::answer("The answer to the first question.")),
            Ok(ScriptedReply::answer("The answer that takes the new comment in.")),
        ],
        DEFAULT_CONCURRENCY,
    );
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);

    let steer_id = post(&h, &draft_id, &thread, "Focus on the security part, please.");
    let steered = h.dispatch("arch", origin.clone(), &steer_id).expect("steer");
    assert_eq!(steered.id, turn.id, "the running turn took the comment");
    assert_eq!(steered.state, AgentTurnState::Running);
    assert_eq!(h.turns().in_flight(Some(&origin)).len(), 1, "no new turn was registered");

    gate.release();
    gate.wait_entered(2);
    gate.release();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    h.settle();

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 2, "one further call for the steering message");
    assert!(
        !user_texts(&exchanges[0]).iter().any(|t| t.contains("security part")),
        "the first call was made before the comment",
    );
    let second = user_texts(&exchanges[1]);
    let steering = second.last().expect("a steering message");
    assert!(steering.starts_with(STEERING_PREFACE), "the fixed line opens it: {steering}");
    assert!(steering.contains("Focus on the security part, please."));
    assert_eq!(
        delivered_bodies(&h, &draft_id, &thread.id),
        vec!["The answer that takes the new comment in.".to_string()],
        "the reply composed before the comment is not delivered",
    );
}

// CVL-FR-SZDM, AGC-FR-URXV: the deadline is null while the turn waits for its
// slot, set when it takes the slot, and set again from the instant the turn
// takes a steering message. Each change is an event.
#[test]
fn a_steering_message_restarts_the_whole_turn_deadline() {
    let (h, gate) = gated(
        vec![Ok(ScriptedReply::answer("First.")), Ok(ScriptedReply::answer("Second."))],
        DEFAULT_CONCURRENCY,
    );
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    assert!(turn.deadline_at.is_none(), "no deadline before the slot");
    gate.wait_entered(1);

    let started = h.turns().in_flight(Some(&origin))[0].clone();
    let first_start = started.deadline_started_at.clone().expect("the slot started it");
    let first_end = started.deadline_at.clone().expect("the slot started it");
    assert!(first_end > first_start);

    // The stamps carry milliseconds, so a wait for the clock to pass the first
    // stamp is a wait on a state rather than on a fixed time.
    while crate::notes::now_rfc3339_millis() <= first_start {
        std::thread::yield_now();
    }
    let steer_id = post(&h, &draft_id, &thread, "One more thing.");
    let steered = h.dispatch("arch", origin, &steer_id).expect("steer");
    let restarted_start = steered.deadline_started_at.clone().expect("still set");
    let restarted_end = steered.deadline_at.clone().expect("still set");
    assert!(restarted_start > first_start, "restarted from the instant it was taken");
    assert!(restarted_end > first_end, "the full timeout again");

    let deadline_events: Vec<AgentTurn> = h
        .deadline_events
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|t| t.id == turn.id)
        .cloned()
        .collect();
    assert_eq!(deadline_events.len(), 2, "the slot taken and the steering message taken");
    assert_eq!(deadline_events[1].deadline_at.as_deref(), Some(restarted_end.as_str()));

    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_terminal(&h, &turn.id);
    h.settle();
}

// CVL-FR-EZKC, AGC-FR-URXV: a turn that waits for its slot takes a steering
// message and reads it before its first call, and carries no deadline while it
// waits.
#[test]
fn a_turn_waiting_for_its_slot_reads_a_steering_message_before_its_first_call() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::answer("Holder.")), Ok(ScriptedReply::answer("Waiter."))], 1);
    h.create_agent("sec", "");
    let (draft_id, thread) = seed_discussion(&h, &["@arch @sec what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let holder = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("holder");
    gate.wait_entered(1);
    let waiter = h.dispatch("sec", origin.clone(), &thread.comments[0].id).expect("waiter");

    let steer_id = post(&h, &draft_id, &thread, "@sec mind the tests.");
    let steered = h.dispatch("sec", origin, &steer_id).expect("steer");
    assert_eq!(steered.id, waiter.id, "the waiting turn took it");
    assert!(steered.deadline_at.is_none(), "a waiting turn carries no deadline");

    gate.release();
    wait_for_terminal(&h, &holder.id);
    gate.wait_entered(2);
    gate.release();
    wait_for_terminal(&h, &waiter.id);
    h.settle();

    let exchanges = h.seam.exchanges();
    let waiter_first = exchanges.last().expect("the waiter's call");
    assert!(
        user_texts(waiter_first).iter().any(|t| t.contains("mind the tests")),
        "the first call of the waiting turn carries the steering message",
    );
}

// AGC-FR-CEKN, CVL-FR-YNOE: a turn that ends on a tool call takes no further
// message, so what it took and did not read goes to one new turn, triggered by
// the newest of those comments. The author sees no refusal.
#[test]
fn a_steering_message_the_turn_could_not_read_goes_to_a_new_turn() {
    let (h, gate) = gated(
        vec![
            Ok(asks("Which part do you mean?")),
            Ok(ScriptedReply::answer("The answer of the new turn.")),
        ],
        DEFAULT_CONCURRENCY,
    );
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    let steer_id = post(&h, &draft_id, &thread, "The second part.");
    assert_eq!(h.dispatch("arch", origin.clone(), &steer_id).expect("steer").id, turn.id);

    gate.release();
    let ended = wait_for_terminal(&h, &turn.id);
    assert_eq!(ended.state, AgentTurnState::AwaitingReply);
    gate.wait_entered(2);
    let fallback = h
        .turns()
        .in_flight(Some(&origin))
        .into_iter()
        .find(|t| t.id != turn.id && t.state == AgentTurnState::Running)
        .expect("a new turn for the message");
    assert_eq!(fallback.trigger_comment_id, steer_id);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &fallback.id).state, AgentTurnState::Delivered);
    h.settle();
    let record = find(&all_records(), "steering messages went to a new turn");
    assert_eq!(field(&record, "newTurnId"), fallback.id);
}

// AGC-FR-YBVM: a cancelled turn discards the steering messages it held, and
// no new turn starts for them.
#[test]
fn a_cancelled_turn_discards_its_steering_messages() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("Never delivered."))], DEFAULT_CONCURRENCY);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    let steer_id = post(&h, &draft_id, &thread, "Actually, stop.");
    assert_eq!(h.dispatch("arch", origin.clone(), &steer_id).expect("steer").id, turn.id);

    let handle = h.app.handle().clone();
    cancel_impl(&handle, &h.turns(), &h.progress(), &turn.id).expect("cancel");
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Cancelled);
    h.settle();
    assert_eq!(h.seam.exchanges().len(), 1, "no call was made for the message");
    assert!(h.turns().in_flight(Some(&origin)).is_empty(), "no new turn started");
}

// CVL-FR-ASBR: the loop records the comment ids it appended and the count,
// and never the body of a steering message.
#[test]
fn the_record_of_a_steering_message_names_the_comment_and_not_its_body() {
    let (h, gate) = gated(
        vec![Ok(ScriptedReply::answer("First.")), Ok(ScriptedReply::answer("Second."))],
        DEFAULT_CONCURRENCY,
    );
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    let steer_id = post(&h, &draft_id, &thread, "A confidential steering body.");
    h.dispatch("arch", origin, &steer_id).expect("steer");
    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_terminal(&h, &turn.id);
    h.settle();

    let records = records_where("turnId", &turn.id);
    let record = find(&records, "agent turn took steering messages");
    assert_eq!(field(&record, "commentIds"), steer_id);
    assert_eq!(field(&record, "appended"), "1");
    let text = format!("{records:?}");
    assert!(!text.contains("confidential steering body"), "no record carries the body");
}

// AGC-FR-HMUV, CVL-FR-SKSE, CVL-FR-DUUO: a turn keeps every call it made with
// its state and key input, and the terminal event carries them. A call of a
// tool that is not attached failed; a call that produced a result is done.
#[test]
fn a_turn_keeps_every_tool_call_with_its_state_and_key_input() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("search_skills", serde_json::json!({ "query": "review" }))
            .and_calls("no_such_tool", serde_json::json!({ "query": "x" }))),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(terminal.active_tool_calls.is_empty());
    let calls: Vec<(String, ToolCallState, Option<String>)> = terminal
        .tool_calls
        .iter()
        .map(|c| (c.tool.clone(), c.state, c.key_input.clone()))
        .collect();
    assert_eq!(
        calls,
        vec![
            ("search_skills".to_string(), ToolCallState::Done, Some("review".to_string())),
            ("no_such_tool".to_string(), ToolCallState::Failed, None),
        ],
    );
    assert_eq!(
        terminal.tool_calls.iter().map(|c| c.activation_seq).collect::<Vec<_>>(),
        vec![1, 2],
    );
}

// AGC-FR-SEZK, CVL-FR-IQZS: the key input reaches the turn record and no log
// record, and a recovery-registry entry holds no tool call.
#[test]
fn the_key_input_reaches_no_log_record_and_no_registry_entry() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "search_skills",
            serde_json::json!({ "query": "a distinctive search phrase" }),
        )),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(
        terminal.tool_calls[0].key_input.as_deref(),
        Some("a distinctive search phrase"),
        "the terminal event carries the key input",
    );
    // The loop's own records. A tool's own records follow TLC-FR-14.
    let records = records_where("turnId", &terminal.id);
    let begun = find(&records, "agent tool call begun");
    assert_eq!(field(&begun, "tool"), "search_skills");
    for message in ["agent tool call begun", "calling model", "agent turn dispatched"] {
        for record in records.iter().filter(|r| r.message == message) {
            assert!(
                !format!("{record:?}").contains("a distinctive search phrase"),
                "{message} carried the key input",
            );
        }
    }
    assert!(
        !format!("{begun:?}").contains("a distinctive search phrase"),
        "the begun record carries no key input",
    );
    let entries = h.turns().recoverable_failures(None);
    assert!(entries.iter().all(|e| e.tool_calls.is_empty()), "an entry holds no tool call");
}
