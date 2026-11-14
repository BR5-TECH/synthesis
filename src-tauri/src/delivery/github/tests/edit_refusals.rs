//! The tests of the refusals and the less common paths of the edits of a
//! GitHub delivery plan (`GDP-github-delivery-plan.md` GDP-FR-GBDF to
//! GDP-FR-MSVS), against a fake tracker adapter in a mock application.

use super::super::edits::{apply_edit_impl, apply_for_target, GithubDeliveryEdit as E, RemoveMode};
use super::super::*;
use super::fake::*;
use crate::flow_validation::model::Kind;

fn edit(h: &Harness, e: E) -> Result<GithubDeliveryPlan, DeliveryError> {
    apply_edit_impl(&h.handle(), e)
}

fn loaded() -> (Harness, GithubDeliveryPlan) {
    let h = Harness::github_plan();
    let plan = load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    (h, plan)
}

fn stamp(plan: &GithubDeliveryPlan, id: &str) -> String {
    plan.tracker.iter().find(|t| t.item_id == id).unwrap().updated_at.clone()
}

fn facts(plan: &GithubDeliveryPlan, id: &str) -> (String, Option<&'static str>) {
    let f = plan.tracker.iter().find(|t| t.item_id == id).unwrap();
    (f.state.to_string(), f.close_reason)
}

/// GDP-FR-GBDF, GDP-FR-WAKG: `remove_item` changes the item and each plan
/// issue under it. Where a member changed on GitHub since the plan was read,
/// the edit is `changed_on_tracker`, nothing is written, and the cache takes
/// the values read.
#[test]
fn a_changed_member_refuses_the_removal() {
    for mode in [RemoveMode::CloseNotPlanned, RemoveMode::RemoveFromBoard] {
        let (h, plan) = loaded();
        {
            let mut s = h.fake.lock();
            let member = s.issues.iter_mut().find(|i| i.number == 3).unwrap();
            member.title = "Changed on GitHub".into();
            member.updated_at = "2026-09-09T09:09:09Z".into();
        }
        let refused = edit(
            &h,
            E::RemoveItem { item_id: "issue-1".into(), mode, updated_at: stamp(&plan, "issue-1") },
        )
        .unwrap_err();
        assert_eq!(refused.code, ERR_CHANGED_ON_TRACKER, "{mode:?}");
        assert!(h.fake.writes().is_empty(), "nothing is written: {:?}", h.fake.writes());
        let cache = h.cache().unwrap();
        assert_eq!(cache.issue(3).unwrap().title, "Changed on GitHub", "{mode:?}");
    }
    // With the plan of the values read, the removal goes through.
    let (h, plan) = loaded();
    h.fake.lock().issues.iter_mut().find(|i| i.number == 3).unwrap().updated_at =
        "2026-09-09T09:09:09Z".into();
    let refused = edit(
        &h,
        E::RemoveItem {
            item_id: "issue-1".into(),
            mode: RemoveMode::CloseNotPlanned,
            updated_at: stamp(&plan, "issue-1"),
        },
    );
    assert_eq!(refused.unwrap_err().code, ERR_CHANGED_ON_TRACKER);
    let fresh = load_plan_impl(&h.handle(), false).unwrap();
    edit(
        &h,
        E::RemoveItem {
            item_id: "issue-1".into(),
            mode: RemoveMode::CloseNotPlanned,
            updated_at: stamp(&fresh, "issue-1"),
        },
    )
    .unwrap();
    assert_eq!(
        h.fake.writes(),
        ["close_issue 3 NotPlanned", "close_issue 2 NotPlanned", "close_issue 1 NotPlanned"]
    );
}

/// GDP-FR-WAKG: `close_not_planned` of an issue item closes it and each plan
/// issue under it as not planned, members first. `remove_from_board` of a
/// milestone item removes its issues from the Project and closes the
/// milestone.
#[test]
fn removal_modes_of_an_issue_and_of_a_milestone() {
    let (h, plan) = loaded();
    let p = edit(
        &h,
        E::RemoveItem {
            item_id: "issue-2".into(),
            mode: RemoveMode::CloseNotPlanned,
            updated_at: stamp(&plan, "issue-2"),
        },
    )
    .unwrap();
    assert_eq!(h.fake.writes(), ["close_issue 3 NotPlanned", "close_issue 2 NotPlanned"]);
    assert_eq!(facts(&p, "issue-3"), ("closed".into(), Some("not_planned")));
    assert_eq!(facts(&p, "issue-2"), ("closed".into(), Some("not_planned")));
    assert_eq!(facts(&p, "issue-1"), ("open".into(), None), "the parent stays open");

    let (h, _) = loaded();
    let mut in_milestone = issue(5, Some("Task"));
    in_milestone.milestone = Some(1);
    h.fake.board_issue(in_milestone, None);
    let plan = load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    let p = edit(
        &h,
        E::RemoveItem {
            item_id: "milestone-1".into(),
            mode: RemoveMode::RemoveFromBoard,
            updated_at: stamp(&plan, "milestone-1"),
        },
    )
    .unwrap();
    assert_eq!(
        h.fake.writes(),
        [format!("remove_project_item {PROJECT} ITEM_5"), "close_milestone 1".to_string()]
    );
    assert!(!p.document.items.iter().any(|i| i.id == "issue-5"), "off the board");
    assert_eq!(facts(&p, "milestone-1").0, "closed");
    assert_eq!(h.fake.lock().issues.iter().find(|i| i.number == 5).unwrap().state, "open");
}

/// GDP-FR-HULQ: a pull request is `tracker_item_not_found`, and nothing is
/// written.
#[test]
fn a_pull_request_is_not_an_existing_issue() {
    let (h, _) = loaded();
    {
        let mut s = h.fake.lock();
        s.issues.push(issue(60, Some("Task")));
        s.pull_requests.push(60);
    }
    let app = h.handle();
    let target = delivery_target(&app).unwrap();
    let refused =
        apply_for_target(&app, &target, &E::AddExistingIssue { number: 60, container_id: None })
            .unwrap_err();
    assert_eq!(refused.code, ERR_TRACKER_ITEM_NOT_FOUND);
    assert_eq!(h.fake.calls(), ["read_issue 60"]);
}

/// GDP-FR-EJTS, GDP-FR-NJJZ: a refused `set_kind` or `move` writes nothing.
#[test]
fn refused_kind_changes_and_moves_write_nothing() {
    let (h, plan) = loaded();
    let refusals = [
        E::SetKind {
            item_id: "issue-3".into(),
            item_kind: Kind::Milestone,
            updated_at: stamp(&plan, "issue-3"),
        },
        E::SetKind {
            item_id: "milestone-1".into(),
            item_kind: Kind::Task,
            updated_at: stamp(&plan, "milestone-1"),
        },
        E::Move {
            item_id: "milestone-1".into(),
            container_id: None,
            updated_at: stamp(&plan, "milestone-1"),
        },
        // An item cannot sit in itself or in one of its members.
        E::Move {
            item_id: "issue-1".into(),
            container_id: Some("issue-1".into()),
            updated_at: stamp(&plan, "issue-1"),
        },
        E::Move {
            item_id: "issue-1".into(),
            container_id: Some("issue-3".into()),
            updated_at: stamp(&plan, "issue-1"),
        },
    ];
    for refusal in refusals {
        let name = refusal.name();
        assert_eq!(edit(&h, refusal).unwrap_err().code, ERR_KIND_CHANGE_UNSUPPORTED, "{name}");
    }
    let unknown = E::Move {
        item_id: "issue-3".into(),
        container_id: Some("issue-77".into()),
        updated_at: stamp(&plan, "issue-3"),
    };
    assert_eq!(edit(&h, unknown).unwrap_err().code, ERR_TRACKER_ITEM_NOT_FOUND);
    let changed = E::SetKind {
        item_id: "issue-3".into(),
        item_kind: Kind::Defect,
        updated_at: "not the stamp".into(),
    };
    assert_eq!(edit(&h, changed).unwrap_err().code, ERR_CHANGED_ON_TRACKER);
    assert!(h.fake.writes().is_empty(), "a refusal writes nothing: {:?}", h.fake.writes());
}

/// GDP-FR-MSVS: a `partial_edit` names the error that stopped it, and a
/// partial create names the issue it made.
#[test]
fn a_partial_edit_names_its_cause() {
    let (h, plan) = loaded();
    h.fake.lock().writes_before_failure = Some(1);
    let refused = edit(
        &h,
        E::Move {
            item_id: "issue-3".into(),
            container_id: Some("issue-1".into()),
            updated_at: stamp(&plan, "issue-3"),
        },
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_PARTIAL_EDIT);
    let cause = crate::github_polling::records::error_text("github_request_failed");
    assert!(refused.message.contains(&cause), "{}", refused.message);
    assert!(refused.message.contains("after 1 of its writes"), "{}", refused.message);

    let (h, _) = loaded();
    h.fake.lock().writes_before_failure = Some(1);
    let app = h.handle();
    let target = delivery_target(&app).unwrap();
    let create = E::CreateItem {
        item_kind: Kind::Task,
        name: "T".into(),
        container_id: Some("issue-2".into()),
        description: None,
        due: None,
    };
    let refused = apply_for_target(&app, &target, &create).unwrap_err();
    assert_eq!(refused.code, ERR_PARTIAL_EDIT);
    assert!(refused.message.contains(&cause));
    assert_eq!(
        refused.created_urls.as_deref(),
        Some(&["https://github.com/acme/widgets/issues/101".to_string()][..])
    );
}

/// GDP-FR-YBHD, GDP-FR-XJDE, DMS-FR-WAUS: `set_milestone_order` comes in the
/// frontend shape, writes the milestone order of the overlay, makes no GitHub
/// request, and the answer and a later cached load follow it. An order that
/// names an id twice is refused with `invalid_overlay`.
#[test]
fn set_milestone_order_writes_the_overlay_and_makes_no_request() {
    let (h, _) = loaded();
    for (n, due) in [(2, None), (3, Some("2026-10-01"))] {
        h.fake.lock().milestones.push(milestone(n, due, "open"));
    }
    load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    let milestones = |p: &GithubDeliveryPlan| -> Vec<String> {
        p.document
            .items
            .iter()
            .filter(|i| i.kind == Kind::Milestone)
            .map(|i| i.id.clone())
            .collect()
    };
    let e: E = serde_json::from_value(serde_json::json!({
        "kind": "set_milestone_order",
        "milestoneIds": ["milestone-2", "milestone-1"]
    }))
    .expect("the frontend shape");
    assert_eq!(e.name(), "set_milestone_order");
    assert!(e.is_overlay());
    let p = edit(&h, e).unwrap();
    assert_eq!(milestones(&p), ["milestone-2", "milestone-1", "milestone-3"]);
    assert!(h.fake.calls().is_empty(), "no request: {:?}", h.fake.calls());
    let overlay = crate::delivery::machine_state::read_delivery_overlay(
        &h.handle(),
        &crate::logging::BUFFER,
        &h.machine_store(),
        PROJECT,
    )
    .unwrap();
    assert_eq!(overlay.milestone_order, ["milestone-2", "milestone-1"]);
    assert_eq!(milestones(&load_plan_impl(&h.handle(), false).unwrap()), milestones(&p));

    let twice =
        E::SetMilestoneOrder { milestone_ids: vec!["milestone-1".into(), "milestone-1".into()] };
    let refused = edit(&h, twice).unwrap_err();
    assert_eq!(refused.code, crate::delivery::ERR_INVALID_OVERLAY);
    assert_eq!(milestones(&load_plan_impl(&h.handle(), false).unwrap()), milestones(&p));
}
