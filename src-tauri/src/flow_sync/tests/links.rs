//! FTS-FR-TPWE, FTS-FR-OBPX, FTS-FR-LMBE, FTS-FR-ALOJ, FTS-FR-TUEJ,
//! FTS-FR-LVTU: the commands that are not a sync, and the result shapes.

use super::*;
use crate::flow_sync::links::{is_tracker_url, snapshot, validated_settings};
use crate::flow_sync::values::type_for;
use crate::flow_sync::BusyClaim;
use crate::flow_validation::model::{Kind, RefKind};

/// FTS-FR-TPWE: an issue's reference and its values on the terms of
/// FTS-FR-PNMA, in the shape of a link's base.
#[test]
fn read_tracker_item_returns_an_issue_as_a_base() {
    let (tracker, d) = plan();
    let mut nine = child(9, "Card form", "Task", Some(2), Some(5));
    nine.body = "Use the SDK.".into();
    nine.blocked_by = vec![key(3)];
    let tracker = tracker.with_issue(nine);
    let snap = snapshot(&tracker, &d, RefKind::Issue, 9).unwrap();
    assert_eq!((snap.reference.number, snap.reference.url.as_str()), (9, url_of(9).as_str()));
    let v = serde_json::to_value(&snap).unwrap();
    assert_eq!(v["ref"]["kind"], json!("issue"));
    assert_eq!(v["ref"]["owner"], json!(OWNER));
    assert_eq!(v["name"], json!("Card form"));
    assert_eq!(v["description"], json!("Use the SDK."));
    assert_eq!(v["state"], json!("open"));
    assert_eq!(v["type"], json!("Task"));
    assert_eq!(v["parent"]["number"], json!(2));
    assert_eq!(v["milestone"], json!(5));
    assert_eq!(v["blockedBy"][0]["number"], json!(3));
    assert!(v.get("due").is_none());
}

/// FTS-FR-TPWE: a milestone carries its due date and no issue field.
#[test]
fn read_tracker_item_returns_a_milestone_with_its_due_date() {
    let (tracker, d) = plan();
    let mut ga = milestone(7, "GA");
    ga.due = Some("2027-09-30".into());
    let tracker = tracker.with_milestone(ga);
    let v = serde_json::to_value(snapshot(&tracker, &d, RefKind::Milestone, 7).unwrap()).unwrap();
    assert_eq!(v["ref"]["kind"], json!("milestone"));
    assert_eq!(v["ref"]["url"], json!(milestone_url(7)));
    assert_eq!(v["due"], json!("2027-09-30"));
    assert!(v.get("type").is_none() && v.get("blockedBy").is_none());
}

/// FTS-FR-TPWE / FTS-FR-VXMN: no such item, a pull request, and an issue
/// transferred to another repository are `tracker_item_not_found`.
#[test]
fn read_tracker_item_refuses_a_missing_item_and_a_pull_request() {
    let (tracker, d) = plan();
    let tracker = tracker.with_issue(issue(9, "A pull request", None));
    tracker.state.borrow_mut().pull_requests.push(9);
    let mut moved = issue(10, "Moved away", Some("Task"));
    moved.repo = "elsewhere".into();
    let tracker = tracker.with_issue(moved);
    for (kind, number) in [
        (RefKind::Issue, 404),
        (RefKind::Issue, 9),
        (RefKind::Issue, 10),
        (RefKind::Milestone, 404),
    ] {
        let error = snapshot(&tracker, &d, kind, number).unwrap_err();
        assert_eq!(error.code, ERR_TRACKER_ITEM_NOT_FOUND, "{kind:?} {number}");
    }
}

/// FTS-FR-TPWE: an item a link of the Flow names is `already_linked`, and no
/// tracker request is made.
#[test]
fn read_tracker_item_refuses_an_item_already_linked() {
    let (tracker, d) = plan();
    assert_eq!(snapshot(&tracker, &d, RefKind::Issue, 3).unwrap_err().code, ERR_ALREADY_LINKED);
    assert_eq!(snapshot(&tracker, &d, RefKind::Milestone, 5).unwrap_err().code, ERR_ALREADY_LINKED);
    assert!(tracker.calls().is_empty());
    // An issue with the number of a linked milestone is not linked.
    let tracker = tracker.with_issue(issue(5, "Five", Some("Task")));
    assert!(snapshot(&tracker, &d, RefKind::Issue, 5).is_ok());
}

/// FTS-FR-OBPX: the HTTPS address of an issue or a milestone of the
/// publication repository is opened.
#[test]
fn an_issue_or_milestone_address_of_the_repository_is_allowed() {
    for url in [
        "https://github.com/acme/payments/issues/42",
        "https://github.com/acme/payments/milestone/3",
        "https://GitHub.com/Acme/Payments/issues/1",
    ] {
        assert!(is_tracker_url(url, HOST, OWNER, REPO), "{url}");
    }
}

/// FTS-FR-OBPX: every other address is `tracker_url_refused`.
#[test]
fn every_other_address_is_refused() {
    for url in [
        "http://github.com/acme/payments/issues/42",
        "https://github.com/acme/other/issues/42",
        "https://github.com/evil/payments/issues/42",
        "https://evil.com/acme/payments/issues/42",
        "https://github.com:8443/acme/payments/issues/42",
        "https://user@github.com/acme/payments/issues/42",
        "https://github.com/acme/payments/issues/42?x=1",
        "https://github.com/acme/payments/issues/42#top",
        "https://github.com/acme/payments/issues/42/",
        "https://github.com/acme/payments/issues/",
        "https://github.com/acme/payments/issues/4a",
        "https://github.com/acme/payments/issues/042",
        "https://github.com/acme/payments/pull/42",
        "https://github.com/acme/payments/milestones",
        "https://github.com/acme/payments/issues/42 ",
        "javascript:alert(1)",
        "",
    ] {
        assert!(!is_tracker_url(url, HOST, OWNER, REPO), "{url:?}");
    }
}

/// FTS-FR-LMBE: the Type of each issue kind, defaults applied, and none for a
/// milestone.
#[test]
fn each_issue_kind_has_its_type() {
    let s = settings();
    assert_eq!(type_for(&s, Kind::Epic), Some("Epic"));
    assert_eq!(type_for(&s, Kind::Feature), Some("Feature"));
    assert_eq!(type_for(&s, Kind::Task), Some("Task"));
    assert_eq!(type_for(&s, Kind::Defect), Some("Bug"));
    assert_eq!(type_for(&s, Kind::Milestone), None);
}

/// FTS-FR-ALOJ: the four Types are trimmed and returned; an empty one is
/// `invalid_flow_tracker_settings`.
#[test]
fn settings_are_trimmed_and_an_empty_type_is_refused() {
    let s = validated_settings(" Initiative ", "Feature", "Task", "Bug").unwrap();
    assert_eq!(s.epic_type, "Initiative");
    for blank in 0..4 {
        let mut names = ["A", "B", "C", "D"];
        names[blank] = "  ";
        let error = validated_settings(names[0], names[1], names[2], names[3]).unwrap_err();
        assert_eq!(error.code, ERR_INVALID_SETTINGS);
        assert!(!error.message.is_empty());
    }
}

/// FTS-FR-LMBE / FTS-FR-ALOJ: the settings go to the frontend as the four
/// camelCase Types, and no other key.
#[test]
fn the_settings_serialize_as_four_types() {
    let s = validated_settings("Epic", "Feature", "Task", "Bug").unwrap();
    assert_eq!(s, settings());
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        json!({ "epicType": "Epic", "featureType": "Feature", "taskType": "Task", "defectType": "Bug" })
    );
}

/// FTS-FR-TUEJ: a second claim of one Flow is refused while the first is
/// held, and a claim is released when it ends.
#[test]
fn one_sync_runs_for_one_flow_at_a_time() {
    let key = "test-root\u{0}plans/busy.flow".to_string();
    let first = BusyClaim::take(key.clone()).expect("first claim");
    assert!(BusyClaim::take(key.clone()).is_none());
    let other = BusyClaim::take("test-root\u{0}plans/other.flow".to_string());
    assert!(other.is_some(), "another Flow is not blocked");
    drop(first);
    assert!(BusyClaim::take(key).is_some());
}

/// FTS-FR-LVTU: the result carries the counts, the conflicts, the skipped
/// issues, and the stale ids, with camelCase names.
#[test]
fn the_sync_result_serializes_in_the_contract_shape() {
    let result = SyncResult {
        created: 1,
        updated: 2,
        added: 3,
        changed: 4,
        conflicts: vec![],
        skipped: vec![
            SkippedIssue {
                number: Some(58),
                title: "Load test plan".into(),
                url: Some(url_of(58)),
                reason: SKIP_NO_KIND_FITS,
                item_id: None,
            },
            SkippedIssue {
                number: None,
                title: "Card form".into(),
                url: None,
                reason: SKIP_TYPE_UNAVAILABLE,
                item_id: Some("t9".into()),
            },
        ],
        stale: vec!["fsc_1".into()],
    };
    let v = serde_json::to_value(&result).unwrap();
    assert_eq!(
        v,
        json!({
            "created": 1, "updated": 2, "added": 3, "changed": 4,
            "conflicts": [],
            "skipped": [
                { "number": 58, "title": "Load test plan", "url": url_of(58), "reason": "no_kind_fits" },
                {
                    "number": null, "title": "Card form", "url": null,
                    "reason": "issue_type_unavailable", "itemId": "t9"
                },
            ],
            "stale": ["fsc_1"],
        })
    );
}

/// FTS contract: each error carries displayable text, and the refusals of
/// the repository and token resolutions keep their codes.
#[test]
fn each_error_code_carries_its_text() {
    for code in [
        ERR_NO_PROJECT,
        ERR_FLOW_NOT_FOUND,
        ERR_FLOW_INVALID,
        ERR_SYNC_NOT_ENABLED,
        ERR_SYNC_IN_PROGRESS,
        ERR_TRACKER_ITEM_NOT_FOUND,
        ERR_ALREADY_LINKED,
        ERR_TRACKER_URL_REFUSED,
        ERR_INVALID_SETTINGS,
        ERR_GITHUB_UNREACHABLE,
        ERR_WRITE_FAILED,
        crate::github_publication::ERR_NO_GITHUB_REMOTE,
        crate::github_tokens::ERR_TOKEN_MISSING,
        crate::github_tokens::ERR_HOST_MISMATCH,
    ] {
        let error = FlowSyncError::of(code);
        assert_eq!(error.code, code);
        assert!(!error.message.is_empty(), "{code}");
        let v = serde_json::to_value(&error).unwrap();
        assert!(v.get("createdUrls").is_none() && v.get("report").is_none());
    }
    assert_eq!(
        FlowSyncError::of(crate::github_publication::ERR_NO_GITHUB_REMOTE).message,
        "This project has no GitHub remote."
    );
}

/// FTS-FR-FDDV: each tracker request is logged with its operation and its
/// outcome, and no record carries a title or a description.
#[test]
fn tracker_requests_are_logged_without_titles_or_descriptions() {
    #[derive(Clone)]
    struct Silent;
    impl crate::logging::LogSink for Silent {
        fn publish(&self, _state: &crate::logging::BufferState) {}
    }
    let (tracker, mut d) = plan();
    let mut t9 = item("t9", "task", "SECRET_TITLE_7f3a", Some("f1"));
    t9["description"] = json!("SECRET_BODY_7f3a");
    d.items.push(serde_json::from_value(t9).unwrap());
    let logged = crate::flow_sync::logged::LoggedAdapter::new(&Silent, &tracker);
    let settings = settings();
    run(&logged, SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] })
        .unwrap();
    let query = |text: &str| {
        let filter = crate::logging::LogFilter {
            min_level: crate::logging::LogLevel::Debug,
            query: Some(text.to_string()),
            ..Default::default()
        };
        crate::logging::BUFFER.query(&filter, None, 1000).unwrap().records
    };
    assert!(query("SECRET_").is_empty());
    let created = query("create_issue");
    assert!(created.iter().any(|r| r.fields.get("outcome") == Some(&json!("ok"))));
}
