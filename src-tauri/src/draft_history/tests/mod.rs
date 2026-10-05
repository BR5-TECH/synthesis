//! Tests for `DHS-draft-history.md`.
//!
//! Everything under test is pure over a `RootFs` except the two commands and the
//! event, which need a Tauri handle to emit through — so the fixture carries a
//! mock app for exactly that reason and nothing else.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::{self as cms, DiscussionTarget};
use crate::draft_proposals::{self as dcp, NewProposal};

const FILE: &str = "spec.md";
const ORIGINAL_TEXT: &str = "# Spec\n\nThe original three paragraphs.\n";
const PROPOSED: &str = "# Spec\n\nA better opening.\n";

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
        title: None,
    }
}

/// A project holding one draft — created, so carrying **no version at all**
/// (DHS-FR-07) — with one discussion on it.
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

    /// The same project with the draft filed in `folder`. Where a draft is filed
    /// is exactly what a composed path gets wrong (DRS-FR-36), so it is a
    /// parameter of the fixture rather than an assumption inside it.
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
        assert_eq!(created.file, FILE);
        let thread = cms::open_discussion_in(
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

    fn hist(&self) -> PathBuf {
        drafts::draft_history_dir(&self.root, &self.draft_id).expect("history dir")
    }

    fn list(&self) -> DraftHistoryList {
        list_impl(&self.root, &self.draft_id).expect("list")
    }

    fn prompt(&self) -> String {
        drafts::load_draft_file_impl(&self.root, &self.draft_id, FILE)
            .expect("prompt")
            .body
    }

    fn write_prompt(&self, body: &str) {
        drafts::save_draft_file_impl(&self.root, &self.draft_id, FILE, body).expect("write");
    }

    fn prompt_body(&self) -> String {
        drafts::load_draft_file_impl(&self.root, &self.draft_id, FILE)
            .map(|c| c.body)
            .unwrap_or_default()
    }

    fn propose(&self, content: &str) -> dcp::DraftChangeProposal {
        // A whole-document proposal names the prompt **as it stands** — which is
        // what a change naming the text it changes has to do (DCP-FR-HRQN).
        let current = drafts::load_draft_file_impl(&self.root, &self.draft_id, FILE)
            .map(|c| c.body)
            .unwrap_or_default();
        dcp::record_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            NewProposal {
                draft_id: &self.draft_id,
                path: FILE,
                hunks: &crate::draft_proposals::hunks::whole_document(&current, content),
                rationale: "The opening buries the point.",
                agent: &agent(),
                target: cms::ThreadRef::discussion(&self.draft_id),
                thread_id: &self.thread_id,
            },
        )
        .expect("recorded")
    }

    fn accept(&self, proposal: &dcp::DraftChangeProposal) -> Result<AcceptedOutcome, String> {
        let candidate = dcp::load_content_impl(&self.root, &proposal.id).expect("candidate");
        apply_acceptance(
            &self.handle(),
            &self.root,
            &self.root,
            Acceptance {
                hunk_id: None,
                resolves: true,
                proposal,
                candidate: &candidate.content,
                comment_body: format!("Accepted the proposed change to `{FILE}`."),
                comment_by: &human(),
            },
        )
    }

    /// Record a proposal and accept it, which is the only way a version comes
    /// into existence (DHS-FR-08) — the first one settling the `Original` it
    /// supersedes alongside the version it produces (DHS-FR-07).
    fn accept_new(&self, content: &str) -> AcceptedOutcome {
        let proposal = self.propose(content);
        self.accept(&proposal).expect("accepted")
    }

    fn discussion(&self) -> cms::Discussion {
        cms::fold_discussion(&self.root, &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    /// Every `draft-history-changed` payload emitted from now on.
    fn watch(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen(HISTORY_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
            }
        });
        seen
    }

    fn history_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.hist())
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

    fn reconcile(&self) -> Result<Reconciliation, String> {
        self.reconcile_of(&self.draft_id)
    }

    fn reconcile_of(&self, draft_id: &str) -> Result<Reconciliation, String> {
        reconcile(&self.handle(), &self.root, &self.root, draft_id)
    }
}


/// Leave the draft in exactly the state a process killed at `phase` leaves it.
///
/// The staging itself lives beside the transaction it replays, so the two never
/// drift: a step added to `apply_acceptance` and not to it would leave every
/// reconciliation test asserting against a state the production path no longer
/// produces.
fn interrupt(f: &Fixture, proposal: &dcp::DraftChangeProposal, phase: Phase) -> Journal {
    let prior = f.prompt();
    stage_for_test(&f.root, &f.draft_id, proposal, &prior, &human(), phase)
        .expect("staged")
}

/// What `list_draft_history` does once its root is resolved (DHS-FR-18): the
/// reconciliation first, then the list — so a state that could not be
/// reconciled is never answered from.
fn list_impl_after_reconcile(f: &Fixture) -> Result<DraftHistoryList, String> {
    f.reconcile()?;
    list_impl(&f.root, &f.draft_id)
}

mod draft_lifecycle;
mod entries;
mod reconciliation;
mod transaction;
