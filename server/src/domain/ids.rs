//! The identifiers of the application entities.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-KDSM, SAS-FR-NKBC, SAS-FR-OZET.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::error::DomainError;

/// Declares one identifier type.
///
/// Each entity carries its own type, so a project identifier never reaches a
/// draft repository by accident. Every one of them wraps a UUID, which is what
/// SAS-FR-NKBC accepts in a path.
macro_rules! identifier {
    ($name:ident, $label:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// The name this identifier carries in a diagnostic.
            pub const LABEL: &'static str = $label;

            /// Reads the identifier from text (SAS-FR-NKBC).
            pub fn parse(text: &str) -> Result<Self, DomainError> {
                Uuid::parse_str(text.trim())
                    .map(Self)
                    .map_err(|_| DomainError::InvalidField {
                        field: $label.to_string(),
                        reason: "the value is not a UUID".to_string(),
                    })
            }

            /// The identifier as a plain UUID, which is how a grant addresses a
            /// target whose kind is known only at run time.
            pub fn raw(self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
    };
}

identifier!(UserId, "user_id");
identifier!(TeamId, "team_id");
identifier!(OrganizationId, "organization_id");
identifier!(MembershipId, "membership_id");
identifier!(InvitationId, "invitation_id");
identifier!(DeviceId, "device_id");
identifier!(ProjectId, "project_id");
identifier!(GrantId, "grant_id");
identifier!(DraftId, "draft_id");
identifier!(ConversationId, "conversation_id");
identifier!(MessageId, "message_id");

/// The source of the identifiers the application layer generates.
///
/// The domain holds no random source of its own, so a test supplies a source
/// that counts rather than one that draws.
pub trait IdSource: Send + Sync {
    /// One new identifier.
    fn next(&self) -> Uuid;
}

/// The source the running service uses: UUID version 4 (SAS-FR-OZET).
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomIds;

impl IdSource for RandomIds {
    fn next(&self) -> Uuid {
        Uuid::new_v4()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_reads_a_uuid_and_refuses_anything_else() {
        let text = "3f0b5b1e-2b8e-4a1a-9f0a-6a2c1c9a5b77";
        assert_eq!(
            UserId::parse(text).expect("a UUID is read").to_string(),
            text
        );
        assert!(matches!(
            UserId::parse("not-a-uuid"),
            Err(DomainError::InvalidField { .. })
        ));
    }

    // SAS-FR-OZET: the generated identifier is a UUID version 4.
    #[test]
    fn the_random_source_yields_version_four_identifiers() {
        let source = RandomIds;
        let first = source.next();
        assert_eq!(first.get_version_num(), 4);
        assert_ne!(first, source.next());
    }
}
