//! How GraphQL answers are read (GPP-FR-DEZO, GPP-FR-EHRC, GPP-FR-XSKT,
//! GPP-FR-IFVC, GPP-FR-WYRP).

use super::*;
use serde_json::json;

/// GPP-FR-DEZO / GPP-FR-EHRC: a missing node, a node that is not a Project,
/// and a forbidden Project are `project_unavailable`; another GraphQL error is
/// the request's.
#[test]
fn the_project_shape_is_read_from_the_node_query() {
    let found = json!({ "data": { "node": { "title": "Roadmap", "field": {
        "id": "F1", "options": [{ "id": "o1", "name": "Ready" }, { "id": "o2", "name": "In Progress" }]
    } } } });
    let shape = client::shape_of(&found).unwrap();
    assert_eq!(shape.title, "Roadmap");
    assert_eq!(shape.status_field.unwrap().options.len(), 2);

    // A `Status` field that is not single-select answers with an empty object.
    let text_field = json!({ "data": { "node": { "title": "Roadmap", "field": {} } } });
    assert_eq!(client::shape_of(&text_field).unwrap().status_field, None);
    let no_field = json!({ "data": { "node": { "title": "Roadmap", "field": null } } });
    assert_eq!(client::shape_of(&no_field).unwrap().status_field, None);

    for unavailable in [
        json!({ "data": { "node": null }, "errors": [{ "type": "NOT_FOUND", "message": "x" }] }),
        json!({ "data": { "node": null }, "errors": [{ "type": "FORBIDDEN", "message": "x" }] }),
        json!({ "data": { "node": {} } }),
        json!({ "data": { "node": null } }),
    ] {
        assert_eq!(client::shape_of(&unavailable), Err(ERR_PROJECT_UNAVAILABLE.into()));
    }
    let broken = json!({ "data": null, "errors": [{ "type": "RATE_LIMITED", "message": "x" }] });
    assert_eq!(client::shape_of(&broken), Err(ERR_REQUEST_FAILED.into()));
}

/// GPP-FR-XSKT: one page of items, with the cursor of the next page.
#[test]
fn an_items_page_is_read_with_its_cursor() {
    let page = json!({ "data": { "node": { "items": {
        "pageInfo": { "hasNextPage": true, "endCursor": "C2" },
        "nodes": [
            { "id": "I1", "fieldValueByName": { "name": "Ready", "optionId": "o1" },
              "content": { "number": 4, "title": "Fix", "url": "https://github.com/acme/widgets/issues/4",
                           "state": "OPEN", "issueType": { "name": "Task" },
                           "repository": { "name": "widgets", "owner": { "login": "acme" } } } },
            { "id": "I2", "fieldValueByName": null, "content": {} },
        ]
    } } } });
    let (items, next) = client::items_page_of(&page).unwrap();
    assert_eq!(next.as_deref(), Some("C2"));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].status.as_deref(), Some("Ready"));
    let issue = items[0].issue.as_ref().unwrap();
    assert_eq!((issue.number, issue.issue_type.as_deref()), (4, Some("Task")));
    assert_eq!(items[1].issue, None, "a draft item or a pull request holds no issue");

    let last = json!({ "data": { "node": { "items": {
        "pageInfo": { "hasNextPage": false, "endCursor": "C3" }, "nodes": [] } } } });
    assert_eq!(client::items_page_of(&last).unwrap().1, None);
}

/// GPP-FR-IFVC: a re-fetched issue carries its body and its Project items; an
/// absent issue is `None`.
#[test]
fn a_refetched_issue_is_read_with_its_project_items() {
    let found = json!({ "data": { "repository": { "name": "widgets", "owner": { "login": "acme" },
        "issue": { "number": 4, "title": "Fix", "body": "Do it", "url": "https://github.com/acme/widgets/issues/4",
                   "state": "OPEN", "issueType": { "name": "Task" },
                   "projectItems": { "nodes": [
                       { "id": "I1", "project": { "id": "P1" }, "fieldValueByName": { "name": "Ready" } }
                   ] } } } } });
    let issue = client::fetched_issue_of(&found).unwrap().unwrap();
    assert_eq!(issue.body, "Do it");
    assert_eq!(issue.repository_owner, "acme");
    assert_eq!(issue.project_items[0].project_id, "P1");
    assert_eq!(issue.project_items[0].status.as_deref(), Some("Ready"));

    let absent = json!({ "data": { "repository": { "name": "widgets", "owner": { "login": "acme" }, "issue": null } } });
    assert_eq!(client::fetched_issue_of(&absent), Ok(None));
    let no_repo = json!({ "data": { "repository": null }, "errors": [{ "type": "NOT_FOUND" }] });
    assert_eq!(client::fetched_issue_of(&no_repo), Err(ERR_REQUEST_FAILED.into()));
}

/// GPP-FR-XSKT: the items are read page by page until the last page, and a
/// read the page cap stops is marked truncated.
#[test]
fn items_are_read_across_pages() {
    let page = |numbers: &[u64], next: Option<&str>| {
        let nodes: Vec<serde_json::Value> = numbers
            .iter()
            .map(|n| json!({ "id": format!("I{n}"), "fieldValueByName": { "name": "Ready" },
                "content": { "number": n, "title": "t", "url": format!("https://github.com/acme/widgets/issues/{n}"),
                    "state": "OPEN", "issueType": { "name": "Task" },
                    "repository": { "name": "widgets", "owner": { "login": "acme" } } } }))
            .collect();
        json!({ "data": { "node": { "items": {
            "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next }, "nodes": nodes } } } })
    };
    let pages = [page(&[1, 2], Some("C2")), page(&[3], None)];
    let mut cursors = Vec::new();
    let read = client::paginate(client::MAX_ITEM_PAGES, |cursor| {
        let index = cursors.len();
        cursors.push(cursor);
        client::items_page_of(&pages[index])
    })
    .unwrap();
    assert_eq!(cursors, vec![None, Some("C2".to_string())]);
    assert!(!read.truncated);
    let kept = eligible_tasks(&read.items, &repo());
    assert_eq!(kept.iter().map(|t| t.issue_number).collect::<Vec<_>>(), vec![1, 2, 3]);

    // A read that always has a next page stops at the cap and says so.
    let mut calls = 0;
    let capped = client::paginate(3, |_| {
        calls += 1;
        client::items_page_of(&page(&[calls], Some("again")))
    })
    .unwrap();
    assert_eq!(calls, 3);
    assert!(capped.truncated);
    assert_eq!(capped.items.len(), 3);
}
