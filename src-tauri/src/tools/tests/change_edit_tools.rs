//! Tests for `PCE-propose-change-edits-tool.md` and
//! `DCE-decide-change-edits-tool.md`.
//!
//! The store behind both tools is tested in `change_proposals/tests/`. What is
//! tested here is the call: the binding, the arguments, the refusals, the end
//! of the turn, and the logs.
//!
//! Fixtures build files with `std::fs`, which `fs/access_tests.rs` allows in a
//! test file.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rig::tool::{PortableTool, ToolErrorKind};
use tempfile::TempDir;

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::change_proposals::{self as cep, FixedSites};
use crate::comments::{self, DiscussionTarget, Participant};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::decide_change_edits::{self, DecideChangeEditsTool};
use crate::tools::decide_changes::{DecideChangesArgs, DecisionArg, Outcome, RefusalCode};
use crate::tools::propose_change_edits::{
    self, ChangeHunkArg, ProposeChangeEditsArgs, ProposeChangeEditsTool,
};
use crate::tools::tests::{block_on, mounted};
use crate::tools::ToolRefusal;

pub(super) const STREAM: &str = "s1";
pub(super) const CHANGE: &str = "add-auth";
const TASKS: &str = "## 1. Login\n\n- [ ] 1.1 Add the form\n";
pub(super) const RATIONALE: &str = "Add logout. <spoken>Add logout.</spoken>";

pub(super) static QUIET: LogBuffer = LogBuffer::new();

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: None,
        title: None,
    }
}

pub(super) struct Fixture {
    pub(super) open: super::Fixture,
    store_path: std::path::PathBuf,
    _wc_dir: TempDir,
    pub(super) wc: crate::fs::RootFs,
    pub(super) thread_id: String,
    pub(super) proposed: Arc<AtomicBool>,
}

impl Fixture {
    pub(super) fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let store_path = crate::changes::canonicalize_lenient(dir.path());
        let wc_dir = TempDir::new().expect("tempdir");
        let tasks = wc_dir.path().join(format!("openspec/changes/{CHANGE}/tasks.md"));
        std::fs::create_dir_all(tasks.parent().expect("parent")).expect("dirs");
        std::fs::write(&tasks, TASKS).expect("tasks");
        let repo = git2::Repository::init(wc_dir.path()).expect("init");
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

        let store = crate::fs::RootFs::for_root(&store_path);
        let thread = comments::open_discussion_unchecked(
            &store,
            &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
            None,
            "@arch arch".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            store_path,
            wc: crate::fs::RootFs::for_root(wc_dir.path()),
            _wc_dir: wc_dir,
            thread_id: thread.id,
            proposed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn store(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.store_path)
    }

    pub(super) fn roots(&self) -> OwnedRoots {
        OwnedRoots { worktree: self.store(), store: self.store() }
    }

    pub(super) fn sites(&self) -> Box<FixedSites> {
        Box::new(FixedSites {
            sites: vec![(STREAM.into(), self.wc.clone())],
            busy: Mutex::new(None),
        })
    }

    pub(super) fn origin(&self) -> ConversationOrigin {
        let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
        let found = comments::locate_discussion_of(&self.store(), &target, &self.thread_id)
            .expect("thread");
        ConversationOrigin::of(&found)
    }

    fn proposer(
        &self,
        buffer: &'static LogBuffer,
    ) -> ProposeChangeEditsTool<tauri::test::MockRuntime> {
        ProposeChangeEditsTool::new(
            self.open.handle(),
            self.roots(),
            self.origin(),
            agent(),
            Arc::clone(&self.proposed),
        )
        .for_test(self.sites(), buffer)
    }

    fn propose(
        &self,
        args: ProposeChangeEditsArgs,
    ) -> Result<propose_change_edits::ProposeChangeEditsOutput, ToolRefusal> {
        block_on(self.proposer(&QUIET).call(args))
    }

    pub(super) fn decider(
        &self,
        sites: Box<FixedSites>,
        buffer: &'static LogBuffer,
    ) -> DecideChangeEditsTool<tauri::test::MockRuntime> {
        let origin = self.origin();
        let turn = super::authorized_turn_for(&self.open.handle(), &self.store(), &origin);
        DecideChangeEditsTool::new(self.open.handle(), self.roots(), origin, agent(), &turn)
            .for_test(human(), sites, buffer)
    }

    fn tasks(&self) -> String {
        std::fs::read_to_string(self.wc.path().join(format!("openspec/changes/{CHANGE}/tasks.md")))
            .expect("tasks")
    }
}

fn hunk(path: &str, before: Option<&str>, after: &str) -> ChangeHunkArg {
    ChangeHunkArg {
        path: path.into(),
        kind: None,
        before: before.map(str::to_string),
        after: Some(after.into()),
        after_text: None,
        lead: None,
        trail: None,
    }
}

fn two_files() -> ProposeChangeEditsArgs {
    ProposeChangeEditsArgs {
        hunks: Some(vec![
            hunk(
                "tasks.md",
                Some("- [ ] 1.1 Add the form\n"),
                "- [ ] 1.1 Add the form\n- [ ] 1.2 Logout\n",
            ),
            hunk("design.md", None, "## Decisions\n"),
        ]),
        updates: None,
        rationale: RATIONALE.into(),
    }
}

pub(super) fn decisions(entries: &[(&str, &str)]) -> DecideChangesArgs {
    DecideChangesArgs {
        instruction: Some(super::DECISION_INSTRUCTION.into()),
        decisions: Some(
            entries
                .iter()
                .map(|(id, action)| DecisionArg {
                    id: Some(id.to_string()),
                    action: Some(action.to_string()),
                })
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------------------
// propose_change_edits
// ---------------------------------------------------------------------------

// PCE-FR-QYDD / PCE-FR-MRWZ: a portable tool with the fixed description, and
// a schema with `hunks` (each with a path), `updates`, and `rationale`.
#[test]
fn pce_is_a_portable_tool_with_its_fixed_text_and_schema() {
    let f = Fixture::new();
    let tool = f.proposer(&QUIET);
    assert_eq!(
        <ProposeChangeEditsTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_change_edits"
    );
    assert_eq!(tool.description(), propose_change_edits::DESCRIPTION);
    let schema = tool.parameters();
    assert_eq!(schema["required"], serde_json::json!(["rationale"]));
    assert!(schema["properties"]["hunks"]["items"]["properties"]["path"].is_object());
    assert!(schema["properties"]["updates"]["items"]["properties"]["path"].is_object());
}

// PCE-FR-ABFJ / PCE-FR-PQUD / PCE-FR-XEYG / PCE-FR-OXSO: a call records one
// proposal across files into the bound change and conversation, the rationale
// is the comment, and the turn ends.
#[test]
fn pce_records_one_proposal_across_files_and_ends_the_turn() {
    let f = Fixture::new();
    let out = f.propose(two_files()).expect("recorded");
    assert_eq!(out.hunk_count, 2);
    assert!(f.proposed.load(Ordering::SeqCst));
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    assert_eq!(record.id, out.proposal_id);
    assert_eq!(record.thread_id, f.thread_id);
    assert!(record.ledger[1].new_file);
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).expect("thread");
    assert_eq!(thread.comments.last().expect("comment").body, RATIONALE);
}

// PCE-FR-OXMY: both forms, or neither, is a retryable invalid-arguments
// refusal, and the turn goes on.
#[test]
fn pce_takes_exactly_one_of_hunks_and_updates() {
    let f = Fixture::new();
    let mut both = two_files();
    both.updates = Some(Vec::new());
    let neither =
        ProposeChangeEditsArgs { hunks: None, updates: None, rationale: RATIONALE.into() };
    for args in [both, neither] {
        let refused = f.propose(args).unwrap_err();
        assert_eq!(
            refused,
            ToolRefusal::InvalidArguments(propose_change_edits::PROPOSAL_NOT_ONE_FORM)
        );
        assert!(refused.retryable());
    }
    assert!(!f.proposed.load(Ordering::SeqCst));
}

// PCE-FR-FEAK: a path outside the change is a retryable `InvalidArgs` with its
// place; a second proposal while one is pending is the pending refusal.
#[test]
fn pce_refusals_carry_the_kinds_of_ppc() {
    let f = Fixture::new();
    let mut outside = two_files();
    outside.hunks.as_mut().expect("hunks")[1].path = "../../x.md".into();
    let refused = f.propose(outside).unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
    assert!(refused.retryable());
    assert!(matches!(refused, ToolRefusal::InvalidArgumentsAt { at: 2, of: 2, .. }));

    f.propose(two_files()).expect("recorded");
    let pending = f.propose(two_files()).unwrap_err();
    assert_eq!(pending.reason(), "proposal_pending");
    assert_eq!(pending.kind(), ToolErrorKind::PermissionDenied);
    assert!(pending.retryable());
}

// PCE-FR-OXMY: `updates` changes the open proposal, an `add` names its path,
// and the turn ends as for a recording.
#[test]
fn pce_updates_the_open_proposal() {
    let f = Fixture::new();
    let out = f.propose(two_files()).expect("recorded");
    f.proposed.store(false, Ordering::SeqCst);
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let update = |action: &str, id: Option<&str>, path: Option<&str>| {
        propose_change_edits::ChangeUpdateArg {
            update: crate::tools::proposal_updates::UpdateArg {
                action: Some(action.into()),
                id: id.map(str::to_string),
                after: Some("## Decisions\n\nUse sessions.\n".into()),
                ..Default::default()
            },
            path: path.map(str::to_string),
        }
    };
    let args = ProposeChangeEditsArgs {
        hunks: None,
        updates: Some(vec![update("withdraw", Some(&record.ledger[1].id), None)]),
        rationale: RATIONALE.into(),
    };
    let updated = f.propose(args).expect("updated");
    assert_eq!(updated.proposal_id, out.proposal_id);
    assert!(f.proposed.load(Ordering::SeqCst));
    let missing = ProposeChangeEditsArgs {
        hunks: None,
        updates: Some(vec![update("add", None, None)]),
        rationale: RATIONALE.into(),
    };
    assert!(matches!(f.propose(missing), Err(ToolRefusal::InvalidUpdateAt { at: 1, .. })));
}

// PCE-FR-CUSY: a resolved conversation is the not retryable PermissionDenied
// refusal, and nothing is recorded.
#[test]
fn pce_a_resolved_conversation_records_nothing() {
    let f = Fixture::new();
    let found = f.origin();
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread =
        comments::locate_discussion_of(&f.store(), &target, found.discussion_id()).expect("thread");
    comments::set_resolution_to(
        &f.store(),
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let refused = f.propose(two_files()).unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refused.retryable());
    assert!(cep::list_proposals_impl(&f.store(), STREAM, CHANGE).is_empty());
}

// PCE-FR-FVPF / PCE-FR-IIDO: a turn not about a change is refused, and with no
// project open the shared refusal is given.
#[test]
fn pce_refuses_a_turn_that_is_not_about_a_change_and_a_closed_project() {
    let f = Fixture::new();
    let tool = ProposeChangeEditsTool::new(
        f.open.handle(),
        f.roots(),
        ConversationOrigin::stub_artifact("t", "a.md", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    )
    .for_test(f.sites(), &QUIET);
    let refused = block_on(tool.call(two_files())).unwrap_err();
    assert_eq!(refused.reason(), "not_a_change_conversation");
    let closed = super::closed_project();
    let tool = ProposeChangeEditsTool::new(
        closed.handle().clone(),
        f.roots(),
        f.origin(),
        agent(),
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(block_on(tool.call(two_files())).unwrap_err(), ToolRefusal::NoProjectOpen);
}

// PCE-FR-XBYT: each call logs the tool, the stream, the change, and the count,
// and no hunk text and no rationale.
#[test]
fn pce_logs_ids_and_counts_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    block_on(f.proposer(&BUFFER).call(two_files())).expect("recorded");
    let records = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .expect("buffer")
        .records;
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("info");
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["tool"], "propose_change_edits");
    assert_eq!(fields["streamId"], STREAM);
    assert_eq!(fields["change"], CHANGE);
    assert_eq!(fields["hunks"], 2);
    let all = serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>())
        .expect("json");
    assert!(!all.contains("Logout") && !all.contains("Add logout"));
}

// ---------------------------------------------------------------------------
// decide_change_edits
// ---------------------------------------------------------------------------

// DCE-FR-QIKC / DCE-FR-ILCP: a portable tool with the fixed description and
// the schema of DPC.
#[test]
fn dce_is_a_portable_tool_with_the_fixed_text_and_the_shared_schema() {
    let f = Fixture::new();
    let tool = f.decider(f.sites(), &QUIET);
    assert_eq!(
        <DecideChangeEditsTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "decide_change_edits"
    );
    assert_eq!(tool.description(), decide_change_edits::DESCRIPTION);
    assert_eq!(
        tool.parameters()["properties"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()),
        crate::tools::decide_prompt_changes::parameters()["properties"]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
    );
}

// DCE-FR-PQAP / DCE-FR-XVLE / DCE-FR-CXZG / DCE-FR-JMQQ / CEP-FR-KLWZ: entries run against
// the bound change's pending proposal; an accept writes the file, the rows
// name each path, and the closing decision appends the comment and commits.
#[test]
fn dce_accepts_and_rejects_and_the_close_commits() {
    let f = Fixture::new();
    f.propose(two_files()).expect("recorded");
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let (tasks, design) = (record.ledger[0].id.clone(), record.ledger[1].id.clone());
    let out = block_on(
        f.decider(f.sites(), &QUIET).call(decisions(&[(&tasks, "accept"), (&design, "reject")])),
    )
    .expect("decided");
    assert!(out.closed);
    assert_eq!(out.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(out.decisions[1].outcome, Outcome::Rejected);
    assert_eq!(out.decisions[0].path.as_deref(), Some("tasks.md"));
    assert_eq!(out.decisions[1].path.as_deref(), Some("design.md"));
    assert!(f.tasks().contains("1.2 Logout"));
    // CEP-FR-KLWZ: the decision is the author's, and its comment ends with
    // the instruction line of the call.
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).expect("thread");
    let decision = thread.comments.last().expect("decision");
    assert_eq!(decision.author, human());
    assert!(decision.body.starts_with("Accepted the proposed edits"), "{}", decision.body);
    assert!(
        decision.body.ends_with("Decided by arch on the author's instruction: “arch”"),
        "{}",
        decision.body
    );
    let repo = git2::Repository::open(f.wc.path()).expect("repo");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        String::from_utf8_lossy(head.message_bytes()).trim(),
        "Accept edits to OpenSpec change add-auth"
    );
}

// DCE-FR-JCHG: an entry refused with `stream_busy` is a retryable row, and the
// other entries of the call still run.
#[test]
fn dce_a_busy_stream_refuses_the_row_and_the_rest_run() {
    let f = Fixture::new();
    f.propose(two_files()).expect("recorded");
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let (tasks, design) = (record.ledger[0].id.clone(), record.ledger[1].id.clone());
    let busy = Box::new(FixedSites {
        sites: vec![(STREAM.into(), f.wc.clone())],
        busy: Mutex::new(Some("run-1".into())),
    });
    let out = block_on(
        f.decider(busy, &QUIET).call(decisions(&[(&tasks, "accept"), (&design, "reject")])),
    )
    .expect("answered");
    let refusal = out.decisions[0].refusal.as_ref().expect("refused");
    assert_eq!(refusal.code, RefusalCode::DecisionFailed);
    assert_eq!(refusal.error.as_deref(), Some(cep::ERR_STREAM_BUSY));
    assert!(refusal.retryable);
    assert_eq!(out.decisions[1].outcome, Outcome::Rejected);
    assert_eq!(f.tasks(), TASKS);
}

// DCE-FR-CXZG: `discuss` opens a hunk discussion of the change, whose opening
// comment names the hunk with its change and path, and holds the hunk.
#[test]
fn dce_discuss_opens_a_hunk_discussion_of_the_change() {
    let f = Fixture::new();
    f.propose(two_files()).expect("recorded");
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let tasks = record.ledger[0].id.clone();
    let out = block_on(f.decider(f.sites(), &QUIET).call(decisions(&[(&tasks, "discuss")])))
        .expect("decided");
    let opened = out.decisions[0].discussion_id.clone().expect("discussion");
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &opened).expect("thread");
    assert_eq!(
        thread.comments[0].attachments,
        vec![comments::Attachment::ProposalHunk {
            proposal_id: record.id.clone(),
            hunk_id: tasks.clone(),
            draft_id: None,
            artifact_id: None,
            change: Some(comments::ChangeOwnerRef {
                stream_id: STREAM.into(),
                change: CHANGE.into()
            }),
            path: "tasks.md".into(),
        }]
    );
    let now = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    assert_eq!(now.row(&tasks).expect("row").state, crate::prompt_proposals::HunkState::Discussing);
}

// DCE-FR-WYJP / DCE-FR-XVLE: a turn not about a change is refused; with no
// open proposal the call is refused whole and records nothing.
#[test]
fn dce_refuses_other_conversations_and_a_change_with_no_proposal() {
    let f = Fixture::new();
    let other = ConversationOrigin::stub_artifact("t", "a.md", false);
    let tool = DecideChangeEditsTool::new(f.open.handle(), f.roots(), other, agent(), "turn")
        .for_test(human(), f.sites(), &QUIET);
    assert!(matches!(
        block_on(tool.call(decisions(&[("x", "accept")]))),
        Err(crate::tools::decide_changes::CallRefusal::WrongConversation(_))
    ));
    let none = block_on(f.decider(f.sites(), &QUIET).call(decisions(&[("x", "accept")])));
    assert!(matches!(none, Err(crate::tools::decide_changes::CallRefusal::NoOpenProposal(_))));
    assert!(cep::list_proposals_impl(&f.store(), STREAM, CHANGE).is_empty());
}

// DCE-FR-GAHP: each call logs the tool, the stream id, the change, and the
// count of each decision.
#[test]
fn dce_logs_the_stream_the_change_and_the_counts() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    f.propose(two_files()).expect("recorded");
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    block_on(f.decider(f.sites(), &BUFFER).call(decisions(&[(&record.ledger[1].id, "reject")])))
        .expect("decided");
    let records = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .expect("buffer")
        .records;
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("info");
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["tool"], "decide_change_edits");
    assert_eq!(fields["streamId"], STREAM);
    assert_eq!(fields["change"], CHANGE);
    assert_eq!(fields["rejected"], 1);
}

// CMS-FR-FACZ / CEP-FR-LYSS: the check `open_discussion` runs for a change
// owner refuses a stream that is not live and a name that is not a change.
#[test]
fn a_change_owner_needs_a_live_stream_and_a_valid_change() {
    let f = Fixture::new();
    assert_eq!(
        cep::require_change_owner(&f.open.handle(), STREAM, CHANGE).map(|_| ()),
        Err(cep::ERR_UNKNOWN_STREAM.to_string())
    );
    assert_eq!(
        cep::require_change_owner(&f.open.handle(), STREAM, "Not A Change").map(|_| ()),
        Err(cep::ERR_CHANGE_NOT_FOUND.to_string())
    );
}

// DCE-FR-RNHP / DCE-FR-XVLE: `confirm` is not an action of a change hunk, so
// the entry is refused and nothing is decided; the tool decides only on the
// author's words.
#[test]
fn dce_refuses_other_actions_and_a_missing_instruction() {
    let f = Fixture::new();
    f.propose(two_files()).expect("recorded");
    let record = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let out = block_on(
        f.decider(f.sites(), &QUIET).call(decisions(&[(&record.ledger[0].id, "confirm")])),
    )
    .expect("answered");
    assert_eq!(out.decisions[0].refusal.as_ref().map(|r| r.code), Some(RefusalCode::InvalidAction));
    let mut wrong = decisions(&[(&record.ledger[0].id, "accept")]);
    wrong.instruction = Some("words the author never wrote".into());
    assert!(matches!(
        block_on(f.decider(f.sites(), &QUIET).call(wrong)),
        Err(crate::tools::decide_changes::CallRefusal::Authority(_))
    ));
    let now = cep::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    assert!(now.ledger.iter().all(|row| row.state == crate::prompt_proposals::HunkState::Pending));
}
