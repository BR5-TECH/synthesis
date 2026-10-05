//! The extractors of the application routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-LFCA, SAS-FR-BZUK, SAS-FR-XRPD, SAS-FR-CJNV,
//! SAS-FR-NKBC, SAS-FR-ZMPC.

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::Json;
use serde::de::DeserializeOwned;

use crate::adapters::http::error::ApiError;
use crate::adapters::http::state::HttpState;
use crate::application::ports::Principal;
use crate::domain::error::DomainError;

/// The scheme the header carries (SAS-FR-LFCA).
const SCHEME: &str = "Bearer ";

/// Resolves the principal from the credential the request carries.
///
/// SAS-FR-CJNV: the boundary answers "which persisted user is this?". A later
/// OIDC mode maps `(issuer, subject)` onto a persisted user here, and changes
/// neither the ownership fields nor the access-control model.
impl FromRequestParts<HttpState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &HttpState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(ApiError(DomainError::Unauthenticated))?;

        let presented = header
            .strip_prefix(SCHEME)
            .ok_or(ApiError(DomainError::Unauthenticated))?;

        // SAS-FR-BZUK: the comparison is constant time over the two values.
        if !state.identity.token.matches(presented.trim()) {
            return Err(ApiError(DomainError::Unauthenticated));
        }

        // SAS-FR-XRPD: the configured token acts as the persisted administrator.
        Ok(Principal::administrator(state.identity.administrator_id))
    }
}

/// A JSON body, with the error answer of SAS-FR-ZMPC when it cannot be read.
pub struct Payload<T>(pub T);

impl<T, S> FromRequest<S> for Payload<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Payload(value)),
            Err(rejection) => Err(ApiError(DomainError::invalid_field(
                "body",
                &rejection.body_text(),
            ))),
        }
    }
}

/// A query string, with the same error answer.
pub struct Params<T>(pub T);

impl<T, S> FromRequestParts<S> for Params<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
    Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Params(value)),
            Err(rejection) => Err(ApiError(DomainError::invalid_field(
                "query",
                &rejection.body_text(),
            ))),
        }
    }
}

/// Reads an identifier from a path segment (SAS-FR-NKBC).
pub fn path_id<T>(text: &str) -> Result<T, ApiError>
where
    T: From<uuid::Uuid>,
{
    uuid::Uuid::parse_str(text.trim())
        .map(T::from)
        .map_err(|_| {
            ApiError(DomainError::invalid_field(
                "path",
                "the value is not a UUID",
            ))
        })
}
