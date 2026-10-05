//! The permission checks of the entities that hold no project ACL.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-XRPD, SAS-FR-EPXN, SAS-FR-TKPZ.
//!
//! A team, an organization, a membership, an invitation, and a device carry no
//! project grant. Who may change one is read from ownership and from the
//! membership role instead. The administrator bypasses every check here, which
//! is the one authorization exception of V1.

use crate::application::ports::{Ports, Principal};
use crate::domain::error::DomainError;
use crate::domain::identity::TargetType;
use crate::domain::ids::{OrganizationId, TeamId, UserId};
use crate::domain::permissions::MemberRole;

/// The checks over ownership and membership role.
#[derive(Clone)]
pub struct Authority {
    ports: Ports,
}

impl Authority {
    /// The checks over the given ports.
    pub fn new(ports: Ports) -> Self {
        Authority { ports }
    }

    /// Refuses when the principal is neither the administrator nor allowed.
    pub fn require(
        &self,
        principal: &Principal,
        allowed: bool,
        action: &str,
    ) -> Result<(), DomainError> {
        if principal.is_administrator || allowed {
            Ok(())
        } else {
            Err(DomainError::Forbidden {
                permission: action.to_string(),
            })
        }
    }

    /// Whether the user owns the target, or holds an active membership of it
    /// with the `owner` or the `administrator` role.
    pub fn manages(&self, user_id: UserId, target_type: TargetType, target_id: uuid::Uuid) -> bool {
        if self.owns(user_id, target_type, target_id) {
            return true;
        }
        self.membership_role(user_id, target_type, target_id)
            .is_some_and(|role| matches!(role, MemberRole::Owner | MemberRole::Administrator))
    }

    /// Whether the user owns the target record.
    pub fn owns(&self, user_id: UserId, target_type: TargetType, target_id: uuid::Uuid) -> bool {
        match target_type {
            TargetType::User => user_id.raw() == target_id,
            TargetType::Team => self
                .ports
                .teams
                .find(TeamId(target_id))
                .is_some_and(|team| team.owner_id == user_id),
            TargetType::Organization => self
                .ports
                .organizations
                .find(OrganizationId(target_id))
                .is_some_and(|organization| organization.owner_id == user_id),
        }
    }

    /// Whether the user owns the target or holds any active membership of it.
    pub fn belongs(&self, user_id: UserId, target_type: TargetType, target_id: uuid::Uuid) -> bool {
        self.owns(user_id, target_type, target_id)
            || self
                .membership_role(user_id, target_type, target_id)
                .is_some()
    }

    /// The role of the user's active membership of the target, when it holds one.
    pub fn membership_role(
        &self,
        user_id: UserId,
        target_type: TargetType,
        target_id: uuid::Uuid,
    ) -> Option<MemberRole> {
        self.ports
            .memberships
            .list()
            .into_iter()
            .find(|membership| {
                membership.member_id == user_id
                    && membership.target_type == target_type
                    && membership.target_id == target_id
                    && membership.is_active()
            })
            .map(|membership| membership.role)
    }
}
