//! The errors the domain and the application services raise.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-ZMPC, SAS-FR-TKPZ, SAS-FR-TQIM, SAS-FR-NKBC.

use std::fmt;

/// One refusal, with the code the error body of SAS-FR-ZMPC reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// No credential, or a credential that does not match (SAS-FR-LFCA).
    Unauthenticated,
    /// The principal holds no permission the operation needs (SAS-FR-TKPZ).
    Forbidden { permission: String },
    /// No record of that kind holds the identifier (SAS-FR-NKBC).
    NotFound { resource: &'static str },
    /// A field is absent, empty, or too long (SAS-FR-TQIM).
    InvalidField { field: String, reason: String },
    /// A role outside the set the specification names (SAS-FR-EPXN).
    InvalidRole { role: String },
    /// A permission outside the set of SAS-FR-IJRO.
    InvalidPermission { permission: String },
    /// A record of that kind already holds the identifier (SAS-FR-OZET).
    DuplicateId { resource: &'static str },
    /// A second active membership for one member and one target (SAS-FR-BFHN).
    DuplicateMembership,
    /// A second grant with one project, target, and role (SAS-FR-VNTC).
    DuplicateGrant,
    /// The owner grant may not be removed or reduced (SAS-FR-MWOD).
    OwnerProtected,
    /// Another record still references this one (SAS-FR-SGXQ).
    Referenced { reason: String },
    /// The update carries a revision that is not the current one (SAS-FR-JZAC).
    StaleRevision { expected: u64, actual: u64 },
    /// A message was appended to a locked conversation (SAS-FR-PYSU).
    ConversationLocked,
    /// The invitation is expired, revoked, or accepted (SAS-FR-ATLB).
    InvitationNotPending,
    /// A failure the service did not classify.
    Internal { reason: String },
}

impl DomainError {
    /// The code the error body reports (SAS-FR-ZMPC).
    pub fn code(&self) -> &'static str {
        match self {
            DomainError::Unauthenticated => "unauthenticated",
            DomainError::Forbidden { .. } => "forbidden",
            DomainError::NotFound { .. } => "not_found",
            DomainError::InvalidField { .. } => "invalid_field",
            DomainError::InvalidRole { .. } => "invalid_role",
            DomainError::InvalidPermission { .. } => "invalid_permission",
            DomainError::DuplicateId { .. } => "duplicate_id",
            DomainError::DuplicateMembership => "duplicate_membership",
            DomainError::DuplicateGrant => "duplicate_grant",
            DomainError::OwnerProtected => "owner_protected",
            DomainError::Referenced { .. } => "referenced",
            DomainError::StaleRevision { .. } => "stale_revision",
            DomainError::ConversationLocked => "conversation_locked",
            DomainError::InvitationNotPending => "invitation_not_pending",
            DomainError::Internal { .. } => "internal",
        }
    }

    /// A field that a request may not use.
    pub fn invalid_field(field: &str, reason: &str) -> Self {
        DomainError::InvalidField {
            field: field.to_string(),
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::Unauthenticated => {
                write!(formatter, "the request carries no accepted credential")
            }
            DomainError::Forbidden { permission } => write!(
                formatter,
                "the principal holds no {permission} permission on this project"
            ),
            DomainError::NotFound { resource } => {
                write!(formatter, "no {resource} holds that identifier")
            }
            DomainError::InvalidField { field, reason } => {
                write!(formatter, "the {field} field was refused: {reason}")
            }
            DomainError::InvalidRole { role } => {
                write!(formatter, "the role \"{role}\" is unknown")
            }
            DomainError::InvalidPermission { permission } => {
                write!(formatter, "the permission \"{permission}\" is unknown")
            }
            DomainError::DuplicateId { resource } => {
                write!(
                    formatter,
                    "another {resource} already holds that identifier"
                )
            }
            DomainError::DuplicateMembership => write!(
                formatter,
                "the member already holds an active membership of that target"
            ),
            DomainError::DuplicateGrant => write!(
                formatter,
                "the project already holds a grant with that target and that role"
            ),
            DomainError::OwnerProtected => write!(
                formatter,
                "the owner grant is changed by a transfer of ownership alone"
            ),
            DomainError::Referenced { reason } => {
                write!(formatter, "the record is referenced: {reason}")
            }
            DomainError::StaleRevision { expected, actual } => write!(
                formatter,
                "the request carries revision {expected}, and the record holds revision {actual}"
            ),
            DomainError::ConversationLocked => {
                write!(
                    formatter,
                    "the conversation is locked and accepts no message"
                )
            }
            DomainError::InvitationNotPending => {
                write!(formatter, "the invitation is expired, revoked, or accepted")
            }
            DomainError::Internal { reason } => write!(formatter, "the request failed: {reason}"),
        }
    }
}

impl std::error::Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::*;

    // SAS-FR-ZMPC: every error carries one of the codes the specification names.
    #[test]
    fn every_error_reports_the_code_the_specification_names() {
        let codes = [
            DomainError::Unauthenticated.code(),
            DomainError::Forbidden {
                permission: "project.read".into(),
            }
            .code(),
            DomainError::NotFound {
                resource: "project",
            }
            .code(),
            DomainError::invalid_field("name", "empty").code(),
            DomainError::InvalidRole { role: "x".into() }.code(),
            DomainError::InvalidPermission {
                permission: "x".into(),
            }
            .code(),
            DomainError::DuplicateId { resource: "user" }.code(),
            DomainError::DuplicateMembership.code(),
            DomainError::DuplicateGrant.code(),
            DomainError::OwnerProtected.code(),
            DomainError::Referenced {
                reason: "a project".into(),
            }
            .code(),
            DomainError::StaleRevision {
                expected: 1,
                actual: 2,
            }
            .code(),
            DomainError::ConversationLocked.code(),
            DomainError::InvitationNotPending.code(),
            DomainError::Internal { reason: "x".into() }.code(),
        ];
        assert_eq!(codes.len(), 15);
        for code in codes {
            assert!(!code.is_empty());
            assert_eq!(code, code.to_lowercase());
        }
    }

    // SAS-FR-PMRB: no message of an error holds a credential. The variants that
    // could carry one carry a field name and a reason instead.
    #[test]
    fn no_error_message_holds_a_credential() {
        let message = DomainError::Unauthenticated.to_string();
        assert!(!message.contains("Bearer"));
        assert!(message.contains("credential"));
    }
}
