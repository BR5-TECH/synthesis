//! The request record of the application routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`
//! (the non-functional record of the route, the status, and the identifiers of
//! the records a request touched; SAS-FR-PMRB for what a record may never
//! hold).
//!
//! One middleware over the application router. It names the method, the route
//! template, the status, and the identifiers the request touched. It reads the
//! identifiers from two places, and from nowhere else:
//!
//! - the path parameters of the matched route, which are identifiers by
//!   construction — every parameter of the surface is `{…_id}`;
//! - the `id` member of a JSON answer, which is how the identifier of a record
//!   the request created reaches the record.
//!
//! No header, no request body, and no other member of the answer is read, so
//! the token of SAS-FR-LFCA, an email address, Draft content, and a
//! Conversation message body cannot reach a record.

use std::collections::BTreeMap;

use axum::body::{to_bytes, Body};
use axum::extract::{FromRequestParts, MatchedPath, RawPathParams, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde_json::{json, Value};

use crate::adapters::http::state::HttpState;

/// The most bytes of a created answer the middleware reads back to find the
/// `id`.
///
/// The answer of a create route is one record, and one record holds at most one
/// content of the 1 000 000-character field limit, so the cap is far above what
/// the route can answer. A list answer is never read at all: it may hold 500
/// records, and buffering one is neither needed nor safe (see
/// `created_identifier`).
const MAX_READ_BYTES: usize = 4 * 1024 * 1024;

/// The route name a request that matched no route carries.
const UNMATCHED: &str = "(unmatched)";

/// Serves the request and writes one record for it.
pub async fn log_requests(
    State(state): State<HttpState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();

    // The path parameters are read before the handler runs, because the handler
    // consumes the request. `MatchedPath` names the route template rather than
    // the path, so an identifier never reaches the record through the route
    // name.
    let (mut parts, body) = request.into_parts();
    let route = parts
        .extensions
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| UNMATCHED.to_string());
    let mut identifiers = path_identifiers(&mut parts).await;
    let request = Request::from_parts(parts, body);

    let response = next.run(request).await;
    let status = response.status().as_u16();

    let (response, created) = created_identifier(response).await;
    if let Some(created) = created {
        identifiers.entry("id".to_string()).or_insert(created);
    }

    state.logs.write(json!({
        "event": "request",
        "method": method,
        "route": route,
        "status": status,
        "records": identifiers,
    }));

    response
}

/// The identifiers the matched route names, keyed by the parameter name.
async fn path_identifiers(parts: &mut axum::http::request::Parts) -> BTreeMap<String, String> {
    let mut identifiers = BTreeMap::new();
    let Ok(params) = RawPathParams::from_request_parts(parts, &()).await else {
        return identifiers;
    };
    for (name, value) in params.iter() {
        identifiers.insert(name.to_string(), value.to_string());
    }
    identifiers
}

/// The `id` member of a JSON answer, and the answer unchanged.
///
/// The body is read back and put together again, because a created record
/// reports its identifier in the answer alone. Nothing else of the body is
/// read, and the bytes the caller receives are the bytes the handler wrote.
///
/// A created answer is the one answer that is read. Every other route names the
/// records it touched in its path already, and a list answer may hold 500
/// records of 1 000 000 characters each; reading one back to find an identifier
/// the record does not need would hold the whole answer in memory for a
/// diagnostic.
async fn created_identifier(response: Response) -> (Response, Option<String>) {
    if response.status() != StatusCode::CREATED {
        return (response, None);
    }

    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_json {
        return (response, None);
    }

    let (parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_READ_BYTES).await else {
        // The answer was larger than the cap, or the body was not readable. The
        // body is already consumed, so the caller receives the status and the
        // headers with no body, and the record names one identifier less.
        return (Response::from_parts(parts, Body::empty()), None);
    };

    let identifier = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|document| {
            document
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    (
        Response::from_parts(parts, Body::from(bytes)),
        identifier.filter(|value| !value.is_empty()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::Json;

    // The identifier of a created record is read from the `id` member, and the
    // answer the caller receives is unchanged.
    #[tokio::test]
    async fn the_created_identifier_is_read_and_the_answer_is_unchanged() {
        let response = (
            StatusCode::CREATED,
            Json(json!({"id": "abc", "display_name": "a name"})),
        )
            .into_response();

        let (response, identifier) = created_identifier(response).await;
        assert_eq!(identifier.as_deref(), Some("abc"));
        assert_eq!(response.status(), StatusCode::CREATED);

        let bytes = to_bytes(response.into_body(), MAX_READ_BYTES)
            .await
            .expect("the body is readable");
        let document: Value = serde_json::from_slice(&bytes).expect("the body is JSON");
        assert_eq!(document["display_name"], "a name");
    }

    // An answer that is not JSON is passed on unread.
    #[tokio::test]
    async fn an_answer_that_is_not_json_names_no_identifier() {
        let response = (StatusCode::CREATED, "plain text").into_response();
        let (_, identifier) = created_identifier(response).await;
        assert_eq!(identifier, None);
    }

    // A created answer that holds no `id` member names no identifier. The other
    // members of the answer never reach the record.
    #[tokio::test]
    async fn an_answer_without_an_id_names_no_identifier() {
        let response = (
            StatusCode::CREATED,
            Json(json!({"items": [], "email": "person@example.com"})),
        )
            .into_response();
        let (_, identifier) = created_identifier(response).await;
        assert_eq!(identifier, None);
    }

    // An answer that reports no created record is passed on unread, whatever
    // its size. A list answer may hold 500 records, and the record of the
    // request needs no identifier from it.
    #[tokio::test]
    async fn an_answer_that_created_nothing_is_never_read() {
        let listed = Json(json!({"items": [{"id": "abc"}]})).into_response();
        let (listed, identifier) = created_identifier(listed).await;
        assert_eq!(identifier, None);
        assert_eq!(listed.status(), StatusCode::OK);

        let bytes = to_bytes(listed.into_body(), MAX_READ_BYTES)
            .await
            .expect("the body is readable");
        let document: Value = serde_json::from_slice(&bytes).expect("the body is JSON");
        assert_eq!(document["items"][0]["id"], "abc");
    }
}
