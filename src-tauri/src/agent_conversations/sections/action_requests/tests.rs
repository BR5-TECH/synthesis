//! Tests for the action requests in `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-KRMV).
//!
//! Fixtures build files with `std::fs`, which the FSA guard allows in a test
//! file.

use std::sync::Mutex;

use tempfile::TempDir;

use super::*;
use crate::change_action_requests::{self as car, ChangeAction, NewActionRequest};
use crate::change_proposals::FixedSites;
use crate::comments::Participant;

const STREAM: &str = "s1";
const CHANGE: &str = "add-auth";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent { agent_id: "a".into(), handle: "arch".into(), model: None, title: None }
}

struct Change {
    _store_dir: TempDir,
    _wc_dir: TempDir,
    app: tauri::App<tauri::test::MockRuntime>,
    store: crate::fs::RootFs,
    sites: FixedSites,
    thread: comments::Discussion,
}

impl Change {
    fn new() -> Change {
        let store_dir = TempDir::new().expect("tempdir");
        let wc_dir = TempDir::new().expect("tempdir");
        let folder = wc_dir.path().join(format!("openspec/changes/{CHANGE}"));
        std::fs::create_dir_all(&folder).expect("dirs");
        std::fs::write(folder.join("proposal.md"), "## Why\n").expect("proposal");
        let store = crate::fs::RootFs::for_root(store_dir.path());
        let thread = comments::open_discussion_unchecked(
            &store,
            &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
            None,
            "@arch is it ready?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Change {
            sites: FixedSites {
                sites: vec![(STREAM.into(), crate::fs::RootFs::for_root(wc_dir.path()))],
                busy: Mutex::new(None),
                rework: Mutex::new(None),
            },
            _store_dir: store_dir,
            _wc_dir: wc_dir,
            app: crate::tools::tests::mock_app(),
            store,
            thread,
        }
    }

    fn roots(&self) -> Roots<'_> {
        Roots { worktree: &self.store, store: &self.store }
    }

    fn record(&self, action: ChangeAction) -> car::ChangeActionRequest {
        car::record_change_action_request(
            self.app.handle(),
            &self.store,
            &self.sites,
            NewActionRequest {
                stream_id: STREAM,
                change: CHANGE,
                action,
                instruction: Some("Revise the tasks."),
                rationale: "Ready. <spoken>Ready.</spoken>",
                agent: &agent(),
                thread_id: &self.thread.id,
            },
        )
        .expect("recorded")
    }

    fn decline(&self, id: &str) {
        car::decline_action_request(self.app.handle(), &self.store, id, None, &human())
            .expect("declined");
    }

    fn history(&self) -> String {
        let thread = car_thread(&self.store, &self.thread.id);
        discussion_history_section(self.roots(), &thread, "").body
    }
}

fn car_thread(store: &crate::fs::RootFs, id: &str) -> comments::Discussion {
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    comments::locate_discussion_of(store, &target, id).expect("thread")
}

// AGC-FR-KRMV / CAR-FR-IFGR: the history of a change conversation ends with
// the ten newest requests, newest first, each with its id, its action, its
// state, and its run with the run's state where one started.
#[test]
fn the_history_lists_the_ten_newest_requests() {
    let c = Change::new();
    let mut ids = Vec::new();
    for _ in 0..11 {
        let record = c.record(ChangeAction::Apply);
        c.decline(&record.id);
        ids.push(record.id);
    }
    let pending = c.record(ChangeAction::Rework);
    // A started request, with its run, written as a confirmation leaves it.
    let dir = c.store.path().join(format!("changes/{STREAM}/{CHANGE}/action-requests"));
    let file = dir.join(format!("{}.toml", ids[10]));
    let text = std::fs::read_to_string(&file).expect("record");
    let started = text.replace("state = \"declined\"", "state = \"started\"\nrunId = \"run-42\"");
    std::fs::write(&file, started).expect("write");
    car::set_test_run_state("run-42", "working");

    let history = c.history();
    let at = history.find("The requests to apply or rework this change").expect("the section");
    let lines: Vec<&str> =
        history[at..].lines().skip(1).take_while(|line| line.starts_with("  ")).collect();
    assert_eq!(lines.len(), 10, "{history}");
    assert_eq!(lines[0].trim(), format!("{} — rework — pending", pending.id));
    assert_eq!(lines[1].trim(), format!("{} — apply — started — run run-42 working", ids[10]));
    assert_eq!(lines[2].trim(), format!("{} — apply — declined", ids[9]));
    for oldest in &ids[..2] {
        assert!(
            !history[at..].contains(oldest.as_str()),
            "the oldest are not listed: {}",
            &history[at..]
        );
    }
    assert!(!history.contains("Revise the tasks."), "no instruction");

    // CMS-FR-QVZP: a comment's `change_action_request` names the action and
    // the request.
    assert!(history
        .contains(&format!("[asked the author to rework this change: request {}]", pending.id)));
}

// AGC-FR-KRMV / AGC-FR-JNOS: the requests stand before the split.
#[test]
fn the_requests_stand_before_the_split() {
    let c = Change::new();
    c.record(ChangeAction::Apply);
    crate::new_change_proposals::record_new_change_proposal(
        c.app.handle(),
        &c.store,
        &c.sites,
        crate::new_change_proposals::NewChange {
            stream_id: STREAM,
            change: CHANGE,
            name: "part-one",
            files: &[crate::new_change_proposals::ProposedFile {
                path: "proposal.md".into(),
                text: "## Why\n\nPart.\n".into(),
            }],
            position: 1,
            total: 2,
            rationale: "Split. <spoken>Split.</spoken>",
            agent: &agent(),
            thread_id: &c.thread.id,
        },
    )
    .expect("proposed");
    let history = c.history();
    let requests = history.find("The requests to apply or rework").expect("requests");
    let split = history.find("\nThe split ").expect("split");
    assert!(requests < split, "{history}");
}

// AGC-FR-KRMV: a change with no request, and a target that is not a change,
// add no section.
#[test]
fn no_request_adds_nothing() {
    let c = Change::new();
    assert!(!c.history().contains("The requests to apply or rework"));
    let target = DiscussionTarget::Draft { draft_id: "d".into() };
    assert!(action_requests_text(c.roots(), &target).is_none());
}
