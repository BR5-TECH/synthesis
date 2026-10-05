//! The project and its access-control grants.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-OKUC, SAS-FR-CGTM, SAS-FR-QSLY, SAS-FR-MWOD.

use serde::{Deserialize, Serialize};

use crate::domain::clock::Timestamp;
use crate::domain::identity::TargetType;
use crate::domain::ids::{GrantId, ProjectId, UserId};
use crate::domain::permissions::{Permission, ProjectRole};

/// An application entity a worker may expose (SAS-FR-OKUC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    /// Presentation only. The relay never routes by this value (SRB-FR-EASV).
    pub display_name: String,
    pub owner_id: UserId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One access-control grant on one project (SAS-FR-CGTM).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub id: GrantId,
    pub project_id: ProjectId,
    pub target_type: TargetType,
    pub target_id: uuid::Uuid,
    pub role: ProjectRole,
    /// The permissions the role resolved to when the grant was written or last
    /// updated. They are stored explicitly, so a reader sees what the grant
    /// gives without resolving the role again.
    pub permissions: Vec<Permission>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// The revocation metadata of SAS-FR-EYUB. A revoked grant contributes no
    /// permission and stays readable until it is deleted.
    pub revoked_at: Option<Timestamp>,
    pub revoked_by: Option<UserId>,
}

impl Grant {
    /// Whether the grant still supplies its permissions (SAS-FR-ZDVR).
    pub fn is_effective(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Whether the grant is the protected owner grant (SAS-FR-MWOD).
    pub fn is_owner_grant(&self, owner_id: UserId) -> bool {
        matches!(self.role, ProjectRole::Owner)
            && self.target_type == TargetType::User
            && self.target_id == owner_id.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(role: ProjectRole, target: TargetType, target_id: uuid::Uuid) -> Grant {
        Grant {
            id: GrantId(uuid::Uuid::nil()),
            project_id: ProjectId(uuid::Uuid::nil()),
            target_type: target,
            target_id,
            role,
            permissions: role.permissions(),
            created_at: Timestamp("2025-01-01T00:00:00Z".to_string()),
            updated_at: Timestamp("2025-01-01T00:00:00Z".to_string()),
            revoked_at: None,
            revoked_by: None,
        }
    }

    // SAS-FR-EYUB: a revoked grant is ineffective at once and stays readable.
    #[test]
    fn a_revoked_grant_supplies_nothing_and_keeps_its_fields() {
        let mut record = grant(ProjectRole::Viewer, TargetType::User, uuid::Uuid::nil());
        assert!(record.is_effective());
        record.revoked_at = Some(Timestamp("2025-02-01T00:00:00Z".to_string()));
        assert!(!record.is_effective());
        assert_eq!(record.permissions, ProjectRole::Viewer.permissions());
    }

    // SAS-FR-MWOD: the owner grant is the owner's own grant with the owner role.
    #[test]
    fn the_owner_grant_is_recognised_by_its_target_and_its_role() {
        let owner = UserId(uuid::Uuid::from_u128(7));
        let other = UserId(uuid::Uuid::from_u128(8));
        assert!(grant(ProjectRole::Owner, TargetType::User, owner.raw()).is_owner_grant(owner));
        assert!(!grant(ProjectRole::Owner, TargetType::User, other.raw()).is_owner_grant(owner));
        assert!(
            !grant(ProjectRole::Administrator, TargetType::User, owner.raw()).is_owner_grant(owner)
        );
        assert!(!grant(ProjectRole::Owner, TargetType::Team, owner.raw()).is_owner_grant(owner));
    }
}
