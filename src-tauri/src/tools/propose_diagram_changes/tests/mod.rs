//! Tests for `PDM-propose-diagram-changes-tool.md`.
//!
//! The group-wide conventions this tool shares with the others are checked once
//! in `tools/tests.rs`. The check of a new source alone is tested in
//! `validate.rs`. What follows is the call: what it records, what it joins,
//! what it refuses, and what it logs.
//!
//! The storage consequences of a recorded proposal belong to
//! `DCP-draft-change-proposals.md` and `PCP-prompt-change-proposals.md`. The
//! turn-level consequences (the loop ending, the tool set of each origin) are
//! tested in `agent_conversations/tests/`.
//!
//! Each scenario runs for a draft origin and for an artifact origin where the
//! rule applies to both. The fixture below builds either one.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;

use super::*;
use crate::comments::{Attachment, DiscussionTarget, Participant, ProposalUpdateCounts};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::scanning::{ArtifactType, Assignment, LibraryAssignments, Scope};
use crate::text_hunks::hunks::ProposalHunk;
use crate::tools::tests::{block_on, closed_project, mounted};
use crate::tools::ToolRefusal;

mod contract;
mod join;
mod log;
mod record;
mod refusals;
mod updates;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The path of the draft's prompt.
pub(super) const DRAFT_FILE: &str = "spec.md";
/// The path of the prompt artifact.
pub(super) const PROMPT: &str = "prompts/review.md";

/// A file with one Mermaid diagram.
pub(super) const BODY: &str =
    "# Spec\n\nIntro line.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd line.\n";
/// A file with two Mermaid diagrams that hold the same source.
pub(super) const TWICE: &str = "# Spec\n\nFirst section.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nSecond section.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd line.\n";
/// A file with a fence of another language and one Mermaid diagram.
pub(super) const OTHER_FENCE: &str =
    "# Spec\n\n```calm\nnodes: [a, b]\n```\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd line.\n";

/// The source of the diagram in `BODY`.
pub(super) const SRC_A: &str = "flowchart TD\n  A --> B";
/// A valid new source for it.
pub(super) const SRC_A2: &str = "flowchart TD\n  A --> C";
/// Another valid new source.
pub(super) const SRC_A3: &str = "flowchart TD\n  A --> D";
/// A valid source for an added diagram.
pub(super) const SRC_B: &str = "graph LR\n  X --> Y";

pub(super) const RATIONALE: &str =
    "The diagram hides the main path. <spoken>Short reason.</spoken>";
pub(super) const UPDATE_RATIONALE: &str =
    "The discussion asked for this. <spoken>I changed it.</spoken>";

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        // CMS-FR-65: the snapshot the turn stamped, which this tool must carry
        // onto the comment it appends unchanged.
        title: Some("Architect".into()),
    }
}

pub(super) fn other_agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-2".into(),
        handle: "sec".into(),
        model: Some("m".into()),
        title: Some("Security".into()),
    }
}

/// The origin kinds the tool is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Subject {
    Draft,
    Artifact,
}

pub(super) const SUBJECTS: [Subject; 2] = [Subject::Draft, Subject::Artifact];

/// One proposal of the file, as the stores hold it. The state of a hunk is its
/// name, so the two stores compare in one type.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Standing {
    pub id: String,
    pub hunks: Vec<ProposalHunk>,
    /// The id, the state, and the diagram flag of each ledger row.
    pub rows: Vec<(String, String, bool)>,
}

/// Everything a refused call must leave as it was.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct World {
    pub proposals: Vec<Standing>,
    pub comments: usize,
    pub body: String,
}

pub(super) struct Fixture {
    open: crate::tools::tests::Fixture,
    pub path: std::path::PathBuf,
    pub subject: Subject,
    pub draft_id: String,
    pub thread_id: String,
    pub proposed: Arc<AtomicBool>,
}

impl Fixture {
    pub fn new(subject: Subject) -> Fixture {
        Fixture::with_body(subject, BODY)
    }

    pub fn with_body(subject: Subject, body: &str) -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let (draft_id, target) = match subject {
            Subject::Draft => {
                let created =
                    crate::drafts::create_draft_at_root(&root, Some("spec")).expect("draft");
                crate::drafts::save_draft_file_impl(&root, &created.draft.id, &created.file, body)
                    .expect("seed");
                assert_eq!(created.file, DRAFT_FILE);
                let target = DiscussionTarget::Draft { draft_id: created.draft.id.clone() };
                (created.draft.id, target)
            }
            Subject::Artifact => {
                write_file(&root, PROMPT, body);
                write_file(&root, "notes/plain.md", "just text\n");
                assign(&root, &[(PROMPT, ArtifactType::Prompt)]);
                (String::new(), DiscussionTarget::Artifact { artifact_id: PROMPT.into() })
            }
        };
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &target,
            None,
            "@arch what would you change?".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            subject,
            draft_id,
            thread_id: thread.id,
            proposed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.open.handle()
    }

    pub fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    /// The pair a conversation-bound tool holds: one temporary directory
    /// serving as both the worktree and the repository machine store.
    pub fn roots(&self) -> crate::agent_conversations::OwnedRoots {
        crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() }
    }

    /// The path a call names: the draft's prompt, or the artifact.
    pub fn file(&self) -> &'static str {
        match self.subject {
            Subject::Draft => DRAFT_FILE,
            Subject::Artifact => PROMPT,
        }
    }

    pub fn origin(&self) -> ConversationOrigin {
        match self.subject {
            Subject::Draft => {
                ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
            }
            Subject::Artifact => {
                ConversationOrigin::stub_artifact(self.thread_id.clone(), PROMPT, false)
            }
        }
    }

    pub fn tool_for(
        &self,
        origin: ConversationOrigin,
        author: Participant,
    ) -> ProposeDiagramChangesTool<tauri::test::MockRuntime> {
        ProposeDiagramChangesTool::new(
            self.handle(),
            self.roots(),
            origin,
            author,
            Arc::clone(&self.proposed),
        )
    }

    pub fn tool(&self) -> ProposeDiagramChangesTool<tauri::test::MockRuntime> {
        self.tool_for(self.origin(), agent())
    }

    pub fn call(
        &self,
        args: ProposeDiagramChangesArgs,
    ) -> Result<ProposeDiagramChangesOutput, ToolRefusal> {
        block_on(self.tool().call(args))
    }

    /// A call with `diagrams` that names the file of this fixture.
    pub fn args(&self, diagrams: Vec<DiagramArg>) -> ProposeDiagramChangesArgs {
        ProposeDiagramChangesArgs {
            path: self.file().into(),
            rationale: RATIONALE.into(),
            updates: None,
            diagrams,
        }
    }

    /// A call with `updates` that names the file of this fixture.
    pub fn updating(&self, updates: Vec<UpdateArg>) -> ProposeDiagramChangesArgs {
        ProposeDiagramChangesArgs {
            path: self.file().into(),
            rationale: UPDATE_RATIONALE.into(),
            updates: Some(updates),
            diagrams: Vec::new(),
        }
    }

    pub fn propose(
        &self,
        diagrams: Vec<DiagramArg>,
    ) -> Result<ProposeDiagramChangesOutput, ToolRefusal> {
        self.call(self.args(diagrams))
    }

    pub fn discussion(&self) -> crate::comments::Discussion {
        let folded = match self.subject {
            Subject::Draft => crate::comments::fold_discussion(&self.root(), &self.draft_id),
            Subject::Artifact => crate::comments::fold_artifact_discussion(&self.root(), PROMPT),
        };
        folded.into_iter().find(|t| t.id == self.thread_id).expect("thread")
    }

    pub fn comment_count(&self) -> usize {
        self.discussion().comments.len()
    }

    /// The proposals of the file, as the stores hold them.
    pub fn proposals(&self) -> Vec<Standing> {
        let root = self.root();
        match self.subject {
            Subject::Draft => crate::draft_proposals::list_proposals_impl(&root, &self.draft_id)
                .expect("list")
                .into_iter()
                .map(|p| Standing {
                    hunks: crate::draft_proposals::load_hunks_impl(&root, &p.id)
                        .expect("hunks")
                        .hunks,
                    rows: p
                        .ledger
                        .iter()
                        .map(|r| (r.id.clone(), format!("{:?}", r.state), r.diagram))
                        .collect(),
                    id: p.id,
                })
                .collect(),
            Subject::Artifact => crate::prompt_proposals::list_proposals_impl(&root, PROMPT)
                .into_iter()
                .map(|p| Standing {
                    hunks: crate::prompt_proposals::load_hunks_impl(&root, &p.id)
                        .expect("hunks")
                        .hunks,
                    rows: p
                        .ledger
                        .iter()
                        .map(|r| (r.id.clone(), format!("{:?}", r.state), r.diagram))
                        .collect(),
                    id: p.id,
                })
                .collect(),
        }
    }

    /// The one proposal the file holds.
    pub fn one(&self) -> Standing {
        let mut all = self.proposals();
        assert_eq!(all.len(), 1, "the file holds one proposal");
        all.remove(0)
    }

    pub fn body(&self) -> String {
        match self.subject {
            Subject::Draft => {
                crate::drafts::load_draft_file_impl(&self.root(), &self.draft_id, DRAFT_FILE)
                    .expect("file")
                    .body
            }
            Subject::Artifact => std::fs::read_to_string(self.path.join(PROMPT)).expect("file"),
        }
    }

    pub fn world(&self) -> World {
        World { proposals: self.proposals(), comments: self.comment_count(), body: self.body() }
    }

    /// What a refused call must leave: the proposals, the comments, and the
    /// file as they were, and the turn still running.
    pub fn assert_untouched(&self, before: &World) {
        assert_eq!(&self.world(), before, "a refused call writes nothing");
        assert!(!self.proposed.load(Ordering::SeqCst), "a refusal ends nothing (PDM-FR-JLYU)");
    }

    /// Hold one hunk for discussion, which leaves it undecided and not pending.
    pub fn hold(&self, proposal_id: &str, hunk_id: &str) {
        match self.subject {
            Subject::Draft => {
                crate::draft_proposals::set_hunk_state(
                    &self.root(),
                    &self.draft_id,
                    proposal_id,
                    hunk_id,
                    crate::draft_proposals::hunks::HunkState::Discussing,
                )
                .expect("held");
            }
            Subject::Artifact => {
                crate::prompt_proposals::hold_hunk_impl(&self.root(), proposal_id, hunk_id, true)
                    .expect("held");
            }
        }
    }

    pub fn resolve_conversation(&self) {
        let thread = match self.subject {
            Subject::Draft => crate::comments::ThreadRef::discussion(&self.draft_id),
            Subject::Artifact => crate::comments::ThreadRef::artifact_discussion(PROMPT),
        };
        crate::comments::set_resolution_to(
            &self.root(),
            thread,
            &self.thread_id,
            true,
            &human(),
            "2026-01-01T00:05:00Z",
        )
        .expect("resolve");
    }

    /// The counts and the proposal id that the attachment of the last comment
    /// carries, and the kind of the attachment: `proposal` or
    /// `prompt_proposal`.
    pub fn last_attachment(&self) -> (&'static str, String, Option<ProposalUpdateCounts>) {
        let thread = self.discussion();
        let comment = thread.comments.last().expect("a comment");
        assert_eq!(comment.attachments.len(), 1, "exactly one attachment");
        match &comment.attachments[0] {
            Attachment::Proposal { proposal_id, update, path, draft_id } => {
                assert_eq!(path, DRAFT_FILE);
                assert_eq!(draft_id, &self.draft_id);
                ("proposal", proposal_id.clone(), *update)
            }
            Attachment::PromptProposal { proposal_id, update, path, artifact_id } => {
                assert_eq!(path, PROMPT);
                assert_eq!(artifact_id, PROMPT);
                ("prompt_proposal", proposal_id.clone(), *update)
            }
            other => panic!("expected a proposal reference, got {other:?}"),
        }
    }

    /// The attachment kind that this origin must carry (CMS-FR-60, CMS-FR-66).
    pub fn attachment_kind(&self) -> &'static str {
        match self.subject {
            Subject::Draft => "proposal",
            Subject::Artifact => "prompt_proposal",
        }
    }

    /// Offer one text change through the text tool of this origin, as the
    /// agent `author`.
    pub fn offer_text_as(
        &self,
        author: Participant,
        before: &str,
        after: &str,
    ) -> Result<(), ToolRefusal> {
        let hunks = serde_json::json!([{ "before": before, "after": after }]);
        self.text_call(author, serde_json::json!({ "hunks": hunks }))
    }

    /// Offer one text change through the text tool of this origin.
    pub fn offer_text(&self, before: &str, after: &str) -> Result<(), ToolRefusal> {
        self.offer_text_as(agent(), before, after)
    }

    /// Send `updates` through the text tool of this origin.
    pub fn text_updates(&self, updates: serde_json::Value) -> Result<(), ToolRefusal> {
        self.text_call(agent(), serde_json::json!({ "updates": updates }))
    }

    /// One call of the text tool of this origin. `extra` holds `hunks` or
    /// `updates`, and the call adds the path and the rationale.
    fn text_call(&self, author: Participant, extra: serde_json::Value) -> Result<(), ToolRefusal> {
        let mut value = serde_json::json!({ "path": self.file(), "rationale": RATIONALE });
        value
            .as_object_mut()
            .expect("an object")
            .extend(extra.as_object().expect("an object").clone());
        // A text tool of its own turn, whose flag no test reads.
        let flag = Arc::new(AtomicBool::new(false));
        match self.subject {
            Subject::Draft => {
                use crate::tools::propose_draft_changes::{
                    ProposeDraftChangesArgs, ProposeDraftChangesTool,
                };
                let args: ProposeDraftChangesArgs =
                    serde_json::from_value(value).expect("arguments");
                let tool = ProposeDraftChangesTool::new(
                    self.handle(),
                    self.roots(),
                    self.origin(),
                    author,
                    flag,
                );
                block_on(tool.call(args)).map(|_| ())
            }
            Subject::Artifact => {
                use crate::tools::propose_prompt_changes::{
                    ProposePromptChangesArgs, ProposePromptChangesTool,
                };
                let args: ProposePromptChangesArgs =
                    serde_json::from_value(value).expect("arguments");
                let tool = ProposePromptChangesTool::new(
                    self.handle(),
                    self.roots(),
                    self.origin(),
                    author,
                    flag,
                );
                block_on(tool.call(args)).map(|_| ())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The arguments
// ---------------------------------------------------------------------------

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// A diagram change with the fields given.
pub(super) fn diagram(
    before: Option<&str>,
    after: Option<&str>,
    after_text: Option<&str>,
) -> DiagramArg {
    DiagramArg {
        before: before.and_then(some),
        after: after.and_then(some),
        after_text: after_text.and_then(some),
        lead: None,
        trail: None,
        note: None,
    }
}

/// A change of the diagram whose source is `before`.
pub(super) fn change(before: &str, after: &str) -> DiagramArg {
    diagram(Some(before), Some(after), None)
}

/// The removal of the diagram whose source is `before`.
pub(super) fn removal(before: &str) -> DiagramArg {
    diagram(Some(before), None, None)
}

/// The addition of a diagram after the line that holds `follows`.
pub(super) fn addition(after: &str, follows: &str) -> DiagramArg {
    diagram(None, Some(after), Some(follows))
}

pub(super) fn with_lead(mut item: DiagramArg, lead: &str) -> DiagramArg {
    item.lead = some(lead);
    item
}

pub(super) fn with_trail(mut item: DiagramArg, trail: &str) -> DiagramArg {
    item.trail = some(trail);
    item
}

/// One entry of `updates`.
pub(super) fn entry(
    action: &str,
    id: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
    after_text: Option<&str>,
) -> UpdateArg {
    UpdateArg {
        action: some(action),
        id: id.and_then(some),
        before: before.and_then(some),
        after: after.and_then(some),
        after_text: after_text.and_then(some),
        ..UpdateArg::default()
    }
}

pub(super) fn write_file(root: &crate::fs::RootFs, rel: &str, body: &str) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, body).expect("write");
}

fn assign(root: &crate::fs::RootFs, entries: &[(&str, ArtifactType)]) {
    let mut assignments = LibraryAssignments::default();
    for (rel, artifact_type) in entries {
        assignments.assignments.insert(
            (*rel).to_string(),
            Assignment { artifact_type: *artifact_type, scope: Scope::File },
        );
    }
    let path = root.path().join(".synthesis");
    std::fs::create_dir_all(&path).expect("dirs");
    std::fs::write(path.join("library.toml"), toml::to_string(&assignments).expect("toml"))
        .expect("write");
}

/// The records `buffer` holds under `domain`, newest last.
pub(super) fn records(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
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

/// Every file under `root`, as a sorted list of relative paths.
pub(super) fn tree_of(root: &std::path::Path) -> Vec<String> {
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
                    path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}
