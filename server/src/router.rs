//! The HTTP surface of the service.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-04, BMS-FR-11, BMS-FR-12, BMS-FR-JQZW, BMS-FR-13,
//! BMS-FR-27.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::adapters::http::state::HttpState;

/// The one path the service routes without a credential.
pub const HEALTH_PATH: &str = "/v1/health";

/// The capabilities the health answer advertises (BMS-FR-JQZW).
///
/// Each identifier names one dedicated specification:
/// `RSN-remote-session.md` and `WSK-websocket.md`. The list is compiled in, so
/// the answer reports no connection state and no application content.
pub const CAPABILITIES: [&str; 2] = ["remote_session", "websocket"];

/// The state the router carries.
///
/// BMS-FR-12: the health handler holds the version constant that was compiled
/// in alone, so it reads no file, opens no connection, and consults no clock,
/// and every call in the life of a process returns the same bytes. The
/// application routes carry their own state.
#[derive(Clone)]
pub struct ServiceState {
    /// The build version the health response reports.
    pub version: &'static str,
    /// The state of the authenticated routes
    /// (`SAS-server-application-service.md` SAS-FR-VBQJ).
    pub http: HttpState,
}

/// The body of the health response (BMS-FR-11).
///
/// The struct holds exactly two members, `version` and `capabilities`, so the
/// JSON it serialises has exactly the key set `{"version","capabilities"}`.
#[derive(Debug, Serialize)]
struct HealthResponse {
    version: &'static str,
    capabilities: [&'static str; 2],
}

/// Builds the router the binary serves (BMS-FR-04).
///
/// This is the one constructor of the HTTP surface. A test drives the same
/// router the binary serves without opening a socket, and a later specification
/// adds a route by extending this function.
///
/// BMS-FR-13: the router carries the health route and the routes of
/// `SAS-server-application-service.md` and `SRB-server-relay-boundary.md`. Axum
/// answers a path none of them routes `404 Not Found`, and another method on a
/// routed path `405 Method Not Allowed`; neither answer carries application
/// content.
///
/// The health route is the one route that needs no credential: it carries the
/// version alone as its state, so the health answer cannot reach a record.
pub fn build_router(state: ServiceState) -> Router {
    let health = Router::new()
        .route(HEALTH_PATH, get(health))
        .with_state(state.version);

    // The record of the route, the status, and the identifiers a request
    // touched is written for the application routes alone. The health answer
    // holds one compiled-in version and names no record, so it writes none.
    let application = crate::adapters::http::routes()
        .with_state(state.http.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.http,
            crate::adapters::http::logging::log_requests,
        ));

    health.merge(application)
}

/// Answers `GET /v1/health` (BMS-FR-11, BMS-FR-12).
async fn health(State(version): State<&'static str>) -> Json<HealthResponse> {
    Json(HealthResponse {
        version,
        capabilities: CAPABILITIES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::header::CONTENT_TYPE;
    use axum::http::{Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::testing::{test_state, TEST_VERSION};

    fn router() -> Router {
        build_router(test_state().0)
    }

    async fn call(method: Method, path: &str) -> (StatusCode, Option<String>, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .expect("the request is well formed");

        let response = router()
            .oneshot(request)
            .await
            .expect("the router answers every request");

        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .map(|value| value.to_str().unwrap_or_default().to_string());
        let body = response
            .into_body()
            .collect()
            .await
            .expect("the body is readable")
            .to_bytes()
            .to_vec();

        (status, content_type, body)
    }

    // BMS-FR-04, BMS-FR-11, BMS-FR-25: the status, the content type, and the exact response shape,
    // asserted against the router of BMS-FR-04 with no socket opened.
    #[tokio::test]
    async fn health_returns_200_json_with_a_version_and_a_capability_list() {
        let (status, content_type, body) = call(Method::GET, HEALTH_PATH).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some("application/json"));

        let parsed: serde_json::Value =
            serde_json::from_slice(&body).expect("the body is a JSON document");
        let object = parsed.as_object().expect("the body is a JSON object");

        assert_eq!(object.len(), 2, "the object holds exactly two members");
        let version = object.get("version").expect("a member is named version");
        assert!(version.is_string(), "the version is a string");
        assert_eq!(version.as_str(), Some(TEST_VERSION));

        let capabilities = object
            .get("capabilities")
            .and_then(|value| value.as_array())
            .expect("a member named capabilities holds an array");
        assert_eq!(
            capabilities
                .iter()
                .map(|value| value.as_str().expect("an identifier is a string"))
                .collect::<Vec<_>>(),
            vec!["remote_session", "websocket"],
        );
    }

    // BMS-FR-JQZW: the identifiers are exactly the two capabilities of V1, in
    // order, and the answer discloses no connection state.
    #[tokio::test]
    async fn the_capability_list_is_exactly_the_two_identifiers_of_v1() {
        assert_eq!(CAPABILITIES, ["remote_session", "websocket"]);

        let (_, _, body) = call(Method::GET, HEALTH_PATH).await;
        let text = String::from_utf8(body).expect("the body is UTF-8");
        for absent in ["handle", "instance_id", "user", "project_id", "token"] {
            assert!(!text.contains(absent), "the answer named {absent}");
        }
    }

    // BMS-FR-12: every call returns byte-identical bytes, because the handler
    // answers from the compiled-in constant alone.
    #[tokio::test]
    async fn every_health_response_is_byte_identical() {
        let (_, _, first) = call(Method::GET, HEALTH_PATH).await;
        for _ in 0..16 {
            let (status, content_type, body) = call(Method::GET, HEALTH_PATH).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(content_type.as_deref(), Some("application/json"));
            assert_eq!(body, first);
        }
    }

    // BMS-FR-13, BMS-FR-27: no other path is routed.
    #[tokio::test]
    async fn every_other_path_is_answered_404() {
        for path in [
            "/v1/status",
            "/health",
            "/v1/ws",
            "/",
            "/v1",
            "/v1/health/",
            "/v1/health/extra",
            "/metrics",
            // The path is matched exactly: neither the case nor an empty
            // segment is folded away.
            "/V1/Health",
            "/v1//health",
        ] {
            let (status, _, body) = call(Method::GET, path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "path {path}");
            assert!(body.is_empty(), "path {path} carried application content");
        }
    }

    /// BMS-FR-13: the route accepts `GET` and `HEAD`. A `HEAD` request asks
    /// for the `GET` answer without its body, so it carries the same status and
    /// the same headers, which is what lets a container orchestrator probe the
    /// service with either method.
    #[tokio::test]
    async fn head_answers_like_get_without_a_body() {
        let (status, content_type, body) = call(Method::HEAD, HEALTH_PATH).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert!(body.is_empty(), "a HEAD answer carries no body");
    }

    // BMS-FR-13, BMS-FR-27: no method other than GET and HEAD is accepted on the health
    // path. HEAD is absent from this list deliberately — see the test above.
    #[tokio::test]
    async fn every_other_method_on_health_is_answered_405() {
        for method in [
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
            Method::TRACE,
            Method::CONNECT,
        ] {
            let (status, _, body) = call(method.clone(), HEALTH_PATH).await;
            assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "method {method}");
            assert!(
                body.is_empty(),
                "method {method} carried application content"
            );
        }
    }

    // A query string names no route. The handler holds no state and reads no
    // input, so a request that carries one is answered exactly as one that does
    // not (BMS-FR-12).
    #[tokio::test]
    async fn a_query_string_changes_nothing() {
        let (plain_status, _, plain_body) = call(Method::GET, HEALTH_PATH).await;
        let (status, content_type, body) =
            call(Method::GET, &format!("{HEALTH_PATH}?anything=1&x=2")).await;

        assert_eq!(status, plain_status);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(body, plain_body);
    }

    // BMS-FR-13, BMS-FR-27: the foundation routes no upgrade of its own. A
    // handshake to a path `WSK-websocket.md` does not name is refused on the
    // same terms as any other request to an unrouted path.
    #[tokio::test]
    async fn a_websocket_handshake_on_an_unrouted_path_is_not_upgraded() {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/ws")
            .header("connection", "Upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Body::empty())
            .expect("the request is well formed");

        let response = router()
            .oneshot(request)
            .await
            .expect("the router answers every request");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.headers().get("upgrade").is_none());
    }
}
