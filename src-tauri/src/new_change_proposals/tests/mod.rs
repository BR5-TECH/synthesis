//! Tests for `NCP-new-change-proposals.md`, against a real Git working copy.
//!
//! Fixtures build files with `std::fs`, which the FSA guard allows in a test
//! file.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::change_proposals::FixedSites;
use crate::comments::DiscussionTarget;

mod deciding;
mod recording;

pub(super) const STREAM: &str = "s1";
pub(super) const CHANGE: &str = "big-change";

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent { agent_id: "a1".into(), handle: "arch".into(), model: None, title: None }
}

pub(super) fn files(name_hint: &str) -> Vec<ProposedFile> {
    vec![
        ProposedFile { path: "proposal.md".into(), text: format!("## Why\n\n{name_hint}\n") },
        ProposedFile { path: "specs/auth/spec.md".into(), text: "## ADDED Requirements\n".into() },
    ]
}

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
        let base = wc_dir.path().join(format!("openspec/changes/{CHANGE}"));
        std::fs::create_dir_all(&base).expect("dirs");
        std::fs::write(base.join("proposal.md"), "## Why\n\nToo big.\n").expect("proposal");
        let repo = git2::Repository::init(wc_dir.path()).expect("init");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Test").expect("name");
        config.set_str("user.email", "t@example.com").expect("email");
        drop(config);
        let mut index = repo.index().expect("index");
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).expect("add");
        index.write().expect("write");
        let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
        let sig = repo.signature().expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).expect("commit");
        let wc = fs::RootFs::for_root(wc_dir.path());
        let store = fs::RootFs::for_root(store_dir.path());
        let thread = comments::open_discussion_unchecked(
            &store,
            &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
            None,
            "@arch split it".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            sites: FixedSites { sites: vec![(STREAM.into(), wc.clone())], busy: Mutex::new(None) },
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

    pub(super) fn watch(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let sink = Arc::clone(&seen);
        self.app.listen(NEW_CHANGE_PROPOSALS_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str(event.payload()) {
                sink.lock().unwrap().push(value);
            }
        });
        seen
    }

    pub(super) fn record_named(
        &self,
        name: &str,
        position: u32,
        total: u32,
    ) -> Result<NewChangeProposal, String> {
        record_new_change_proposal(
            &self.handle(),
            &self.store,
            &self.sites,
            NewChange {
                stream_id: STREAM,
                change: CHANGE,
                name,
                files: &files(name),
                position,
                total,
                rationale: "Split. <spoken>Split.</spoken>",
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }

    pub(super) fn decide(
        &self,
        id: &str,
        accept: bool,
    ) -> Result<NewChangeDecisionOutcome, String> {
        decide_new_change(
            &self.handle(),
            &self.store,
            &self.sites,
            NewChangeDecision { proposal_id: id, accept, feedback: None, by: &human() },
        )
    }

    pub(super) fn thread(&self) -> comments::Discussion {
        locate_thread(&self.store, STREAM, CHANGE, &self.thread_id).expect("thread")
    }

    pub(super) fn wc_file(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.wc.path().join(rel)).ok()
    }

    pub(super) fn head(&self) -> (String, Vec<String>) {
        let repo = git2::Repository::open(self.wc.path()).expect("repo");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let parent = head.parent(0).ok().map(|c| c.tree().unwrap());
        let diff =
            repo.diff_tree_to_tree(parent.as_ref(), Some(&head.tree().unwrap()), None).unwrap();
        let mut paths: Vec<String> = diff
            .deltas()
            .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        paths.sort();
        (String::from_utf8_lossy(head.message_bytes()).trim().to_string(), paths)
    }
}
