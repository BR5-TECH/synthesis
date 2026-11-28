//! PPT-FR-ZMBA, PPT-FR-IRFE, PPT-FR-NXPZ, PPT-FR-DPQP: several calls of one
//! turn post one card, and `parent_row` names the parent of a changed issue.

use super::*;
use crate::comments::ProposalUpdateCounts;

fn later_updates(updates: Value) -> Value {
    json!({ "updates": updates })
}

// PPT-FR-ZMBA: a second call with `rows` in the same turn is refused, and
// nothing is recorded.
#[test]
fn a_turn_posts_one_card() {
    let f = Fixture::new();
    f.in_one_turn();
    f.rows(json!([draft("a")])).unwrap();
    let refusal = refused(f.rows(json!([draft("b")])));
    assert_eq!(refusal.code, "one_card_a_turn");
    assert_eq!(refusal.message, ONE_CARD_A_TURN);
    assert!(refusal.retryable);
    assert_eq!(f.proposals().len(), 1);
}

// PPT-FR-IRFE, PPT-FR-NXPZ: a later call of the turn updates the card with no
// comment and no rationale, and its counts add up on the card of the turn.
#[test]
fn later_calls_update_the_card_silently() {
    let f = Fixture::new();
    f.in_one_turn();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let comments = f.plan.comments();
    let out = f
        .call(later_updates(json!([
            { "action": "add", "key": "c", "row": draft("c") },
            { "action": "withdraw", "key": "b" },
        ])))
        .unwrap();
    assert_eq!(out, json!({ "updated": true, "changed": 0, "added": 1, "withdrawn": 1 }));
    f.call(later_updates(json!([
        { "action": "change", "key": "a", "row": { "op": "new_change", "name": "N", "body": "B" } },
    ])))
    .unwrap();
    assert_eq!(f.plan.comments(), comments, "a later call appends no comment");
    let card = f.card();
    assert_eq!((card.changed, card.added, card.withdrawn), (1, 1, 1));
    assert!(card.changed_after_comment());
    assert_eq!(card.counts(), ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 1 });
    assert_eq!(f.proposals().len(), 1);
    let keys: Vec<String> = f.proposals()[0].rows.iter().map(|r| r.key.clone()).collect();
    assert_eq!(keys, ["a", "c"]);
}

// PPT-FR-NXPZ, PPT-FR-ISZR: the first call of a turn with `updates` needs its
// rationale and posts it as one comment, and it makes the card of the turn.
#[test]
fn a_first_update_of_a_turn_posts_its_rationale() {
    let f = Fixture::new();
    f.rows(json!([draft("a")])).unwrap();
    let proposal = f.proposals().remove(0);
    let comments = f.plan.comments();
    f.in_one_turn();
    let blank = refused(f.call(later_updates(json!([{ "action": "withdraw", "key": "a" }]))));
    assert_eq!(blank.code, "rationale_blank");
    f.call(json!({
        "rationale": RATIONALE,
        "updates": [{ "action": "add", "key": "b", "row": draft("b") }],
    }))
    .unwrap();
    assert_eq!(f.plan.comments(), comments + 1);
    assert_eq!(f.card().proposal_id, Some(proposal.id));
    assert!(!f.card().changed_after_comment());
}

// PPT-FR-ZMBA: where no row of the card waits, a later update is the **no open
// proposal** refusal.
#[test]
fn a_decided_card_takes_no_later_update() {
    let f = Fixture::new();
    f.in_one_turn();
    f.rows(json!([draft("a")])).unwrap();
    f.decide_all();
    let refusal =
        refused(f.call(later_updates(json!([{ "action": "add", "key": "b", "row": draft("b") }]))));
    assert_eq!(refusal.message, crate::tools::propose_plan_changes::updates::NO_OPEN_PROPOSAL);
}

// PPT-FR-DPQP, MPR-FR-YLKL: a `modify_issue` row names an earlier
// `create_issue` row as the new parent of its issue.
#[test]
fn a_changed_issue_can_take_a_new_issue_of_the_call_as_parent() {
    let f = Fixture::new();
    f.rows(json!([create("epic", "New epic"), modify("move", 2, json!({ "parent_row": "epic" }))]))
        .unwrap();
    let row = f.proposals()[0].rows[1].clone();
    let parent = modify_fields(&row).changes.iter().find(|c| c.field == "parent").unwrap().clone();
    assert_eq!(parent.after, json!({ "row": "epic" }));
    assert_eq!(parent.before, json!({ "issue": 1 }));
    let blocked =
        crate::map_plan::read_open_proposal(&f.plan.app(), &f.plan.discussion_id).unwrap();
    assert_eq!(blocked.rows[1].blocked_by.as_ref().map(|b| b.row_key.as_str()), Some("epic"));
    f.decide_all();
    let later = refused(f.rows(json!([modify("move", 2, json!({ "parent_row": "missing" }))])));
    assert_eq!(
        later.message,
        "`parent_row` of row 1 must name the `key` of an earlier `create_issue` row."
    );
}
