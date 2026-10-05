//! Tests for `PCP-prompt-change-proposals.md`.
//!
//! Every function under test here is pure over a `RootFs` and an `AppHandle`, so
//! none of this needs a running window. What the *commands* add on top — the
//! identity resolution — is asserted where a runtime exists: the tool's own
//! refusals in `tools/propose_prompt_changes/tests.rs`.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, Participant};
use crate::scanning::{Assignment, ArtifactType, LibraryAssignments, Scope};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROMPT: &str = "prompts/review.md";
const ORIGINAL: &str = "# Review\n\n## Steps\n\nRead the file, summarise it and file a note.\n";
const PROPOSED: &str = "# Review\n\n## Steps\n\nRead the file.\n\nSummarise it, then file a note.\n";

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
        title: Some("Architect".into()),
    }
}

/// A project holding one prompt artifact and one discussion on it.
struct Fixture {
    _dir: TempDir,
    app: tauri::App<tauri::test::MockRuntime>,
    root: fs::RootFs,
    thread_id: String,
    tracker: ContentTracker,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let root = fs::RootFs::for_root(dir.path());
        write_file(&root, PROMPT, ORIGINAL);
        assign(&root, &[(PROMPT, ArtifactType::Prompt)]);
        let thread = comments::open_discussion_in(
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
            _dir: dir,
            app: crate::tools::tests::mock_app(),
            root,
            thread_id: thread.id,
            tracker: ContentTracker::default(),
        }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    /// Every `prompt-change-proposals-changed` payload emitted from now on.
    fn watch(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen(PROMPT_PROPOSALS_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
            }
        });
        seen
    }

    fn propose_against(
        &self,
        artifact_id: &str,
        content: &str,
    ) -> Result<PromptChangeProposal, RecordRefusal> {
        record_prompt_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            NewPromptProposal {
                artifact_id,
                content,
                rationale: "The instructions bury the important step.",
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }

    fn propose(&self, content: &str) -> Result<PromptChangeProposal, RecordRefusal> {
        self.propose_against(PROMPT, content)
    }

    /// The proposal every test that needs a standing one starts from.
    fn pending(&self) -> PromptChangeProposal {
        self.propose(PROPOSED).expect("recorded")
    }

    fn accept(&self, id: &str) -> Result<PromptDecisionOutcome, String> {
        self.accept_with(id, None)
    }

    fn accept_with(
        &self,
        id: &str,
        feedback: Option<&str>,
    ) -> Result<PromptDecisionOutcome, String> {
        decide(
            &self.handle(),
            &self.root,
            &self.root,
            id,
            Decision::Accept,
            feedback,
            &human(),
            &self.tracker,
        )
    }

    fn decline(&self, id: &str) -> Result<PromptDecisionOutcome, String> {
        self.decline_with(id, None)
    }

    fn decline_with(
        &self,
        id: &str,
        feedback: Option<&str>,
    ) -> Result<PromptDecisionOutcome, String> {
        decide(
            &self.handle(),
            &self.root,
            &self.root,
            id,
            Decision::Decline,
            feedback,
            &human(),
            &self.tracker,
        )
    }

    fn dir(&self) -> std::path::PathBuf {
        proposals_dir(&self.root).expect("dir")
    }

    fn folder_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir())
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn artifact_body(&self) -> String {
        std::fs::read_to_string(self.root.path().join(PROMPT)).expect("artifact")
    }

    fn discussion(&self) -> comments::Discussion {
        comments::fold_artifact_discussion(&self.root, PROMPT)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn lock(&self) {
        comments::set_lock_to(
            &self.root,
            comments::ThreadRef::artifact_discussion(PROMPT),
            &self.thread_id,
            true,
            &human(),
            "2026-01-01T00:05:00Z",
        )
        .expect("lock");
    }

    /// What `complete_prompt_change_decision` answers with — the reconciliation
    /// of PCP-FR-14 and the outcome it settled.
    ///
    /// The command itself resolves no identity and takes no store, so this is
    /// the whole of it minus the `require_root` every command shares.
    fn complete(&self, proposal_id: &str) -> Result<PromptDecisionOutcome, String> {
        let _guard = artifact_lock(&read_proposal(&self.root, proposal_id)?.artifact_id);
        let reconciled = reconcile_locked(&self.handle(), &self.root, &self.root, proposal_id)?;
        let record = read_proposal(&self.root, proposal_id)?;
        let (_, discussion) = locate_thread(&self.root, &self.root, &record.thread_id)?;
        let comment_id = match &reconciled {
            Reconciliation::Settled {
                proposal_id: completed,
                comment_id,
            } if completed == proposal_id => comment_id.clone(),
            _ => None,
        };
        Ok(PromptDecisionOutcome {
            proposal: record,
            comment_id,
            origin_kind: origin_kind_of(discussion),
        })
    }

    /// What the `list_prompt_change_proposals` command answers with — the
    /// reconciliation of PCP-FR-14 and then the list.
    fn list(&self) -> Vec<PromptChangeProposal> {
        for record in list_proposals_impl(&self.root, PROMPT) {
            let _ = reconcile(&self.handle(), &self.root, &self.root, &record.id);
        }
        list_proposals_impl(&self.root, PROMPT)
    }
}

fn write_file(root: &fs::RootFs, rel: &str, body: &str) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, body).expect("write");
}

/// ASC-FR-06 level 1: a per-file assignment, which is how a test says a file's
/// resolved type is exactly `prompt` without relying on a naming convention.
fn assign(root: &fs::RootFs, entries: &[(&str, ArtifactType)]) {
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
// Staging an interrupted acceptance
// ---------------------------------------------------------------------------

/// Leave the folder in the state a process killed inside an acceptance leaves
/// it: every write the transaction of PCP-FR-14 makes, up to or short of the
/// commit point, with the journal standing.
///
/// Replayed rather than run-and-rewound, because the one thing that
/// distinguishes the two readings is whether the artifact write landed, and
/// running the real transaction always lands it.
fn stage_for_test(f: &Fixture, proposal: &PromptChangeProposal, committed: bool) -> Journal {
    let dir = f.dir();
    let prior = std::fs::read(f.root.path().join(PROMPT)).expect("prior");
    let candidate = load_content_impl(&f.root, &proposal.id).expect("candidate");
    let normalised = normalise_for_write(&f.root, &candidate.content);
    let journal = Journal {
        operation_id: new_note_id(),
        proposal_id: proposal.id.clone(),
        artifact_id: proposal.artifact_id.clone(),
        path: proposal.path.clone(),
        prior_sha256: fs::sha256_bytes(&prior),
        candidate_sha256: fs::sha256_bytes(normalised.as_bytes()),
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: proposal.thread_id.clone(),
        comment_body: format!("Accepted the proposed change to `{}`.", proposal.path),
        created_at: now_rfc3339(),
        comment_by: human(),
    };
    std::fs::write(dir.join(format!("{}.prior", proposal.id)), &prior).expect("prior");
    f.root
        .write_toml_atomic(dir.join(format!("{}.journal", proposal.id)), &journal)
        .expect("journal");
    let decided = PromptChangeProposal {
        state: PromptProposalState::Accepted,
        decided_at: Some(now_rfc3339()),
        comment_owed: true,
        ..proposal.clone()
    };
    f.root
        .write_toml_atomic(dir.join(format!("{}.toml", proposal.id)), &decided)
        .expect("record");
    if committed {
        write_file(&f.root, PROMPT, &normalised);
    }
    journal
}

mod accepting;
mod candidate;
mod containment;
mod deciding;
mod recording;
