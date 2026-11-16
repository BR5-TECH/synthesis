//! MPR-FR-QUYY, MPR-FR-MQEQ, MPR-FR-SKLA, MPR-FR-HKYH: the decision on one row.

use super::*;

/// MPR-FR-SKLA, MPR-FR-MQEQ: rejecting a row sets its state, writes nothing to
/// GitHub, creates no draft, makes no request, and leaves every other row as it
/// was.
#[test]
fn rejecting_a_row_touches_nothing_else() {
    let plan = setup();
    let proposal = record(
        &plan,
        vec![
            create_row("a", new_issue("One")),
            modify_row("b", 2, vec![change("title", json!("T"))]),
            draft_row("c", "Three", "z"),
        ],
    );
    let decided = reject(&plan, &proposal, "b").unwrap();
    assert_eq!(state_of(&decided, "a"), RowState::Waiting);
    assert_eq!(state_of(&decided, "b"), RowState::Rejected);
    assert_eq!(state_of(&decided, "c"), RowState::Waiting);
    assert!(plan.h.fake.calls().is_empty(), "a reject makes no request");
    assert_eq!(listed(&plan)[0], decided);
    let drafts = plan.h.root().list_dir(plan.h.root().path().join(".synthesis/drafts"));
    assert!(drafts.map(|d| d.is_empty()).unwrap_or(true), "a reject creates no draft");
}

/// MPR-FR-SKLA: a reject needs no GitHub source and no configuration.
#[test]
fn a_reject_needs_no_github_source() {
    let plan = setup();
    plan.h.save_delivery_settings(&crate::delivery::settings::DeliverySettings::default());
    plan.h.select_project(None);
    let proposal = record(&plan, vec![create_row("a", new_issue("One"))]);
    let decided = reject(&plan, &proposal, "a").unwrap();
    assert_eq!(state_of(&decided, "a"), RowState::Rejected);
}

/// MPR-FR-QUYY: a decided row refuses every later decision with `row_decided`,
/// and nothing changes.
#[test]
fn a_decided_row_is_final() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), draft_row("b", "Two", "y")]);
    let accepted = accept(&plan, &proposal, "a").unwrap();
    reject(&plan, &proposal, "b").unwrap();
    for _ in 0..2 {
        assert_eq!(accept(&plan, &proposal, "a").unwrap_err().code, "row_decided");
        assert_eq!(reject(&plan, &proposal, "a").unwrap_err().code, "row_decided");
        assert_eq!(accept(&plan, &proposal, "b").unwrap_err().code, "row_decided");
        assert_eq!(reject(&plan, &proposal, "b").unwrap_err().code, "row_decided");
    }
    let now = &listed(&plan)[0];
    assert_eq!(row_of(now, "a"), row_of(&accepted, "a"));
    assert_eq!(state_of(now, "b"), RowState::Rejected);
}

/// MPR-FR-QUYY: a proposal, a row, or a discussion that does not match is
/// refused with its typed error.
#[test]
fn unknown_targets_are_refused() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x")]);
    let app = plan.app();
    assert_eq!(
        accept_row(&app, &plan.discussion, "nope", "a").unwrap_err().code,
        "proposal_not_found"
    );
    assert_eq!(
        accept_row(&app, &plan.discussion, "../x", "a").unwrap_err().code,
        "proposal_not_found"
    );
    assert_eq!(
        accept_row(&app, "other", &proposal.id, "a").unwrap_err().code,
        "proposal_not_found"
    );
    assert_eq!(
        reject_row(&app, &plan.discussion, &proposal.id, "z").unwrap_err().code,
        "row_not_found"
    );
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Waiting);
}

/// MPR-FR-HKYH: decisions on one row run one at a time. Two that arrive
/// together make one decision, and the second finds the row decided.
#[test]
fn two_decisions_on_one_row_make_one() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x")]);
    let outcomes: Vec<Result<MapPlanProposal, MapPlanError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|n| {
                let app = plan.app();
                let (discussion, id) = (plan.discussion.clone(), proposal.id.clone());
                scope.spawn(move || {
                    if n % 2 == 0 {
                        accept_row(&app, &discussion, &id, "a")
                    } else {
                        reject_row(&app, &discussion, &id, "a")
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(outcomes.iter().filter(|o| o.is_ok()).count(), 1);
    for refused in outcomes.iter().filter_map(|o| o.as_ref().err()) {
        assert_eq!(refused.code, "row_decided");
    }
    // One decision won. An accept made one draft, and a reject made none.
    let made = crate::drafts::list_drafts_impl(&plan.h.root()).drafts.len();
    let state = state_of(&listed(&plan)[0], "a");
    assert_eq!(made, usize::from(state == RowState::Accepted));
}
