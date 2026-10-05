//! Tests for `PPC-propose-prompt-changes-tool.md`.
//!
//! The group-wide conventions this tool shares with the others are checked once
//! in `tools/tests.rs`; what follows is what is true of this tool alone.
//!
//! The storage consequences of a recorded proposal — the candidate, the
//! one-pending rule, and what an acceptance writes — belong to
//! `PCP-prompt-change-proposals.md` and are tested in `prompt_proposals/tests.rs`.
//! The turn-level consequences — the loop ending (CVL-FR-15) and the
//! `awaiting_reply` state (AGC-FR-28) — are tested in
//! `agent_conversations/tests.rs`. What is tested here is the call: what it
//! records, what it refuses, and what it reports.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, Participant};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::scanning::{ArtifactType, Assignment, LibraryAssignments, Scope};
use crate::tools::tests::{block_on, closed_project, mounted};
use crate::tools::ToolRefusal;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROMPT: &str = "prompts/review.md";
const ORIGINAL: &str = "# Review\n\nThe original three paragraphs.\n";
const PROPOSED: &str = "# Review\n\nA better opening.\n\nAnd a second paragraph.\n";
const RATIONALE: &str = "The instructions bury the important step.";

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
        title: Some("Architect".into()),
    }
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    thread_id: String,
    proposed: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        write_file(&root, PROMPT, ORIGINAL);
        write_file(&root, "specifications/ui/EDT-editor.md", "# Editor\n");
        write_file(&root, "notes/plain.md", "just text\n");
        write_file(&root, "flows/build.flow", "{}\n");
        assign(
            &root,
            &[
                (PROMPT, ArtifactType::Prompt),
                ("specifications/ui/EDT-editor.md", ArtifactType::Spec),
                ("flows/build.flow", ArtifactType::Flow),
            ],
        );
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Artifact {
                artifact_id: PROMPT.into(),
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
        ConversationOrigin::stub_artifact(self.thread_id.clone(), PROMPT, false)
    }

    fn tool_for(
        &self,
        origin: ConversationOrigin,
    ) -> ProposePromptChangesTool<tauri::test::MockRuntime> {
        ProposePromptChangesTool::new(
            self.handle(),
            self.roots(),
            origin,
            agent(),
            Arc::clone(&self.proposed),
        )
    }

    fn tool(&self) -> ProposePromptChangesTool<tauri::test::MockRuntime> {
        self.tool_for(self.origin())
    }

    fn call_with(
        &self,
        path: &str,
        content: &str,
        rationale: &str,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        block_on(self.tool().call(ProposePromptChangesArgs {
            path: path.into(),
            content: content.into(),
            rationale: rationale.into(),
        }))
    }

    fn call(&self) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        self.call_with(PROMPT, PROPOSED, RATIONALE)
    }

    fn discussion(&self) -> crate::comments::Discussion {
        crate::comments::fold_artifact_discussion(&self.root(), PROMPT)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn proposals(&self) -> Vec<crate::prompt_proposals::PromptChangeProposal> {
        crate::prompt_proposals::list_proposals_impl(&self.root(), PROMPT)
    }

    fn artifact_body(&self) -> String {
        std::fs::read_to_string(self.path.join(PROMPT)).expect("artifact")
    }

    fn tree(&self) -> Vec<String> {
        let mut out = Vec::new();
        walk(&self.path, &self.path, &mut out);
        out.sort();
        out
    }
}

fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            out.push(
                path.strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn write_file(root: &crate::fs::RootFs, rel: &str, body: &str) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, body).expect("write");
}

fn assign(root: &crate::fs::RootFs, entries: &[(&str, ArtifactType)]) {
    let mut assignments = LibraryAssignments::default();
    for (rel, artifact_type) in entries {
        assignments.assignments.insert(
            (*rel).to_string(),
            Assignment {
                artifact_type: *artifact_type,
                scope: Scope::File,
            },
        );
    }
    let path = root.path().join(".synthesis");
    std::fs::create_dir_all(&path).expect("dirs");
    std::fs::write(
        path.join("library.toml"),
        toml::to_string(&assignments).expect("toml"),
    )
    .expect("write");
}

// ---------------------------------------------------------------------------
// The definition (PPC-FR-01 … PPC-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn it_is_a_portable_tool_with_no_command_or_event_of_its_own() {
    // PPC-FR-01.
    let f = Fixture::new();
    let definition = rig::tool::portable_tool_definition(&f.tool());
    assert_eq!(definition.name, NAME);
    assert_eq!(
        <ProposePromptChangesTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_prompt_changes",
    );
    const LIB: &str = include_str!("../../lib.rs");
    assert!(
        !LIB.contains(NAME),
        "no Tauri command and no event belongs to this tool",
    );
    const MODULE: &str = include_str!("../propose_prompt_changes.rs");
    assert!(!MODULE.contains("#[tauri::command]"));
    assert!(!MODULE.contains("app.emit"));
}

#[test]
fn its_definition_is_byte_identical_wherever_it_appears() {
    // PPC-FR-02.
    let one = Fixture::new();
    let two = Fixture::new();
    let a = rig::tool::portable_tool_definition(&one.tool());
    let b = rig::tool::portable_tool_definition(&two.tool_for(
        ConversationOrigin::stub_artifact(two.thread_id.clone(), "a.md", true),
    ));
    assert_eq!(a.name, b.name);
    assert_eq!(a.description, b.description);
    assert_eq!(a.parameters, b.parameters);
    assert_eq!(a.description, DESCRIPTION);

    let schema = parameters();
    assert_eq!(
        schema["required"],
        serde_json::json!(["path", "content", "rationale"]),
    );
    for (name, description) in [
        ("path", PATH_DESCRIPTION),
        ("content", CONTENT_DESCRIPTION),
        ("rationale", RATIONALE_DESCRIPTION),
    ] {
        assert_eq!(schema["properties"][name]["description"], description);
    }
}

#[test]
fn no_argument_names_a_conversation_an_agent_or_a_participant() {
    // PPC-FR-03.
    let schema = parameters();
    let properties = schema["properties"].as_object().expect("properties");
    let mut names: Vec<&String> = properties.keys().collect();
    names.sort();
    assert_eq!(names, vec!["content", "path", "rationale"]);
    for forbidden in ["thread", "conversation", "agent", "participant", "artifact_id"] {
        assert!(
            !properties.contains_key(forbidden),
            "no argument names {forbidden}",
        );
    }
}

// ---------------------------------------------------------------------------
// Recording (PPC-FR-04 … PPC-FR-06, PPC-FR-12 … PPC-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn a_successful_call_records_one_proposal_and_changes_no_file() {
    // PPC-FR-04, PPC-FR-05, and the declared side effects.
    let f = Fixture::new();
    let before = f.tree();
    let output = f.call().expect("recorded");

    assert!(output.proposed);
    let proposals = f.proposals();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].id, output.proposal_id);
    assert_eq!(
        crate::prompt_proposals::load_content_impl(&f.root(), &output.proposal_id)
            .expect("candidate")
            .content,
        PROPOSED,
        "the candidate is the content byte-for-byte",
    );
    assert_eq!(f.artifact_body(), ORIGINAL, "the artifact is untouched");

    // PPC-FR-20 / side effects: the only new files are the candidate, its
    // record, and the folder's own ignore rules.
    let after = f.tree();
    let added: Vec<&String> = after.iter().filter(|p| !before.contains(p)).collect();
    for path in &added {
        assert!(
            path.starts_with(".synthesis/"),
            "nothing outside .synthesis/ was created: {path}",
        );
    }
    assert!(
        before.iter().all(|p| after.contains(p)),
        "and nothing was deleted, renamed, or moved",
    );
}

#[test]
fn a_fragment_is_proposed_as_a_whole_file() {
    // PCP-FR-13 / PPC-FR-04: the tool applies no patch and merges nothing.
    let f = Fixture::new();
    let output = f
        .call_with(PROMPT, "Just the one paragraph I rewrote.\n", RATIONALE)
        .expect("recorded");
    assert_eq!(
        crate::prompt_proposals::load_content_impl(&f.root(), &output.proposal_id)
            .expect("candidate")
            .content,
        "Just the one paragraph I rewrote.\n",
    );
}

#[test]
fn a_path_naming_nothing_the_project_holds_is_refused_and_creates_nothing() {
    // PPC-FR-05, PPC-FR-06.
    let f = Fixture::new();
    for path in ["prompts/missing.md", "/etc/hosts", "../outside.md", "prompts"] {
        let refusal = f
            .call_with(path, PROPOSED, RATIONALE)
            .expect_err("refused");
        assert_eq!(refusal, ToolRefusal::PromptProposalPathMissing, "{path}");
        assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
        assert!(refusal.retryable());
        assert!(f.proposals().is_empty());
    }
    assert!(!f.path.join("prompts/missing.md").exists());
    assert!(!f.path.join("outside.md").exists());
    assert_eq!(
        f.discussion().comments.len(),
        1,
        "and no comment was posted for any of them",
    );
}

#[test]
fn a_file_of_another_type_is_refused_distinctly_from_a_missing_one() {
    // PPC-FR-06.
    let f = Fixture::new();
    for path in [
        "specifications/ui/EDT-editor.md",
        "flows/build.flow",
        "notes/plain.md",
    ] {
        let refusal = f
            .call_with(path, "different\n", RATIONALE)
            .expect_err("refused");
        assert_eq!(refusal, ToolRefusal::NotAPromptArtifact, "{path}");
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        assert_ne!(
            refusal.to_string(),
            ToolRefusal::PromptProposalPathMissing.to_string(),
            "the two messages differ",
        );
    }
    assert!(f.proposals().is_empty());

    // …and assigning the spec file the type `prompt` makes the same call
    // succeed.
    assign(
        &f.root(),
        &[
            (PROMPT, ArtifactType::Prompt),
            ("specifications/ui/EDT-editor.md", ArtifactType::Prompt),
        ],
    );
    f.call_with("specifications/ui/EDT-editor.md", "# Editor\n\nBetter.\n", RATIONALE)
        .expect("recorded");
}

#[test]
fn a_turn_that_is_not_about_a_file_of_the_project_refuses_for_good() {
    // PPC-FR-07.
    let f = Fixture::new();
    for origin in [
        ConversationOrigin::stub_draft(f.thread_id.clone(), "draft-1", false),
        ConversationOrigin::stub_note(f.thread_id.clone(), "note-1"),
    ] {
        let refusal = block_on(f.tool_for(origin).call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            content: PROPOSED.into(),
            rationale: RATIONALE.into(),
        }))
        .expect_err("refused");
        assert_eq!(refusal, ToolRefusal::NotAnArtifactConversation);
        assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
        assert!(!refusal.retryable());
        assert!(refusal.to_string().contains("will not succeed"));
    }
    assert!(f.proposals().is_empty());
}

#[test]
fn a_blank_rationale_and_a_no_change_candidate_are_told_apart() {
    // PPC-FR-08.
    let f = Fixture::new();
    let blank = f.call_with(PROMPT, PROPOSED, "   ").expect_err("refused");
    let same = f.call_with(PROMPT, ORIGINAL, RATIONALE).expect_err("refused");
    for refusal in [&blank, &same] {
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
    }
    assert_ne!(blank.to_string(), same.to_string());
    assert_eq!(
        blank,
        ToolRefusal::InvalidArguments(crate::tools::PROMPT_PROPOSAL_RATIONALE_BLANK),
    );
    assert_eq!(
        same,
        ToolRefusal::InvalidArguments(crate::tools::PROPOSAL_NO_CHANGE),
    );
    assert!(f.proposals().is_empty());
    assert_eq!(f.artifact_body(), ORIGINAL);

    // A candidate differing from the file only in its line endings changes
    // nothing on disk, so it is the same refusal.
    let crlf = ORIGINAL.replace('\n', "\r\n");
    assert_eq!(
        f.call_with(PROMPT, &crlf, RATIONALE).expect_err("refused"),
        ToolRefusal::InvalidArguments(crate::tools::PROPOSAL_NO_CHANGE),
    );

    // …and none of this tool's refusals refuses a `content` for being empty.
    const MODULE: &str = include_str!("../propose_prompt_changes.rs");
    assert!(
        !MODULE.contains("content.is_empty()"),
        "no refusal turns on an empty candidate",
    );
    for text in [
        crate::tools::PROMPT_PROPOSAL_RATIONALE_BLANK,
        crate::tools::PROPOSAL_NO_CHANGE,
        crate::tools::PROMPT_PROPOSAL_PATH_MISSING,
        crate::tools::NOT_A_PROMPT_ARTIFACT,
        CONTENT_DESCRIPTION,
    ] {
        assert!(
            !text.to_lowercase().contains("empty proposal"),
            "nothing states that an empty candidate is invalid: {text}",
        );
    }
}

#[test]
fn an_empty_candidate_is_recorded_like_any_other() {
    // PCR-FR-02, PCP-FR-09, PCP-FR-13 / PPC-FR-08.
    let f = Fixture::new();
    let output = f.call_with(PROMPT, "", RATIONALE).expect("recorded");
    assert_eq!(
        crate::prompt_proposals::load_content_impl(&f.root(), &output.proposal_id)
            .expect("candidate")
            .content,
        "",
    );
    assert_eq!(f.discussion().comments.len(), 2, "one comment was posted");
    assert_eq!(f.artifact_body(), ORIGINAL);
}

#[test]
fn a_pending_proposal_refuses_the_next_call_retryably() {
    // PPC-FR-09, PPC-FR-18.
    let f = Fixture::new();
    f.call().expect("recorded");
    let refusal = f
        .call_with(PROMPT, "# Review\n\nAnother.\n", RATIONALE)
        .expect_err("refused");
    assert_eq!(refusal, ToolRefusal::PromptProposalPending);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(refusal.retryable());
    let message = refusal.to_string();
    assert!(message.contains("still waiting on the author"));
    assert!(message.contains("Say so plainly in your reply"));
    assert!(message.contains("do not describe this change as proposed"));
    assert_eq!(f.proposals().len(), 1, "nothing was recorded");

    // A second instance for a second agent sees the same refusal — the rule is
    // the artifact's rather than the instance's.
    let other = ProposePromptChangesTool::new(
        f.handle(),
        crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
        f.origin(),
        Participant::Agent {
            agent_id: "agent-2".into(),
            handle: "sec".into(),
            model: Some("m".into()),
            title: Some("Security".into()),
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(
        block_on(other.call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            content: "# Review\n\nMine.\n".into(),
            rationale: RATIONALE.into(),
        }))
        .expect_err("refused"),
        ToolRefusal::PromptProposalPending,
    );

    // A different prompt artifact has its own slot.
    let second = "prompts/other.md";
    write_file(&f.root(), second, "# Other\n");
    assign(
        &f.root(),
        &[
            (PROMPT, ArtifactType::Prompt),
            (second, ArtifactType::Prompt),
        ],
    );
    f.call_with(second, "# Other\n\nBetter.\n", RATIONALE)
        .expect("a second artifact succeeds");
}

#[test]
fn a_locked_conversation_refuses_for_good_and_records_nothing() {
    // PPC-FR-10, PPC-FR-16.
    let f = Fixture::new();
    crate::comments::set_lock_to(
        &f.root(),
        crate::comments::ThreadRef::artifact_discussion(PROMPT),
        &f.thread_id,
        true,
        &human(),
        "2026-01-01T00:05:00Z",
    )
    .expect("lock");

    let refusal = f.call().expect_err("refused");
    assert_eq!(refusal, ToolRefusal::ProposalConversationLocked);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable());
    assert!(refusal.to_string().contains("will not succeed"));
    assert!(f.proposals().is_empty());
    assert_eq!(f.discussion().comments.len(), 1);
    // PPC-FR-16: a refusal ends nothing, so the loop is left running.
    assert!(!f.proposed.load(Ordering::SeqCst));
}

#[test]
fn a_candidate_that_cannot_be_written_is_the_retryable_other_refusal() {
    // PPC-FR-12: the candidate and the comment are one act, and a
    // failure of either produces neither.
    let f = Fixture::new();
    let synthesis = f.path.join(".synthesis");
    let restore = std::fs::metadata(&synthesis).expect("dir").permissions();
    let mut sealed = restore.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        sealed.set_mode(0o500);
    }
    #[cfg(not(unix))]
    sealed.set_readonly(true);
    std::fs::set_permissions(&synthesis, sealed).expect("seal");

    let refusal = f.call().expect_err("refused");
    std::fs::set_permissions(&synthesis, restore).expect("unseal");

    assert_eq!(refusal, ToolRefusal::ProposalNotRecorded);
    assert_eq!(refusal.kind(), ToolErrorKind::Other);
    assert!(refusal.retryable(), "a write that failed once may succeed next time");
    assert!(f.proposals().is_empty(), "no candidate was left behind");
    assert_eq!(
        f.discussion().comments.len(),
        1,
        "and the conversation gained no line",
    );
    assert!(!f.proposed.load(Ordering::SeqCst), "the turn did not end");
}

#[test]
fn two_turns_racing_one_artifact_record_exactly_one_proposal() {
    // PPC-FR-18 / PPC-FR-09, PPC-FR-18: two turns for two agents in flight on
    // one prompt artifact, neither instance observing the other's call.
    let f = Fixture::new();
    let outcomes: Vec<Result<ProposePromptChangesOutput, ToolRefusal>> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = ["arch", "sec"]
                .into_iter()
                .map(|nickname| {
                    let tool = ProposePromptChangesTool::new(
                        f.handle(),
                        crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
                        f.origin(),
                        Participant::Agent {
                            agent_id: format!("agent-{nickname}"),
                            handle: nickname.into(),
                            model: Some("m".into()),
                            title: None,
                        },
                        // Each turn holds its own flag, so neither observes the
                        // other's call (PPC-FR-18).
                        Arc::new(AtomicBool::new(false)),
                    );
                    scope.spawn(move || {
                        block_on(tool.call(ProposePromptChangesArgs {
                            path: PROMPT.into(),
                            content: format!("# Review\n\nFrom {nickname}.\n"),
                            rationale: RATIONALE.into(),
                        }))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("joined")).collect()
        });

    assert_eq!(
        outcomes.iter().filter(|o| o.is_ok()).count(),
        1,
        "exactly one proposal was recorded: {outcomes:?}",
    );
    assert!(
        outcomes
            .iter()
            .any(|o| o.as_ref().err() == Some(&ToolRefusal::PromptProposalPending)),
        "and the other refused for the pending one: {outcomes:?}",
    );
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.discussion().comments.len(), 2);
}

#[test]
fn the_comment_is_the_rationale_and_one_reference_and_nothing_else() {
    // CMS-FR-66, CMS-FR-41, CMS-FR-65 / CMT-FR-48 / PPC-FR-13, PPC-FR-14.
    let f = Fixture::new();
    let output = f.call().expect("recorded");

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), 2, "exactly one comment was produced");
    let posted = thread.comments.last().expect("the agent's comment");
    assert_eq!(posted.body, RATIONALE);
    for forbidden in [PROMPT, &output.proposal_id, "proposal", "accept"] {
        assert!(
            !posted.body.contains(forbidden),
            "the body carries no {forbidden}",
        );
    }
    assert_eq!(posted.attachments.len(), 1);
    match &posted.attachments[0] {
        crate::comments::Attachment::PromptProposal {
            proposal_id,
            artifact_id,
            path,
        } => {
            assert_eq!(proposal_id, &output.proposal_id);
            assert_eq!(artifact_id, PROMPT);
            assert_eq!(path, PROMPT);
        }
        other => panic!("expected a prompt proposal reference, got {other:?}"),
    }
    assert_eq!(posted.author, agent(), "the turn's own agent participant");
}

#[test]
fn the_output_carries_the_identity_and_no_part_of_the_candidate() {
    // PPC-FR-15.
    let f = Fixture::new();
    let output = f.call().expect("recorded");
    let encoded = serde_json::to_value(&output).expect("json");
    assert_eq!(encoded["proposed"], serde_json::json!(true));
    assert_eq!(encoded["proposal_id"], serde_json::json!(output.proposal_id));
    let text = encoded.to_string();
    assert!(!text.contains("better opening"), "no part of the candidate");
    assert!(!text.contains(RATIONALE));
}

#[test]
fn no_project_open_is_the_shared_refusal() {
    // PPC-FR-17.
    let app = closed_project();
    let dir = TempDir::new().expect("tempdir");
    let tool = ProposePromptChangesTool::new(
        app.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(dir.path()), store: crate::fs::RootFs::for_root(dir.path()) },
        ConversationOrigin::stub_artifact("t1", PROMPT, false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    );
    let refusal = block_on(tool.call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        content: PROPOSED.into(),
        rationale: RATIONALE.into(),
    }))
    .expect_err("refused");
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(!refusal.retryable());
    assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
}

#[test]
fn each_call_records_the_tool_and_the_path_and_nothing_a_model_composed() {
    // PPC-FR-19 / PPC-FR-20 / PPC-FR-19, PPC-FR-20.
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        content: PROPOSED.into(),
        rationale: RATIONALE.into(),
    }))
    .expect("recorded");
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        content: "# Review\n\nAnother.\n".into(),
        rationale: RATIONALE.into(),
    }))
    .expect_err("refused");

    for domain in [Domain::Ai, Domain::Backend] {
        let records = records(&BUFFER, domain);
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
            assert!(fields.contains(NAME), "names the tool");
            assert!(fields.contains(PROMPT), "and the path it rewrote");
            let whole = format!("{} {}", record.message, fields);
            assert!(!whole.contains("better opening"), "no content: {whole}");
            assert!(!whole.contains(RATIONALE), "no rationale: {whole}");
            assert!(!whole.contains("original three"), "no file text: {whole}");
        }
        assert!(
            serde_json::to_string(&warn.fields)
                .expect("fields")
                .contains("proposal_pending"),
            "the refusal names its reason",
        );
    }
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

#[test]
fn it_reaches_nothing_of_the_draft_proposal_tool() {
    // CVL-FR-08 / PPC-FR-21.
    // A mention in a comment is how the module says the two are separate; a path
    // expression is what would make it false.
    const MODULE: &str = include_str!("../propose_prompt_changes.rs");
    for reached in [
        "propose_draft_changes::",
        "draft_proposals::",
        "drafts::",
        "draft_id",
    ] {
        assert!(!MODULE.contains(reached), "this tool reaches no {reached}");
    }
    // CVL-FR-08: the two are attached to disjoint origin kinds, so no turn
    // carries both — asserted where the attachment is decided.
    const LOOP: &str = include_str!("../../agent_conversations.rs");
    assert!(
        LOOP.contains("} else if origin.is_artifact() {"),
        "the two proposal tools are attached exclusively",
    );
}
