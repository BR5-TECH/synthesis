//! The answers the application routes write.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-LEQB, SAS-FR-ZDVR.

use serde::Serialize;

use crate::domain::ids::UserId;
use crate::domain::permissions::{MemberRole, Permission, ProjectRole};

/// A list answer.
#[derive(Debug, Serialize)]
pub struct ListBody<T> {
    pub items: Vec<T>,
}

impl<T> ListBody<T> {
    /// The answer of a list route.
    pub fn new(items: Vec<T>) -> Self {
        ListBody { items }
    }
}

/// One role and the permissions it bundles (SAS-FR-LEQB).
#[derive(Debug, Serialize)]
pub struct RoleBundle {
    pub role: &'static str,
    pub permissions: Vec<&'static str>,
}

/// The published role-to-permission mapping (SAS-FR-LEQB).
#[derive(Debug, Serialize)]
pub struct RolesBody {
    pub permissions: Vec<&'static str>,
    pub project_roles: Vec<RoleBundle>,
    pub team_roles: Vec<&'static str>,
    pub organization_roles: Vec<&'static str>,
}

impl RolesBody {
    /// The mapping the service publishes.
    pub fn current() -> Self {
        RolesBody {
            permissions: Permission::ALL.iter().map(|item| item.as_str()).collect(),
            project_roles: ProjectRole::ALL
                .iter()
                .map(|role| RoleBundle {
                    role: role.as_str(),
                    permissions: role
                        .permissions()
                        .into_iter()
                        .map(|permission| permission.as_str())
                        .collect(),
                })
                .collect(),
            team_roles: MemberRole::ALL.iter().map(|role| role.as_str()).collect(),
            organization_roles: MemberRole::ALL.iter().map(|role| role.as_str()).collect(),
        }
    }
}

/// The effective permissions of one user on one project (SAS-FR-ZDVR).
#[derive(Debug, Serialize)]
pub struct EffectivePermissionsBody {
    pub user_id: UserId,
    pub permissions: Vec<&'static str>,
}

impl EffectivePermissionsBody {
    /// The answer of the effective-permissions route.
    pub fn new(user_id: UserId, permissions: Vec<Permission>) -> Self {
        EffectivePermissionsBody {
            user_id,
            permissions: permissions
                .into_iter()
                .map(|permission| permission.as_str())
                .collect(),
        }
    }
}
