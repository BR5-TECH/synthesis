//! Tests for `DCP-draft-change-proposals.md`.
//!
//! Every function under test here is pure over a `RootFs`, so none of this needs
//! a Tauri runtime. What the *commands* add on top — the identity resolution, the
//! two events, and the log records — is asserted where a runtime exists: the
//! turn-level consequences in `agent_conversations/tests.rs`, and the tool's own
//! refusals in `tools/propose_draft_changes/tests.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, Participant, ThreadRef};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FILE: &str = "spec.md";
const ORIGINAL: &str = "# Spec\n\nThe original three paragraphs.\n";
const PROPOSED: &str = "# Spec\n\nA better opening.\n\nAnd a second paragraph.\n";

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
        title: Some("Developer".into()),
    }
}

/// A project holding one draft with one file and one discussion on it.
///
/// `app` is here because `record_proposal` announces (DCP-FR-16) and therefore
/// needs a handle to emit through — the emission being the thing the tab's
/// marker, the rail's control and the shell's notification all redraw from.
struct Fixture {
    _dir: TempDir,
    app: tauri::App<tauri::test::MockRuntime>,
    root: fs::RootFs,
    draft_id: String,
    thread_id: String,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture::in_folder("")
    }

    /// The same project with the draft filed in `folder`, whose chain of drafts
    /// folders is created first. `""` is the drafts root.
    ///
    /// Where a draft is filed is exactly what a composed path gets wrong
    /// (DRS-FR-36), so it is a parameter of the fixture rather than an
    /// assumption inside it.
    fn in_folder(folder: &str) -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let root = fs::RootFs::for_root(dir.path());
        let mut parent = String::new();
        for segment in folder.split('/').filter(|s| !s.is_empty()) {
            drafts::create_drafts_folder_impl(&root, &parent, segment).expect("drafts folder");
            parent = if parent.is_empty() {
                segment.to_string()
            } else {
                format!("{parent}/{segment}")
            };
        }
        let created =
            drafts::create_draft_impl(&root, Some("spec"), Some(folder)).expect("draft");
        let draft_id = created.draft.id.clone();
        drafts::save_draft_file_impl(&root, &draft_id, &created.file, ORIGINAL).expect("seed");
        // The draft is created holding `spec.md` already (DRS-FR-06); the
        // constant is asserted rather than assumed so a change to the derivation
        // fails here rather than somewhere puzzling.
        assert_eq!(created.file, FILE);
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft {
                draft_id: draft_id.clone(),
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
            draft_id,
            thread_id: thread.id,
        }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    /// Every `draft-change-proposals-changed` payload emitted from now on.
    fn watch_proposals(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen(PROPOSALS_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
            }
        });
        seen
    }

    /// Every `comment-thread-changed` payload emitted from now on.
    fn watch_threads(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app
            .listen(comments::DISCUSSION_CHANGED, move |event| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
                }
            });
        seen
    }

    fn target(&self) -> ThreadRef<'_> {
        ThreadRef::discussion(&self.draft_id)
    }

    /// A whole-document proposal expressed as the one change it is: replace
    /// everything the prompt holds with the text given.
    fn whole_document(&self, path: &str, content: &str) -> Vec<hunks::ProposedHunk> {
        let current = drafts::load_draft_file_impl(&self.root, &self.draft_id, path)
            .map(|c| c.body)
            .unwrap_or_default();
        vec![hunks::ProposedHunk {
            kind: crate::draft_proposals::anchors::HunkKind::Replace,
            before: Some(current),
            after: Some(content.to_string()),
            after_text: None,
            note: None,
        }]
    }

    fn propose(&self, path: &str, content: &str) -> Result<DraftChangeProposal, RecordRefusal> {
        self.propose_hunks(path, &self.whole_document(path, content))
    }

    fn propose_hunks(
        &self,
        path: &str,
        hunks: &[hunks::ProposedHunk],
    ) -> Result<DraftChangeProposal, RecordRefusal> {
        record_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            NewProposal {
                draft_id: &self.draft_id,
                path,
                hunks,
                rationale: "The opening buries the point.",
                agent: &agent(),
                target: self.target(),
                thread_id: &self.thread_id,
            },
        )
    }

    /// The proposal every test that needs a standing one starts from.
    fn pending(&self) -> DraftChangeProposal {
        self.propose(FILE, PROPOSED).expect("recorded")
    }

    fn proposals_dir(&self) -> std::path::PathBuf {
        drafts::draft_proposals_dir(&self.root, &self.draft_id).expect("dir")
    }

    fn file_body(&self) -> String {
        drafts::load_draft_file_impl(&self.root, &self.draft_id, FILE)
            .expect("file")
            .body
    }

    fn discussion(&self) -> comments::Discussion {
        comments::fold_discussion(&self.root, &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn lock(&self) {
        comments::set_lock_to(
            &self.root,
            self.target(),
            &self.thread_id,
            true,
            &human(),
            "2026-01-01T00:05:00Z",
        )
        .expect("lock");
    }

    fn accept(&self, id: &str) -> Result<Decided, String> {
        decide(&self.handle(), &self.root, &self.root, id, Decision::Accept, None, None, &human())
    }

    fn accept_hunk(&self, id: &str, hunk: &str) -> Result<Decided, String> {
        decide(&self.handle(), &self.root, &self.root, id, Decision::Accept, Some(hunk), None, &human())
    }

    fn reject_hunk(&self, id: &str, hunk: &str) -> Result<Decided, String> {
        decide(&self.handle(), &self.root, &self.root, id, Decision::Decline, Some(hunk), None, &human())
    }

    fn edit_hunk(
        &self,
        id: &str,
        hunk: &str,
        after: &str,
        baseline: &str,
    ) -> Result<crate::draft_proposals::CandidateSaved, String> {
        crate::draft_proposals::edit_hunk_impl(&self.root, id, hunk, after, baseline)
    }

    fn reread(&self, id: &str) -> DraftChangeProposal {
        find_proposal(&self.root, id).expect("proposal").1
    }

    fn decline(&self, id: &str) -> Result<Decided, String> {
        decide(&self.handle(), &self.root, &self.root, id, Decision::Decline, None, None, &human())
    }

    fn proposals_folder_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.proposals_dir())
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

    /// What the `list_draft_change_proposals` command answers with — the
    /// reconciliation of DCP-FR-28 and then the list.
    fn list_through_command(&self) -> Vec<DraftChangeProposal> {
        crate::draft_history::reconcile(&self.handle(), &self.root, &self.root, &self.draft_id)
            .expect("reconciled");
        list_proposals_impl(&self.root, &self.draft_id).expect("list")
    }

    fn decline_with(&self, id: &str, feedback: Option<&str>) -> Result<Decided, String> {
        decide(&self.handle(), &self.root, &self.root, id, Decision::Decline, None, feedback, &human())
    }
}

mod candidate_edits;
mod deciding;
mod hunk_decisions;
mod hunk_reads;
mod lifecycle;
mod recording;
