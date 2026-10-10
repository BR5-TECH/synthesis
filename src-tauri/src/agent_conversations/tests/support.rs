//! What the scenarios of this module share besides the harness itself: the
//! waits, the seeds, the readers of the log, and the scripted seams a scenario
//! hands to `Harness`.
//!
//! Apart from `mod.rs` because the harness is one thing to understand and these
//! are another; `mod.rs` re-exports them, so a scenario reaches them through
//! `use super::*` exactly as it reaches the harness.

use super::*;

/// Wait until the number of open agent sessions settles at `expected`.
///
/// The turn thread closes its session on the way out (CVL-FR-24), and on the
/// cancellation path that happens *after* the registry has already reported the
/// turn terminated — so a test asserting on it has to poll rather than read once.
pub(super) fn wait_for_sessions(h: &Harness, expected: usize) {
    for _ in 0..600 {
        if h.agent_sessions() == expected {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "agent sessions settled at {} rather than {expected}",
        h.agent_sessions(),
    );
}

pub(super) fn human(login: &str) -> Participant {
    Participant::Human {
        login: login.into(),
        display_name: None,
        email: None,
    }
}

/// A prober that panics if a probe is attempted. Nothing in this module probes
/// an endpoint, and a test that made one would be a test reaching the network.
pub(super) struct NoProbe;

impl crate::ai_shared::EndpointProber for NoProbe {
    fn probe(
        &self,
        _request: &crate::ai_shared::ProbeRequest,
    ) -> Result<Vec<ModelOption>, crate::ai_shared::ProbeError> {
        panic!("no test here may probe an endpoint (CVL-FR-11)");
    }
}

/// A keychain stand-in that always holds a key, so the endpoint resolves and the
/// interesting failures are the model's rather than the store's.
pub(super) struct FixedSecrets;

impl crate::ai_shared::SecretStore for FixedSecrets {
    fn get(&self, _id: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        Ok(Some("sk-secret-value".into()))
    }
    fn set(&self, _id: &str, _secret: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
    fn delete(&self, _id: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
    fn has(&self, _id: &str) -> Result<bool, crate::ai_shared::SecretUnavailable> {
        Ok(true)
    }
}

/// A credential store that will not answer at all — the keychain locked, or the
/// OS refusing access (AAP-FR-20).
pub(super) struct LockedSecrets;

impl crate::ai_shared::SecretStore for LockedSecrets {
    fn get(&self, _id: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("locked".into()))
    }
    fn set(&self, _id: &str, _secret: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("locked".into()))
    }
    fn delete(&self, _id: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("locked".into()))
    }
    /// Presence is what listing reads, and a store that will not answer has to
    /// say so rather than claim absence.
    fn has(&self, _id: &str) -> Result<bool, crate::ai_shared::SecretUnavailable> {
        Err(crate::ai_shared::SecretUnavailable("locked".into()))
    }
}

/// Wait for a turn's terminal event and return the turn it carried.
///
/// Polls the capture the harness has been holding since construction, rather
/// than subscribing now: `listen` does not replay, and a turn that terminates
/// before a late subscription would strand the test on a timeout that says
/// nothing about the code.
pub(super) fn wait_for_terminal(h: &Harness, turn_id: &str) -> AgentTurn {
    for _ in 0..1000 {
        if let Some(turn) = h
            .terminal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|t| t.id == turn_id)
            .cloned()
        {
            return turn;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Named, rather than the bare "no terminal event": the turn that did not end
    // is a thread this test cannot see, so the message carries the two things
    // that tell the three cases apart. An empty terminal set with this turn
    // among the events means it registered and stalled; an empty set with no
    // event for it at all means it never started; other turns' terminal events
    // beside it mean this one alone is stuck rather than the whole harness.
    panic!(
        "no terminal event for {turn_id}; terminal events seen: {:?}; \
         states published for {turn_id}: {:?}",
        h.terminal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|t| (t.id.clone(), t.state))
            .collect::<Vec<_>>(),
        h.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|t| t.id == turn_id)
            .map(|t| t.state)
            .collect::<Vec<_>>(),
    );
}


/// A draft holding its one prompt, and a discussion on it carrying `earlier`
/// before the comment that addresses the agent.
pub(super) fn seed_discussion(h: &Harness, earlier: &[&str]) -> (String, Discussion) {
    let created =
        crate::drafts::create_draft_at_root(&h.root(), Some("artifact-window")).expect("draft");
    let draft_id = created.draft.id.clone();
    crate::drafts::save_draft_file_impl(&h.root(), &draft_id, &created.file, "Loose notes.")
        .expect("write");

    let mut thread = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Draft {
            draft_id: draft_id.clone(),
        },
        None,
        earlier
            .first()
            .copied()
            .unwrap_or("@arch what now?")
            .to_string(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    for (i, body) in earlier.iter().skip(1).enumerate() {
        thread = crate::comments::add_comment_to(
            &h.root(),
            crate::comments::ThreadRef::discussion(&draft_id),
            &thread.id,
            (*body).to_string(),
            Vec::new(),
            Vec::new(),
            &human("ada"),
            &format!("2026-01-01T00:0{}:00Z", i + 1),
        )
        .expect("reply");
    }
    (draft_id, thread)
}


/// The section a tag names, or a failure naming the tag that was missing.
pub(super) fn section_of<'a>(sections: &'a [InputSection], tag: &str) -> &'a InputSection {
    sections
        .iter()
        .find(|s| s.tag == tag)
        .unwrap_or_else(|| panic!("no <{tag}> section among {:?}", tags_of(sections)))
}


/// Dispatch one turn against a seeded artifact thread and wait for it to end.
pub(super) fn run_one(h: &Harness, nickname: &str) -> (AgentTurn, Discussion) {
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            nickname,
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let terminal = wait_for_terminal(h, &turn.id);
    (terminal, thread)
}


/// The thread as it now stands on disk.
pub(super) fn folded(h: &Harness, artifact_id: &str, thread_id: &str) -> Discussion {
    crate::comments::list_fragment_discussions_in(&h.root(), artifact_id)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("thread")
}


pub(super) fn all_records() -> Vec<LogRecord> {
    TEST_BUFFER
        .query(&LogFilter::default(), None, logging::BUFFER_CAPACITY)
        .expect("the buffer answers a default filter")
        .records
}


/// An endpoint to assemble a completion request against. Which provider it
/// names is irrelevant to the preamble, which is the point of CVL-FR-10.
pub(super) fn any_endpoint() -> AiApiCall {
    AiApiCall {
        turn_timeout_ms: None,
        provider: "openrouter".into(),
        base_url: "https://example/v1".into(),
        api_key: Some("k".into()),
        model_id: Some("m".into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    }
}


/// CVL-FR-08: the tenth on an artifact origin, and never offered beside the one
/// above.
pub(super) const ARTIFACT_ONLY_TOOL: &str = "propose_prompt_changes";


/// A reply that asks the author something.
pub(super) fn asks(question: &str) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::ask_user_comment::NAME,
        serde_json::json!({ "question": question }),
    )
}


/// CVL-FR-08: the tenth on a draft origin.
///
/// `search_drafts`, `read_draft`, and `search_notes` are deliberately *not*
/// here: reading what the project has planned and what its author has noted are
/// moves every origin has, and only offering a rewrite is bound to one.
pub(super) const DRAFT_ONLY_TOOL: &str = "propose_draft_changes";


/// The nine every turn gets, in the order `conversation_tools` names them.
///
/// `search_notes` is among them on every origin kind, which is
/// `../../specifications/tools/NST-note-search-tool.md` NST-FR-01, CVL-FR-08's claim as
/// well as CVL-FR-30, CVL-FR-08's: every test below that sweeps the five kinds sweeps it.


/// One field as text, or empty where the record does not carry it.
///
/// For *filtering* only: a record that carries no `turnId` at all is not this
/// turn's, which is what the empty string expresses. Assertions go through
/// [`require`] instead, so `assert_eq!(field(r, k), "")` can never quietly pass
/// against a field that was never emitted.
pub(super) fn field(record: &LogRecord, key: &str) -> String {
    match record.fields.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}


/// The newest record carrying `message`, cloned so a caller can filter and read
/// in one expression.
///
/// Newest rather than first: a test that provokes the same message twice — three
/// refusals on one thread, say — means the one it just caused.
pub(super) fn find(records: &[LogRecord], message: &str) -> LogRecord {
    records
        .iter()
        .rev()
        .find(|r| r.message == message)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no record reads {message:?}; the set holds {:?}",
                records.iter().map(|r| r.message.as_str()).collect::<Vec<_>>()
            )
        })
}


/// Every message of an exchange that carries a provider-native entry type
/// anywhere in it, however it was shaped.
pub(super) fn mentions_a_provider_entry(exchange: &[rig::completion::Message]) -> bool {
    exchange.iter().any(|message| {
        serde_json::to_string(message)
            .expect("serialisable")
            .contains("openrouter:web_")
    })
}


/// Every provider-run call and result an exchange holds, in order — as
/// `(name, arguments)` for a call and `(id, result)` for a result.
///
/// After CVL-FR-31 the answer is always none: OpenRouter runs its own tool loop
/// **inside** one model call and gives the model every result before the reply
/// is composed, so nothing of it is replayed into a later request. The helper
/// exists to assert exactly that, and it looks for the shape the loop used to
/// build — a function call named for a provider entry type — because that is the
/// shape a provider refuses.
pub(super) fn native_traffic(
    exchange: &[rig::completion::Message],
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    use rig::completion::message::{AssistantContent, ToolResultContent, UserContent};
    let mut calls = Vec::new();
    let mut results = Vec::new();
    let mut native_ids: Vec<String> = Vec::new();
    for message in exchange {
        if let rig::completion::Message::Assistant(rig::completion::message::AssistantMessage { content, .. }) = message {
            for part in content.iter() {
                if let AssistantContent::ToolCall(call) = part {
                    if call.function.name.starts_with("openrouter:") {
                        native_ids.push(call.id.to_string());
                    }
                }
            }
        }
    }
    for message in exchange {
        match message {
            rig::completion::Message::Assistant(rig::completion::message::AssistantMessage { content, .. }) => {
                for part in content.iter() {
                    if let AssistantContent::ToolCall(call) = part {
                        if call.function.name.starts_with("openrouter:") {
                            calls.push((
                                call.function.name.to_string(),
                                call.function.arguments_value().to_string(),
                            ));
                        }
                    }
                }
            }
            rig::completion::Message::User { content } => {
                for part in content.iter() {
                    if let UserContent::ToolResult(result) = part {
                        if native_ids.contains(&result.call.to_string()) {
                            let text: String = result
                                .content
                                .iter()
                                .filter_map(|part| match part {
                                    ToolResultContent::Text(text) => Some(text.text.clone()),
                                    _ => None,
                                })
                                .collect();
                            results.push((result.call.to_string(), text));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (calls, results)
}


/// A seam that panics, so the guard around the turn's body can be exercised.
pub(super) struct PanickingCompletion;

impl CompletionSeam for PanickingCompletion {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        panic!("a defect somewhere inside the loop");
    }
}


/// The records naming one tool the provider ran, for one turn, in order.
pub(super) fn provider_tool_records(turn_id: &str) -> Vec<logging::LogRecord> {
    all_records()
        .into_iter()
        .filter(|record| {
            field(record, "turnId") == turn_id
                && (record.message == "provider ran its own tool"
                    || record.message == "provider tool produced no result")
        })
        .collect()
}


pub(super) fn records_where(key: &str, value: &str) -> Vec<LogRecord> {
    all_records()
        .into_iter()
        .filter(|r| field(r, key) == value)
        .collect()
}


/// One field a record must carry, as text.
pub(super) fn require(record: &LogRecord, key: &str) -> String {
    assert!(
        record.fields.contains_key(key),
        "{:?} carries no {key:?}: {:?}",
        record.message,
        record.fields,
    );
    field(record, key)
}


/// A request a wire test carries: a compiled prompt and one input section.
/// The framework refuses a request whose only message holds no text, and a
/// real turn always carries its input.
pub(super) fn wire_request() -> AgentRequest {
    AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue.", ""),
        input: vec![InputSection {
            tag: TAG_CURRENT_COMMENT.into(),
            attributes: Vec::new(),
            body: "What do you think?".into(),
            truncated: false,
            parts: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    }
}

/// A tool result answering the call `id` to the tool `name`.
pub(super) fn tool_result_message(
    id: &str,
    name: &str,
    text: impl Into<String>,
) -> rig::completion::Message {
    rig::completion::Message::tool_result(
        rig::completion::message::CallId::from_wire(id),
        rig::completion::message::ToolName::new(name).expect("a tool name"),
        text,
    )
}

pub(super) fn rig_tool_call(
    id: &str,
    name: &str,
    arguments: serde_json::Value,
) -> rig::completion::message::ToolCall {
    rig::completion::message::ToolCall::from_wire(
        id,
        rig::completion::message::ToolFunction::new(
            rig::completion::message::ToolName::new(name).expect("a tool name"),
            arguments,
        ),
    )
}


/// Write a `draft`-scoped log by hand.
///
/// No surface produces one yet — the rail's draft scope is CMS-FR-36's and is
/// not implemented — so the builder is exercised against a log written exactly
/// the way that surface will write it.
pub(super) fn seed_draft_thread(h: &Harness, draft_id: &str, file_rel: &str) -> Discussion {
    // CMS-FR-37: in the repository machine store, under the draft's stable id.
    let dir = h.root().join(format!("drafts/{draft_id}/comments"));
    std::fs::create_dir_all(&dir).expect("comments dir");
    let log = dir.join(format!("{}.jsonl", crate::comments::log_id(file_rel)));
    let thread_id = "draft-thread-1";
    let opened = serde_json::json!({
        "v": 1, "eventId": "e1", "threadId": thread_id, "at": "2026-01-01T00:00:00Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "thread_opened",
        "artifactPath": file_rel,
        "anchor": { "start": 0, "end": 5, "quote": "Hello" },
    });
    let commented = serde_json::json!({
        "v": 1, "eventId": "e2", "threadId": thread_id, "at": "2026-01-01T00:00:01Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "comment_added",
        "commentId": "c1", "body": "@arch is this two specs?", "quotes": [],
    });
    std::fs::write(&log, format!("{opened}\n{commented}\n")).expect("log");
    crate::comments::list_draft_fragment_discussions_in(&h.root(), draft_id, file_rel)
        .into_iter()
        .next()
        .expect("draft thread")
}


/// A note and its discussion, returned as the origin a turn would carry.
pub(super) fn seed_note_discussion(
    h: &Harness,
    scope: crate::notes::NoteScope,
    body: &str,
    opening: &str,
) -> (String, ConversationOrigin) {
    let root = h.root();
    let note = crate::notes::create_note_in(
        &root,
        scope,
        body.into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("note");
    let (thread, _) = crate::comments::get_or_create_note_discussion_in(
        &root,
        &root,
        &note.id,
        opening.into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:01:00Z",
    )
    .expect("discussion");
    (
        note.id.clone(),
        ConversationOrigin::of(&thread),
    )
}


pub(super) fn tags_of(sections: &[InputSection]) -> Vec<String> {
    sections.iter().map(|s| s.tag.clone()).collect()
}


/// The text of every tool result in an exchange, in order.
pub(super) fn tool_results(exchange: &[rig::completion::Message]) -> Vec<String> {
    exchange
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .filter_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => {
                match result.content.first()? {
                    rig::completion::message::ToolResultContent::Text(text) => {
                        Some(text.text.clone())
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}


/// Every file under `root`, by relative path, with its bytes.
pub(super) fn tree_snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let key = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, bytes);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}


/// Wait for one record about `turn_id` to appear, for the assertions that have
/// to observe a *thread's* progress rather than a command's return.
pub(super) fn wait_for_record(turn_id: &str, message: &str) -> LogRecord {
    // Through `LogBuffer::find` rather than through [`records_where`], and the
    // difference is not a tidiness one. `records_where` reads the buffer with
    // `query`, which clones **every** record it answers with, and clones them
    // while it holds the buffer's mutex. At the two thousand records this
    // module accumulates one such read measured between four and nine
    // milliseconds, and this loop repeats it every ten — so one waiting test
    // held the mutex for about a third of the time and two held it for most of
    // it. Every turn thread logs through that same mutex, so a wait for a
    // record delayed the thread that emits it, and a test waiting on something
    // else entirely paid for it: the terminal-event wait above expired at ten
    // seconds against a turn whose own work had stalled. The read below holds
    // the lock for one pass of borrowed comparisons and clones one record.
    for _ in 0..1000 {
        if let Some(record) = TEST_BUFFER.find(|r| {
            // Message first: a `&str` comparison that rejects almost every
            // record without touching the field map. `as_str` rather than
            // [`field`]: every `turnId` in this crate is emitted from a `String`,
            // so `json!` always makes one a JSON string. An emit site that ever
            // passed a number would stop matching here — and would wait out the
            // whole budget rather than fail — so keep them strings.
            r.message == message
                && r.fields.get("turnId").and_then(|v| v.as_str()) == Some(turn_id)
        }) {
            return record;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // The messages that *were* recorded for this turn, because the log path is
    // where a stalled turn shows itself: they tell "the turn never got this far"
    // from "no such turn ran" from "the message string drifted".
    panic!(
        "no record reading {message:?} for {turn_id}; messages recorded for it: {:?}",
        // Through `records_where` deliberately: this is the failure path, where
        // one expensive read costs nothing and the loop above is already over.
        records_where("turnId", turn_id)
            .into_iter()
            .map(|r| r.message)
            .collect::<Vec<_>>(),
    );
}


/// A provider that never answers, reached through the production timeout path.
/// [`NeverAnswers`] that counts what it was asked, so a test can see how many
/// physical attempts a turn made when the scripted queue is not in play.
pub(super) struct CountingNeverAnswers {
    pub(super) calls: Arc<AtomicUsize>,
    /// What each call was given as its deadline, in order — the observable that
    /// distinguishes the per-call bound of CVL-FR-17 from the whole-turn one.
    pub(super) deadlines: Arc<Mutex<Vec<Duration>>>,
}

impl CompletionSeam for CountingNeverAnswers {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.deadlines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(timeout);
        block_on_with_timeout(
            async {
                std::future::pending::<()>().await;
                Ok(ModelReply::default())
            },
            timeout,
        )
    }
}


/// A seam whose first call asks for a tool and whose next blocks until released.
///
/// Lets a test put a cancellation *inside* the loop deterministically: the turn
/// cannot reach delivery while the second call is held, so the only thing racing
/// is the test's own release.
pub(super) struct ToolThenGated {
    pub(super) gate: Arc<(Mutex<bool>, Condvar)>,
}

impl CompletionSeam for ToolThenGated {
    fn complete(
        &self,
        _request: &AgentRequest,
        exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        // Which round this is, read off the exchange rather than off a counter:
        // one seam serves every turn, and a counter cannot tell two turns'
        // first calls from one turn's first two. A turn's opening call is the
        // one carrying the input alone (CVL-FR-12).
        if exchange.len() == 1 {
            return Ok(ModelReply {
                text: String::new(),
                tool_calls: vec![rig_tool_call("call-0", "list_skills", serde_json::json!({}))],
                native_calls: Vec::new(),
                native_usage: None,
            native_entries_dropped: 0,
            reply_repairs: Default::default(),
            served_by: None,
            prompt_tokens: None,
            output_tokens: None,
                input_tokens: None,
                ..Default::default()
            });
        }
        let (lock, cvar) = &*self.gate;
        let mut released = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*released {
            released = cvar.wait(released).unwrap_or_else(|e| e.into_inner());
        }
        Ok(ModelReply {
            text: "Should never be delivered.".into(),
            tool_calls: Vec::new(),
            native_calls: Vec::new(),
            native_usage: None,
            native_entries_dropped: 0,
            reply_repairs: Default::default(),
            served_by: None,
            prompt_tokens: None,
            output_tokens: None,
            input_tokens: None,
            ..Default::default()
        })
    }
}

pub(super) const EXPECTED_TOOLS: [&str; 11] = [
    "search_specifications",
    "read_file",
    "search_drafts",
    "read_draft",
    "search_notes",
    "search_documents",
    "get_document",
    "search_skills",
    "list_skills",
    "load_skill",
    "ask_user_comment",
];

/// CVL-FR-08: the tool a **discussion** origin is attached beside the nine, and
/// an anchored comment origin is attached not at all (ADQ-FR-LFDX).
pub(super) const DISCUSSION_ONLY_TOOL: &str = crate::tools::ask_discussion_questions::NAME;
