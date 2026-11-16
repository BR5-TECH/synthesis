//! Tests for `PPT-propose-plan-changes-tool.md`, against the fake GitHub tracker
//! and a mock application with a project open. No test reaches a network.

use rig::tool::{PortableTool, ToolErrorKind};
use serde_json::{json, Value};

use super::*;
use crate::comments::Attachment;
use crate::delivery::github::tests::fake::{link, Board, Mock, SECRET};
use crate::logging::{Domain, LogFilter, LogLevel};
use crate::map_plan::{MapPlanProposal, RowFields};
use crate::tools::tests::plan_fixture::{agent, human, PlanFixture};

type Tool = ProposePlanChangesTool<Mock>;

const RATIONALE: &str = "I offer one step. <spoken>One step first.</spoken>";

struct Fixture {
    plan: PlanFixture,
    proposed: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture { plan: PlanFixture::new(), proposed: Arc::new(AtomicBool::new(false)) }
    }

    fn tool_for(&self, origin: crate::agent_conversations::ConversationOrigin) -> Tool {
        Tool::new(self.plan.app(), self.plan.roots(), origin, agent(), Arc::clone(&self.proposed))
    }

    fn tool(&self) -> Tool {
        self.tool_for(self.plan.origin())
    }

    /// One call with the arguments as the model composed them.
    fn call(&self, args: Value) -> Result<Value, PlanRefusal> {
        run(&self.tool(), args)
    }

    /// A call with the standard rationale and these rows.
    fn rows(&self, rows: Value) -> Result<Value, PlanRefusal> {
        self.call(json!({ "rationale": RATIONALE, "rows": rows }))
    }

    fn proposals(&self) -> Vec<MapPlanProposal> {
        crate::map_plan::proposals_in(&self.plan.store(), &self.plan.discussion_id)
    }

    fn only_row(&self) -> crate::map_plan::MapPlanRow {
        let all = self.proposals();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].rows.len(), 1);
        all[0].rows[0].clone()
    }

    /// Reject every row of every proposal, so no row waits.
    fn decide_all(&self) {
        for proposal in self.proposals() {
            for row in &proposal.rows {
                crate::map_plan::reject_row(
                    &self.plan.app(),
                    &self.plan.discussion_id,
                    &proposal.id,
                    &row.key,
                )
                .unwrap();
            }
        }
    }
}

fn run(tool: &Tool, args: Value) -> Result<Value, PlanRefusal> {
    let parsed = serde_json::from_value(args).expect("arguments");
    tauri::async_runtime::block_on(tool.call(parsed))
        .map(|output| serde_json::to_value(output).expect("output"))
}

fn create(key: &str, title: &str) -> Value {
    json!({ "key": key, "op": "create_issue", "title": title })
}

fn modify(key: &str, number: i64, fields: Value) -> Value {
    let mut row = json!({ "key": key, "op": "modify_issue", "number": number });
    row.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    row
}

fn draft(key: &str) -> Value {
    json!({ "key": key, "op": "new_draft", "name": "Finish relay", "body": "Finish the relay." })
}

/// The refusal of a call, or a failure of the test.
fn refused(result: Result<Value, PlanRefusal>) -> PlanRefusal {
    result.expect_err("the call is refused")
}

fn modify_fields(row: &crate::map_plan::MapPlanRow) -> &crate::map_plan::ModifyIssueFields {
    match &row.fields {
        RowFields::ModifyIssue(fields) => fields,
        other => panic!("a modify row, found {other:?}"),
    }
}

fn create_fields(row: &crate::map_plan::MapPlanRow) -> &crate::map_plan::CreateIssueFields {
    match &row.fields {
        RowFields::CreateIssue(fields) => fields,
        other => panic!("a create row, found {other:?}"),
    }
}

// PPT-FR-STDD, TLC-FR-17, TLC-FR-MOWF: the tool is named as the spec says, and
// one successful call produces exactly one plan proposal and one comment. It
// writes nothing to GitHub, creates no draft and no label, and changes no cache.
#[test]
fn a_call_makes_one_proposal_and_one_comment_and_nothing_else() {
    let f = Fixture::new();
    assert_eq!(<Tool as PortableTool>::NAME, "propose_plan_changes");
    let cache = f.plan.h.cache();
    let labels = f.plan.h.fake.lock().labels.clone();
    let comments = f.plan.comments();
    f.plan.h.fake.lock().calls.clear();
    f.rows(json!([create("a", "New"), draft("b"), modify("c", 2, json!({ "title": "Renamed" }))]))
        .unwrap();
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.plan.comments(), comments + 1);
    assert!(f.plan.h.fake.writes().is_empty(), "{:?}", f.plan.h.fake.writes());
    assert_eq!(f.plan.h.cache(), cache, "the read cache is unchanged");
    assert_eq!(f.plan.h.fake.lock().labels, labels, "no label is created");
    let drafts = f.plan.h.root().list_dir(f.plan.h.root().path().join(".synthesis/drafts"));
    assert!(drafts.map(|d| d.is_empty()).unwrap_or(true), "no draft is created");
}

// PPT-FR-FFML, TLC-FR-05, TLC-FR-06: the description is the fixed text and the
// schema carries a sentence for each parameter, whatever the conversation.
#[test]
fn the_description_and_the_schema_are_fixed() {
    let f = Fixture::new();
    let other = Fixture::new();
    assert_eq!(f.tool().description(), DESCRIPTION);
    assert_eq!(other.tool().description(), DESCRIPTION);
    assert_eq!(f.tool().parameters(), other.tool().parameters());
    assert!(DESCRIPTION.starts_with("Propose work on the delivery plan"));
    assert!(DESCRIPTION.ends_with("Your turn ends when the proposal is recorded."));
    let schema = f.tool().parameters();
    assert_eq!(schema["required"], json!(["rationale", "rows"]));
    assert_eq!(schema["properties"]["rows"]["minItems"], 1);
    assert_eq!(schema["properties"]["rows"]["maxItems"], 20);
    let row = &schema["properties"]["rows"]["items"];
    assert_eq!(row["required"], json!(["key", "op"]));
    assert_eq!(
        row["properties"]["op"]["enum"],
        json!(["create_issue", "modify_issue", "new_draft"])
    );
    assert_eq!(row["properties"]["close_reason"]["enum"], json!(["completed", "not_planned"]));
    assert_eq!(
        row["properties"]["labels_add"]["description"],
        "Labels to add. Each must already exist in the repository."
    );
    assert_eq!(
        row["properties"]["close_reason"]["description"],
        "Closes the issue with this reason."
    );
    for (name, property) in row["properties"].as_object().unwrap() {
        assert!(property["description"].as_str().is_some_and(|d| !d.is_empty()), "{name}");
    }
    assert!(schema["properties"]["rationale"]["description"]
        .as_str()
        .unwrap()
        .contains("<spoken>"));
}

// PPT-FR-VFOX, TLC-FR-15: the tool is bound to the turn's Map discussion and
// agent. No argument names a discussion, a repository, a conversation, or a path.
#[test]
fn the_tool_is_bound_to_its_turn() {
    let f = Fixture::new();
    let schema = f.tool().parameters();
    let row = schema["properties"]["rows"]["items"]["properties"].as_object().unwrap();
    let names: Vec<&String> =
        schema["properties"].as_object().unwrap().keys().chain(row.keys()).collect();
    for banned in
        ["discussion", "discussion_id", "repository", "repo", "conversation", "path", "file"]
    {
        assert!(!names.iter().any(|n| n.as_str() == banned), "{banned}");
    }
    f.rows(json!([draft("a")])).unwrap();
    let proposals = f.proposals();
    assert_eq!(proposals[0].discussion_id, f.plan.discussion_id);
    let thread = crate::comments::fold_map_discussions(&f.plan.store())
        .into_iter()
        .find(|d| d.id == f.plan.discussion_id)
        .unwrap();
    assert_eq!(thread.comments.last().unwrap().author, agent());
}

// PPT-FR-DDSX: a turn that is not a Map turn, a plan that is not available, and
// no open project each have their refusal. Nothing is recorded.
#[test]
fn only_a_map_turn_with_a_plan_can_call_the_tool() {
    let f = Fixture::new();
    let elsewhere =
        crate::agent_conversations::ConversationOrigin::stub_artifact("t", "spec.md", false);
    let refusal = refused(run(
        &f.tool_for(elsewhere),
        json!({ "rationale": RATIONALE, "rows": [draft("a")] }),
    ));
    assert_eq!(refusal.kind, ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable);
    assert_eq!(refusal.message, NOT_A_MAP);

    f.plan.h.select_project(None);
    let refusal = refused(f.rows(json!([draft("a")])));
    assert_eq!((refusal.kind, refusal.retryable), (ToolErrorKind::NotFound, false));
    assert_eq!(refusal.message, PLAN_UNAVAILABLE);

    let tool = f.tool();
    f.plan.close();
    let refusal = refused(run(&tool, json!({ "rationale": RATIONALE, "rows": [draft("a")] })));
    assert_eq!(refusal, PlanRefusal::from(ToolRefusal::NoProjectOpen));
    assert!(!f.proposed.load(Ordering::SeqCst));
}

// PPT-FR-PMQL: 1 to 20 rows, each with its own key and an op. One call records
// one proposal.
#[test]
fn a_call_carries_one_to_twenty_rows_with_their_own_keys() {
    let f = Fixture::new();
    let twenty: Vec<Value> = (0..20).map(|n| draft(&format!("k{n}"))).collect();
    let twenty_one: Vec<Value> = (0..21).map(|n| draft(&format!("k{n}"))).collect();
    let cases = [
        json!([]),
        json!(twenty_one),
        json!([draft("a"), draft("a")]),
        json!([draft(" "), draft("b")]),
        json!([{ "key": "a", "op": "rename_issue" }]),
        json!([{ "key": "a" }]),
        json!([{ "key": "a", "op": "new_draft", "name": "n", "body": "b", "number": 2 }]),
        json!([{ "op": "new_draft", "name": "n", "body": "b" }]),
    ];
    for rows in cases {
        let refusal = refused(f.rows(rows.clone()));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs, "{rows}");
        assert!(refusal.retryable);
        assert_eq!(refusal.message, rows::BAD_ROWS);
    }
    assert!(f.proposals().is_empty());
    assert!(!f.proposed.load(Ordering::SeqCst));
    let out = f.rows(json!(twenty)).unwrap();
    assert_eq!(out, json!({ "proposed": true, "rows": 20 }));
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.proposals()[0].rows.len(), 20);
}

// PPT-FR-OVKT, MPR-FR-YCHW: a new issue carries a title and, optionally, a body,
// a Type, a milestone, a parent, and labels. A blank title, an unknown Type, and
// an unknown milestone are refused.
#[test]
fn a_new_issue_is_checked_against_the_plan() {
    let f = Fixture::new();
    for (row, message, code) in [
        (
            json!({ "key": "a", "op": "create_issue", "title": "  " }),
            rows::BLANK_TITLE.to_string(),
            "blank_title",
        ),
        (json!({ "key": "a", "op": "create_issue" }), rows::BLANK_TITLE.to_string(), "blank_title"),
        (
            json!({ "key": "a", "op": "create_issue", "title": "T", "type": "Saga" }),
            "The Type Saga is not offered by the repository.".to_string(),
            "unknown_type",
        ),
        (
            json!({ "key": "a", "op": "create_issue", "title": "T", "milestone": "milestone 1" }),
            "No milestone of the plan has the title milestone 1.".to_string(),
            "unknown_milestone",
        ),
    ] {
        let refusal = refused(f.rows(json!([row])));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!((refusal.message.as_str(), refusal.code), (message.as_str(), code));
    }
    assert!(f.proposals().is_empty());
    f.rows(json!([{
        "key": "a", "op": "create_issue", "title": " Relay auth ", "body": "Why.",
        "type": "feature", "milestone": "Milestone 1", "parent_issue": 2, "labels": ["DOCS"]
    }]))
    .unwrap();
    let row = f.only_row();
    assert_eq!(row.state, crate::map_plan::RowState::Waiting);
    let fields = create_fields(&row);
    assert_eq!(fields.title, "Relay auth");
    assert_eq!(fields.body.as_deref(), Some("Why."));
    assert_eq!(fields.issue_type.as_deref(), Some("Feature"));
    assert_eq!(
        fields.milestone.as_ref().map(|m| (m.number, m.title.as_str())),
        Some((1, "Milestone 1"))
    );
    assert_eq!(fields.parent, Some(crate::map_plan::ParentRef::Issue(2)));
    assert_eq!(fields.labels, vec!["docs".to_string()]);
}

// PPT-FR-KVFB, MPR-FR-GKOL, MPR-FR-RTGH: a change names a plan issue and at least
// one change. It records the `updated_at` of the issue and the value before and
// after of each changed field. A field that does not change is not in the row.
#[test]
fn a_change_records_the_values_before_and_after() {
    let f = Fixture::new();
    f.rows(json!([modify(
        "a",
        3,
        json!({
            "title": "Issue 3", "body": "A new body", "type": "Bug", "milestone": null,
            "parent_issue": 1, "status": "Done"
        })
    )]))
    .unwrap();
    let row = f.only_row();
    let fields = modify_fields(&row);
    assert_eq!((fields.number, fields.title.as_str()), (3, "Issue 3"));
    assert_eq!(fields.base_updated_at, "2026-01-01T00:00:03Z");
    let changes: Vec<&str> = fields.changes.iter().map(|c| c.field.as_str()).collect();
    assert_eq!(changes, ["body", "type", "milestone", "parent", "status"], "the title is the same");
    let by = |field: &str| fields.changes.iter().find(|c| c.field == field).unwrap();
    assert_eq!(
        (by("body").before.clone(), by("body").after.clone()),
        (json!("Body 3"), json!("A new body"))
    );
    assert_eq!(
        (by("type").before.clone(), by("type").after.clone()),
        (json!("Task"), json!("Bug"))
    );
    assert_eq!(by("milestone").before, json!({ "number": 1, "title": "Milestone 1" }));
    assert_eq!(by("milestone").after, Value::Null);
    assert_eq!(
        (by("parent").before.clone(), by("parent").after.clone()),
        (json!({ "issue": 2 }), json!({ "issue": 1 }))
    );
    assert_eq!(
        (by("status").before.clone(), by("status").after.clone()),
        (json!("Ready"), json!("Done"))
    );
}

// PPT-FR-KVFB: a number that is not a plan issue, and a row that changes
// nothing, are refused.
#[test]
fn a_change_needs_a_plan_issue_and_a_real_change() {
    let f = Fixture::new();
    f.plan.h.fake.lock().pull_requests.push(40);
    let refusal = refused(f.rows(json!([modify("a", 99, json!({ "title": "T" }))])));
    assert_eq!(refusal.kind, ToolErrorKind::NotFound);
    assert!(refusal.retryable);
    assert_eq!(refusal.message, "Issue number 99 is not part of the delivery plan.");
    assert_eq!(
        refused(f.rows(json!([modify("a", -1, json!({ "title": "T" }))]))).code,
        "not_a_plan_issue"
    );

    for fields in [
        json!({}),
        json!({ "title": "Issue 2" }),
        json!({ "milestone": null }),
        json!({ "parent_issue": 1 }),
        json!({ "status": "todo" }),
        json!({ "type": "Feature", "body": "Body 2" }),
    ] {
        let refusal = refused(f.rows(json!([draft("x"), modify("a", 2, fields.clone())])));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs, "{fields}");
        assert!(refusal.retryable);
        assert_eq!(refusal.message, "Row 2 changes nothing in issue 2 as it stands.", "{fields}");
    }
    let missing = refused(f.rows(json!([{ "key": "a", "op": "modify_issue", "title": "T" }])));
    assert_eq!(missing.message, rows::BAD_ROWS);
    assert!(f.proposals().is_empty());
}

// PPT-FR-DPQP: `parent_row` names an earlier `create_issue` row of the call.
// `parent_issue` names a plan issue, and `null` removes the parent. A row never
// gives both.
#[test]
fn a_parent_is_a_plan_issue_or_an_earlier_new_issue() {
    let f = Fixture::new();
    let child = |parent: Value| {
        let mut row = create("b", "Child");
        row.as_object_mut().unwrap().extend(parent.as_object().unwrap().clone());
        row
    };
    for rows in [
        json!([child(json!({ "parent_row": "missing" }))]),
        json!([child(json!({ "parent_row": "b" }))]),
        json!([child(json!({ "parent_row": "a" })), create("a", "Parent")]),
        json!([draft("a"), child(json!({ "parent_row": "a" }))]),
    ] {
        let refusal = refused(f.rows(rows.clone()));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs, "{rows}");
        assert!(refusal.retryable);
        assert!(refusal.message.contains("`parent_row` of row"), "{rows}: {}", refusal.message);
        assert!(refusal.message.contains("must name the `key` of an earlier `create_issue` row."));
    }
    // PPT-FR-DPQP: `parent_row` is for `create_issue` rows only.
    let on_modify = refused(
        f.rows(json!([create("a", "Parent"), modify("b", 2, json!({ "parent_row": "a" }))])),
    );
    assert_eq!(on_modify.kind, ToolErrorKind::InvalidArgs);
    assert!(on_modify.retryable);
    assert_eq!(
        on_modify.message,
        "`parent_row` of row 2 is for a `create_issue` row only. A `modify_issue` row sets `parent_issue`."
    );
    let both = child(json!({ "parent_row": "a", "parent_issue": 1 }));
    assert_eq!(refused(f.rows(json!([create("a", "Parent"), both]))).message, rows::BAD_ROWS);
    let outside = refused(f.rows(json!([child(json!({ "parent_issue": 77 }))])));
    assert_eq!(outside.message, "Issue number 77 is not part of the delivery plan.");
    assert!(f.proposals().is_empty());

    f.rows(json!([
        create("a", "Parent"),
        child(json!({ "parent_row": "a" })),
        modify("c", 2, json!({ "parent_issue": null }))
    ]))
    .unwrap();
    let rows = &f.proposals()[0].rows;
    assert_eq!(create_fields(&rows[1]).parent, Some(crate::map_plan::ParentRef::Row("a".into())));
    let removal = &modify_fields(&rows[2]).changes[0];
    assert_eq!(
        (removal.field.as_str(), removal.before.clone(), removal.after.clone()),
        ("parent", json!({ "issue": 1 }), Value::Null)
    );
}

// PPT-FR-XOMU, MPR-FR-NGSK: labels must exist in the repository, in the
// spelling of the repository. The tool creates none. A label in both lists is
// refused.
#[test]
fn labels_must_exist_and_are_never_created() {
    let f = Fixture::new();
    let known = f.plan.h.fake.lock().labels.clone();
    for row in [
        json!({ "key": "a", "op": "create_issue", "title": "T", "labels": ["docs", "invented"] }),
        modify("a", 2, json!({ "labels_add": ["invented"] })),
        modify("a", 2, json!({ "labels_remove": ["invented"] })),
    ] {
        let refusal = refused(f.rows(json!([row])));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!(
            refusal.message,
            "The label invented does not exist in the repository. This tool creates no label. Use an existing label."
        );
    }
    let both = refused(f.rows(json!([modify(
        "a",
        2,
        json!({ "labels_add": ["docs"], "labels_remove": ["DOCS"] })
    )])));
    assert_eq!(both.message, rows::BAD_ROWS);
    assert_eq!(f.plan.h.fake.lock().labels, known);

    f.plan.h.fake.lock().issue_labels.insert(2, vec!["docs".into()]);
    f.rows(json!([modify(
        "a",
        2,
        json!({ "labels_add": ["needs review", "docs"], "labels_remove": ["bug", "DOCS"] })
    )]))
    .unwrap_err();
    f.rows(json!([modify(
        "a",
        2,
        json!({ "labels_add": ["needs review", "docs"], "labels_remove": ["bug"] })
    )]))
    .unwrap();
    let row = f.only_row();
    let labels = &modify_fields(&row).changes[0];
    assert_eq!(labels.added, Some(vec!["Needs Review".to_string()]));
    assert_eq!(labels.removed, Some(Vec::new()), "a label the issue lacks is not removed");
    assert_eq!(labels.after, json!(["docs", "Needs Review"]));
}

// PPT-FR-JPFE, MPR-FR-WBKZ: `close_reason` is `completed` or `not_planned` and
// closes the issue. Another value is refused, and so is a close of a closed issue.
#[test]
fn a_close_has_a_reason_and_needs_an_open_issue() {
    let f = Fixture::new();
    let refusal = refused(f.rows(json!([modify("a", 2, json!({ "close_reason": "done" }))])));
    assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable);
    assert_eq!(refusal.message, rows::BAD_CLOSE_REASON);
    f.rows(json!([modify("a", 2, json!({ "close_reason": "not_planned" }))])).unwrap();
    let close = modify_fields(&f.only_row()).changes[0].clone();
    assert_eq!((close.field.as_str(), close.reason.as_deref()), ("close", Some("not_planned")));
    assert_eq!((close.before, close.after), (json!("open"), json!("closed")));

    let mut closed = crate::delivery::github::tests::fake::issue(4, Some("Task"));
    closed.state = "closed".into();
    f.plan.h.fake.board_issue(closed, None);
    f.plan.refresh();
    f.decide_all();
    let again = refused(f.rows(json!([modify("a", 4, json!({ "close_reason": "completed" }))])));
    assert_eq!(again.message, "Row 1 changes nothing in issue 4 as it stands.");
}

// PPT-FR-YIFZ, MPR-FR-GOHN: `status` names an option of the Status field.
#[test]
fn a_status_must_be_an_option_of_the_project() {
    let f = Fixture::new();
    let refusal = refused(f.rows(json!([modify("a", 2, json!({ "status": "Blocked" }))])));
    assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable);
    assert_eq!(refusal.message, "The Status Blocked is not an option of the Project.");
    f.rows(json!([modify("a", 2, json!({ "status": "in progress" }))])).unwrap();
    let change = modify_fields(&f.only_row()).changes[0].clone();
    assert_eq!((change.field.as_str(), change.after), ("status", json!("In Progress")));
}

// PPT-FR-QCTM: a draft row carries a name and a full prompt. It creates no
// issue, and it needs no GitHub access.
#[test]
fn a_new_draft_needs_a_name_and_a_prompt_and_no_github() {
    let f = Fixture::new();
    for (row, message) in [
        (json!({ "key": "a", "op": "new_draft", "body": "b" }), rows::BLANK_NAME),
        (json!({ "key": "a", "op": "new_draft", "name": "  ", "body": "b" }), rows::BLANK_NAME),
        (json!({ "key": "a", "op": "new_draft", "name": "n" }), rows::BLANK_BODY),
        (json!({ "key": "a", "op": "new_draft", "name": "n", "body": " \n" }), rows::BLANK_BODY),
    ] {
        let refusal = refused(f.rows(json!([row])));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!(refusal.message, message);
    }
    f.plan.h.fake.lock().calls.clear();
    f.rows(json!([draft("a")])).unwrap();
    assert!(f.plan.h.fake.calls().is_empty(), "{:?}", f.plan.h.fake.calls());
    match f.only_row().fields {
        RowFields::NewDraft(fields) => {
            assert_eq!(
                (fields.name.as_str(), fields.body.as_str()),
                ("Finish relay", "Finish the relay.")
            )
        }
        other => panic!("a draft row, found {other:?}"),
    }
}

// PPT-FR-AYQT, MPR-FR-SBYA: a call while a row waits is refused. A call is
// accepted again when every row is decided.
#[test]
fn a_card_that_waits_refuses_a_new_call() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let refusal = refused(f.rows(json!([draft("c")])));
    assert_eq!(refusal.kind, ToolErrorKind::PermissionDenied);
    assert!(refusal.retryable);
    assert_eq!(refusal.message, CARD_OPEN);
    assert!(refusal.message.contains("Wait for them"));
    let proposal = f.proposals().remove(0);
    crate::map_plan::reject_row(&f.plan.app(), &f.plan.discussion_id, &proposal.id, "a").unwrap();
    assert_eq!(refused(f.rows(json!([draft("c")]))).message, CARD_OPEN, "one row still waits");
    crate::map_plan::accept_row(&f.plan.app(), &f.plan.discussion_id, &proposal.id, "b").unwrap();
    f.rows(json!([draft("c")])).unwrap();
    assert_eq!(f.proposals().len(), 2);
}

// PPT-FR-NXPZ: a blank rationale, and a rationale with no spoken part, are
// refused.
#[test]
fn the_rationale_needs_words_and_a_spoken_part() {
    let f = Fixture::new();
    for rationale in ["", "  \n "] {
        let refusal = refused(f.call(json!({ "rationale": rationale, "rows": [draft("a")] })));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!(refusal.message, BLANK_RATIONALE);
    }
    for rationale in ["Only prose.", "Empty <spoken>  </spoken> tag."] {
        let refusal = refused(f.call(json!({ "rationale": rationale, "rows": [draft("a")] })));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!(refusal.message, RATIONALE_NO_SPOKEN_PART);
    }
    assert!(f.proposals().is_empty());
}

// PPT-FR-BSSO, CMS-FR-17: a resolved conversation refuses the call for good.
#[test]
fn a_resolved_conversation_refuses_the_call() {
    let f = Fixture::new();
    crate::comments::set_resolution_to(
        &f.plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &f.plan.discussion_id,
        true,
        &human(),
        "2026-03-02T10:00:00Z",
    )
    .unwrap();
    let refusal = refused(f.rows(json!([draft("a")])));
    assert_eq!(refusal.kind, ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable);
    assert_eq!(refusal.message, CONVERSATION_RESOLVED);
    assert!(f.proposals().is_empty());
}

// PPT-FR-UZEK: a failure to record produces neither the proposal nor the
// comment, and is the retryable `Other` refusal.
#[test]
fn a_failure_to_record_leaves_neither_a_proposal_nor_a_comment() {
    let f = Fixture::new();
    let ghost = crate::agent_conversations::ConversationOrigin::stub_map("no-such-thread");
    let comments = f.plan.comments();
    let refusal =
        refused(run(&f.tool_for(ghost), json!({ "rationale": RATIONALE, "rows": [draft("a")] })));
    assert_eq!(refusal.kind, ToolErrorKind::Other);
    assert!(refusal.retryable);
    assert_eq!(refusal.message, NOT_RECORDED);
    assert!(f.proposals().is_empty());
    assert_eq!(f.plan.comments(), comments);
    assert!(!f.proposed.load(Ordering::SeqCst));
    let unknown =
        record_refusal(&MapPlanError { code: "store_unavailable".into(), message: String::new() });
    assert_eq!(unknown.message, NOT_RECORDED);
    assert_eq!((unknown.kind, unknown.retryable), (ToolErrorKind::Other, true));
}

// PPT-FR-CXFG, CMS-FR-65, CMS-FR-OIUO: the comment holds the rationale less its
// citation markers, with the spoken part in place, and no row. Its one
// `map_plan_proposal` attachment names the proposal. The agent of the turn wrote it.
#[test]
fn the_comment_is_the_rationale_with_one_proposal_attachment() {
    let f = Fixture::new();
    let rationale = "Auth first \u{E200}cite\u{E202}turn0\u{E201}. <spoken>Auth first.</spoken>";
    f.call(json!({ "rationale": rationale, "rows": [create("a", "TITLE-IN-ROW")] })).unwrap();
    let proposal = f.proposals().remove(0);
    let thread = crate::comments::fold_map_discussions(&f.plan.store())
        .into_iter()
        .find(|d| d.id == f.plan.discussion_id)
        .unwrap();
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.id, proposal.comment_id);
    assert_eq!(comment.body, "Auth first. <spoken>Auth first.</spoken>");
    assert!(!comment.body.contains("TITLE-IN-ROW"));
    assert_eq!(comment.author, agent());
    let references: Vec<&Attachment> = comment
        .attachments
        .iter()
        .filter(|a| matches!(a, Attachment::MapPlanProposal { .. }))
        .collect();
    assert_eq!(references, vec![&Attachment::MapPlanProposal { proposal_id: proposal.id.clone() }]);
}

// PPT-FR-PRPX, CVL-FR-15: a valid call returns the data object and sets the
// flag that ends the turn. A refused call ends nothing. A second proposal of
// one reply is told why it was not recorded.
#[test]
fn a_proposal_ends_the_turn_and_a_refusal_does_not() {
    let f = Fixture::new();
    refused(f.call(json!({ "rationale": RATIONALE, "rows": [] })));
    assert!(!f.proposed.load(Ordering::SeqCst), "a refused call ends nothing");
    let out = f.rows(json!([draft("a"), create("b", "T")])).unwrap();
    assert_eq!(out, json!({ "proposed": true, "rows": 2 }));
    assert!(f.proposed.load(Ordering::SeqCst));
    assert!(ONE_AT_A_TIME.contains("Only one plan proposal can be recorded in a turn"));
}

// PPT-FR-WBYJ, TLC-FR-15: the tool writes nothing to GitHub, creates no draft
// and no label, changes no cache, and holds no state between calls: one tool
// serves two calls.
#[test]
fn the_tool_changes_nothing_outside_the_proposal_and_holds_no_state() {
    let f = Fixture::new();
    let tool = f.tool();
    let cache = f.plan.h.cache();
    run(
        &tool,
        json!({ "rationale": RATIONALE, "rows": [modify("a", 2, json!({ "title": "One" }))] }),
    )
    .unwrap();
    f.decide_all();
    run(
        &tool,
        json!({ "rationale": RATIONALE, "rows": [modify("a", 2, json!({ "title": "Two" }))] }),
    )
    .unwrap();
    assert_eq!(f.proposals().len(), 2);
    assert!(f.plan.h.fake.writes().is_empty());
    assert_eq!(f.plan.h.cache(), cache);
    assert_eq!(modify_fields(&f.proposals()[1].rows[0]).changes[0].after, json!("Two"));
}

// PPT-FR-DGAA, TLC-FR-14: one INFO record at a recording names the tool, the
// discussion, and the count of rows of each kind. One WARN record at a refusal
// names the tool and the reason. Both are under `ai` and `backend`, and none
// carries a title, a body, a label name, a draft name, or the rationale.
#[test]
fn each_call_is_logged_without_content() {
    static BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();
    let f = Fixture::new();
    let rationale = "ZEBRA-RATIONALE <spoken>x</spoken>";
    let rows = json!([
        { "key": "a", "op": "create_issue", "title": "ZEBRA-TITLE", "body": "ZEBRA-BODY", "labels": ["docs"] },
        { "key": "b", "op": "new_draft", "name": "ZEBRA-NAME", "body": "ZEBRA-PROMPT" },
        { "key": "c", "op": "new_draft", "name": "ZEBRA-NAME-2", "body": "ZEBRA-PROMPT" },
    ]);
    let args = json!({ "rationale": rationale, "rows": rows });
    run(&f.tool().with_buffer(&BUFFER), args.clone()).unwrap();
    refused(run(&f.tool().with_buffer(&BUFFER), args));
    let records = |domain: Domain| {
        BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    domains: vec![domain],
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .unwrap()
            .records
    };
    let ai = records(Domain::Ai);
    assert_eq!(records(Domain::Backend).len(), ai.len());
    let info: Vec<_> = ai.iter().filter(|r| r.level == LogLevel::Info).collect();
    let warn: Vec<_> = ai.iter().filter(|r| r.level == LogLevel::Warn).collect();
    assert_eq!((info.len(), warn.len()), (1, 1));
    assert_eq!(info[0].fields["tool"], "propose_plan_changes");
    assert_eq!(info[0].fields["discussionId"], json!(f.plan.discussion_id));
    assert_eq!(info[0].fields["rows"], 3);
    assert_eq!(
        (info[0].fields["createRows"].clone(), info[0].fields["draftRows"].clone()),
        (json!(1), json!(2))
    );
    assert_eq!(warn[0].fields["tool"], "propose_plan_changes");
    assert_eq!(warn[0].fields["reason"], "card_open");
    let text = serde_json::to_string(&ai).unwrap();
    for marker in ["ZEBRA", "docs", SECRET] {
        assert!(!text.contains(marker), "a record carries content");
    }
}

// PPT-FR-XMZF, TLC-FR-18, TLC-FR-MOWF: the tool reads the Types, the labels, and
// the Status options through the delivery plan, at most three requests, writes
// nothing, and carries no credential in an argument or a record.
#[test]
fn a_call_makes_at_most_three_reads_and_carries_no_credential() {
    let f = Fixture::new();
    f.plan.h.fake.lock().calls.clear();
    f.rows(json!([
        { "key": "a", "op": "create_issue", "title": "T", "type": "Task", "labels": ["docs"] },
        modify("b", 2, json!({ "status": "Done", "milestone": "Milestone 1" })),
        { "key": "c", "op": "new_draft", "name": "n", "body": "b" },
    ]))
    .unwrap();
    let calls = f.plan.h.fake.calls();
    assert!(calls.len() <= 3, "{calls:?}");
    assert!(calls.iter().all(|c| c == "list_issue_types" || c == "list_labels"), "{calls:?}");
    assert!(f.plan.h.fake.writes().is_empty());
    f.decide_all();
    f.plan.h.fake.lock().calls.clear();
    f.rows(json!([modify("a", 2, json!({ "status": "Done" }))])).unwrap();
    assert!(f.plan.h.fake.calls().is_empty(), "the Status options come from the read cache");
    let text = format!("{:?}{}", DESCRIPTION, f.tool().parameters());
    assert!(!text.contains(SECRET) && !text.to_lowercase().contains("token"));
}

// PPT-FR-KVFB: a pull request and an issue of another repository are not plan
// issues, so a change to one is refused.
#[test]
fn a_pull_request_is_never_a_plan_issue() {
    let f = Fixture::new();
    f.plan.h.fake.lock().board.push(("PR_1".into(), None, Board::PullRequest));
    f.plan.h.fake.lock().pull_requests.push(40);
    f.plan.refresh();
    let refusal = refused(f.rows(json!([modify("a", 40, json!({ "title": "T" }))])));
    assert_eq!(refusal.code, "not_a_plan_issue");
    let mut foreign = crate::delivery::github::tests::fake::issue(41, Some("Task"));
    foreign.owner = "other".into();
    foreign.parent = Some(link(1, "open"));
    f.plan.h.fake.board_issue(foreign, None);
    f.plan.refresh();
    assert_eq!(
        refused(f.rows(json!([modify("a", 41, json!({ "title": "T" }))]))).code,
        "not_a_plan_issue"
    );
}
