//! The project access-control rules.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-ZDVR, SAS-FR-BJHF, SAS-FR-QSLY, SAS-FR-MWOD,
//! SAS-FR-TKPZ, SAS-FR-UPKA, SAS-FR-XRPD.

use crate::application::ports::{Ports, Principal};
use crate::domain::error::DomainError;
use crate::domain::identity::TargetType;
use crate::domain::ids::{ProjectId, UserId};
use crate::domain::permissions::{Permission, ProjectRole};
use crate::domain::project::Project;

/// The evaluator of the effective permissions.
#[derive(Clone)]
pub struct AccessService {
    ports: Ports,
}

impl AccessService {
    /// An evaluator over the given ports.
    pub fn new(ports: Ports) -> Self {
        AccessService { ports }
    }

    /// The effective permissions of a user on a project (SAS-FR-ZDVR).
    ///
    /// The result is the union of the grants to the user, of the grants to each
    /// team of which the user holds an active membership, and of the grants to
    /// each organization of which the user holds an active membership. A
    /// revoked grant contributes nothing, the owner holds every permission, and
    /// there is no deny grant (SAS-FR-QSLY).
    pub fn effective_permissions(&self, user_id: UserId, project: &Project) -> Vec<Permission> {
        if project.owner_id == user_id {
            return ProjectRole::Owner.permissions();
        }

        // SAS-FR-BJHF: membership is read now, and a suspended membership
        // contributes nothing. There is no nested membership in V1: a team
        // membership grants nothing through an organization.
        let mut teams = Vec::new();
        let mut organizations = Vec::new();
        for membership in self.ports.memberships.list() {
            if membership.member_id != user_id || !membership.is_active() {
                continue;
            }
            match membership.target_type {
                TargetType::Team => teams.push(membership.target_id),
                TargetType::Organization => organizations.push(membership.target_id),
                TargetType::User => {}
            }
        }

        let mut permissions = Vec::new();
        for grant in self.ports.grants.list() {
            if grant.project_id != project.id || !grant.is_effective() {
                continue;
            }
            let applies = match grant.target_type {
                TargetType::User => grant.target_id == user_id.raw(),
                TargetType::Team => teams.contains(&grant.target_id),
                TargetType::Organization => organizations.contains(&grant.target_id),
            };
            if applies {
                permissions.extend(grant.permissions.iter().copied());
            }
        }

        permissions.sort();
        permissions.dedup();
        permissions
    }

    /// The project, when the principal may read it and holds the permission
    /// (SAS-FR-TKPZ).
    ///
    /// A principal that may not read the project is answered `not_found`, so
    /// the existence of a project is not disclosed. The administrator bypasses
    /// both checks (SAS-FR-XRPD).
    pub fn authorize(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        permission: Permission,
    ) -> Result<Project, DomainError> {
        let project = self.ports.projects.get(project_id)?;
        if principal.is_administrator {
            return Ok(project);
        }

        let held = self.effective_permissions(principal.user_id, &project);
        if !held.contains(&Permission::ProjectRead) {
            return Err(DomainError::NotFound {
                resource: "project",
            });
        }
        if !held.contains(&permission) {
            return Err(DomainError::Forbidden {
                permission: permission.as_str().to_string(),
            });
        }
        Ok(project)
    }

    /// Whether the principal may read the project at all, used by a list route
    /// that hides what the principal may not read.
    pub fn may_read(&self, principal: &Principal, project: &Project) -> bool {
        principal.is_administrator
            || self
                .effective_permissions(principal.user_id, project)
                .contains(&Permission::ProjectRead)
    }

    /// Refuses a grant whose target does not exist (SAS-FR-UPKA).
    pub fn check_target_exists(
        &self,
        target_type: TargetType,
        target_id: uuid::Uuid,
    ) -> Result<(), DomainError> {
        let exists = match target_type {
            TargetType::User => self.ports.users.find(UserId(target_id)).is_some(),
            TargetType::Team => self
                .ports
                .teams
                .find(crate::domain::ids::TeamId(target_id))
                .is_some(),
            TargetType::Organization => self
                .ports
                .organizations
                .find(crate::domain::ids::OrganizationId(target_id))
                .is_some(),
        };
        if exists {
            Ok(())
        } else {
            Err(DomainError::NotFound {
                resource: match target_type {
                    TargetType::User => "user",
                    TargetType::Team => "team",
                    TargetType::Organization => "organization",
                },
            })
        }
    }
}
