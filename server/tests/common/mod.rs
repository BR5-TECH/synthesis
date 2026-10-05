//! The fixtures the integration tests share.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.

#![allow(dead_code)]

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use synthesis_server::adapters::relay::port::RelayApi;
use synthesis_server::application::commands::{CreateGrant, CreateProject, CreateUser};
use synthesis_server::application::logging::CaptureSink;
use synthesis_server::application::ports::{Ports, Principal};
use synthesis_server::application::Application;
use synthesis_server::domain::ids::{ProjectId, UserId};
use synthesis_server::domain::project::Project;
use synthesis_server::router::{build_router, ServiceState};
use synthesis_server::testing::{test_state_with_logs, TEST_ADMIN_ID, TEST_TOKEN};

/// One service, as an integration test drives it.
pub struct Api {
    state: ServiceState,
    pub ports: Ports,
    /// The records the service wrote, for a test that reads them back.
    pub logs: std::sync::Arc<CaptureSink>,
}

/// One answer, read as a status and a JSON document.
pub struct Answer {
    pub status: StatusCode,
    pub body: Value,
}

impl Answer {
    /// The error code the body reports (SAS-FR-ZMPC).
    pub fn code(&self) -> String {
        self.body["error"]["code"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    /// A member of the body.
    pub fn member(&self, name: &str) -> &Value {
        &self.body[name]
    }

    /// The identifier the answer reports.
    pub fn id(&self) -> String {
        self.body["id"]
            .as_str()
            .expect("the answer names an id")
            .to_string()
    }
}

impl Api {
    /// A service over a store that holds the administrator alone.
    pub fn new() -> Self {
        let (state, ports, logs) = test_state_with_logs();
        Api { state, ports, logs }
    }

    /// The application services, for a test that drives them directly.
    pub fn application(&self) -> Application {
        self.state.http.application.clone()
    }

    /// The relay routing state.
    pub fn relay(&self) -> std::sync::Arc<dyn RelayApi> {
        std::sync::Arc::clone(&self.state.http.relay)
    }

    /// The administrator principal.
    pub fn administrator(&self) -> Principal {
        Principal::administrator(UserId::parse(TEST_ADMIN_ID).expect("the fixture is a UUID"))
    }

    fn router(&self) -> Router {
        build_router(self.state.clone())
    }

    /// One request that carries the configured token.
    pub async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Answer {
        self.call_with(method, path, body, Some(&Api::bearer()))
            .await
    }

    /// One request, with the credential the test names.
    pub async fn call_with(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        credential: Option<&str>,
    ) -> Answer {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(credential) = credential {
            builder = builder.header(AUTHORIZATION, credential);
        }
        let request = match body {
            None => builder.body(Body::empty()),
            Some(value) => builder
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string())),
        }
        .expect("the request is well formed");

        let response = self
            .router()
            .oneshot(request)
            .await
            .expect("the router answers every request");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body is readable")
            .to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Answer { status, body }
    }

    pub async fn get(&self, path: &str) -> Answer {
        self.call(Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Answer {
        self.call(Method::POST, path, Some(body)).await
    }

    pub async fn patch(&self, path: &str, body: Value) -> Answer {
        self.call(Method::PATCH, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> Answer {
        self.call(Method::DELETE, path, None).await
    }

    /// The bearer header the tests present.
    pub fn bearer() -> String {
        format!("Bearer {TEST_TOKEN}")
    }
}

/// One user, written as the administrator.
pub fn make_user(application: &Application, principal: &Principal, name: &str) -> UserId {
    application
        .identity
        .create_user(
            principal,
            CreateUser {
                display_name: name.to_string(),
                ..CreateUser::default()
            },
        )
        .expect("the user is written")
        .id
}

/// One project, owned by the named user.
pub fn make_project(
    application: &Application,
    principal: &Principal,
    owner: UserId,
    name: &str,
) -> Project {
    application
        .projects
        .create_project(
            principal,
            CreateProject {
                display_name: name.to_string(),
                owner_id: Some(owner.to_string()),
                ..CreateProject::default()
            },
        )
        .expect("the project is written")
}

/// One grant on a project.
pub fn make_grant(
    application: &Application,
    principal: &Principal,
    project: ProjectId,
    target_type: &str,
    target_id: &str,
    role: &str,
) -> synthesis_server::domain::project::Grant {
    application
        .projects
        .create_grant(
            principal,
            project,
            CreateGrant {
                target_type: target_type.to_string(),
                target_id: target_id.to_string(),
                role: role.to_string(),
                ..CreateGrant::default()
            },
        )
        .expect("the grant is written")
}

/// The body of a request that names nothing.
pub fn empty() -> Value {
    json!({})
}
