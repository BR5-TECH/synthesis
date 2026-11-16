//! MPR-FR-XPXP: a `new_draft` row.

use super::*;

/// MPR-FR-XPXP, MPR-FR-VMXC: accepting a `new_draft` row creates one draft with
/// the name and the prompt of the row, holds its id, and makes no request to
/// GitHub. It needs no GitHub source.
#[test]
fn accepting_a_draft_row_creates_the_draft() {
    let plan = setup();
    plan.h.select_project(None);
    plan.h.save_delivery_settings(&crate::delivery::settings::DeliverySettings::default());
    let proposal = record(&plan, vec![draft_row("a", "Finish the thing", "Write the prompt.\n")]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    let row = row_of(&decided, "a");
    assert_eq!(row.state, RowState::Accepted);
    let id = row.result.clone().unwrap().draft_id.unwrap();
    let root = plan.h.root();
    let record = crate::drafts::draft_record(&root, &id).unwrap();
    assert_eq!(record.name, "Finish the thing");
    let file = crate::drafts::prompt_file_name("Finish the thing");
    let contents = crate::drafts::load_draft_file_impl(&root, &id, &file).unwrap();
    assert_eq!(contents.body, "Write the prompt.\n");
    assert!(plan.h.fake.calls().is_empty(), "a draft row makes no request");
    assert_eq!(listed(&plan)[0], decided);
}

/// MPR-FR-XPXP: two draft rows make two drafts, each its own.
#[test]
fn each_draft_row_makes_one_draft() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), draft_row("b", "Two", "y")]);
    let one = accept(&plan, &proposal, "a").unwrap();
    let two = accept(&plan, &proposal, "b").unwrap();
    let id =
        |p: &MapPlanProposal, key: &str| row_of(p, key).result.clone().unwrap().draft_id.unwrap();
    assert_ne!(id(&one, "a"), id(&two, "b"));
}
