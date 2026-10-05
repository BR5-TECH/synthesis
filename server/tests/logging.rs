//! The tests of what the service logs.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`
//! (the non-functional record of the route, the status, and the identifiers of
//! the records a request touched; SAS-FR-PMRB) and
//! `specifications/server/SRB-server-relay-boundary.md` SRB-FR-IZAB.

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::json;

use synthesis_server::testing::{TEST_ADMIN_ID, TEST_TOKEN};

/// The records of the served requests.
fn requests(api: &Api) -> Vec<serde_json::Value> {
    api.logs.records_of("request")
}

// The service records the route, the status, and the identifier of the record
// the request created.
#[tokio::test]
async fn a_create_request_is_recorded_with_its_route_status_and_identifier() {
    let api = Api::new();
    let answer = api
        .post("/v1/users", json!({"display_name": "A user"}))
        .await;
    assert_eq!(answer.status, StatusCode::CREATED);

    let records = requests(&api);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["method"], "POST");
    assert_eq!(records[0]["route"], "/v1/users");
    assert_eq!(records[0]["status"], 201);
    assert_eq!(records[0]["records"]["id"], answer.id());
}

// The record names the route template rather than the path, and the identifiers
// the path carried reach the record as the parameters they are.
#[tokio::test]
async fn a_path_identifier_is_recorded_under_the_name_of_its_parameter() {
    let api = Api::new();
    let project = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await;
    let project_id = project.id();
    api.logs.clear();

    let answer = api.get(&format!("/v1/projects/{project_id}")).await;
    assert_eq!(answer.status, StatusCode::OK);

    let records = requests(&api);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["route"], "/v1/projects/{project_id}");
    assert_eq!(records[0]["records"]["project_id"], project_id);
}

// A refused request is recorded with the status it was refused with, so a
// failure leaves a trail.
#[tokio::test]
async fn a_refused_request_is_recorded_with_its_status() {
    let api = Api::new();
    let answer = api.call_with(Method::GET, "/v1/projects", None, None).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);

    let records = requests(&api);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["route"], "/v1/projects");
    assert_eq!(records[0]["status"], 401);
}

// SAS-FR-PMRB: no record holds the token, in any request that presents it and
// in any request that presents another one.
#[tokio::test]
async fn no_record_holds_the_token() {
    let api = Api::new();
    api.get("/v1/projects").await;
    api.call_with(
        Method::GET,
        "/v1/projects",
        None,
        Some(&format!("Bearer {TEST_TOKEN}")),
    )
    .await;
    api.call_with(
        Method::GET,
        "/v1/projects",
        None,
        Some("Bearer another-token"),
    )
    .await;

    let text = api.logs.text();
    assert!(!text.contains(TEST_TOKEN), "{text}");
    assert!(!text.contains("another-token"), "{text}");
}

// The non-functional logging rule: no email address, no Draft content, and no
// Conversation message body reaches a record.
#[tokio::test]
async fn no_record_holds_an_email_a_draft_content_or_a_message_body() {
    let api = Api::new();
    let user = api
        .post(
            "/v1/users",
            json!({"display_name": "A user", "email": "person@example.com"}),
        )
        .await;
    assert_eq!(user.status, StatusCode::CREATED);

    let project = api
        .post(
            "/v1/projects",
            json!({"display_name": "A project", "owner_id": TEST_ADMIN_ID}),
        )
        .await;
    let project_id = project.id();

    let draft = api
        .post(
            &format!("/v1/projects/{project_id}/drafts"),
            json!({"title": "A draft", "content": "The secret draft content"}),
        )
        .await;
    assert_eq!(draft.status, StatusCode::CREATED);

    let conversation = api
        .post(
            &format!("/v1/projects/{project_id}/conversations"),
            json!({"title": "A conversation"}),
        )
        .await;
    let conversation_id = conversation.id();
    let message = api
        .post(
            &format!("/v1/conversations/{conversation_id}/messages"),
            json!({"author_id": TEST_ADMIN_ID, "body": "The secret message body"}),
        )
        .await;
    assert_eq!(message.status, StatusCode::CREATED);

    let text = api.logs.text();
    assert!(!text.contains("person@example.com"), "{text}");
    assert!(!text.contains("The secret draft content"), "{text}");
    assert!(!text.contains("The secret message body"), "{text}");
    // The title of a record is content as well: a record names identifiers.
    assert!(!text.contains("A draft"), "{text}");
    assert!(!text.contains("A conversation"), "{text}");
}

// BMS-FR-11: the health route needs no credential and names no record, so it
// writes none.
#[tokio::test]
async fn the_health_route_writes_no_record() {
    let api = Api::new();
    let answer = api.call_with(Method::GET, "/v1/health", None, None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(api.logs.records().is_empty(), "{}", api.logs.text());
}

// SRB-FR-IZAB, SRB-FR-SEKN: an introspection route writes the record of the
// request and the record of the relay operation it drove.
#[tokio::test]
async fn a_relay_route_records_the_request_and_the_relay_operation() {
    let api = Api::new();
    let answer = api.get("/v1/relay/workers/instance-a").await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);

    let request = requests(&api);
    assert_eq!(request.len(), 1);
    assert_eq!(request[0]["route"], "/v1/relay/workers/{instance_id}");
    assert_eq!(request[0]["records"]["instance_id"], "instance-a");
    assert_eq!(request[0]["status"], 404);

    let relay = api.logs.records_of("relay");
    assert_eq!(relay.len(), 1);
    assert_eq!(relay[0]["operation"], "worker");
    assert_eq!(relay[0]["instance_id"], "instance-a");
    assert_eq!(relay[0]["outcome"], "not_found");
}

// The answer the caller receives is the answer the handler wrote: reading the
// identifier back does not change one byte of it.
#[tokio::test]
async fn reading_the_identifier_leaves_the_answer_unchanged() {
    let api = Api::new();
    let created = api.post("/v1/teams", json!({"name": "A team"})).await;
    assert_eq!(created.status, StatusCode::CREATED);

    let read = api.get(&format!("/v1/teams/{}", created.id())).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.member("name"), "A team");
    assert_eq!(read.id(), created.id());
}

// A read answer is never buffered: the record of a read names the identifiers
// of the path alone, and the answer reaches the caller whole.
#[tokio::test]
async fn a_list_answer_names_no_identifier_of_its_own() {
    let api = Api::new();
    api.post("/v1/users", json!({"display_name": "A user"}))
        .await;
    api.logs.clear();

    let listed = api.get("/v1/users").await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(
        listed.member("items").as_array().expect("the items").len(),
        2
    );

    let records = requests(&api);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["route"], "/v1/users");
    assert_eq!(records[0]["records"], json!({}));
}

// A path no route matches and a method a route refuses are recorded as well,
// each with the status the service answered.
#[tokio::test]
async fn an_unmatched_path_and_a_refused_method_are_recorded() {
    let api = Api::new();
    let missing = api.get("/v1/nothing-here").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let refused = api.call(Method::DELETE, "/v1/users", None).await;
    assert_eq!(refused.status, StatusCode::METHOD_NOT_ALLOWED);

    let records = requests(&api);
    let statuses: Vec<u64> = records
        .iter()
        .map(|record| record["status"].as_u64().expect("a status"))
        .collect();
    assert_eq!(statuses, vec![404, 405]);
    assert_eq!(records[0]["route"], "(unmatched)");
    assert_eq!(records[1]["route"], "/v1/users");
}
