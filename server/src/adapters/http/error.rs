//! The error answer of every application route.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-ZMPC, SAS-FR-LFCA, SAS-FR-TKPZ.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::domain::error::DomainError;

/// One refusal, as the HTTP surface answers it.
#[derive(Debug, Clone)]
pub struct ApiError(pub DomainError);

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        ApiError(error)
    }
}

/// The body of an error answer (SAS-FR-ZMPC).
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorMember,
}

#[derive(Debug, Serialize)]
struct ErrorMember {
    code: &'static str,
    message: String,
}

/// The status each refusal carries.
pub fn status_of(error: &DomainError) -> StatusCode {
    match error {
        DomainError::Unauthenticated => StatusCode::UNAUTHORIZED,
        DomainError::Forbidden { .. } => StatusCode::FORBIDDEN,
        DomainError::NotFound { .. } => StatusCode::NOT_FOUND,
        DomainError::InvalidField { .. }
        | DomainError::InvalidRole { .. }
        | DomainError::InvalidPermission { .. } => StatusCode::BAD_REQUEST,
        DomainError::DuplicateId { .. }
        | DomainError::DuplicateMembership
        | DomainError::DuplicateGrant
        | DomainError::OwnerProtected
        | DomainError::Referenced { .. }
        | DomainError::StaleRevision { .. }
        | DomainError::ConversationLocked
        | DomainError::InvitationNotPending => StatusCode::CONFLICT,
        DomainError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status_of(&self.0);
        let body = Json(ErrorBody {
            error: ErrorMember {
                code: self.0.code(),
                // SAS-FR-PMRB: the message is the refusal's own words. No
                // variant holds a credential, so none reaches an answer.
                message: self.0.to_string(),
            },
        });

        if status == StatusCode::UNAUTHORIZED {
            // The challenge names the scheme alone (SAS-FR-LFCA).
            return (status, [(header::WWW_AUTHENTICATE, "Bearer")], body).into_response();
        }
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAS-FR-ZMPC: each refusal carries the status the contract names.
    #[test]
    fn each_refusal_carries_its_status() {
        let cases = [
            (DomainError::Unauthenticated, StatusCode::UNAUTHORIZED),
            (
                DomainError::Forbidden {
                    permission: "draft.read".to_string(),
                },
                StatusCode::FORBIDDEN,
            ),
            (
                DomainError::NotFound { resource: "draft" },
                StatusCode::NOT_FOUND,
            ),
            (
                DomainError::invalid_field("title", "empty"),
                StatusCode::BAD_REQUEST,
            ),
            (DomainError::DuplicateGrant, StatusCode::CONFLICT),
            (DomainError::OwnerProtected, StatusCode::CONFLICT),
            (
                DomainError::StaleRevision {
                    expected: 1,
                    actual: 2,
                },
                StatusCode::CONFLICT,
            ),
            (
                DomainError::Internal {
                    reason: "x".to_string(),
                },
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(status_of(&error), expected, "{error}");
        }
    }
}
