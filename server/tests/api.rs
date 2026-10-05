//! The tests of the HTTP surface.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`,
//! `specifications/server/SRB-server-relay-boundary.md`.

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::json;

use synthesis_server::adapters::relay::port::ExposedProject;
use synthesis_server::testing::{TEST_ADMIN_ID, TEST_TOKEN};

/// The paths a credential is needed for.
const AUTHENTICATED_PATHS: [&str; 8] = [
    "/v1/users",
    "/v1/teams",
    "/v1/organizations",
    "/v1/memberships",
    "/v1/invitations",
    "/v1/devices",
    "/v1/projects",
    "/v1/relay/workers",
];

// SAS-FR-LFCA: every application route needs the bearer token, and a request
// that carries none is answered 401 with the `unauthenticated` code.
#[tokio::test]
async fn every_application_route_needs_the_token() {
    let api = Api::new();
    for path in AUTHENTICATED_PATHS {
        let answer = api.call_with(Method::GET, path, None, None).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "path {path}");
        assert_eq!(answer.code(), "unauthenticated", "path {path}");
    }
}

// SAS-FR-LFCA, SAS-FR-BZUK: another token, another scheme, and a prefix of the
// token are each refused.
#[tokio::test]
async fn only_the_configured_token_is_accepted() {
    let api = Api::new();
    let refused = [
        format!("Bearer {}", &TEST_TOKEN[..TEST_TOKEN.len() - 1]),
        format!("Bearer {TEST_TOKEN}x"),
        format!("Basic {TEST_TOKEN}"),
        TEST_TOKEN.to_string(),
        "Bearer".to_string(),
        "Bearer ".to_string(),
    ];
    for credential in refused {
        let answer = api
            .call_with(Method::GET, "/v1/projects", None, Some(&credential))
            .await;
        assert_eq!(
            answer.status,
            StatusCode::UNAUTHORIZED,
            "credential {credential}"
        );
    }

    let accepted = api
        .call_with(
            Method::GET,
            "/v1/projects",
            None,
            Some(&format!("Bearer {TEST_TOKEN}")),
        )
        .await;
    assert_eq!(accepted.status, StatusCode::OK);
}

// BMS-FR-11, BMS-FR-JQZW, SAS-FR-LFCA: the health route needs no credential and
// answers the version and the capability list.
#[tokio::test]
async fn the_health_route_needs_no_credential() {
    let api = Api::new();
    let answer = api.call_with(Method::GET, "/v1/health", None, None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(answer.member("version").is_string());
    assert_eq!(
        answer.member("capabilities"),
        &json!(["remote_session", "websocket"])
    );
}

// SAS-FR-ZMPC: every error answer is one object with an `error` member holding
// a code and a message, and no other member.
#[tokio::test]
async fn every_error_answer_carries_the_same_shape() {
    let api = Api::new();
    let answers = [
        api.get("/v1/projects/not-a-uuid").await,
        api.get(&format!("/v1/projects/{}", uuid::Uuid::from_u128(3)))
            .await,
        api.post("/v1/users", json!({"display_name": ""})).await,
    ];
    for answer in answers {
        let object = answer.body.as_object().expect("the body is an object");
        assert_eq!(object.len(), 1, "{:?}", answer.body);
        let error = object["error"].as_object().expect("the error is an object");
        assert_eq!(error.len(), 2);
        assert!(error["code"].is_string());
        assert!(error["message"].is_string());
    }
}

// SAS-FR-NKBC: a path identifier that is not a UUID is 400, and one no record
// holds is 404.
#[tokio::test]
async fn a_malformed_identifier_and_an_unknown_one_differ() {
    let api = Api::new();
    let malformed = api.get("/v1/users/not-a-uuid").await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    assert_eq!(malformed.code(), "invalid_field");

    let unknown = api
        .get(&format!("/v1/users/{}", uuid::Uuid::from_u128(11)))
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.code(), "not_found");
}

// SAS-FR-LEQB: the role-to-permission mapping is published.
#[tokio::test]
async fn the_role_mapping_is_published() {
    let api = Api::new();
    let answer = api.get("/v1/roles").await;
    assert_eq!(answer.status, StatusCode::OK);

    let permissions = answer.member("permissions").as_array().expect("a list");
    assert_eq!(permissions.len(), 15);
    assert!(permissions.iter().any(|item| item == "project.manage_acl"));

    let roles = answer.member("project_roles").as_array().expect("a list");
    assert_eq!(roles.len(), 4);
    let viewer = roles
        .iter()
        .find(|role| role["role"] == "viewer")
        .expect("the viewer role is published");
    assert_eq!(viewer["permissions"].as_array().expect("a list").len(), 3);
    assert_eq!(
        answer
            .member("team_roles")
            .as_array()
            .expect("a list")
            .len(),
        4
    );
}

// SAS-FR-OKUC, SAS-FR-MWOD: a project is created with its owner and its owner
// grant, and is read, updated, and deleted over the surface.
#[tokio::test]
async fn a_project_is_created_read_updated_and_deleted() {
    let api = Api::new();
    let created = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.member("owner_id").as_str(), Some(TEST_ADMIN_ID));
    let project_id = created.id();

    let read = api.get(&format!("/v1/projects/{project_id}")).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.member("display_name"), "A project");

    let updated = api
        .patch(
            &format!("/v1/projects/{project_id}"),
            json!({"display_name": "A renamed project"}),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.member("display_name"), "A renamed project");

    let grants = api.get(&format!("/v1/projects/{project_id}/grants")).await;
    assert_eq!(grants.status, StatusCode::OK);
    let items = grants.member("items").as_array().expect("a list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["role"], "owner");
    assert_eq!(
        items[0]["permissions"].as_array().expect("a list").len(),
        15
    );

    let deleted = api.delete(&format!("/v1/projects/{project_id}")).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(
        api.get(&format!("/v1/projects/{project_id}")).await.status,
        StatusCode::NOT_FOUND
    );
}

// SAS-FR-VNTC, SAS-FR-MWOD: the conflict rules answer 409 over the surface.
#[tokio::test]
async fn the_conflict_rules_answer_over_the_surface() {
    let api = Api::new();
    let project_id = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await
        .id();
    let user_id = api
        .post("/v1/users", json!({"display_name": "A member"}))
        .await
        .id();

    let grant = json!({"target_type": "user", "target_id": user_id, "role": "viewer"});
    let created = api
        .post(&format!("/v1/projects/{project_id}/grants"), grant.clone())
        .await;
    assert_eq!(created.status, StatusCode::CREATED);

    let duplicate = api
        .post(&format!("/v1/projects/{project_id}/grants"), grant)
        .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    assert_eq!(duplicate.code(), "duplicate_grant");

    let owner_role = api
        .post(
            &format!("/v1/projects/{project_id}/grants"),
            json!({"target_type": "user", "target_id": user_id, "role": "owner"}),
        )
        .await;
    assert_eq!(owner_role.status, StatusCode::CONFLICT);
    assert_eq!(owner_role.code(), "owner_protected");

    let unknown_role = api
        .post(
            &format!("/v1/projects/{project_id}/grants"),
            json!({"target_type": "user", "target_id": user_id, "role": "editor"}),
        )
        .await;
    assert_eq!(unknown_role.status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_role.code(), "invalid_role");

    // The grant is revoked and then deleted.
    let grant_id = created.id();
    let revoked = api
        .post(
            &format!("/v1/projects/{project_id}/grants/{grant_id}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert!(revoked.member("revoked_at").is_string());
    assert_eq!(
        api.delete(&format!("/v1/projects/{project_id}/grants/{grant_id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

// SAS-FR-GBWS, SAS-FR-JZAC: a draft is written, updated by revision, and
// refused when the revision is stale.
#[tokio::test]
async fn a_draft_is_written_and_updated_by_revision() {
    let api = Api::new();
    let project_id = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await
        .id();

    let created = api
        .post(
            &format!("/v1/projects/{project_id}/drafts"),
            json!({"title": "A draft", "content": "First"}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let draft_id = created.id();
    assert_eq!(created.member("revision"), 1);

    let updated = api
        .patch(
            &format!("/v1/drafts/{draft_id}"),
            json!({"revision": 1, "content": "Second"}),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.member("revision"), 2);

    let stale = api
        .patch(
            &format!("/v1/drafts/{draft_id}"),
            json!({"revision": 1, "content": "Third"}),
        )
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.code(), "stale_revision");
}

// SAS-FR-XONP, SAS-FR-NUZG: a conversation holds an append-only sequence.
#[tokio::test]
async fn a_conversation_appends_and_locks() {
    let api = Api::new();
    let project_id = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await
        .id();
    let conversation_id = api
        .post(
            &format!("/v1/projects/{project_id}/conversations"),
            json!({"title": "A conversation"}),
        )
        .await
        .id();

    for expected in 1..=3u64 {
        let appended = api
            .post(
                &format!("/v1/conversations/{conversation_id}/messages"),
                json!({"body": format!("message {expected}")}),
            )
            .await;
        assert_eq!(appended.status, StatusCode::CREATED);
        assert_eq!(appended.member("sequence"), expected);
    }

    let messages = api
        .get(&format!("/v1/conversations/{conversation_id}/messages"))
        .await;
    assert_eq!(
        messages.member("items").as_array().expect("a list").len(),
        3
    );

    assert_eq!(
        api.post(
            &format!("/v1/conversations/{conversation_id}/lock"),
            json!({})
        )
        .await
        .status,
        StatusCode::OK
    );
    let refused = api
        .post(
            &format!("/v1/conversations/{conversation_id}/messages"),
            json!({"body": "Refused"}),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.code(), "conversation_locked");
}

// SAS-FR-ZDVR: the effective permissions of a user are read over the surface.
#[tokio::test]
async fn the_effective_permissions_are_read_over_the_surface() {
    let api = Api::new();
    let project_id = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await
        .id();
    let user_id = api
        .post("/v1/users", json!({"display_name": "A member"}))
        .await
        .id();
    api.post(
        &format!("/v1/projects/{project_id}/grants"),
        json!({"target_type": "user", "target_id": user_id, "role": "contributor"}),
    )
    .await;

    let answer = api
        .get(&format!(
            "/v1/projects/{project_id}/permissions?user_id={user_id}"
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.member("user_id").as_str(), Some(user_id.as_str()));
    let permissions = answer.member("permissions").as_array().expect("a list");
    assert_eq!(permissions.len(), 10);
    assert!(permissions.iter().any(|item| item == "draft.create"));
    assert!(!permissions.iter().any(|item| item == "project.manage_acl"));
}

// SAS-FR-TQIM: a page size above the maximum is refused.
#[tokio::test]
async fn a_page_size_above_the_maximum_is_refused() {
    let api = Api::new();
    let refused = api.get("/v1/users?limit=1000").await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.code(), "invalid_field");

    let accepted = api.get("/v1/users?limit=1").await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(
        accepted.member("items").as_array().expect("a list").len(),
        1
    );
}

// SRB-FR-SEKN: the relay introspection routes report the active routes, and
// need the token.
#[tokio::test]
async fn the_relay_routes_report_the_active_workers() {
    let api = Api::new();
    let relay = api.relay();
    relay
        .register_worker(
            "instance-a",
            "connection-1",
            vec![ExposedProject {
                project_id: "p1".to_string(),
                display_name: "The first project".to_string(),
            }],
        )
        .expect("the worker is registered");
    relay
        .select_project("instance-a", "p1")
        .expect("the project is selected");
    let client = relay
        .register_client("reg-1", "instance-a")
        .expect("a registration");
    let client = relay
        .resolve_registration(&client.handle, "instance-a", true)
        .expect("the IDE accepted the registration");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");

    let listed = api.get("/v1/relay/workers").await;
    assert_eq!(listed.status, StatusCode::OK);
    let items = listed.member("items").as_array().expect("a list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["instance_id"], "instance-a");
    assert_eq!(items[0]["selected_project_id"], "p1");
    assert_eq!(items[0]["attached_clients"], 1);

    let read = api.get("/v1/relay/workers/instance-a").await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.member("connection_id"), "connection-1");

    // The answer names no handle.
    assert!(
        !read.body.to_string().contains(&client.handle),
        "the answer holds a client handle"
    );
    assert_eq!(
        api.get("/v1/relay/workers/instance-none").await.status,
        StatusCode::NOT_FOUND
    );
}

// SRB-FR-TOQF, WSK-FR-KTRB: the relay opens the three WebSocket routes of
// `WSK-websocket.md` and no other.
#[tokio::test]
async fn no_websocket_route_beyond_the_three_of_the_transport_is_served() {
    let api = Api::new();
    for path in ["/v1/relay/ws", "/v1/ws", "/v1/relay/worker", "/v1/relay"] {
        assert_eq!(
            api.get(path).await.status,
            StatusCode::NOT_FOUND,
            "path {path}"
        );
    }
}

// SAS-FR-PDNU: a restart loses every record, and reconciles the administrator
// again.
#[tokio::test]
async fn a_restart_loses_every_record_and_keeps_the_administrator() {
    let first = Api::new();
    first
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await;
    first
        .post("/v1/users", json!({"display_name": "A member"}))
        .await;
    assert_eq!(
        first
            .get("/v1/users")
            .await
            .member("items")
            .as_array()
            .expect("a list")
            .len(),
        2
    );

    let second = Api::new();
    assert!(second
        .get("/v1/projects")
        .await
        .member("items")
        .as_array()
        .expect("a list")
        .is_empty());
    let users = second.get("/v1/users").await;
    let items = users.member("items").as_array().expect("a list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(TEST_ADMIN_ID));
    assert_eq!(items[0]["role"], "administrator");
}

// SAS-FR-PMRB: no answer of the surface holds the token.
#[tokio::test]
async fn no_answer_holds_the_token() {
    let api = Api::new();
    let project_id = api
        .post("/v1/projects", json!({"display_name": "A project"}))
        .await
        .id();

    for path in [
        "/v1/health".to_string(),
        "/v1/users".to_string(),
        "/v1/roles".to_string(),
        format!("/v1/projects/{project_id}"),
        format!("/v1/projects/{project_id}/grants"),
    ] {
        let answer = api.get(&path).await;
        assert!(
            !answer.body.to_string().contains(TEST_TOKEN),
            "the answer of {path} holds the token"
        );
    }

    let refused = api
        .call_with(Method::GET, "/v1/users", None, Some("Bearer wrong-token"))
        .await;
    assert!(!refused.body.to_string().contains("wrong-token"));
}
