//! PPT-FR-MYQS to PPT-FR-ISZR: the `updates` form of `propose_plan_changes`,
//! which changes the open proposal in place.

use super::*;
use crate::comments::ProposalUpdateCounts;
use crate::map_plan::RowState;

impl Fixture {
    /// A call with the standard rationale and these updates.
    fn updates(&self, updates: Value) -> Result<Value, PlanRefusal> {
        self.call(json!({ "rationale": RATIONALE, "updates": updates }))
    }

    fn keys(&self) -> Vec<String> {
        self.proposals().last().unwrap().rows.iter().map(|r| r.key.clone()).collect()
    }

    fn row(&self, key: &str) -> crate::map_plan::MapPlanRow {
        self.proposals().last().unwrap().rows.iter().find(|r| r.key == key).unwrap().clone()
    }

    fn last_attachments(&self) -> Vec<Attachment> {
        let thread = crate::comments::find_map_discussion_in(&self.plan.store(), None).unwrap();
        thread.comments.last().unwrap().attachments.clone()
    }
}

fn change(key: &str, row: Value) -> Value {
    json!({ "action": "change", "key": key, "row": row })
}

fn add(key: &str, row: Value) -> Value {
    json!({ "action": "add", "key": key, "row": row })
}

fn withdraw(key: &str) -> Value {
    json!({ "action": "withdraw", "key": key })
}

// PPT-FR-STLY, PPT-FR-ISZR, PPT-FR-PRPX, CMS-FR-BSQB: one call changes, adds,
// and withdraws rows of the open proposal, appends one comment whose attachment
// carries the counts, and ends the turn. It writes nothing to GitHub.
#[test]
fn an_update_changes_the_open_proposal_in_place() {
    let f = Fixture::new();
    f.rows(json!([create("a", "First"), create("b", "Second"), draft("c")])).unwrap();
    let proposal = f.proposals().remove(0);
    let comments = f.plan.comments();
    f.proposed.store(false, Ordering::SeqCst);
    f.plan.h.fake.lock().calls.clear();
    let out = f
        .updates(json!([
            change("a", json!({ "op": "create_issue", "title": "First, shorter" })),
            withdraw("b"),
            add("d", draft("ignored")),
        ]))
        .unwrap();
    assert_eq!(out, json!({ "updated": true, "changed": 1, "added": 1, "withdrawn": 1 }));
    assert!(f.proposed.load(Ordering::SeqCst), "the turn ends");
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.proposals()[0].id, proposal.id);
    assert_eq!(f.keys(), ["a", "c", "d"]);
    assert_eq!(create_fields(&f.row("a")).title, "First, shorter");
    assert_eq!(f.plan.comments(), comments + 1);
    assert_eq!(
        f.last_attachments(),
        [Attachment::MapPlanProposal {
            proposal_id: proposal.id,
            update: Some(ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 1 }),
        }]
    );
    assert!(f.plan.h.fake.writes().is_empty());
}

// PPT-FR-STLY, MPR-FR-KSRG: a rejected row can be changed, and it waits for the
// author again.
#[test]
fn a_rejected_row_can_be_changed() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let proposal = f.proposals().remove(0);
    crate::map_plan::reject_row(&f.plan.app(), &f.plan.discussion_id, &proposal.id, "a").unwrap();
    f.updates(json!([change(
        "a",
        json!({ "op": "new_draft", "name": "Better", "body": "A better prompt." })
    )]))
    .unwrap();
    assert_eq!(f.row("a").state, RowState::Waiting);
}

// PPT-FR-MYQS: a call gives `rows` or `updates`, never both and never neither.
#[test]
fn a_call_gives_one_form() {
    let f = Fixture::new();
    f.rows(json!([draft("a")])).unwrap();
    let before = f.proposals();
    let comments = f.plan.comments();
    for args in [
        json!({ "rationale": RATIONALE }),
        json!({ "rationale": RATIONALE, "rows": [draft("b")], "updates": [withdraw("a")] }),
    ] {
        let refusal = refused(f.call(args));
        assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable);
        assert_eq!(refusal.message, super::super::updates::NOT_ONE_FORM);
    }
    assert_eq!(f.proposals(), before);
    assert_eq!(f.plan.comments(), comments);
}

// PPT-FR-STLY, MPR-FR-NSEP: with no waiting row there is nothing to update.
#[test]
fn no_waiting_row_is_no_open_proposal() {
    let f = Fixture::new();
    let refusal = refused(f.updates(json!([add("a", draft("a"))])));
    assert_eq!(refusal.message, super::super::updates::NO_OPEN_PROPOSAL);
    f.rows(json!([draft("a")])).unwrap();
    f.decide_all();
    let refusal = refused(f.updates(json!([change("a", draft("a"))])));
    assert_eq!(refusal.code, "no_open_proposal");
    assert_eq!(f.row("a").state, RowState::Rejected);
}

// PPT-FR-DPRP, PPT-FR-KOCG: a malformed update, a key that names no row, a key
// named twice, and an added key that a row holds are refused, with the place of
// the update where the refusal is about one update.
#[test]
fn malformed_updates_are_refused_with_their_place() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let before = f.proposals();
    let malformed = [
        json!([withdraw("b"), { "action": "rename", "key": "a" }]),
        json!([withdraw("b"), { "action": "withdraw" }]),
        json!([withdraw("b"), { "action": "change", "key": "a" }]),
        json!([withdraw("b"), { "action": "add", "key": "z" }]),
        json!([withdraw("b"), { "action": "withdraw", "key": "a", "row": draft("a") }]),
    ];
    for updates in malformed {
        let refusal = refused(f.updates(updates.clone()));
        assert_eq!(refusal.code, "update_malformed", "{updates}");
        assert!(refusal.message.starts_with("Update 2 of 2: "), "{}", refusal.message);
    }
    let missing = refused(f.updates(json!([withdraw("z")])));
    assert_eq!(missing.code, "no_such_row");
    assert!(missing.message.starts_with("Update 1 of 1: "));
    let twice = refused(f.updates(json!([withdraw("a"), change("a", draft("a"))])));
    assert_eq!(twice.code, "row_named_twice");
    let taken = refused(f.updates(json!([add("a", draft("a"))])));
    assert_eq!(taken.code, "bad_rows");
    assert_eq!(f.proposals(), before);
}

// PPT-FR-JDUV: an accepted row is settled, a change keeps the `op`, and an
// update that leaves no row is refused.
#[test]
fn settled_rows_new_ops_and_empty_proposals_are_refused() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let proposal = f.proposals().remove(0);
    crate::map_plan::accept_row(&f.plan.app(), &f.plan.discussion_id, &proposal.id, "a").unwrap();
    let settled = refused(f.updates(json!([change("a", draft("a"))])));
    assert_eq!(settled.code, "row_settled");
    assert!(refused(f.updates(json!([withdraw("a")]))).code == "row_settled");
    let op = refused(f.updates(json!([change("b", create("b", "Issue"))])));
    assert_eq!(op.code, "op_changed");
    let fresh = Fixture::new();
    fresh.rows(json!([draft("a"), draft("b")])).unwrap();
    let empty = refused(fresh.updates(json!([withdraw("a"), withdraw("b")])));
    assert_eq!(empty.message, super::super::updates::NO_ROW_LEFT);
    assert_eq!(fresh.keys(), ["a", "b"]);
}

// PPT-FR-KOTV: the row of a change follows the rules of a row of `rows`, and a
// refusal about it names the place of the update.
#[test]
fn a_changed_row_follows_the_rules_of_rows() {
    let f = Fixture::new();
    f.rows(json!([create("a", "First"), draft("b")])).unwrap();
    let blank = refused(f.updates(json!([
        withdraw("b"),
        change("a", json!({ "op": "create_issue", "title": " " })),
    ])));
    assert_eq!(blank.message, format!("Update 2 of 2: {}", rows::BLANK_TITLE));
    let label = refused(f.updates(json!([change(
        "a",
        json!({ "op": "create_issue", "title": "T", "labels": ["nope"] })
    )])));
    assert_eq!(label.code, "unknown_label");
    assert!(label.message.starts_with("Update 1 of 1: "));
    assert_eq!(create_fields(&f.row("a")).title, "First");
}

// PPT-FR-KOTV, PPT-FR-UZEK: a changed `modify_issue` row reads the issue from
// GitHub again, so it carries the current `updated_at` and values before.
#[test]
fn a_changed_modify_row_reads_the_issue_again() {
    let f = Fixture::new();
    f.rows(json!([modify("a", 3, json!({ "title": "Renamed" }))])).unwrap();
    {
        let mut fake = f.plan.h.fake.lock();
        let issue = fake.issues.iter_mut().find(|i| i.number == 3).unwrap();
        issue.title = "Renamed on GitHub".into();
        issue.updated_at = "2026-02-02T00:00:00Z".into();
    }
    f.updates(json!([change("a", modify("a", 3, json!({ "title": "Renamed again" })))])).unwrap();
    let row = f.row("a");
    let fields = modify_fields(&row);
    assert_eq!(fields.base_updated_at, "2026-02-02T00:00:00Z");
    let title = fields.changes.iter().find(|c| c.field == "title").unwrap();
    assert_eq!(title.before, json!("Renamed on GitHub"));
    assert_eq!(title.after, json!("Renamed again"));
}

// PPT-FR-KOTV, MPR-FR-HQXX: `parent_row` names a `create_issue` row that the
// proposal holds after the update and that comes before it. A kept child keeps
// its parent, so that parent can not be withdrawn alone.
#[test]
fn parent_rows_name_rows_that_stay() {
    let f = Fixture::new();
    f.rows(json!([
        create("p", "Parent"),
        { "key": "c", "op": "create_issue", "title": "Child", "parent_row": "p" },
    ]))
    .unwrap();
    let orphan = refused(f.updates(json!([withdraw("p")])));
    assert_eq!(orphan.code, "bad_parent_row");
    let later = refused(f.updates(json!([
        change("p", json!({ "op": "create_issue", "title": "P", "parent_row": "n" })),
        add("n", create("n", "New")),
    ])));
    assert_eq!(later.code, "bad_parent_row");
    assert!(later.message.starts_with("Update 1 of 2: "));
    f.updates(json!([add(
        "n",
        json!({ "op": "create_issue", "title": "Grandchild", "parent_row": "c" })
    )]))
    .unwrap();
    assert_eq!(f.keys(), ["p", "c", "n"]);
    f.updates(json!([withdraw("n"), withdraw("c"), withdraw("p"), add("d", draft("d"))])).unwrap();
    assert_eq!(f.keys(), ["d"]);
}

// PPT-FR-KOTV: the proposal holds at most 20 rows after the update.
#[test]
fn an_update_keeps_at_most_twenty_rows() {
    let f = Fixture::new();
    let rows: Vec<Value> = (0..20).map(|n| draft(&format!("r{n}"))).collect();
    f.rows(json!(rows)).unwrap();
    let refusal = refused(f.updates(json!([add("extra", draft("extra"))])));
    assert_eq!(refusal.code, "bad_rows");
    assert_eq!(f.keys().len(), 20);
}

// PPT-FR-KOCG: a call with `updates` is all or nothing. A refused update after
// a good one writes nothing, appends no comment, and does not end the turn.
#[test]
fn an_update_call_is_all_or_nothing() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    f.proposed.store(false, Ordering::SeqCst);
    let before = f.proposals();
    let comments = f.plan.comments();
    refused(f.updates(json!([withdraw("a"), withdraw("z")])));
    assert_eq!(f.proposals(), before);
    assert_eq!(f.plan.comments(), comments);
    assert!(!f.proposed.load(Ordering::SeqCst));
}

// PPT-FR-NXPZ, PPT-FR-BSSO: an update keeps the rules of the rationale and of
// a resolved conversation.
#[test]
fn an_update_keeps_the_rationale_and_resolution_rules() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let silent = refused(f.call(json!({ "rationale": "No tag.", "updates": [withdraw("a")] })));
    assert_eq!(silent.code, "no_spoken_part");
    crate::comments::set_resolution_to(
        &f.plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &f.plan.discussion_id,
        true,
        &human(),
        "2026-03-02T10:00:00Z",
    )
    .unwrap();
    let resolved = refused(f.updates(json!([withdraw("a")])));
    assert_eq!(resolved.message, CONVERSATION_RESOLVED);
    assert_eq!(f.keys(), ["a", "b"]);
}

// PPT-FR-DGAA: an update logs one INFO record with the count of updates of each
// action, and no content.
#[test]
fn an_update_is_logged_with_counts() {
    static BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let args = json!({
        "rationale": "ZEBRA-RATIONALE <spoken>x</spoken>",
        "updates": [
            withdraw("a"),
            add("c", json!({ "op": "new_draft", "name": "ZEBRA-NAME", "body": "ZEBRA-PROMPT" })),
        ],
    });
    run(&f.tool().with_buffer(&BUFFER), args).unwrap();
    let ai = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records;
    let info: Vec<_> = ai.iter().filter(|r| r.level == LogLevel::Info).collect();
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].fields["updates"], 2);
    assert_eq!(info[0].fields["withdrawUpdates"], 1);
    assert_eq!(info[0].fields["addUpdates"], 1);
    assert_eq!(info[0].fields["changeUpdates"], 0);
    assert_eq!(info[0].fields["draftRows"], 1);
    assert!(!serde_json::to_string(&ai).unwrap().contains("ZEBRA"));
}

// PPT-FR-PMQL: a call gives 1 to 20 updates.
#[test]
fn a_call_gives_one_to_twenty_updates() {
    let f = Fixture::new();
    f.rows(json!([draft("a")])).unwrap();
    assert_eq!(refused(f.updates(json!([]))).code, "bad_rows");
    let many: Vec<Value> = (0..21).map(|n| add(&format!("n{n}"), draft("x"))).collect();
    assert_eq!(refused(f.updates(json!(many))).code, "bad_rows");
    assert_eq!(f.keys(), ["a"]);
}

// PPT-FR-JDUV: a failed row is settled too, and the refusal names its place.
#[test]
fn a_failed_row_is_settled() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let mut stored = f.proposals().remove(0);
    stored.rows[0].state = RowState::Failed;
    let dir = crate::comments::map_proposals_dir(&f.plan.store()).unwrap();
    f.plan
        .store()
        .write_text_atomic(
            dir.join(format!("{}.json", stored.id)),
            &serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();
    let settled = refused(f.updates(json!([withdraw("b"), change("a", draft("a"))])));
    assert_eq!(settled.code, "row_settled");
    assert!(settled.message.starts_with("Update 2 of 2: "), "{}", settled.message);
    assert_eq!(f.keys(), ["a", "b"]);
}
