//! PPT-FR-DDSX, PPT-FR-VFOX: the tool in a discussion that is not a Map thread.

use super::*;

// PPT-FR-DDSX, PPT-FR-VFOX, PPT-FR-CXFG, PPT-FR-STLY: a turn of a passage
// thread of a file records a proposal, and its comment goes to that thread.
// A later turn there updates the same proposal, and the Map thread of the
// fixture gets nothing.
#[test]
fn a_passage_thread_turn_records_and_updates_its_own_proposal() {
    let f = Fixture::new();
    let passage = f.plan.passage_thread();
    let map_before = f.plan.comments();
    let recorded =
        run(&f.tool_for(passage.clone()), json!({ "rationale": RATIONALE, "rows": [draft("a")] }))
            .expect("a proposal");
    assert_eq!(recorded, json!({ "proposed": true, "rows": 1 }));
    let proposals = crate::map_plan::proposals_in(&f.plan.store(), passage.discussion_id());
    assert_eq!(proposals.len(), 1);
    let comments = f.plan.comments_of(&passage);
    assert_eq!(comments.len(), 2, "the opening and the proposal");
    assert!(matches!(
        comments[1].attachments.as_slice(),
        [Attachment::MapPlanProposal { proposal_id, update: None }] if *proposal_id == proposals[0].id
    ));
    let updated = run(
        &f.tool_for(passage.clone()),
        json!({
            "rationale": RATIONALE,
            "updates": [{ "action": "add", "key": "b", "row": draft("b") }],
        }),
    )
    .expect("an update");
    assert_eq!(updated["added"], 1);
    assert_eq!(f.plan.comments_of(&passage).len(), 3);
    assert_eq!(f.plan.comments(), map_before, "the Map thread gets nothing");
    assert!(f.proposals().is_empty(), "the Map thread has no proposal");
}
