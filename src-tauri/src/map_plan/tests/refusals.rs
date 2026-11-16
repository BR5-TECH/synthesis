//! MPR-FR-VMXC, MPR-FR-RVTT, MPR-FR-KZRN: refusals and logs.

use super::*;
use crate::delivery::settings::{DeliverySettings, DeliverySourceChoice};

/// MPR-FR-VMXC: an issue row is refused with `not_github_source` where the
/// delivery source is not a GitHub Project. The row stays `waiting`, and no
/// request is made. A draft row is not refused.
#[test]
fn another_source_refuses_an_issue_row() {
    let plan = setup();
    plan.h.write("plans/a.flow", "");
    plan.h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: "plans/a.flow".into() },
        ..DeliverySettings::default()
    });
    let proposal = record(
        &plan,
        vec![
            create_row("a", new_issue("One")),
            modify_row("b", 2, vec![change("title", json!("T"))]),
            draft_row("c", "Three", "z"),
        ],
    );
    for key in ["a", "b"] {
        assert_eq!(accept(&plan, &proposal, key).unwrap_err().code, "not_github_source");
    }
    assert!(plan.h.fake.calls().is_empty());
    let now = &listed(&plan)[0];
    assert_eq!(state_of(now, "a"), RowState::Waiting);
    assert_eq!(state_of(now, "b"), RowState::Waiting);
    assert_eq!(state_of(&accept(&plan, &proposal, "c").unwrap(), "c"), RowState::Accepted);
}

/// MPR-FR-VMXC: with no Project selected the source resolves to none (per
/// DSR-FR-AECK), so an issue row is refused with `not_github_source`, and the
/// row stays `waiting`.
#[test]
fn an_unselected_project_refuses_an_issue_row() {
    let plan = setup();
    plan.h.select_project(None);
    let proposal = record(&plan, vec![create_row("a", new_issue("One"))]);
    let refused = accept(&plan, &proposal, "a").unwrap_err();
    assert_eq!(refused.code, "not_github_source");
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Waiting);
}

/// MPR-FR-VMXC: a configuration that is not valid refuses an issue row with its
/// error, and the row stays `waiting`.
#[test]
fn an_invalid_configuration_refuses_an_issue_row() {
    let plan = setup();
    plan.h.fake.lock().status_field = None;
    let _ = crate::delivery::github::load_plan_impl(&plan.app(), true);
    let proposal = record(&plan, vec![create_row("a", new_issue("One"))]);
    let refused = accept(&plan, &proposal, "a").unwrap_err();
    assert_eq!(refused.code, "status_field_missing");
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Waiting);
    assert!(plan.h.fake.writes().is_empty());
}

/// MPR-FR-RVTT: with no project open, each operation refuses with
/// `no_project_open`.
#[test]
fn no_project_is_refused() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x")]);
    plan.h.close();
    let app = plan.app();
    assert_eq!(list_proposals(&app, &plan.discussion).unwrap_err().code, "no_project_open");
    assert_eq!(
        accept_row(&app, &plan.discussion, &proposal.id, "a").unwrap_err().code,
        "no_project_open"
    );
    assert_eq!(
        reject_row(&app, &plan.discussion, &proposal.id, "a").unwrap_err().code,
        "no_project_open"
    );
    let recorded = record_map_plan_proposal(
        &app,
        &plan.discussion,
        vec![draft_row("b", "Two", "y")],
        comment(),
    );
    assert_eq!(recorded.unwrap_err().code, "no_project_open");
    assert!(!open_row_exists(&app, &plan.discussion));
}

fn log_text() -> String {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    let page = crate::logging::BUFFER.query(&filter, None, 1000).unwrap();
    serde_json::to_string(&page.records).unwrap()
}

/// MPR-FR-KZRN: each recording, decision, and refusal logs its operation, its
/// outcome, the row kind, and the typed error, and no record carries a title, a
/// body, a label name, or a draft name.
#[test]
fn logs_name_the_operation_and_no_content() {
    let plan = setup();
    let mut fields = new_issue("TITLE-SECRET-1");
    fields.body = Some("BODY-SECRET-2".into());
    fields.labels = vec!["LABEL-SECRET-3".into()];
    let proposal = record(
        &plan,
        vec![create_row("a", fields), draft_row("b", "DRAFT-SECRET-4", "BODY-SECRET-5")],
    );
    accept(&plan, &proposal, "a").unwrap();
    accept(&plan, &proposal, "b").unwrap();
    assert_eq!(accept(&plan, &proposal, "b").unwrap_err().code, "row_decided");
    let text = log_text();
    for secret in ["SECRET-1", "SECRET-2", "SECRET-3", "SECRET-4", "SECRET-5", "ghp_"] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(text.contains("map plan operation done"));
    assert!(text.contains("map plan operation refused"));
    assert!(text.contains("label_unknown"));
    assert!(text.contains("row_decided"));
    assert!(text.contains("create_issue") && text.contains("new_draft"));
}
