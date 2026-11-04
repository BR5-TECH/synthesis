//! Tests for `PND-propose-new-draft-tool.md`.

use rig::tool::{PortableTool, ToolErrorKind};
use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, Participant};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted};

const RATIONALE: &str = "The auth part ships alone. <spoken>Auth first.</spoken>";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: Some("Developer".into()),
    }
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    proposed: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("relay")).expect("draft");
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch decompose it".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            draft_id: created.draft.id,
            thread_id: thread.id,
            proposed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn tool_for(
        &self,
        origin: ConversationOrigin,
    ) -> ProposeNewDraftTool<tauri::test::MockRuntime> {
        ProposeNewDraftTool::new(
            self.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() },
            origin,
            agent(),
            Arc::clone(&self.proposed),
        )
    }

    fn tool(&self) -> ProposeNewDraftTool<tauri::test::MockRuntime> {
        self.tool_for(ConversationOrigin::stub_draft(
            self.thread_id.clone(),
            self.draft_id.clone(),
            false,
        ))
    }

    fn call(&self, args: serde_json::Value) -> Result<ProposeNewDraftOutput, ToolRefusal> {
        block_on(self.tool().call(serde_json::from_value(args).expect("args")))
    }

    fn comments(&self) -> usize {
        crate::comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .map(|t| t.comments.len())
            .unwrap_or(0)
    }

    fn proposals(&self) -> Vec<nd::NewDraftProposal> {
        nd::list_impl(&self.root(), &self.draft_id).unwrap()
    }
}

fn new_draft(name: &str, position: i64, total: i64) -> serde_json::Value {
    serde_json::json!({
        "rationale": RATIONALE,
        "name": name,
        "body": "# Part\n\nThe part.\n",
        "position": position,
        "total": total,
    })
}

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

// PND-FR-UXPK / PND-FR-XJHL / PND-FR-JNGE: a portable tool with the fixed
// description and a schema that names no draft, folder, or path.
#[test]
fn it_is_a_portable_tool_with_the_fixed_description() {
    let f = Fixture::new();
    let tool = f.tool();
    assert_eq!(
        <ProposeNewDraftTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_new_draft"
    );
    assert_eq!(tool.description(), DESCRIPTION);
    let schema = tool.parameters();
    let mut keys: Vec<_> = schema["properties"].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["body", "name", "position", "rationale", "total", "update"]);
    assert_eq!(schema["required"], serde_json::json!(["rationale"]));
}

// PND-FR-INGY / PND-FR-KGRN / PND-FR-LKHU / PND-FR-HXUB / PND-FR-VHYX: a call
// records one proposal and one comment by the agent, sets the flag, and
// returns the data object.
#[test]
fn a_call_records_one_proposal_and_one_comment_and_ends_the_turn() {
    let f = Fixture::new();
    let before = f.comments();
    let out = f.call(new_draft("relay-auth", 1, 3)).expect("proposed");
    assert_eq!(out, ProposeNewDraftOutput { proposed: true });
    assert!(f.proposed.load(Ordering::SeqCst));
    assert_eq!(f.comments(), before + 1);
    let all = f.proposals();
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].name.as_str(), all[0].position, all[0].total), ("relay-auth", 1, 3));
    assert_eq!(all[0].agent, agent());
    assert_eq!(all[0].rationale, RATIONALE);
}

// PND-FR-UQCD: the original draft and its prompt are unchanged and no draft is
// created.
#[test]
fn a_call_changes_nothing_of_the_original_and_creates_no_draft() {
    let f = Fixture::new();
    let root = f.root();
    let record = crate::drafts::read_draft_record(&root, &f.draft_id).unwrap();
    let prompt = crate::drafts::read_draft_prompt(&root, &f.draft_id).unwrap().content;
    f.call(new_draft("a", 1, 2)).unwrap();
    assert_eq!(crate::drafts::read_draft_record(&root, &f.draft_id).unwrap(), record);
    assert_eq!(crate::drafts::read_draft_prompt(&root, &f.draft_id).unwrap().content, prompt);
    assert_eq!(crate::drafts::list_drafts_impl(&root).drafts.len(), 1);
}

// PND-FR-YFES: a turn that is not about a draft, and no open project, refuse.
#[test]
fn a_turn_not_on_a_draft_and_no_open_project_refuse() {
    let f = Fixture::new();
    let origin = ConversationOrigin::stub_artifact("t", "spec.md", false);
    let refused =
        block_on(f.tool_for(origin).call(serde_json::from_value(new_draft("a", 1, 1)).unwrap()))
            .unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refused.retryable());
    assert_eq!(refused.to_string(), NOT_A_DRAFT);

    let closed = closed_project();
    let tool = ProposeNewDraftTool::new(
        closed.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
        ConversationOrigin::stub_draft("t", "d", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    );
    let refused =
        block_on(tool.call(serde_json::from_value(new_draft("a", 1, 1)).unwrap())).unwrap_err();
    assert_eq!(refused, ToolRefusal::NoProjectOpen);
}

// PND-FR-AUFP / PND-FR-AFEX / PND-FR-DDUJ: the argument refusals, each
// retryable, and nothing recorded.
#[test]
fn the_argument_refusals_record_nothing() {
    let f = Fixture::new();
    let cases = [
        (
            serde_json::json!({"rationale": RATIONALE, "name": " ", "body": "b", "position": 1, "total": 1}),
            BLANK_NAME,
        ),
        (
            serde_json::json!({"rationale": RATIONALE, "name": "n", "body": "", "position": 1, "total": 1}),
            BLANK_BODY,
        ),
        (
            serde_json::json!({"rationale": RATIONALE, "name": "n", "body": "b", "position": 0, "total": 1}),
            BAD_POSITION,
        ),
        (
            serde_json::json!({"rationale": RATIONALE, "name": "n", "body": "b", "position": 3, "total": 2}),
            BAD_POSITION,
        ),
        (serde_json::json!({"rationale": RATIONALE, "name": "n", "body": "b"}), BAD_POSITION),
        (
            serde_json::json!({"rationale": "  ", "name": "n", "body": "b", "position": 1, "total": 1}),
            BLANK_RATIONALE,
        ),
        (
            serde_json::json!({"rationale": "no tag", "name": "n", "body": "b", "position": 1, "total": 1}),
            RATIONALE_NO_SPOKEN_PART,
        ),
        (
            serde_json::json!({"rationale": "no tag", "update": {"action": "withdraw"}}),
            RATIONALE_NO_SPOKEN_PART,
        ),
    ];
    for (args, message) in cases {
        let refused = f.call(args).unwrap_err();
        assert_eq!(refused, ToolRefusal::InvalidArguments(message));
        assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
        assert!(refused.retryable());
    }
    assert!(f.proposals().is_empty());
    assert!(!f.proposed.load(Ordering::SeqCst));
}

// PND-FR-AFEX: an earlier proposal at the same position refuses no call.
#[test]
fn the_same_position_can_be_proposed_again_after_a_rejection() {
    let f = Fixture::new();
    f.call(new_draft("a", 2, 3)).unwrap();
    let id = f.proposals()[0].id.clone();
    nd::decide_new_draft_in_process(
        &f.open.handle(),
        &f.root(),
        &f.root(),
        &id,
        nd::NewDraftDecision::Reject,
        None,
        &human(),
    )
    .unwrap();
    assert!(f.call(new_draft("a2", 2, 3)).is_ok());
}

// PND-FR-BGJI: a second proposal while one is open is the retryable
// PermissionDenied refusal.
#[test]
fn a_proposal_while_one_is_open_is_refused_and_retryable() {
    let f = Fixture::new();
    f.call(new_draft("a", 1, 2)).unwrap();
    let refused = f.call(new_draft("b", 2, 2)).unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);
    assert!(refused.retryable());
    assert_eq!(refused.to_string(), PROPOSAL_OPEN);
    assert_eq!(f.proposals().len(), 1);
}

// PND-FR-GPNN / PND-FR-BVNO: a closed draft and a resolved conversation are
// not retryable, each with its own message.
#[test]
fn a_closed_draft_and_a_resolved_conversation_are_not_retryable() {
    let f = Fixture::new();
    crate::drafts::set_draft_status_impl(
        &f.root(),
        &f.draft_id,
        crate::drafts::DraftStatus::Archived,
    )
    .unwrap();
    let refused = f.call(new_draft("a", 1, 1)).unwrap_err();
    assert_eq!(refused.to_string(), DRAFT_ARCHIVED);
    assert!(!refused.retryable());
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);

    let g = Fixture::new();
    crate::comments::set_resolution_to(
        &g.root(),
        crate::comments::ThreadRef::discussion(&g.draft_id),
        &g.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let refused = g.call(new_draft("a", 1, 1)).unwrap_err();
    assert_eq!(refused.to_string(), CONVERSATION_RESOLVED);
    assert!(!refused.retryable());
}

// PND-FR-GPNN: every closed state maps to its own fixed refusal.
#[test]
fn every_closed_state_maps_to_its_own_refusal() {
    for (error, message) in [
        (nd::ERR_DRAFT_ARCHIVED, DRAFT_ARCHIVED),
        (nd::ERR_DRAFT_GRADUATED, DRAFT_GRADUATED),
        (nd::ERR_DRAFT_LOCKED, DRAFT_LOCKED),
        (nd::ERR_DRAFT_GITHUB_SHADOW, DRAFT_GITHUB_TASK),
        (nd::ERR_DECOMPOSITION_CONFIRMED, DECOMPOSITION_CONFIRMED),
    ] {
        let refusal = refusal_for(error);
        assert_eq!(refusal.to_string(), message);
        assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
        assert!(!refusal.retryable());
    }
    // PND-FR-SQTD: a write that failed is the retryable `Other`.
    let failed = refusal_for(nd::ERR_WRITE_FAILED);
    assert_eq!(failed.kind(), ToolErrorKind::Other);
    assert!(failed.retryable());
    assert_eq!(failed.to_string(), NOT_RECORDED);
}

// PND-FR-XGNE: an open draft change proposal refuses no call of this tool.
#[test]
fn an_open_change_proposal_refuses_no_new_draft() {
    let f = Fixture::new();
    let root = f.root();
    let created = crate::drafts::read_draft_record(&root, &f.draft_id).unwrap();
    let path = created.prompt_path.unwrap();
    crate::drafts::save_draft_file_impl(&root, &f.draft_id, &path, "Loose notes.\n").unwrap();
    crate::draft_proposals::record_proposal(
        &f.open.handle(),
        &root,
        &root,
        crate::draft_proposals::NewProposal {
            draft_id: &f.draft_id,
            path: &path,
            hunks: &[crate::draft_proposals::hunks::ProposedHunk {
                kind: crate::draft_proposals::anchors::HunkKind::Replace,
                before: Some("Loose notes.".into()),
                after: Some("Tight notes.".into()),
                after_text: None,
                lead: None,
                trail: None,
                note: None,
            }],
            rationale: RATIONALE,
            agent: &agent(),
            target: crate::comments::ThreadRef::discussion(&f.draft_id),
            thread_id: &f.thread_id,
        },
    )
    .expect("a change proposal");
    assert!(f.call(new_draft("a", 1, 1)).is_ok());
}

// PND-FR-APJP / PND-FR-HVWR / PND-FR-WQJQ / PND-FR-VHYX / PDC-FR-DTNJ: a change
// replaces the
// fields given and a withdraw takes the proposal back, each with one comment.
#[test]
fn an_update_changes_or_withdraws_the_open_proposal() {
    let f = Fixture::new();
    f.call(new_draft("a", 1, 2)).unwrap();
    let before = f.comments();
    f.call(serde_json::json!({"rationale": RATIONALE, "update": {"action": "change"}, "total": 4}))
        .unwrap();
    let p = &f.proposals()[0];
    assert_eq!((p.name.as_str(), p.total, p.revision), ("a", 4, 2));
    assert_eq!(f.comments(), before + 1);
    f.call(serde_json::json!({"rationale": RATIONALE, "update": {"action": "withdraw"}})).unwrap();
    assert_eq!(f.proposals()[0].state, nd::NewDraftState::Withdrawn);
    assert_eq!(f.comments(), before + 2);
}

// PND-FR-OHTJ / PND-FR-FMRO: no open proposal, and a malformed update, refuse.
#[test]
fn an_update_without_an_open_proposal_or_malformed_is_refused() {
    let f = Fixture::new();
    let none = f
        .call(serde_json::json!({"rationale": RATIONALE, "update": {"action": "withdraw"}}))
        .unwrap_err();
    assert_eq!(none, ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL));
    f.call(new_draft("a", 1, 2)).unwrap();
    for update in [
        serde_json::json!({"rationale": RATIONALE, "update": {}}),
        serde_json::json!({"rationale": RATIONALE, "update": {"action": "rename"}}),
        serde_json::json!({"rationale": RATIONALE, "update": {"action": "change"}}),
        serde_json::json!({"rationale": RATIONALE, "update": {"action": "withdraw"}, "name": "x"}),
    ] {
        assert_eq!(f.call(update).unwrap_err(), ToolRefusal::InvalidArguments(UPDATE_MALFORMED));
    }
    let same = f
        .call(serde_json::json!({"rationale": RATIONALE, "update": {"action": "change"}, "name": "a"}))
        .unwrap_err();
    assert_eq!(same, ToolRefusal::InvalidArguments(NO_CHANGE));
    assert_eq!(f.proposals()[0].revision, 1);
}

static PND_LOG: LogBuffer = LogBuffer::new();

// PND-FR-HGCC / PND-FR-RXRE: one INFO on success and one WARN on refusal, under
// `ai` and `backend`, carrying no name, body, or rationale.
#[test]
fn each_call_is_logged_without_the_name_the_body_or_the_rationale() {
    let f = Fixture::new();
    let args = serde_json::json!({
        "rationale": "ZEBRA-RATIONALE <spoken>x</spoken>",
        "name": "ZEBRA-NAME",
        "body": "ZEBRA-BODY",
        "position": 1,
        "total": 2,
    });
    block_on(f.tool().with_buffer(&PND_LOG).call(serde_json::from_value(args.clone()).unwrap()))
        .unwrap();
    block_on(f.tool().with_buffer(&PND_LOG).call(serde_json::from_value(args).unwrap()))
        .unwrap_err();
    let ai = records(&PND_LOG, Domain::Ai);
    let backend = records(&PND_LOG, Domain::Backend);
    assert!(ai.iter().any(|r| r.level == LogLevel::Info));
    assert!(ai.iter().any(|r| r.level == LogLevel::Warn));
    assert!(!backend.is_empty());
    let text = serde_json::to_string(&ai).unwrap();
    // The markers stand for user content, and an assertion message names
    // none of them.
    for marker in ["ZEBRA-RATIONALE", "ZEBRA-NAME", "ZEBRA-BODY"] {
        assert!(!text.contains(marker), "a log record carries a marked value");
    }
    assert!(text.contains(&f.draft_id));
}

// PND-FR-YPCH: the result a second call of one reply is told.
#[test]
fn a_second_call_in_one_reply_is_told_one_at_a_time() {
    assert!(ONE_AT_A_TIME.contains("Only one new draft"));
}

// PDC-FR-WEWV / PND-FR-XGNE: a pending new-draft proposal does not take the
// pending slot of `propose_draft_changes`.
#[test]
fn a_pending_new_draft_does_not_block_a_change_proposal() {
    let f = Fixture::new();
    f.call(new_draft("a", 1, 2)).unwrap();
    let root = f.root();
    let path = crate::drafts::read_draft_record(&root, &f.draft_id).unwrap().prompt_path.unwrap();
    crate::drafts::save_draft_file_impl(&root, &f.draft_id, &path, "Loose notes.\n").unwrap();
    let recorded = crate::draft_proposals::record_proposal(
        &f.open.handle(),
        &root,
        &root,
        crate::draft_proposals::NewProposal {
            draft_id: &f.draft_id,
            path: &path,
            hunks: &[crate::draft_proposals::hunks::ProposedHunk {
                kind: crate::draft_proposals::anchors::HunkKind::Replace,
                before: Some("Loose notes.".into()),
                after: Some("Tight notes.".into()),
                after_text: None,
                lead: None,
                trail: None,
                note: None,
            }],
            rationale: RATIONALE,
            agent: &agent(),
            target: crate::comments::ThreadRef::discussion(&f.draft_id),
            thread_id: &f.thread_id,
        },
    );
    assert!(recorded.is_ok());
}
