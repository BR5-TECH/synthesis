//! Tests for `DPC-decide-prompt-changes-tool.md`.
//!
//! The entry loop is shared with `decide_draft_changes` and is tested in
//! `tools/decide_changes/tests.rs`. What is tested here is the prompt proposal
//! store as the tool reaches it.
//!
//! Fixtures build files with `std::fs`, which `fs/access_tests.rs` allows in a
//! file named `tests.rs` alone.

use std::sync::{Arc, Mutex};

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::{Listener, Manager};
use tempfile::TempDir;

use super::*;
use crate::comments::Attachment;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::prompt_proposals::{NewPromptProposal, ProposedChange};
use crate::scanning::{ArtifactType, Assignment, LibraryAssignments, Scope};
use crate::text_hunks::hunks::ProposedHunk;
use crate::tools::decide_changes::{DecisionArg, Outcome, RefusalCode};
use crate::tools::tests::{block_on, closed_project, mounted};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROMPT: &str = "prompts/review.md";
const ORIGINAL: &str = "# Review\n\nAlpha line.\n\nBeta line.\n";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: None,
    }
}

static QUIET: LogBuffer = LogBuffer::new();

fn write_file(root: &crate::fs::RootFs, rel: &str, body: &str) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, body).expect("write");
}

fn assign_prompt(root: &crate::fs::RootFs) {
    let mut assignments = LibraryAssignments::default();
    assignments.assignments.insert(
        PROMPT.to_string(),
        Assignment { artifact_type: ArtifactType::Prompt, scope: Scope::File },
    );
    let path = root.path().join(".synthesis");
    std::fs::create_dir_all(&path).expect("dirs");
    std::fs::write(path.join("library.toml"), toml::to_string(&assignments).expect("toml"))
        .expect("write");
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    thread_id: String,
    proposal_id: String,
    hunks: Vec<String>,
}

impl Fixture {
    /// A prompt artifact with one open proposal of two hunks.
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        write_file(&root, PROMPT, ORIGINAL);
        assign_prompt(&root);
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Artifact { artifact_id: PROMPT.into() },
            None,
            "@arch what would you change?".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        let open = mounted(dir);
        // PCP-FR-MXSB: the accept path writes through the artifact's own save
        // path, which records into the content tracker.
        open.app.manage(ContentTracker::default());
        let proposed: Vec<ProposedHunk> = ["Alpha", "Beta"]
            .iter()
            .map(|word| {
                ProposedHunk::from_fields(
                    Some(format!("{word} line.")),
                    Some(format!("{word} better.")),
                    None,
                    None,
                    None,
                    None,
                )
            })
            .collect();
        let recorded = prompt_proposals::record_prompt_proposal(
            &open.handle(),
            &root,
            &root,
            NewPromptProposal {
                artifact_id: PROMPT,
                change: ProposedChange::Hunks(&proposed),
                rationale: "Sharper. <spoken>Sharper.</spoken>",
                agent: &agent(),
                thread_id: &thread.id,
            },
        )
        .expect("recorded");
        Fixture {
            open,
            path,
            thread_id: thread.id,
            proposal_id: recorded.id.clone(),
            hunks: recorded.ledger.iter().map(|row| row.id.clone()).collect(),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_artifact(self.thread_id.clone(), PROMPT, false)
    }

    fn tool_for(
        &self,
        origin: ConversationOrigin,
        buffer: &'static LogBuffer,
    ) -> DecidePromptChangesTool<tauri::test::MockRuntime> {
        let turn =
            crate::tools::tests::authorized_turn_for(&self.open.handle(), &self.root(), &origin);
        DecidePromptChangesTool::new(
            self.open.handle(),
            OwnedRoots { worktree: self.root(), store: self.root() },
            origin,
            agent(),
            &turn,
        )
        .for_test(human(), buffer)
    }

    fn call(&self, decisions: &[(&str, &str)]) -> Result<DecideChangesOutput, CallRefusal> {
        block_on(self.tool_for(self.origin(), &QUIET).call(args(decisions)))
    }

    fn hunk(&self, place: usize) -> &str {
        &self.hunks[place - 1]
    }

    fn body(&self) -> String {
        std::fs::read_to_string(self.path.join(PROMPT)).expect("artifact")
    }

    fn record(&self) -> prompt_proposals::PromptChangeProposal {
        prompt_proposals::read_proposal(&self.root(), &self.proposal_id).expect("record")
    }

    fn state_of(&self, place: usize) -> HunkState {
        let id = self.hunk(place).to_string();
        self.record().ledger.iter().find(|row| row.id == id).expect("row").state
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(
            &self.root(),
            &DiscussionTarget::Artifact { artifact_id: PROMPT.into() },
        )
    }

    fn thread(&self) -> comments::Discussion {
        self.discussions().into_iter().find(|d| d.id == self.thread_id).expect("thread")
    }
}

fn args(decisions: &[(&str, &str)]) -> DecideChangesArgs {
    DecideChangesArgs {
        instruction: Some(crate::tools::tests::DECISION_INSTRUCTION.into()),
        decisions: Some(
            decisions
                .iter()
                .map(|(id, action)| DecisionArg {
                    id: Some(id.to_string()),
                    action: Some(action.to_string()),
                })
                .collect(),
        ),
    }
}

fn code_of(output: &DecideChangesOutput, place: usize) -> Option<RefusalCode> {
    output.decisions[place - 1].refusal.as_ref().map(|r| r.code)
}

// ---------------------------------------------------------------------------
// The definition
// ---------------------------------------------------------------------------

// DPC-FR-AFSM, DPC-FR-RJBJ, DPC-FR-QMZE, DPC-FR-HWOT
#[test]
fn it_is_a_portable_tool_with_the_fixed_description_and_the_shared_schema() {
    let f = Fixture::new();
    let definition = rig::tool::tool_definition(&f.tool_for(f.origin(), &QUIET));
    assert_eq!(definition.name, "decide_prompt_changes");
    assert_eq!(definition.description, DESCRIPTION);
    assert!(DESCRIPTION.starts_with("Decide changes of the open proposal of the prompt"));
    assert!(DESCRIPTION.contains("Call it only when the author's latest comment"));
    assert!(DESCRIPTION.contains("Do not read the decision back and wait for a confirmation"));
    assert!(DESCRIPTION.contains("count the changes in their order and skip the withdrawn ones"));
    // DPC-FR-RJBJ: the schema of DDC, with this tool's text for `action`.
    let mut draft = crate::tools::decide_draft_changes::parameters();
    let mut prompt = definition.parameters.clone();
    let action = |schema: &mut serde_json::Value| {
        schema["properties"]["decisions"]["items"]["properties"]["action"]
            .as_object_mut()
            .expect("action")
            .remove("description")
            .expect("described")
    };
    assert!(action(&mut prompt).as_str().is_some_and(|d| d.contains("into the file")));
    action(&mut draft);
    // DDC-FR-GGWO: DDC's `id` text names a proposed draft too, so the two
    // schemas share their structure and each keeps its own descriptions.
    let id = |schema: &mut serde_json::Value| {
        schema["properties"]["decisions"]["items"]["properties"]["id"]
            .as_object_mut()
            .expect("id")
            .remove("description");
    };
    id(&mut prompt);
    id(&mut draft);
    // DPC-FR-RJBJ: `confirm` is left out of this tool's enum.
    let actions = |schema: &mut serde_json::Value| {
        schema["properties"]["decisions"]["items"]["properties"]["action"]
            .as_object_mut()
            .expect("action")
            .remove("enum")
            .expect("enum")
    };
    assert_eq!(actions(&mut prompt), serde_json::json!(["accept", "reject", "discuss"]));
    assert_eq!(actions(&mut draft), serde_json::json!(["accept", "reject", "discuss", "confirm"]));
    assert_eq!(prompt, draft);
    // DPC-FR-HWOT: the description asks for the author's words.
    assert!(
        DESCRIPTION.contains("Copy into `instruction` the words of the author's latest comment")
    );
    // DPC-FR-QMZE: no argument names an artifact, a path, a proposal, a
    // conversation, or a participant.
    let properties = definition.parameters["properties"].as_object().expect("properties");
    let mut names: Vec<&String> = properties.keys().collect();
    names.sort();
    assert_eq!(names, vec!["decisions", "instruction"]);
}

// ---------------------------------------------------------------------------
// The whole-call refusals
// ---------------------------------------------------------------------------

// DPC-FR-JJGY
#[test]
fn a_turn_that_is_not_about_an_artifact_is_refused() {
    let f = Fixture::new();
    for origin in
        [ConversationOrigin::stub_draft("t", "d1", false), ConversationOrigin::stub_note("t", "n1")]
    {
        let refused = block_on(f.tool_for(origin, &QUIET).call(args(&[(f.hunk(1), "accept")])))
            .expect_err("refused");
        let error = refused.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::PermissionDenied);
        assert_eq!(error.retryable(), Some(false));
        assert_eq!(refused.to_string(), decide_changes::NOT_AN_ARTIFACT_CONVERSATION);
    }
    assert_eq!(f.state_of(1), HunkState::Pending);

    let app = closed_project();
    let dir = TempDir::new().expect("tempdir");
    let root = crate::fs::RootFs::for_root(dir.path());
    let tool = DecidePromptChangesTool::new(
        app.handle().clone(),
        OwnedRoots { worktree: root.clone(), store: root },
        f.origin(),
        agent(),
        "turn-closed",
    );
    let refused = block_on(tool.call(args(&[("h", "accept")]))).expect_err("closed");
    assert_eq!(refused, CallRefusal::NoProjectOpen);
}

// DPC-FR-RGGP
#[test]
fn the_arguments_and_the_whole_call_refusals_are_those_of_ddc() {
    let f = Fixture::new();
    let refused = block_on(f.tool_for(f.origin(), &QUIET).call(DecideChangesArgs {
        instruction: Some(crate::tools::tests::DECISION_INSTRUCTION.into()),
        decisions: Some(Vec::new()),
    }))
    .expect_err("malformed");
    assert_eq!(refused, CallRefusal::Malformed);
    f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "reject")]).expect("closed");
    let refused = f.call(&[(f.hunk(1), "accept")]).expect_err("nothing open");
    assert_eq!(refused, CallRefusal::NoOpenProposal(decide_changes::NO_OPEN_FILE_PROPOSAL));
    assert_eq!(refused.to_execution_error().kind(), ToolErrorKind::InvalidArgs);
}

// ---------------------------------------------------------------------------
// The actions
// ---------------------------------------------------------------------------

// DPC-FR-DQQN, DPC-FR-TVKX, DPC-FR-IHKP, DPC-FR-UEXT, PCP-FR-VCBX, PCP-FR-MXSB
#[test]
fn accept_writes_the_file_and_the_row_says_no_comment_is_owed() {
    let f = Fixture::new();
    let events = Arc::new(Mutex::new(0usize));
    let seen = Arc::clone(&events);
    f.open.app.listen(prompt_proposals::PROMPT_PROPOSALS_CHANGED, move |_| {
        *seen.lock().unwrap() += 1;
    });
    let output = f.call(&[(f.hunk(1), "accept")]).expect("decided");
    let row = &output.decisions[0];
    assert_eq!(row.outcome, Outcome::Accepted);
    assert_eq!(row.comment_owed, Some(false));
    assert_eq!(f.body(), "# Review\n\nAlpha better.\n\nBeta line.\n");
    assert!(*events.lock().unwrap() >= 1, "the event of PCP-FR-17");
}

// DPC-FR-TVKX, DPC-FR-DQQN, PCR-FR-LAGZ: an accept by tool resolves the hunk
// in the file as it stands, so an edit that the author saved in another place
// of the file stays. An edit that is not saved is the Editor's to raise.
#[test]
fn accept_merges_into_the_file_as_saved_and_keeps_the_saved_edit_of_the_author() {
    let f = Fixture::new();
    let saved = "# Review by the author\n\nAlpha line.\n\nBeta line.\n\nA saved note.\n";
    write_file(&f.root(), PROMPT, saved);
    let output = f.call(&[(f.hunk(1), "accept")]).expect("decided");
    assert_eq!(output.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(
        f.body(),
        "# Review by the author\n\nAlpha better.\n\nBeta line.\n\nA saved note.\n"
    );
    assert_eq!(f.state_of(1), HunkState::Accepted);
}

// DPC-FR-DQQN, DPC-FR-QVKH, DPC-FR-IHKP, DPC-FR-QTIW
#[test]
fn a_rejected_hunk_is_accepted_and_the_close_appends_the_decision_comment() {
    let f = Fixture::new();
    let output = f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "reject")]).expect("rejected");
    assert!(output.closed);
    assert_eq!(f.body(), ORIGINAL, "a reject writes nothing into the project");
    let comments_after_close = f.thread().comments.len();
    assert_eq!(comments_after_close, 3, "opening, proposal, decision comment");
    assert_eq!(f.thread().comments.last().expect("comment").author, human());

    // A second proposal: a rejected hunk is accepted while it is pending.
    let f = Fixture::new();
    f.call(&[(f.hunk(2), "reject")]).expect("rejected");
    let output = f.call(&[(f.hunk(2), "accept")]).expect("accepted");
    assert_eq!(output.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(f.state_of(2), HunkState::Accepted);
    assert!(f.body().contains("Beta better."));
}

// DPC-FR-WCBF, DPC-FR-AFVA, DPC-FR-QPNB
#[test]
fn discuss_opens_a_new_artifact_discussion_and_holds_a_pending_hunk() {
    let f = Fixture::new();
    let output = f.call(&[(f.hunk(2), "discuss")]).expect("opened");
    let row = &output.decisions[0];
    assert_eq!(row.outcome, Outcome::DiscussionOpened);
    let id = row.discussion_id.clone().expect("id");
    let opened = f.discussions().into_iter().find(|d| d.id == id).expect("opened");
    assert_eq!(opened.comments.len(), 1);
    assert_eq!(opened.comments[0].body, decide_changes::DISCUSSION_BODY);
    assert_eq!(opened.comments[0].author, agent());
    assert_eq!(
        opened.comments[0].attachments,
        vec![Attachment::ProposalHunk {
            change: None,
            proposal_id: f.proposal_id.clone(),
            hunk_id: f.hunk(2).to_string(),
            draft_id: None,
            artifact_id: Some(PROMPT.into()),
            path: PROMPT.into(),
        }]
    );
    assert_eq!(f.state_of(2), HunkState::Discussing);
    assert_eq!(f.thread().comments.len(), 2, "no comment in the calling conversation");
    assert_eq!(f.body(), ORIGINAL);
}

// DPC-FR-NXZF, DPC-FR-JEXJ
#[test]
fn an_entry_is_refused_on_the_codes_of_ddc() {
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "accept"), (f.hunk(2), "discuss")]).expect("first");
    let output = f
        .call(&[("x", "keep"), ("x", "accept"), (f.hunk(1), "reject"), (f.hunk(2), "discuss")])
        .expect("answers");
    assert_eq!(code_of(&output, 1), Some(RefusalCode::InvalidAction));
    assert_eq!(code_of(&output, 2), Some(RefusalCode::UnknownHunk));
    assert_eq!(code_of(&output, 3), Some(RefusalCode::HunkAccepted));
    // DPC-FR-NXZF: the discussion the tool opened names the hunk and the
    // artifact, so a second discuss is already in state.
    assert_eq!(code_of(&output, 4), Some(RefusalCode::AlreadyInState));
    for (index, row) in output.decisions.iter().enumerate() {
        let message = &row.refusal.as_ref().expect("refusal").message;
        assert!(message.starts_with(&format!("Decision {} of 4: ", index + 1)), "{message}");
    }
    // DPC-FR-JEXJ: only these two typed errors are retryable.
    assert_eq!(RETRYABLE_ERRORS, ["write_failed", "acceptance_in_progress"]);
    let tool = f.tool_for(f.origin(), &QUIET);
    let backend = PromptBackend {
        tool: &tool,
        artifact_id: PROMPT,
        author: Ok(human()),
        line: String::new(),
    };
    for (error, retryable) in [
        ("write_failed", true),
        ("acceptance_in_progress", true),
        ("artifact_not_found", false),
        ("not_a_prompt_artifact", false),
        ("hunk_already_decided", false),
    ] {
        assert_eq!(backend.retryable_error(error), retryable, "{error}");
    }
}

// DPC-FR-NXZF — the confirmed rule: a lost hunk is refused for every action.
#[test]
fn a_lost_hunk_is_refused_for_every_action() {
    let f = Fixture::new();
    write_file(&f.root(), PROMPT, "# Review\n\nAlpha line.\n\nRewritten by hand.\n");
    let output = f
        .call(&[(f.hunk(2), "accept"), (f.hunk(2), "reject"), (f.hunk(2), "discuss")])
        .expect("answers");
    for place in 1..=3 {
        assert_eq!(code_of(&output, place), Some(RefusalCode::HunkLost));
        let message = &output.decisions[place - 1].refusal.as_ref().expect("refusal").message;
        assert!(message.contains("no longer in the file"), "{message}");
    }
    assert_eq!(f.state_of(2), HunkState::Pending);
}

// DPC-FR-WNHU, DPC-FR-UEXT
#[test]
fn an_incomplete_acceptance_is_an_accepted_row_with_the_comment_owed() {
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "reject")]).expect("rejected");
    // The comment log takes no append, so the decision that closes the
    // proposal lands its file and owes its comment.
    let log = std::fs::read_dir(f.path.join("comments"))
        .expect("comments dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(".discussion.jsonl"))
        .expect("the discussion's log");
    let restore = std::fs::metadata(&log).expect("log").permissions();
    let mut sealed = restore.clone();
    sealed.set_readonly(true);
    std::fs::set_permissions(&log, sealed).expect("seal");
    let output = f.call(&[(f.hunk(2), "accept")]).expect("answers");
    std::fs::set_permissions(&log, restore).expect("unseal");
    let row = &output.decisions[0];
    assert_eq!(row.outcome, Outcome::Accepted, "the change has landed");
    assert_eq!(row.comment_owed, Some(true), "the completion also failed");
    assert!(row.refusal.is_none());
    assert!(f.body().contains("Beta better."));
    assert!(f.record().comment_owed);
    let json = serde_json::to_value(&output).expect("json");
    assert_eq!(json["decisions"][0]["comment_owed"], true);
}

// DPC-FR-SNEB, DPC-FR-QTIW, PCP-FR-21
#[test]
fn the_tool_never_makes_changes_or_files_and_dispatches_no_turn() {
    let source = include_str!("../decide_prompt_changes.rs");
    for forbidden in [
        "record_prompt_proposal",
        "update_prompt_proposal",
        "edit_hunk_impl",
        "decline_prompt_change_proposal",
        "create_artifact",
        "dispatch_impl",
        "dispatch_agent_turn(",
        "AtomicBool",
    ] {
        assert!(!source.contains(forbidden), "the tool reaches {forbidden}");
    }
    let loop_body = include_str!("../../agent_conversations/turn_body.rs");
    assert!(!loop_body.contains("decide_prompt_changes"), "CVL-FR-15: it ends no turn");
}

// DPC-FR-CIRL
#[test]
fn each_call_logs_with_the_artifact_path_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    block_on(f.tool_for(f.origin(), &BUFFER).call(args(&[(f.hunk(1), "reject"), ("q", "accept")])))
        .expect("answers");
    let records = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .expect("buffer")
        .records;
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("info");
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["tool"], NAME);
    assert_eq!(fields["path"], PROMPT);
    assert_eq!(fields["rejected"], 1);
    assert_eq!(fields["refused"], 1);
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("warn");
    let warn = serde_json::to_value(&warn.fields).expect("fields");
    assert_eq!(warn["place"], 2);
    let all = serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>())
        .expect("json");
    assert!(!all.contains("Alpha") && !all.contains("better"));
    // The tool holds no state between calls: a second call reads storage.
    let again = f.call(&[(f.hunk(1), "reject")]).expect("again");
    assert_eq!(code_of(&again, 1), Some(RefusalCode::AlreadyInState));
}

// DPC-FR-QTIW
#[test]
fn the_comment_thread_is_found_through_the_thread_ref_of_the_artifact() {
    // A close by tool appends the decision comment and nothing else: no
    // comment of the agent and no turn.
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "accept"), (f.hunk(2), "accept")]).expect("closed");
    let thread = f.thread();
    let agents = thread.comments.iter().filter(|c| c.author == agent()).count();
    assert_eq!(agents, 1, "the proposal's own comment, and no other");
}

// ---------------------------------------------------------------------------
// Authority (DPC-FR-VJIG, PCP-FR-QKCT)
// ---------------------------------------------------------------------------

// DPC-FR-VJIG / DDC-FR-IHAY: a turn whose newest comment is not an author
// comment decides nothing on a file either.
#[test]
fn the_authority_rules_of_ddc_hold_for_a_file() {
    let f = Fixture::new();
    let handle = f.open.handle();
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &handle,
        f.origin(),
        None,
        None,
    );
    let tool = DecidePromptChangesTool::new(
        handle,
        OwnedRoots { worktree: f.root(), store: f.root() },
        f.origin(),
        agent(),
        &turn,
    )
    .for_test(human(), &QUIET);
    let refused = block_on(tool.call(args(&[(f.hunk(1), "accept")]))).expect_err("refused");
    assert_eq!(
        refused,
        CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::NotAuthorized)
    );
    assert_eq!(refused.to_execution_error().kind(), ToolErrorKind::PermissionDenied);
    assert_eq!(f.state_of(1), HunkState::Pending);
    // DDC-FR-PZBH: an absent instruction is malformed here too.
    let mut call = args(&[(f.hunk(1), "accept")]);
    call.instruction = None;
    assert_eq!(block_on(f.tool_for(f.origin(), &QUIET).call(call)), Err(CallRefusal::Malformed));
}

// DPC-FR-VJIG: a file has no decomposition, so `confirm` is `invalid_action`.
#[test]
fn a_confirm_entry_is_an_invalid_action_for_a_file() {
    let f = Fixture::new();
    let out = f.call(&[(f.hunk(1), "confirm")]).expect("answers");
    assert_eq!(code_of(&out, 1), Some(RefusalCode::InvalidAction));
    let message = &out.decisions[0].refusal.as_ref().unwrap().message;
    assert!(!message.contains("decomposition"), "{message}");
    assert_eq!(f.state_of(1), HunkState::Pending);
}

// PCP-FR-QKCT / DPC-FR-VJIG: the accept that resolves the proposal appends a
// decision comment, through its journal, that ends with the instruction line.
#[test]
fn a_resolving_accept_by_tool_ends_its_comment_with_the_instruction_line() {
    let f = Fixture::new();
    let out = f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "accept")]).expect("decided");
    assert!(out.closed);
    assert_eq!(out.decisions[1].comment_owed, Some(false));
    let thread =
        f.discussions().into_iter().find(|d| d.id == f.thread_id).expect("the conversation");
    let last = thread.comments.last().expect("the decision comment");
    assert!(last.body.starts_with("Accepted the proposed change"), "{}", last.body);
    assert!(last.body.ends_with("\n\nDecided by arch on the author's instruction: “arch”"));
}

impl Fixture {
    /// An author comment with `body` in the conversation, and a turn it
    /// triggers. The spent comments are left as they are.
    fn author_turn(&self, body: &str) -> (String, String) {
        let id = crate::notes::new_note_id();
        comments::add_comment_with_id(
            &self.root(),
            comments::ThreadRef::artifact_discussion(PROMPT),
            &self.thread_id,
            id.clone(),
            body.into(),
            &human(),
            "2026-01-01T00:00:02Z",
        )
        .expect("comment");
        let turn = crate::agent_conversations::authority_for_test::register_turn(
            &self.open.handle(),
            self.origin(),
            Some(&id),
            None,
        );
        crate::agent_conversations::note_author_comment(&self.open.handle(), &id);
        (id, turn)
    }

    fn call_on(
        &self,
        turn: &str,
        instruction: &str,
        decisions: &[(&str, &str)],
    ) -> Result<DecideChangesOutput, CallRefusal> {
        let tool = DecidePromptChangesTool::new(
            self.open.handle(),
            OwnedRoots { worktree: self.root(), store: self.root() },
            self.origin(),
            agent(),
            turn,
        )
        .for_test(human(), &QUIET);
        let mut call = args(decisions);
        call.instruction = Some(instruction.into());
        block_on(tool.call(call))
    }
}

// DPC-FR-VJIG / DDC-FR-VONX: a call that decides spends the comment, a call
// that decides nothing does not, and a call refused whole after the
// reservation gives it back.
#[test]
fn a_comment_authorizes_one_deciding_call_on_a_file() {
    let f = Fixture::new();
    let used =
        CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::InstructionUsed);

    let (_, turn) = f.author_turn("@arch reject the first one");
    let out = f.call_on(&turn, "reject the first one", &[("nope", "reject")]).unwrap();
    assert_eq!(code_of(&out, 1), Some(RefusalCode::UnknownHunk));
    let out = f.call_on(&turn, "reject the first one", &[(f.hunk(1), "reject")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Rejected);
    assert_eq!(f.call_on(&turn, "reject the first one", &[(f.hunk(2), "reject")]), Err(used));
    assert_eq!(f.state_of(2), HunkState::Pending);

    // DDC-FR-NKAB on a file.
    let (_, other) = f.author_turn("@arch accept the second");
    assert_eq!(
        f.call_on(&other, "reject the second", &[(f.hunk(2), "reject")]),
        Err(CallRefusal::Authority(
            crate::agent_conversations::AuthorityRefusal::InstructionNotFound
        ))
    );
    // Close the proposal, then a whole-call refusal after the reservation
    // leaves the comment unspent.
    f.call_on(&other, "accept the second", &[(f.hunk(2), "accept")]).unwrap();
    let (late_comment, late) = f.author_turn("@arch accept the second");
    assert!(matches!(
        f.call_on(&late, "accept the second", &[(f.hunk(2), "accept")]),
        Err(CallRefusal::NoOpenProposal(_))
    ));
    assert!(!crate::agent_conversations::authority_for_test::is_spent(
        &f.open.handle(),
        &late_comment
    ));
}
