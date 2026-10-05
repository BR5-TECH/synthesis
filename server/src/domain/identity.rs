//! The identity entities: users, teams, organizations, memberships,
//! invitations, and devices.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-KDSM, SAS-FR-GVAB, SAS-FR-RHYT, SAS-FR-MZQF,
//! SAS-FR-YCWK, SAS-FR-FQVS, SAS-FR-TWEL.

use serde::{Deserialize, Serialize};

use crate::domain::clock::Timestamp;
use crate::domain::error::DomainError;
use crate::domain::ids::{DeviceId, InvitationId, MembershipId, OrganizationId, TeamId, UserId};
use crate::domain::permissions::MemberRole;

/// The role a user holds on the service itself (SAS-FR-KDSM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    /// The one administrator of SAS-FR-DTXV, which bypasses every check.
    Administrator,
    /// Every other user.
    Member,
}

/// Whether a record still takes part (SAS-FR-KDSM, SAS-FR-MZQF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Suspended,
}

/// The mapping a later OIDC mode writes onto a user (SAS-FR-TWEL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityClaim {
    /// The issuer that asserted the subject.
    pub issuer: String,
    /// The subject the issuer asserted.
    pub subject: String,
}

/// A person who uses the IDE and the remote client (SAS-FR-KDSM).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    /// The `(issuer, subject)` pair, when one is mapped. The email is a profile
    /// attribute and is never the identity key (SAS-FR-TWEL).
    pub identity: Option<IdentityClaim>,
    pub display_name: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub status: Status,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl User {
    /// Whether the user bypasses every access-control check (SAS-FR-XRPD).
    pub fn is_administrator(&self) -> bool {
        matches!(self.role, UserRole::Administrator)
    }
}

/// A group of users (SAS-FR-GVAB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    pub owner_id: UserId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A group of teams and users (SAS-FR-RHYT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    pub id: OrganizationId,
    pub name: String,
    pub owner_id: UserId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// What a membership, an invitation, or a grant addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    User,
    Team,
    Organization,
}

impl TargetType {
    /// The name the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            TargetType::User => "user",
            TargetType::Team => "team",
            TargetType::Organization => "organization",
        }
    }

    /// Reads a target type by name.
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        match text {
            "user" => Ok(TargetType::User),
            "team" => Ok(TargetType::Team),
            "organization" => Ok(TargetType::Organization),
            other => Err(DomainError::invalid_field(
                "target_type",
                &format!("the value \"{other}\" is not user, team, or organization"),
            )),
        }
    }

    /// The target types a membership addresses: a user is a member, never a
    /// target (SAS-FR-MZQF).
    pub fn parse_membership_target(text: &str) -> Result<Self, DomainError> {
        match TargetType::parse(text)? {
            TargetType::User => Err(DomainError::invalid_field(
                "target_type",
                "a membership addresses a team or an organization",
            )),
            other => Ok(other),
        }
    }
}

/// The explicit relation between a user and a team or an organization
/// (SAS-FR-MZQF).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Membership {
    pub id: MembershipId,
    pub member_id: UserId,
    pub target_type: TargetType,
    pub target_id: uuid::Uuid,
    pub role: MemberRole,
    pub status: Status,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Membership {
    /// Whether the membership counts when a permission is evaluated
    /// (SAS-FR-BJHF).
    pub fn is_active(&self) -> bool {
        matches!(self.status, Status::Active)
    }
}

/// The state of an invitation (SAS-FR-YCWK).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationState {
    Pending,
    Accepted,
    Revoked,
}

/// A pending request to add a user to a team or an organization
/// (SAS-FR-YCWK).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub id: InvitationId,
    pub target_type: TargetType,
    pub target_id: uuid::Uuid,
    pub inviter_id: UserId,
    /// The invitee, named by user identifier or by email address.
    pub invitee_user_id: Option<UserId>,
    pub invitee_email: Option<String>,
    pub role: MemberRole,
    pub expires_at: Timestamp,
    pub state: InvitationState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Invitation {
    /// Whether the invitation may still be accepted at `now` (SAS-FR-ATLB).
    ///
    /// The times are RFC 3339 at UTC, which sorts as text in the same order as
    /// it sorts in time.
    pub fn is_acceptable(&self, now: &Timestamp) -> bool {
        matches!(self.state, InvitationState::Pending) && now.as_str() < self.expires_at.as_str()
    }
}

/// What a device is (SAS-FR-FQVS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    /// A registered IDE installation.
    Ide,
    /// A registered remote-client installation.
    Client,
}

impl DeviceType {
    /// Reads a device type by name.
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        match text {
            "ide" => Ok(DeviceType::Ide),
            "client" => Ok(DeviceType::Client),
            other => Err(DomainError::invalid_field(
                "device_type",
                &format!("the value \"{other}\" is not ide or client"),
            )),
        }
    }

    /// The name the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceType::Ide => "ide",
            DeviceType::Client => "client",
        }
    }
}

/// A registered IDE or remote-client installation (SAS-FR-FQVS).
///
/// The identifier is stable and is separate from the worker `instance_id` and
/// from the opaque client handle, which both live and die with a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub owner_id: UserId,
    pub device_type: DeviceType,
    pub display_name: String,
    pub created_at: Timestamp,
    pub last_seen_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

impl Device {
    /// Whether the device may still authenticate.
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(text: &str) -> Timestamp {
        Timestamp(text.to_string())
    }

    // SAS-FR-MZQF: a membership addresses a team or an organization, never a user.
    #[test]
    fn a_membership_target_is_a_team_or_an_organization() {
        assert_eq!(
            TargetType::parse_membership_target("team").expect("a team is a target"),
            TargetType::Team
        );
        assert!(TargetType::parse_membership_target("user").is_err());
        assert!(TargetType::parse_membership_target("cluster").is_err());
    }

    // SAS-FR-ATLB: an invitation is acceptable while it is pending and before it expires.
    #[test]
    fn an_invitation_is_acceptable_while_pending_and_unexpired() {
        let mut invitation = Invitation {
            id: InvitationId(uuid::Uuid::nil()),
            target_type: TargetType::Team,
            target_id: uuid::Uuid::nil(),
            inviter_id: UserId(uuid::Uuid::nil()),
            invitee_user_id: None,
            invitee_email: Some("person@example.com".to_string()),
            role: MemberRole::Member,
            expires_at: stamp("2030-01-01T00:00:00Z"),
            state: InvitationState::Pending,
            created_at: stamp("2020-01-01T00:00:00Z"),
            updated_at: stamp("2020-01-01T00:00:00Z"),
        };

        assert!(invitation.is_acceptable(&stamp("2025-01-01T00:00:00Z")));
        assert!(!invitation.is_acceptable(&stamp("2031-01-01T00:00:00Z")));

        invitation.state = InvitationState::Revoked;
        assert!(!invitation.is_acceptable(&stamp("2025-01-01T00:00:00Z")));
        invitation.state = InvitationState::Accepted;
        assert!(!invitation.is_acceptable(&stamp("2025-01-01T00:00:00Z")));
    }
}
