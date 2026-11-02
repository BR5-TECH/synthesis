//! Speech turns: the first chunk, the chunk messages, the held reply, and the
//! end of a speech (AGC-FR-SJQP .. AGC-FR-ISCZ, CVL-FR-VCYR .. CVL-FR-WHEY).
//!
//! Each scenario holds the model call in a gate that the test opens, and waits
//! for a published state. The idle limit reads a clock that the test moves.
//! Nothing here waits on time.

use super::*;

/// A seam that holds every call until the test lets one through, and then
/// answers it from the scripted queue.
struct GatedScript {
    inner: Arc<ScriptedCompletion>,
    gate: Arc<Gate>,
}

#[derive(Default)]
struct Gate {
    /// How many calls entered the seam, and how many can leave it.
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

fn gated(replies: Vec<Result<ScriptedReply, &'static str>>) -> (Harness, Arc<Gate>) {
    let inner = Arc::new(ScriptedCompletion::scripted(replies));
    let gate = Arc::new(Gate::default());
    let seam = GatedScript { inner: inner.clone(), gate: gate.clone() };
    let h = Harness::build_with(
        Box::new(seam),
        inner,
        DEFAULT_CONCURRENCY,
        Box::new(FixedSecrets),
        Duration::from_secs(30),
    );
    h.mount();
    h.create_agent("arch", "");
    (h, gate)
}

/// AGC-FR-YLJW: a clock that moves only when the test moves it.
struct ManualClock(Mutex<std::time::Instant>);

impl ManualClock {
    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

impl SpeechClock for ManualClock {
    fn now(&self) -> std::time::Instant {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn speak(
    h: &Harness,
    origin: &ConversationOrigin,
    speech_id: &str,
    text: &str,
) -> Result<AgentTurn, String> {
    let root = h.root();
    dispatch_speech_impl(
        &h.app.handle().clone(),
        Roots::same(&root),
        PROJECT_KEY,
        "arch",
        origin.clone(),
        speech_id,
        text,
        &human("ada"),
    )
}

fn steer(h: &Harness, speech_id: &str, text: &str) -> Vec<AgentTurn> {
    steer_speech_impl(&h.app.handle().clone(), speech_id, text)
}

fn end(h: &Harness, speech_id: &str, comment_id: Option<&str>) -> Vec<AgentTurn> {
    let root = h.root();
    end_speech_impl(&h.app.handle().clone(), Some(Roots::same(&root)), speech_id, comment_id)
}

/// Every event the harness captured, whichever sink holds it.
fn every_event(h: &Harness) -> Vec<AgentTurn> {
    [&h.events, &h.context_events, &h.deadline_events]
        .iter()
        .flat_map(|sink| sink.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .collect()
}

/// Wait for a published state of the turn, and return it.
fn wait_for_event(h: &Harness, turn_id: &str, what: impl Fn(&AgentTurn) -> bool) -> AgentTurn {
    for _ in 0..1000 {
        let found = every_event(h).into_iter().find(|t| t.id == turn_id && what(t));
        if let Some(turn) = found {
            return turn;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no such event for {turn_id}");
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

// AGC-FR-SJQP, AGC-FR-ANBX, AGC-FR-06, AGC-FR-13, AGC-FR-KXOJ, AGC-FR-TCDQ,
// AGC-FR-QYNZ, AGC-FR-ISCZ, CVL-FR-VCYR, CVL-FR-WLTP, CVL-FR-SZKK,
// CVL-FR-XXKL, CVL-FR-TVNM: a speech registers a turn whose
// `current_comment` holds the first chunk, a later chunk steers it, the
// reply is held until the speech ends, and the final message carries the
// posted comment. The turn delivers one answer.
#[test]
fn a_speech_registers_a_turn_that_holds_its_reply_until_the_speech_ends() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::answer("First.")),
        Ok(ScriptedReply::answer("Second.")),
        Ok(ScriptedReply::answer("Final.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-1", "Please look at the first").expect("dispatch");
    assert_eq!(turn.state, AgentTurnState::Running);
    assert_eq!(turn.trigger_comment_id, None, "a speech turn has no trigger comment");
    assert_eq!(turn.speech_id.as_deref(), Some("s-1"));
    assert!(!turn.retry_permitted);
    assert!(!turn.reply_held);
    gate.wait_entered(1);

    let steered = steer(&h, "s-1", "part and then the second part");
    assert_eq!(steered.iter().map(|t| t.id.clone()).collect::<Vec<_>>(), vec![turn.id.clone()]);
    gate.release();
    gate.wait_entered(2);
    gate.release();

    let held = wait_for_event(&h, &turn.id, |t| t.reply_held);
    assert_eq!(held.state, AgentTurnState::Running);
    assert!(held.deadline_started_at.is_none(), "no deadline while the reply is held");
    assert!(held.deadline_at.is_none(), "no deadline while the reply is held");
    assert_eq!(h.seam.call_count(), 2, "the held reply makes no call");

    let comment_id =
        post(&h, &draft_id, &thread, "Please look at the first part and then the second part.");
    let ended = end(&h, "s-1", Some(&comment_id));
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].id, turn.id);
    assert_eq!(ended[0].trigger_comment_id.as_deref(), Some(comment_id.as_str()));
    assert_eq!(ended[0].speech_id, None, "the turn is bound to no ended speech");

    let after = wait_for_event(&h, &turn.id, |t| {
        !t.reply_held && t.speech_id.is_none() && t.deadline_at.is_some()
    });
    assert!(after.deadline_started_at.is_some(), "the deadline restarts after the hold");
    gate.wait_entered(3);
    gate.release();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(terminal.trigger_comment_id.as_deref(), Some(comment_id.as_str()));
    h.settle();

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 3);
    let input = &user_texts(&exchanges[0])[0];
    assert!(input.contains("@arch what now?"), "the history holds the conversation");
    assert!(
        input.contains(&format!("{PARTIAL_SPEECH_LINE}\n\n@ada:\nPlease look at the first")),
        "current_comment holds the first chunk under the fixed line: {input}",
    );
    assert_eq!(
        user_texts(&exchanges[1]).last().expect("a chunk message"),
        &format!("{PARTIAL_SPEECH_LINE}\n\npart and then the second part"),
    );
    let last = user_texts(&exchanges[2]);
    let final_message = last.last().expect("the final message");
    assert!(final_message.starts_with(FULL_SPEECH_LINE), "{final_message}");
    assert!(final_message.contains("Please look at the first part and then the second part."));
    assert!(!final_message.contains(STEERING_PREFACE));
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Final.")]);
}

// CVL-FR-VCYR, CVL-FR-WLTP: the two fixed lines, byte for byte.
#[test]
fn the_speech_lines_are_the_fixed_lines_of_the_spec() {
    assert_eq!(
        PARTIAL_SPEECH_LINE,
        "Partial speech: the author is still speaking. This is a part of what the author says. \
         More text follows. Do not reply to this part alone.",
    );
    assert_eq!(
        FULL_SPEECH_LINE,
        "Full speech: the author stopped speaking. This comment is all that the author said. \
         It replaces the partial speech before it.",
    );
}

// AGC-FR-QYNZ, AGC-FR-ANBX: the wire shape that a surface reads.
#[test]
fn a_speech_turn_carries_its_speech_on_the_wire() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("Held."))]);
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-wire", "Hello").expect("dispatch");
    let wire = serde_json::to_value(&turn).expect("serialise");
    assert_eq!(wire["speechId"], serde_json::json!("s-wire"));
    assert_eq!(wire["replyHeld"], serde_json::json!(false));
    assert_eq!(wire["triggerCommentId"], serde_json::Value::Null);
    assert_eq!(wire["retryPermitted"], serde_json::json!(false));
    gate.wait_entered(1);
    end(&h, "s-wire", None);
    gate.release();
    h.settle();
}

// AGC-FR-KXOJ, CVL-FR-ASBR: a chunk is held in memory only. No file and no
// log record carries its text.
#[test]
fn a_chunk_is_never_written_to_disk_or_to_the_log() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::answer("First.")), Ok(ScriptedReply::answer("x"))]);
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn =
        speak(&h, &ConversationOrigin::of(&thread), "s-disk", "zebra quartz one").expect("speak");
    gate.wait_entered(1);
    steer(&h, "s-disk", "zebra quartz two");
    gate.release();
    gate.wait_entered(2);
    let records = format!("{:?}", records_where("turnId", &turn.id));
    let snapshot = tree_snapshot(h.root.path());
    end(&h, "s-disk", None);
    gate.release();
    h.settle();

    for (path, bytes) in snapshot {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("zebra quartz"), "{path} holds the text of a chunk");
    }
    assert!(!records.contains("zebra quartz"), "a log record holds the text of a chunk");
    let all = format!("{:?}", all_records());
    assert!(!all.contains("zebra quartz"), "a log record holds the text of a chunk");
    let appended = find(&records_where("turnId", &turn.id), "agent turn took steering messages");
    assert_eq!(field(&appended, "chunks"), "1");
}

// AGC-FR-JPPB, AGC-FR-KXOJ: a chunk registers no new turn. A turn that ends
// on a tool call drops the chunk that it did not read.
#[test]
fn a_chunk_never_registers_a_new_turn() {
    let (h, gate) = gated(vec![Ok(asks("Which part?"))]);
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-ask", "Look at").expect("speak");
    assert!(steer(&h, "s-unknown", "nothing").is_empty());
    gate.wait_entered(1);
    assert_eq!(steer(&h, "s-ask", "the second part").len(), 1);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::AwaitingReply);
    h.join_turns();
    let outstanding = h.turns().in_flight(Some(&origin));
    assert_eq!(outstanding.len(), 1, "only the awaiting turn: {outstanding:?}");
    assert_eq!(h.seam.call_count(), 1);
    assert!(steer(&h, "s-ask", "more").is_empty(), "a turn that ended takes no chunk");
}

// AGC-FR-NFYK, CVL-FR-SZKK: a speech with no comment cancels the turn it
// registered and drops its output.
#[test]
fn a_speech_with_no_comment_cancels_the_turn_it_registered() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("Never delivered."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-x", "Hmm").expect("speak");
    gate.wait_entered(1);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);

    let ended = end(&h, "s-x", None);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].state, AgentTurnState::Cancelled);
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Cancelled);
    h.settle();
    assert!(delivered_bodies(&h, &draft_id, &thread.id).is_empty(), "no output is delivered");
}

// AGC-FR-NFYK, CVL-FR-AEJO, AGC-FR-SJQP: a turn that ran before the first
// chunk takes it and keeps running when the speech ends with no comment. Its
// held reply delivers as it stands.
#[test]
fn an_earlier_turn_delivers_its_held_reply_when_the_speech_ends_with_no_comment() {
    let (h, gate) = gated(vec![
        Ok(ScriptedReply::answer("First.")),
        Ok(ScriptedReply::answer("Second, as it stands.")),
    ]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    let bound = speak(&h, &origin, "s-e", "also this").expect("speak");
    assert_eq!(bound.id, turn.id, "the running turn took the first chunk");
    assert_eq!(bound.speech_id.as_deref(), Some("s-e"));
    assert_eq!(h.turns().in_flight(Some(&origin)).len(), 1, "no new turn");
    gate.release();
    gate.wait_entered(2);
    gate.release();
    wait_for_event(&h, &turn.id, |t| t.reply_held);

    let ended = end(&h, "s-e", None);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].state, AgentTurnState::Running, "the earlier turn keeps running");
    assert_eq!(ended[0].speech_id, None);
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();
    assert_eq!(h.seam.call_count(), 2);
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("Second, as it stands.")],);
}

// AGC-FR-NFYK: at the end of a speech with no comment, an earlier turn drops
// the chunks that the loop did not append yet.
#[test]
fn an_earlier_turn_drops_the_chunks_it_did_not_read() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("First."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = h.dispatch("arch", origin.clone(), &thread.comments[0].id).expect("dispatch");
    gate.wait_entered(1);
    speak(&h, &origin, "s-d", "chunk alpha").expect("speak");
    steer(&h, "s-d", "chunk beta");
    end(&h, "s-d", None);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();
    assert_eq!(h.seam.call_count(), 1, "no call for a dropped chunk");
    assert_eq!(delivered_bodies(&h, &draft_id, &thread.id), vec![said("First.")]);
}

// AGC-FR-FPYB, AGC-FR-TCDQ, CVL-FR-WLTP: at the end of a speech with a
// comment, the final message replaces the chunks the loop did not append.
#[test]
fn the_final_message_replaces_the_chunks_not_yet_appended() {
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::answer("First.")), Ok(ScriptedReply::answer("Final."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-f", "Start").expect("speak");
    gate.wait_entered(1);
    steer(&h, "s-f", "chunk gamma");
    let comment_id = post(&h, &draft_id, &thread, "Start and then the whole request.");
    assert_eq!(end(&h, "s-f", Some(&comment_id)).len(), 1);
    gate.release();
    gate.wait_entered(2);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    h.settle();
    let second = user_texts(&h.seam.exchanges()[1]);
    assert!(!second.iter().any(|t| t.contains("chunk gamma")), "the chunk is dropped");
    assert!(second.last().expect("final").starts_with(FULL_SPEECH_LINE));
}

// AGC-FR-VILT, AGC-FR-TCDQ: an agent whose bound turn takes no further
// steering message gets a new turn with the comment as its trigger.
#[test]
fn an_agent_whose_turn_ended_gets_a_new_turn_for_the_final_comment() {
    let (h, gate) =
        gated(vec![Ok(asks("Which part?")), Ok(ScriptedReply::answer("The new answer."))]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-v", "Look").expect("speak");
    gate.wait_entered(1);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::AwaitingReply);
    let comment_id = post(&h, &draft_id, &thread, "Look at the second part.");
    let ended = end(&h, "s-v", Some(&comment_id));
    assert_eq!(ended.len(), 1);
    assert_ne!(ended[0].id, turn.id, "a new turn");
    assert_eq!(ended[0].trigger_comment_id.as_deref(), Some(comment_id.as_str()));
    assert_eq!(ended[0].speech_id, None);
    gate.wait_entered(2);
    gate.release();
    assert_eq!(wait_for_terminal(&h, &ended[0].id).state, AgentTurnState::Delivered);
    h.settle();
}

// AGC-FR-TCXX, AGC-FR-BMLY: a speech ends once. A second end and a later
// chunk change nothing, and a dispatch for the ended speech is refused with
// `speech_ended` and registers nothing.
#[test]
fn a_speech_ends_once_and_refuses_a_later_dispatch() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("Held."))]);
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    speak(&h, &origin, "s-once", "Hi").expect("speak");
    assert!(end(&h, "s-unknown", None).is_empty(), "an unknown speech changes nothing");
    assert_eq!(end(&h, "s-once", None).len(), 1);
    assert!(end(&h, "s-once", None).is_empty(), "a speech ends once");
    assert!(steer(&h, "s-once", "more").is_empty());
    gate.release();
    h.join_turns();

    assert_eq!(speak(&h, &origin, "s-once", "again"), Err(FAIL_SPEECH_ENDED.to_string()));
    assert!(h.turns().in_flight(Some(&origin)).is_empty(), "the refusal registered nothing");
}

// AGC-FR-YLJW, AGC-FR-NFYK: a speech that gets no command for 120 seconds
// ends as a speech with no comment ends.
#[test]
fn an_idle_speech_ends_after_two_minutes() {
    let (h, gate) = gated(vec![Ok(ScriptedReply::answer("Held."))]);
    let clock = Arc::new(ManualClock(Mutex::new(std::time::Instant::now())));
    h.turns().set_speech_clock(clock.clone());
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let origin = ConversationOrigin::of(&thread);
    let turn = speak(&h, &origin, "s-idle", "Hi").expect("speak");
    gate.wait_entered(1);

    clock.advance(Duration::from_secs(119));
    expire_idle_speeches(&h.app.handle().clone());
    assert_eq!(steer(&h, "s-idle", "still here").len(), 1, "a chunk keeps the speech open");
    clock.advance(Duration::from_secs(119));
    expire_idle_speeches(&h.app.handle().clone());
    assert_eq!(h.turns().in_flight(Some(&origin)).len(), 1, "not idle for the limit yet");

    clock.advance(Duration::from_secs(1));
    expire_idle_speeches(&h.app.handle().clone());
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Cancelled);
    gate.release();
    h.settle();
    assert_eq!(speak(&h, &origin, "s-idle", "late"), Err(FAIL_SPEECH_ENDED.to_string()));
}

// CVL-FR-WHEY: the reply of the last call under the logical bound is not
// held, because the turn takes no further steering message.
#[test]
fn the_reply_of_the_last_call_under_the_bound_is_not_held() {
    let mut replies: Vec<Result<ScriptedReply, &'static str>> = (1..MAX_MODEL_CALLS)
        .map(|_| Ok(ScriptedReply::calls("search_skills", serde_json::json!({ "query": "x" }))))
        .collect();
    replies.push(Ok(ScriptedReply::answer("At the bound.")));
    let h = Harness::scripted(replies);
    h.mount();
    h.create_agent("arch", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-bound", "Search").expect("speak");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered, "the open speech held nothing");
    assert!(!every_event(&h).iter().any(|t| t.reply_held));
    h.settle();
    end(&h, "s-bound", None);
}

// AGC-FR-ANBX: a turn that a speech registered is not entered in the
// recovery registry before its speech ends with a comment.
#[test]
fn a_failed_speech_turn_offers_no_retry() {
    let h = Harness::scripted((0..6).map(|_| Err(FAIL_UNREACHABLE)).collect());
    h.mount();
    h.create_agent("arch", "");
    let (_draft_id, thread) = seed_discussion(&h, &["@arch what now?"]);
    let turn = speak(&h, &ConversationOrigin::of(&thread), "s-fail", "Hi").expect("speak");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_UNREACHABLE));
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    h.settle();
    end(&h, "s-fail", None);
}
