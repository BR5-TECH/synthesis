//! Tests for `PDC-propose-draft-changes-tool.md`.
//!
//! The group-wide conventions this tool shares with the other six are checked
//! once in `tools/tests.rs`; what follows is what is true of this tool alone.
//!
//! The storage consequences of a recorded proposal — the candidate, the
//! one-pending rule, and what an acceptance writes — belong to
//! `DCP-draft-change-proposals.md` and are tested in `draft_proposals/tests.rs`.
//! The turn-level consequences — the loop ending, which belongs to
//! `../ai/CVL-conversation-loop.md`, and the `awaiting_reply` state and the
//! decision that dispatches a fresh turn, which belong to
//! `AGC-agent-conversations.md` — are tested in `agent_conversations/tests.rs`. What is tested here is the
//! call: what it records, what it refuses, and what it reports.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, Participant};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted};
use crate::tools::ToolRefusal;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FILE: &str = "spec.md";
const ORIGINAL: &str = "# Spec\n\nThe original.\n";
const PROPOSED: &str = "# Spec\n\nA better opening.\n";
const RATIONALE: &str = "The opening buries the point.";

fn human() -> Participant {
    Participant::Human {
        login: "raver119".into(),
        display_name: None,
        email: None,
    }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        // CMS-FR-65: the snapshot the turn stamped, which this tool must carry
        // onto the comment it appends unchanged.
        title: Some("Developer".into()),
    }
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    proposed: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("spec")).expect("draft");
        crate::drafts::save_draft_file_impl(&root, &created.draft.id, &created.file, ORIGINAL)
            .expect("seed");
        assert_eq!(created.file, FILE);
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft {
                draft_id: created.draft.id.clone(),
            },
            None,
            "@arch what would you change?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            draft_id: created.draft.id,
            thread_id: thread.id,
            proposed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.open.handle()
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    /// The pair a conversation-bound tool holds: one temporary directory
    /// serving as both the worktree and the repository machine store.
    fn roots(&self) -> crate::agent_conversations::OwnedRoots {
        crate::agent_conversations::OwnedRoots {
            worktree: self.root(),
            store: self.root(),
        }
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
    }

    fn tool_for(
        &self,
        origin: ConversationOrigin,
    ) -> ProposeDraftChangesTool<tauri::test::MockRuntime> {
        ProposeDraftChangesTool::new(
            self.handle(),
            self.roots(),
            origin,
            agent(),
            Arc::clone(&self.proposed),
        )
    }

    fn tool(&self) -> ProposeDraftChangesTool<tauri::test::MockRuntime> {
        self.tool_for(self.origin())
    }

    fn call(
        &self,
        args: ProposeDraftChangesArgs,
    ) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
        block_on(self.tool().call(args))
    }

    fn discussion(&self) -> crate::comments::Discussion {
        crate::comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn proposals(&self) -> Vec<crate::draft_proposals::DraftChangeProposal> {
        crate::draft_proposals::list_proposals_impl(&self.root(), &self.draft_id).expect("list")
    }

    /// The changes of the one proposal the draft holds, as they were recorded.
    fn hunks_of_the_one_proposal(&self) -> Vec<crate::draft_proposals::hunks::ProposalHunk> {
        let proposals = self.proposals();
        assert_eq!(proposals.len(), 1, "the draft holds one proposal");
        crate::draft_proposals::load_hunks_impl(&self.root(), &proposals[0].id)
            .expect("the changes are readable")
            .hunks
    }

    fn file_body(&self) -> String {
        crate::drafts::load_draft_file_impl(&self.root(), &self.draft_id, FILE)
            .expect("file")
            .body
    }

    fn lock(&self) {
        crate::comments::set_lock_to(
            &self.root(),
            crate::comments::ThreadRef::discussion(&self.draft_id),
            &self.thread_id,
            true,
            &human(),
            "2026-01-01T00:05:00Z",
        )
        .expect("lock");
    }
}

fn args(path: &str, content: &str, rationale: &str) -> ProposeDraftChangesArgs {
    ProposeDraftChangesArgs {
        path: path.into(),
        rationale: rationale.into(),
        revises: None,
        // The tool now takes changes, so a test that names a whole document
        // expresses it as the one change it is.
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some(ORIGINAL.into()),
            after: Some(content.into()),
            after_text: None,
            note: None,
        }],
    }
}

fn good() -> ProposeDraftChangesArgs {
    args(FILE, PROPOSED, RATIONALE)
}

/// The property names of a JSON Schema object, sorted.
fn schema_keys(properties: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = properties
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// The records `buffer` holds under `domain`, newest last.
fn records(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the buffer answers")
        .records
}

// ---------------------------------------------------------------------------
// PDC-FR-01 / PDC-FR-02 — the definition (PDC-FR-01, PDC-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn the_definition_is_fixed_text_and_names_the_three_required_arguments() {
    let f = Fixture::new();
    let tool = f.tool();

    assert_eq!(
        <ProposeDraftChangesTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_draft_changes",
    );
    assert_eq!(tool.description(), DESCRIPTION);

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema_keys(&schema["properties"]),
        vec!["hunks", "path", "rationale", "revises"],
    );
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    // PDC-FR-GMWR: `revises` is the one optional argument — a proposal that
    // revises nothing simply leaves it out.
    assert_eq!(required.len(), 3, "only `revises` has a default");
    for key in ["path", "hunks", "rationale"] {
        assert!(
            schema["properties"][key]["description"]
                .as_str()
                .is_some_and(|d| !d.is_empty()),
            "{key} carries its own description (TLC-FR-06)",
        );
    }

    // PDC-FR-04 / PDC-FR-TZKQ: a change names the text it changes, and the two
    // mistakes the backend refuses — text the prompt does not hold, and text
    // that names more than one place — are both correctable only if the model
    // was told the rule first. The guard must be in the description and in
    // `hunks`' own description, not one or the other.
    for text in [
        tool.description(),
        schema["properties"]["hunks"]["description"]
            .as_str()
            .expect("description")
            .to_string(),
    ] {
        let lowered = text.to_lowercase();
        assert!(
            lowered.contains("exact") && lowered.contains("before"),
            "it says to copy the existing text exactly: {text:?}",
        );
        assert!(
            lowered.contains("once"),
            "and to include enough context that it names one place: {text:?}",
        );
    }
}

#[test]
fn two_instances_for_two_drafts_are_offered_byte_identical_definitions() {
    let f = Fixture::new();
    let other = Fixture::new();
    let a = f.tool();
    let b = other.tool_for(ConversationOrigin::stub_draft("t", "d", true));
    assert_eq!(a.description(), b.description());
    assert_eq!(a.parameters(), b.parameters());
}

// ---------------------------------------------------------------------------
// PDC-FR-03 — the containment (PDC-FR-03, PDC-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn no_argument_reaches_a_draft_the_turn_was_not_asked_about() {
    let f = Fixture::new();
    let schema = f.tool().parameters();
    let keys = schema_keys(&schema["properties"]);
    for forbidden in ["draft", "draftId", "thread", "threadId", "conversation"] {
        assert!(
            !keys.contains(&forbidden.to_string()),
            "no parameter names a conversation: {keys:?}",
        );
    }

    // And a path that tries to climb out of the draft is refused before it is
    // resolved against anything.
    for path in ["../../etc/passwd", "/etc/passwd", "a/../../b"] {
        assert_eq!(
            f.call(args(path, PROPOSED, RATIONALE)).unwrap_err(),
            ToolRefusal::ProposalPathMissing,
            "path {path:?}",
        );
    }
    assert!(f.proposals().is_empty(), "nothing was recorded");
}

// ---------------------------------------------------------------------------
// PDC-FR-05 / DCP-FR-12 — a proposal changes no file (PDC-FR-04, side effects)
// ---------------------------------------------------------------------------

#[test]
fn a_successful_call_records_a_proposal_and_writes_no_file_of_the_draft() {
    let f = Fixture::new();
    assert_eq!(
        f.call(good()).expect("proposed"),
        ProposeDraftChangesOutput { proposed: true },
    );

    assert_eq!(f.file_body(), ORIGINAL, "the draft file is untouched");
    let proposals = f.proposals();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].path, FILE);
    assert_eq!(proposals[0].rationale, RATIONALE);
    assert!(
        f.proposed.load(Ordering::SeqCst),
        "the loop is told a proposal went out (CVL-FR-15)",
    );
}

#[test]
fn a_fragment_is_proposed_as_a_whole_file_because_nothing_can_tell_it_apart() {
    let f = Fixture::new();
    // PDC-FR-04: the model passed one paragraph. There is no way to distinguish
    // that from a deliberate one-paragraph document, so it is recorded as the
    // whole file — which is exactly what accepting it would produce.
    f.call(args(FILE, "just this line", RATIONALE)).expect("proposed");
    let stored = crate::draft_proposals::load_content_impl(&f.root(), &f.proposals()[0].id)
        .expect("content");
    assert_eq!(stored.content, "just this line");
    assert_eq!(f.file_body(), ORIGINAL, "and still nothing was written");
}

// ---------------------------------------------------------------------------
// PDC-FR-05 — a path the draft does not hold (PDC-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn a_path_naming_nothing_in_the_draft_is_a_retryable_not_found() {
    let f = Fixture::new();
    let refusal = f.call(args("missing.md", PROPOSED, RATIONALE)).unwrap_err();
    assert_eq!(refusal, ToolRefusal::ProposalPathMissing);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(
        refusal.retryable(),
        "a different path reaches a file the draft has (PDC-FR-05)",
    );
    assert!(f.proposals().is_empty());
    assert_eq!(f.discussion().comments.len(), 1, "and nothing was appended");
}

// ---------------------------------------------------------------------------
// PDC-FR-06 — an artifact conversation (PDC-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn an_artifact_conversation_is_refused_and_says_a_further_call_will_not_succeed() {
    let f = Fixture::new();
    for origin in [
        ConversationOrigin::stub_artifact("t", "a.md", true),
        ConversationOrigin::stub_artifact("t", "spec.md", false),
    ] {
        let refusal = block_on(f.tool_for(origin).call(good())).unwrap_err();
        assert_eq!(refusal, ToolRefusal::NotADraftConversation);
        assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
        assert!(!refusal.retryable(), "no argument makes an artifact a draft");
        assert!(
            refusal.to_string().contains("will not succeed"),
            "and the message says so too (TLC-FR-11)",
        );
    }
    assert!(f.proposals().is_empty());
}

// ---------------------------------------------------------------------------
// PDC-FR-07 — the three argument refusals, each distinct (PDC-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn blank_content_blank_rationale_and_a_no_op_change_refuse_distinctly() {
    let f = Fixture::new();

    // PDC-FR-07: a proposal carrying no changes at all has nothing to decide.
    let mut empty = args(FILE, PROPOSED, RATIONALE);
    empty.hunks.clear();
    let blank_content = f.call(empty).unwrap_err();
    let blank_rationale = f.call(args(FILE, PROPOSED, "   ")).unwrap_err();
    let no_change = f.call(args(FILE, ORIGINAL, RATIONALE)).unwrap_err();

    for refusal in [&blank_content, &blank_rationale, &no_change] {
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable(), "each is the model's to correct");
    }
    let messages: Vec<String> = [&blank_content, &blank_rationale, &no_change]
        .iter()
        .map(|r| r.to_string())
        .collect();
    let unique: std::collections::BTreeSet<&String> = messages.iter().collect();
    assert_eq!(
        unique.len(),
        3,
        "each calls for a different correction, so each says something different: {messages:?}",
    );
    assert!(f.proposals().is_empty(), "and none of the three recorded anything");
    assert_eq!(f.discussion().comments.len(), 1);
}

// ---------------------------------------------------------------------------
// PDC-FR-08 — one pending at a time (PDC-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn a_second_proposal_is_refused_while_one_is_pending_and_is_retryable() {
    let f = Fixture::new();
    f.call(good()).expect("first");

    let refusal = f.call(args(FILE, "different again", RATIONALE)).unwrap_err();
    assert_eq!(refusal, ToolRefusal::ProposalPending);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(
        refusal.retryable(),
        "the author's decision is what makes a further call succeed (PDC-FR-08)",
    );
    assert_eq!(f.proposals().len(), 1);

    // PDC-FR-08: the message has to leave the model with the sentence to say.
    // A model that answers this refusal by writing that it has submitted the
    // change tells the author something untrue, and the author then waits for a
    // proposal nothing recorded — while the one they have not decided is the
    // very thing holding the slot.
    let message = refusal.to_string();
    assert!(
        message.contains("waiting on the author's decision"),
        "it names the decision that is outstanding: {message:?}",
    );
    assert!(
        message.contains("Say so plainly in your reply"),
        "it tells the model to say so to the author: {message:?}",
    );
    assert!(
        message.contains("do not describe this change as proposed"),
        "and not to describe the refused change as proposed: {message:?}",
    );
    assert!(
        message.contains("was not recorded"),
        "saying outright that nothing was recorded: {message:?}",
    );

    // PDC-FR-08: retryable is a claim about the world, not a flag — the
    // author's decision is what makes a further call succeed, and this is the
    // assertion that the claim is true. It is also the way out of the wedge the
    // refusal would otherwise be: an author who cannot decide leaves the agent
    // refused for good.
    let pending = f
        .proposals()
        .into_iter()
        .find(|p| p.state == crate::draft_proposals::ProposalState::Pending)
        .expect("the standing proposal");
    crate::draft_proposals::decline_for_test(&f.handle(), &f.root(), &f.root(), &pending.id, &human())
        .expect("decline");

    f.call(args(FILE, "different again", RATIONALE))
        .expect("the slot is free once the author has decided");
    assert_eq!(f.proposals().len(), 2);
}

// ---------------------------------------------------------------------------
// PDC-FR-09 — a locked conversation (PDC-FR-09, PDC-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn a_locked_conversation_refuses_permanently_and_records_nothing() {
    let f = Fixture::new();
    f.lock();

    let refusal = f.call(good()).unwrap_err();
    assert_eq!(refusal, ToolRefusal::ProposalConversationLocked);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable());
    assert!(
        refusal.to_string().contains("will not succeed"),
        "the message says so as well as the flag (TLC-FR-11)",
    );
    assert!(f.proposals().is_empty(), "nothing was recorded");
    assert_eq!(f.discussion().comments.len(), 1, "and the log gained no line");
    assert!(
        !f.proposed.load(Ordering::SeqCst),
        "a refusal ends nothing, so the loop runs on (PDC-FR-15)",
    );
}

// ---------------------------------------------------------------------------
// PDC-FR-10 — it records and returns (PDC-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn the_call_resolves_without_being_polled_because_it_waits_for_nothing() {
    let f = Fixture::new();
    // The future is ready on construction: nothing here awaits a decision, holds
    // a timer, or opens a watcher.
    let tool = f.tool();
    let future = tool.call(good());
    let outcome = block_on(future);
    assert!(outcome.is_ok());
    assert_eq!(f.proposals().len(), 1);
}

// ---------------------------------------------------------------------------
// CMS-FR-60, CMS-FR-41, CMS-FR-65 / CMT-FR-48 — the comment (PDC-FR-12, PDC-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn the_comment_is_the_rationale_and_one_reference_by_the_agent() {
    let f = Fixture::new();
    f.call(good()).expect("proposed");

    let thread = f.discussion();
    let posted = thread.comments.last().expect("the proposal's comment");
    assert_eq!(posted.body, RATIONALE, "the rationale and nothing else");
    match &posted.author {
        Participant::Agent {
            agent_id,
            handle,
            model,
            title,
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(handle, "arch");
            assert_eq!(model.as_deref(), Some("m"));
            // PDC-FR-13 / CMS-FR-65: the proposal's comment carries the turn's
            // title snapshot, exactly as a delivered answer does.
            assert_eq!(title.as_deref(), Some("Developer"));
        }
        other => panic!("expected the turn's agent participant, got {other:?}"),
    }
    assert_eq!(posted.attachments.len(), 1);
    match &posted.attachments[0] {
        crate::comments::Attachment::Proposal { path, draft_id, .. } => {
            assert_eq!(path, FILE);
            assert_eq!(draft_id, &f.draft_id);
        }
        other => panic!("expected a proposal reference, got {other:?}"),
    }

    // PDC-FR-12: nothing machine-shaped in the body a person reads.
    for marker in [
        &f.proposals()[0].id[..],
        &f.draft_id[..],
        FILE,
        "proposal",
        "propose_draft_changes",
    ] {
        assert!(
            !posted.body.contains(marker),
            "the body carries no identifier or marker: {marker:?} in {:?}",
            posted.body,
        );
    }
}

// ---------------------------------------------------------------------------
// PDC-FR-14 — the output (PDC-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn the_output_is_the_fact_of_the_proposal_and_carries_nothing_of_it() {
    let f = Fixture::new();
    let output = f.call(good()).expect("proposed");
    let json = serde_json::to_value(&output).expect("serialises");
    assert_eq!(json, serde_json::json!({ "proposed": true }));

    let text = json.to_string();
    assert!(!text.contains(&f.proposals()[0].id), "no identity");
    assert!(!text.contains("better opening"), "no part of the candidate");
}

// ---------------------------------------------------------------------------
// PDC-FR-16 — no project (PDC-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn with_no_project_open_it_gives_the_shared_refusal() {
    let closed = closed_project();
    // A real directory, because building a `RootFs` over one that does not exist
    // panics — and what is under test is the refusal, not the root.
    let elsewhere = TempDir::new().expect("tempdir");
    let tool = ProposeDraftChangesTool::new(
        closed.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(elsewhere.path()), store: crate::fs::RootFs::for_root(elsewhere.path()) },
        ConversationOrigin::stub_draft("t", "d", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    );
    let refusal = block_on(tool.call(good())).unwrap_err();
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(!refusal.retryable());
    assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
}

// ---------------------------------------------------------------------------
// PDC-FR-18 — logging (PDC-FR-18)
// ---------------------------------------------------------------------------

static PDC_LOG: LogBuffer = LogBuffer::new();
static PDC_BOUND_LOG: LogBuffer = LogBuffer::new();

#[test]
fn a_call_records_the_draft_the_conversation_and_the_path_and_nothing_composed() {
    let f = Fixture::new();
    PDC_LOG.clear();

    // The refusal first: once a proposal stands, every later call refuses for the
    // draft's one pending slot (PDC-FR-08) rather than for the path, and this
    // test is about what a *path* refusal records.
    block_on(
        f.tool()
            .with_buffer(&PDC_LOG)
            .call(args("missing.md", PROPOSED, RATIONALE)),
    )
    .unwrap_err();
    block_on(f.tool().with_buffer(&PDC_LOG).call(good())).expect("proposed");

    for domain in [Domain::Ai, Domain::Backend] {
        let records = records(&PDC_LOG, domain);
        assert_eq!(records.len(), 2, "one per call, under {domain:?}");
        let info = records
            .iter()
            .find(|r| r.level == LogLevel::Info)
            .expect("the success");
        let warn = records
            .iter()
            .find(|r| r.level == LogLevel::Warn)
            .expect("the refusal");

        for record in [info, warn] {
            let fields = serde_json::to_string(&record.fields).expect("fields");
            assert!(fields.contains("propose_draft_changes"), "names the tool");
            assert!(fields.contains(&f.draft_id), "names the draft");
            assert!(fields.contains(&f.thread_id), "names the conversation");
            // PDC-FR-18: `path` is this tool's one loggable argument.
            let whole = format!("{} {}", record.message, fields);
            assert!(
                !whole.contains("better opening"),
                "no part of the proposed text: {whole}",
            );
            assert!(
                !whole.contains(RATIONALE),
                "no part of the rationale: {whole}",
            );
            assert!(
                !whole.contains("The original"),
                "and no part of the file being replaced: {whole}",
            );
        }
        assert!(
            serde_json::to_string(&info.fields).expect("fields").contains(FILE),
            "the success names which file was rewritten",
        );
        assert!(
            serde_json::to_string(&warn.fields)
                .expect("fields")
                .contains("proposal_path_missing"),
            "and the refusal names the reason",
        );
    }
}

#[test]
fn a_very_long_path_is_bounded_before_it_reaches_a_record() {
    let f = Fixture::new();
    let long = "x".repeat(5_000);
    block_on(
        f.tool()
            .with_buffer(&PDC_BOUND_LOG)
            .call(args(&long, PROPOSED, RATIONALE)),
    )
    .unwrap_err();

    let emitted = records(&PDC_BOUND_LOG, Domain::Backend);
    let fields = serde_json::to_string(&emitted[0].fields).expect("fields");
    assert!(
        fields.len() < 1_000,
        "the argument is bounded so it cannot cost the record its tool and \
         reason (PDC-FR-18, LGC-FR-08): {} chars",
        fields.len(),
    );
    assert!(fields.contains("propose_draft_changes"));
    assert!(fields.contains("proposal_path_missing"));
}

// ---------------------------------------------------------------------------
// DRS-FR-04 — the whole side effect (side effects, PDC-FR-19)
// ---------------------------------------------------------------------------

#[test]
fn a_successful_call_changes_only_the_proposals_folder_and_the_conversation_log() {
    let f = Fixture::new();
    let before = tree_of(&f.path);

    f.call(good()).expect("proposed");

    let after = tree_of(&f.path);
    let added: Vec<&String> = after.iter().filter(|p| !before.contains(*p)).collect();
    assert!(
        added
            .iter()
            .all(|p| p.contains("/proposals/") || p.ends_with("discussion.jsonl")),
        "only the candidate and the conversation's own log: {added:?}",
    );
    let removed: Vec<&String> = before.iter().filter(|p| !after.contains(*p)).collect();
    assert!(removed.is_empty(), "nothing was removed: {removed:?}");
    assert_eq!(f.file_body(), ORIGINAL, "and the draft's file is as it was");
}

/// Every file under `root`, as a sorted list of relative paths.
fn tree_of(root: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

mod second;
mod third;
