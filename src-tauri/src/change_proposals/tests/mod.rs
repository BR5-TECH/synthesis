//! Tests for `CEP-change-edit-proposals.md`.
//!
//! The working copy of the stream is a real Git repository in a temporary
//! folder, so the commit of a closed proposal is checked against `HEAD`. The
//! store is a second temporary folder. A [`FixedSites`] resolver stands for the
//! stream registry and the run order.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::DiscussionTarget;
use crate::prompt_proposals::HunkState;
use crate::text_hunks::hunks::ProposedHunk;

mod committing;
mod deciding;
mod recording;
mod updating;

pub(super) const STREAM: &str = "s1";
pub(super) const CHANGE: &str = "add-auth";
pub(super) const PROPOSAL_MD: &str =
    "## Why\n\nUsers need to log in.\n\n## What Changes\n\n- Add login.\n";
pub(super) const TASKS_MD: &str = "## 1. Login\n\n- [ ] 1.1 Add the form\n";

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: Some("Architect".into()),
    }
}

/// A hunk that replaces `before` with `after` in `path`.
pub(super) fn replace(path: &str, before: &str, after: &str) -> PathedHunk {
    PathedHunk {
        path: path.to_string(),
        hunk: ProposedHunk::from_fields(
            Some(before.to_string()),
            Some(after.to_string()),
            None,
            None,
            None,
            None,
        ),
    }
}

/// A hunk that creates `path` with `text`.
pub(super) fn new_file(path: &str, text: &str) -> PathedHunk {
    PathedHunk {
        path: path.to_string(),
        hunk: ProposedHunk::from_fields(None, Some(text.to_string()), None, None, None, None),
    }
}

/// A stream working copy that holds one change, a store, and a whole-change
/// discussion of it.
pub(super) struct Fixture {
    _wc_dir: TempDir,
    _store_dir: TempDir,
    pub(super) app: tauri::App<tauri::test::MockRuntime>,
    pub(super) wc: fs::RootFs,
    pub(super) store: fs::RootFs,
    pub(super) sites: FixedSites,
    pub(super) thread_id: String,
}

impl Fixture {
    pub(super) fn new() -> Fixture {
        let wc_dir = TempDir::new().expect("tempdir");
        let store_dir = TempDir::new().expect("tempdir");
        let wc = fs::RootFs::for_root(wc_dir.path());
        let store = fs::RootFs::for_root(store_dir.path());
        let change_dir = format!("openspec/changes/{CHANGE}");
        wc.write_text_atomic(wc.path().join(format!("{change_dir}/proposal.md")), PROPOSAL_MD)
            .expect("proposal");
        wc.write_text_atomic(wc.path().join(format!("{change_dir}/tasks.md")), TASKS_MD)
            .expect("tasks");
        init_repo(wc_dir.path());
        let thread = comments::open_discussion_unchecked(
            &store,
            &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
            None,
            "@arch what is missing?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            sites: FixedSites {
                sites: vec![(STREAM.to_string(), wc.clone())],
                busy: Mutex::new(None),
            },
            _wc_dir: wc_dir,
            _store_dir: store_dir,
            app: crate::tools::tests::mock_app(),
            wc,
            store,
            thread_id: thread.id,
        }
    }

    pub(super) fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    /// Every `change-edit-proposals-changed` payload emitted from now on.
    pub(super) fn watch(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen(CHANGE_PROPOSALS_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
            }
        });
        seen
    }

    pub(super) fn record(&self, hunks: &[PathedHunk]) -> Result<ChangeEditProposal, RecordRefusal> {
        record_change_proposal(
            &self.handle(),
            &self.store,
            &self.sites,
            NewChangeProposal {
                stream_id: STREAM,
                change: CHANGE,
                hunks,
                rationale: "The tasks miss the logout. <spoken>Add logout.</spoken>",
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }

    /// The proposal most tests start from: one edit of `tasks.md` and one new
    /// file `specs/auth/spec.md`.
    pub(super) fn pending(&self) -> ChangeEditProposal {
        self.record(&[
            replace(
                "tasks.md",
                "- [ ] 1.1 Add the form\n",
                "- [ ] 1.1 Add the form\n- [ ] 1.2 Add logout\n",
            ),
            new_file("specs/auth/spec.md", "## ADDED Requirements\n"),
        ])
        .expect("recorded")
    }

    pub(super) fn decide(
        &self,
        proposal_id: &str,
        decision: Decision,
        hunk: Option<&str>,
    ) -> Result<ChangeDecisionOutcome, String> {
        decide(
            &self.handle(),
            &self.store,
            &self.sites,
            DecisionRequest { proposal_id, decision, hunk, feedback: None, by: &human() },
        )
    }

    pub(super) fn accept(
        &self,
        proposal_id: &str,
        hunk: &str,
    ) -> Result<ChangeDecisionOutcome, String> {
        self.decide(proposal_id, Decision::Accept, Some(hunk))
    }

    pub(super) fn reject(
        &self,
        proposal_id: &str,
        hunk: &str,
    ) -> Result<ChangeDecisionOutcome, String> {
        self.decide(proposal_id, Decision::Decline, Some(hunk))
    }

    /// The text of a file of the change in the working copy.
    pub(super) fn file(&self, path: &str) -> Option<String> {
        self.wc.read_text(self.wc.path().join(format!("openspec/changes/{CHANGE}/{path}"))).ok()
    }

    pub(super) fn reread(&self, proposal_id: &str) -> ChangeEditProposal {
        read_proposal(&self.store, proposal_id).expect("record").0
    }

    pub(super) fn thread(&self) -> comments::Discussion {
        locate_thread(&self.store, STREAM, CHANGE, &self.thread_id).expect("thread")
    }

    /// The subject line of the commit at `HEAD`, and the paths it changed.
    pub(super) fn head(&self) -> (String, Vec<String>) {
        let repo = git2::Repository::open(self.wc.path()).expect("repo");
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        let parent = head.parent(0).ok().map(|c| c.tree().expect("tree"));
        let diff = repo
            .diff_tree_to_tree(parent.as_ref(), Some(&head.tree().expect("tree")), None)
            .expect("diff");
        let mut paths: Vec<String> = diff
            .deltas()
            .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        paths.sort();
        (
            String::from_utf8_lossy(head.message_bytes()).lines().next().unwrap_or("").to_string(),
            paths,
        )
    }

    /// The number of commits on `HEAD`.
    pub(super) fn commits(&self) -> usize {
        let repo = git2::Repository::open(self.wc.path()).expect("repo");
        let mut walk = repo.revwalk().expect("walk");
        walk.push_head().expect("head");
        walk.count()
    }
}

/// A repository with one commit that holds every file of the folder.
fn init_repo(path: &std::path::Path) {
    let repo = git2::Repository::init(path).expect("init");
    let mut config = repo.config().expect("config");
    config.set_str("user.name", "Test").expect("name");
    config.set_str("user.email", "test@example.com").expect("email");
    drop(config);
    let mut index = repo.index().expect("index");
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).expect("add");
    index.write().expect("write");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
    let sig = repo.signature().expect("sig");
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).expect("commit");
}

pub(super) fn row_id(record: &ChangeEditProposal, path: &str) -> String {
    record.ledger.iter().find(|r| r.path == path).expect("row").id.clone()
}

pub(super) fn state_of(record: &ChangeEditProposal, id: &str) -> HunkState {
    record.row(id).expect("row").state
}

// CEP-FR-GGDV / CEP-FR-EODY / CEP-FR-FQMZ: the module decides nothing on its
// own (no timer, no thread, no agent turn, no model), reaches the disk through
// FSA alone, and calls no operation of the other two proposal stores.
#[test]
fn the_module_holds_no_timer_no_model_and_no_direct_disk_access() {
    // Every file of the module, read at test time, so a new file is scanned too.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/change_proposals");
    let mut sources: Vec<String> =
        vec![std::fs::read_to_string(dir.with_extension("rs")).expect("root module")];
    for entry in std::fs::read_dir(&dir).expect("module folder").flatten() {
        if entry.path().extension().is_some_and(|e| e == "rs") {
            sources.push(std::fs::read_to_string(entry.path()).expect("source"));
        }
    }
    assert!(sources.len() >= 12, "the scan reads the module");
    for source in &sources {
        for forbidden in [
            "std::fs::",
            "std::thread",
            "spawn",
            "sleep(",
            "dispatch_agent_turn",
            "crate::ai_api",
            "prompt_proposals::accept",
            "prompt_proposals::record",
            "draft_proposals::",
            "push(",
        ] {
            let hits: Vec<&str> = source
                .lines()
                .filter(|l| l.contains(forbidden) && !l.trim_start().starts_with("//"))
                .collect();
            if forbidden == "push(" {
                // A `Vec::push` is fine; a Git push is not.
                assert!(
                    hits.iter().all(|l| !l.contains("git") && !l.contains("remote")),
                    "{hits:?}"
                );
            } else {
                assert!(hits.is_empty(), "{forbidden} in {hits:?}");
            }
        }
    }
}
