//! Tests for the new-draft proposals of `DCP-draft-change-proposals.md`.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::{self, DiscussionTarget, Participant, ThreadRef};

mod confirming;
mod deciding;

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: Some("Developer".into()),
    }
}

/// A project with one draft `relay` filed in `UI/Sub` and one discussion.
pub(super) struct Fixture {
    pub _dir: TempDir,
    pub app: tauri::App<tauri::test::MockRuntime>,
    pub root: fs::RootFs,
    pub draft_id: String,
    pub thread_id: String,
    pub events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let root = fs::RootFs::for_root(dir.path());
        drafts::create_drafts_folder_impl(&root, "", "UI").unwrap();
        drafts::create_drafts_folder_impl(&root, "UI", "Sub").unwrap();
        let draft_id =
            drafts::create_draft_impl(&root, Some("relay"), Some("UI/Sub")).unwrap().draft.id;
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: draft_id.clone() },
            None,
            "@arch please decompose this".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .unwrap();
        let app = crate::tools::tests::mock_app();
        let events: Arc<Mutex<Vec<(String, serde_json::Value)>>> = Arc::default();
        for name in [NEW_DRAFT_PROPOSALS_CHANGED, drafts::DRAFTS_CHANGED] {
            let sink = Arc::clone(&events);
            app.listen(name, move |event| {
                let value = serde_json::from_str(event.payload()).unwrap_or_default();
                sink.lock().unwrap().push((name.to_string(), value));
            });
        }
        Fixture { _dir: dir, app, root, draft_id, thread_id: thread.id, events }
    }

    pub fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    pub fn record(
        &self,
        name: &str,
        body: &str,
        position: i64,
        total: i64,
    ) -> Result<NewDraftProposal, String> {
        record_new_draft_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            NewDraftInput {
                draft_id: &self.draft_id,
                name,
                body,
                position,
                total,
                rationale: "<spoken>This part ships alone.</spoken>",
                agent: &agent(),
                target: ThreadRef::discussion(&self.draft_id),
                thread_id: &self.thread_id,
            },
        )
    }

    pub fn update(&self, update: NewDraftUpdate) -> Result<NewDraftProposal, String> {
        update_new_draft_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            &self.draft_id,
            update,
            "<spoken>I changed it.</spoken>",
            &agent(),
            (ThreadRef::discussion(&self.draft_id), &self.thread_id),
        )
    }

    pub fn decide(
        &self,
        id: &str,
        decision: NewDraftDecision,
        feedback: Option<&str>,
    ) -> Result<NewDraftDecisionOutcome, String> {
        decide_new_draft_in_process(
            &self.handle(),
            &self.root,
            &self.root,
            id,
            decision,
            feedback,
            &human(),
        )
    }

    pub fn confirm(&self) -> Result<drafts::DraftRecord, String> {
        confirm_decomposition_impl(
            &self.handle(),
            &self.root,
            &self.root,
            &self.draft_id,
            &human(),
            None,
        )
    }

    pub fn thread(&self) -> comments::Discussion {
        comments::locate_discussion_of(
            &self.root,
            &DiscussionTarget::Draft { draft_id: self.draft_id.clone() },
            &self.thread_id,
        )
        .expect("the thread")
    }

    pub fn new_drafts_dir(&self) -> std::path::PathBuf {
        dir_of(&self.root, &self.draft_id).unwrap()
    }

    /// Every file under the drafts root and its bytes, for "nothing changed".
    pub fn tree(&self) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        fn walk(path: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
            let Ok(entries) = std::fs::read_dir(path) else { return };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(self.root.path(), &mut out);
        out.sort();
        out
    }

    pub fn causes(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|(name, v)| match name.as_str() {
                NEW_DRAFT_PROPOSALS_CHANGED => v["cause"].as_str().unwrap_or("").to_string(),
                _ => "drafts".to_string(),
            })
            .collect()
    }

    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }
}

// DCP-FR-TGRN / DCP-FR-VQWK / DCP-FR-ZDSP / CMS-FR-YKGI: a recording writes the
// body and the record in `proposals/new-drafts/`, appends one agent comment with
// one reference, and emits `recorded`.
#[test]
fn a_recording_writes_the_body_the_record_and_one_comment() {
    let f = Fixture::new();
    let p = f.record("relay-auth", "# Auth\n\nThe auth part.\n", 1, 3).unwrap();
    assert_eq!(p.state, NewDraftState::Pending);
    assert_eq!(p.revision, 1);
    assert_eq!((p.position, p.total), (1, 3));
    let dir = f.new_drafts_dir();
    assert!(dir.ends_with("proposals/new-drafts"));
    assert_eq!(
        std::fs::read_to_string(dir.join(format!("{}.md", p.id))).unwrap(),
        "# Auth\n\nThe auth part.\n"
    );
    assert!(dir.join(format!("{}.toml", p.id)).exists());
    let thread = f.thread();
    let last = thread.comments.last().unwrap();
    assert_eq!(last.id, p.comment_id);
    assert_eq!(last.body, "<spoken>This part ships alone.</spoken>");
    assert_eq!(
        last.attachments,
        vec![comments::Attachment::NewDraftProposal {
            proposal_id: p.id.clone(),
            draft_id: f.draft_id.clone(),
            update: None,
        }]
    );
    assert_eq!(f.causes(), vec!["recorded"]);
    // DCP-FR-ZDSP: the list reads the record.
    assert_eq!(list_impl(&f.root, &f.draft_id).unwrap(), vec![p]);
}

// DCP-FR-OBAW: one pending proposal at a time; a second refuses and changes
// nothing.
#[test]
fn a_second_recording_while_one_is_pending_is_refused() {
    let f = Fixture::new();
    f.record("a", "body", 1, 2).unwrap();
    let before = f.tree();
    let comments_before = f.thread().comments.len();
    f.clear_events();
    assert_eq!(f.record("b", "body", 2, 2), Err(ERR_PROPOSAL_PENDING.to_string()));
    assert_eq!(f.tree(), before);
    assert_eq!(f.thread().comments.len(), comments_before);
    assert!(f.causes().is_empty());
}

// DCP-FR-OBAW: the validation errors, each writing nothing.
#[test]
fn a_blank_name_a_blank_body_and_a_bad_position_are_refused() {
    let f = Fixture::new();
    let before = f.tree();
    assert_eq!(f.record("  ", "body", 1, 1), Err(ERR_BLANK_NAME.into()));
    assert_eq!(f.record("n", "\n ", 1, 1), Err(ERR_BLANK_BODY.into()));
    assert_eq!(f.record("n", "body", 0, 1), Err(ERR_BAD_POSITION.into()));
    assert_eq!(f.record("n", "body", 3, 2), Err(ERR_BAD_POSITION.into()));
    assert_eq!(f.tree(), before);
    // A position equal to the total is accepted.
    assert!(f.record("n", "body", 2, 2).is_ok());
}

// DCP-FR-TGRN / DCP-FR-OBAW: a resolved conversation refuses, and the files
// written before the append are removed.
#[test]
fn a_resolved_conversation_refuses_and_leaves_no_file() {
    let f = Fixture::new();
    comments::set_resolution_to(
        &f.root,
        ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    assert_eq!(f.record("a", "body", 1, 2), Err(ERR_DISCUSSION_RESOLVED.into()));
    assert!(records_in(&f.root, &f.new_drafts_dir()).is_empty());
    let leftover = std::fs::read_dir(f.new_drafts_dir()).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftover, 0);
}

// DCP-FR-CKWC: every proposal of a draft shares one set id; another draft has
// its own.
#[test]
fn every_proposal_of_a_draft_shares_one_set_id() {
    let f = Fixture::new();
    let first = f.record("a", "body", 1, 2).unwrap();
    f.update(NewDraftUpdate::Withdraw).unwrap();
    let second = f.record("b", "body", 1, 2).unwrap();
    assert_eq!(first.set_id, second.set_id);
    let other = Fixture::new();
    assert_ne!(other.record("c", "body", 1, 1).unwrap().set_id, first.set_id);
}

// DCP-FR-GDTC / CMS-FR-YKGI: a change replaces the fields it carries, keeps the
// id, bumps the revision, and appends one comment carrying the action.
#[test]
fn a_change_update_replaces_the_given_fields_and_keeps_the_id() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    f.clear_events();
    let changed = f
        .update(NewDraftUpdate::Change { name: None, body: None, position: None, total: Some(4) })
        .unwrap();
    assert_eq!(changed.id, p.id);
    assert_eq!(changed.revision, 2);
    assert_eq!((changed.name.as_str(), changed.position, changed.total), ("a", 1, 4));
    assert_eq!(load_body_impl(&f.root, &p.id).unwrap(), "body");
    let last = f.thread().comments.last().unwrap().clone();
    assert_eq!(
        last.attachments,
        vec![comments::Attachment::NewDraftProposal {
            proposal_id: p.id.clone(),
            draft_id: f.draft_id.clone(),
            update: Some(comments::NewDraftUpdateAction::Change),
        }]
    );
    assert_eq!(f.causes(), vec!["updated"]);
    let rebody = f
        .update(NewDraftUpdate::Change {
            name: Some("b".into()),
            body: Some("new body".into()),
            position: None,
            total: None,
        })
        .unwrap();
    assert_eq!(rebody.name, "b");
    assert_eq!(load_body_impl(&f.root, &p.id).unwrap(), "new body");
}

// DCP-FR-ZJRW: no change, a bad result, and no open proposal each refuse, and
// a refusal leaves both files as they were.
#[test]
fn update_refusals_leave_both_files_as_they_were() {
    let f = Fixture::new();
    assert_eq!(f.update(NewDraftUpdate::Withdraw), Err(ERR_NO_OPEN_PROPOSAL.into()));
    f.record("a", "body", 2, 3).unwrap();
    let before = f.tree();
    let same = NewDraftUpdate::Change {
        name: Some(" a ".into()),
        body: Some("body".into()),
        position: Some(2),
        total: Some(3),
    };
    assert_eq!(f.update(same), Err(ERR_NO_CHANGE.into()));
    let bad = NewDraftUpdate::Change { name: None, body: None, position: None, total: Some(1) };
    assert_eq!(f.update(bad), Err(ERR_BAD_POSITION.into()));
    assert_eq!(f.tree(), before);
}

// DCP-FR-GDTC: a withdraw decides the proposal as withdrawn, keeps its body, and
// frees the slot.
#[test]
fn a_withdraw_settles_the_proposal_and_frees_the_slot() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    let withdrawn = f.update(NewDraftUpdate::Withdraw).unwrap();
    assert_eq!(withdrawn.state, NewDraftState::Withdrawn);
    assert!(withdrawn.decided_at.is_some());
    assert_eq!(load_body_impl(&f.root, &p.id).unwrap(), "body");
    assert!(f.record("b", "body", 1, 2).is_ok());
}

// DCP-FR-KLPP / DRS-FR-WMIK / DCP-FR-BZHR: a pending new-draft proposal sets the
// list's pending marker, and a decided one does not.
#[test]
fn a_pending_proposal_sets_the_drafts_pending_marker() {
    let f = Fixture::new();
    let pending = |f: &Fixture| {
        drafts::list_drafts_impl(&f.root)
            .drafts
            .into_iter()
            .find(|d| d.id == f.draft_id)
            .unwrap()
            .has_pending_proposal
    };
    assert!(!pending(&f));
    let p = f.record("a", "body", 1, 2).unwrap();
    assert!(pending(&f));
    f.decide(&p.id, NewDraftDecision::Reject, None).unwrap();
    assert!(!pending(&f));
}

// DCP-FR-FKMT: the turn read is oldest first, carries the pending body alone,
// and writes nothing.
#[test]
fn the_turn_read_is_oldest_first_with_the_pending_body() {
    let f = Fixture::new();
    assert!(read_new_draft_proposals(&f.root, &f.draft_id).is_none());
    let first = f.record("a", "first body", 1, 2).unwrap();
    f.decide(&first.id, NewDraftDecision::Reject, None).unwrap();
    let second = f.record("b", "second body", 2, 2).unwrap();
    let before = f.tree();
    let (all, body) = read_new_draft_proposals(&f.root, &f.draft_id).unwrap();
    assert_eq!(all.iter().map(|p| p.id.clone()).collect::<Vec<_>>(), vec![first.id, second.id]);
    assert_eq!(body.as_deref(), Some("second body"));
    assert_eq!(f.tree(), before);
}

// DCP-FR-LTEZ: an archived, graduated, GitHub-shadow, or superseded draft
// refuses a recording with its own error.
#[test]
fn a_draft_in_a_closed_state_refuses_a_recording() {
    let f = Fixture::new();
    drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Archived).unwrap();
    assert_eq!(f.record("a", "b", 1, 1), Err(ERR_DRAFT_ARCHIVED.into()));

    let g = Fixture::new();
    drafts::set_draft_graduated(&g.root, &g.draft_id, "run-1").unwrap();
    assert_eq!(g.record("a", "b", 1, 1), Err(ERR_DRAFT_GRADUATED.into()));

    let s = Fixture::new();
    drafts::mark_github_shadow_for_test(
        &s.root,
        &s.draft_id,
        drafts::GithubIssueLink {
            repository_host: "github.com".into(),
            repository_owner: "o".into(),
            repository_name: "r".into(),
            issue_number: 1,
            issue_url: "https://github.com/o/r/issues/1".into(),
            project_node_id: "P".into(),
            claim_state: drafts::GithubClaimState::Claimed,
        },
    );
    assert_eq!(s.record("a", "b", 1, 1), Err(ERR_DRAFT_GITHUB_SHADOW.into()));

    let c = Fixture::new();
    drafts::mark_draft_superseded(
        &c.root,
        &c.draft_id,
        drafts::SupersededMarker { set_id: "s".into(), confirmed_at: "t".into(), drafts: vec![] },
    )
    .unwrap();
    drafts::set_draft_status_impl(&c.root, &c.draft_id, drafts::DraftStatus::Active).unwrap();
    assert_eq!(c.record("a", "b", 1, 1), Err(ERR_DECOMPOSITION_CONFIRMED.into()));
}

// DCP-FR-DIHV / DCP-FR-VQWK: a recording writes nothing outside
// `proposals/new-drafts/` of the worktree, and the folder goes with the draft.
#[test]
fn a_recording_writes_only_under_new_drafts_and_goes_with_the_draft() {
    let f = Fixture::new();
    let drafts_root = f.root.path().join(".synthesis/drafts");
    let snapshot = |f: &Fixture| -> Vec<_> {
        f.tree().into_iter().filter(|(p, _)| p.starts_with(&drafts_root)).collect()
    };
    let before = snapshot(&f);
    f.record("a", "body", 1, 1).unwrap();
    let after = snapshot(&f);
    let dir = f.new_drafts_dir();
    for (path, bytes) in &after {
        if !before.iter().any(|(p, b)| p == path && b == bytes) {
            assert!(
                path.starts_with(&dir) || path.ends_with(".gitignore"),
                "unexpected write: {path:?}"
            );
        }
    }
    drafts::delete_draft_impl(&f.root, &f.root, &f.draft_id).unwrap();
    assert!(!dir.exists());
}

// DCP-FR-EEGX: a refusal reason in a record is a typed code; an untyped error,
// which can carry a path, is logged as `write_failed`.
#[test]
fn a_refusal_reason_is_logged_as_a_typed_code() {
    assert_eq!(typed_reason(ERR_PROPOSAL_PENDING), ERR_PROPOSAL_PENDING);
    assert_eq!(typed_reason("io error: /Users/someone/private/drafts"), ERR_WRITE_FAILED);
}
