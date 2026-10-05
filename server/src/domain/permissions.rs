//! The project permissions and the roles that bundle them.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-IJRO, SAS-FR-HXAP, SAS-FR-EPXN, SAS-FR-LEQB.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::error::DomainError;

/// One project permission (SAS-FR-IJRO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    #[serde(rename = "project.read")]
    ProjectRead,
    #[serde(rename = "project.update")]
    ProjectUpdate,
    #[serde(rename = "project.manage_acl")]
    ProjectManageAcl,
    #[serde(rename = "draft.read")]
    DraftRead,
    #[serde(rename = "draft.create")]
    DraftCreate,
    #[serde(rename = "draft.update")]
    DraftUpdate,
    #[serde(rename = "draft.archive")]
    DraftArchive,
    #[serde(rename = "draft.delete")]
    DraftDelete,
    #[serde(rename = "draft.graduate")]
    DraftGraduate,
    #[serde(rename = "conversation.read")]
    ConversationRead,
    #[serde(rename = "conversation.create")]
    ConversationCreate,
    #[serde(rename = "conversation.append")]
    ConversationAppend,
    #[serde(rename = "conversation.update")]
    ConversationUpdate,
    #[serde(rename = "conversation.resolve")]
    ConversationResolve,
    #[serde(rename = "conversation.lock")]
    ConversationLock,
}

impl Permission {
    /// Every permission of SAS-FR-IJRO, in the order the specification states.
    pub const ALL: [Permission; 15] = [
        Permission::ProjectRead,
        Permission::ProjectUpdate,
        Permission::ProjectManageAcl,
        Permission::DraftRead,
        Permission::DraftCreate,
        Permission::DraftUpdate,
        Permission::DraftArchive,
        Permission::DraftDelete,
        Permission::DraftGraduate,
        Permission::ConversationRead,
        Permission::ConversationCreate,
        Permission::ConversationAppend,
        Permission::ConversationUpdate,
        Permission::ConversationResolve,
        Permission::ConversationLock,
    ];

    /// The name the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::ProjectRead => "project.read",
            Permission::ProjectUpdate => "project.update",
            Permission::ProjectManageAcl => "project.manage_acl",
            Permission::DraftRead => "draft.read",
            Permission::DraftCreate => "draft.create",
            Permission::DraftUpdate => "draft.update",
            Permission::DraftArchive => "draft.archive",
            Permission::DraftDelete => "draft.delete",
            Permission::DraftGraduate => "draft.graduate",
            Permission::ConversationRead => "conversation.read",
            Permission::ConversationCreate => "conversation.create",
            Permission::ConversationAppend => "conversation.append",
            Permission::ConversationUpdate => "conversation.update",
            Permission::ConversationResolve => "conversation.resolve",
            Permission::ConversationLock => "conversation.lock",
        }
    }

    /// Reads a permission by name, and refuses one the specification does not
    /// name (SAS-FR-IJRO).
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        Permission::ALL
            .into_iter()
            .find(|permission| permission.as_str() == text)
            .ok_or_else(|| DomainError::InvalidPermission {
                permission: text.to_string(),
            })
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A project role (SAS-FR-HXAP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Owner,
    Administrator,
    Contributor,
    Viewer,
}

impl ProjectRole {
    /// Every project role.
    pub const ALL: [ProjectRole; 4] = [
        ProjectRole::Owner,
        ProjectRole::Administrator,
        ProjectRole::Contributor,
        ProjectRole::Viewer,
    ];

    /// The name the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectRole::Owner => "owner",
            ProjectRole::Administrator => "administrator",
            ProjectRole::Contributor => "contributor",
            ProjectRole::Viewer => "viewer",
        }
    }

    /// Reads a role by name and refuses an unknown one (SAS-FR-HXAP).
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        ProjectRole::ALL
            .into_iter()
            .find(|role| role.as_str() == text)
            .ok_or_else(|| DomainError::InvalidRole {
                role: text.to_string(),
            })
    }

    /// The permissions the role bundles (SAS-FR-HXAP).
    ///
    /// The bundles are nested: a contributor holds every viewer permission, and
    /// an administrator holds every contributor permission.
    pub fn permissions(self) -> Vec<Permission> {
        const VIEWER: [Permission; 3] = [
            Permission::ProjectRead,
            Permission::DraftRead,
            Permission::ConversationRead,
        ];
        const CONTRIBUTOR: [Permission; 7] = [
            Permission::DraftCreate,
            Permission::DraftUpdate,
            Permission::DraftArchive,
            Permission::ConversationCreate,
            Permission::ConversationAppend,
            Permission::ConversationUpdate,
            Permission::ConversationResolve,
        ];
        const ADMINISTRATOR: [Permission; 5] = [
            Permission::ProjectUpdate,
            Permission::ProjectManageAcl,
            Permission::DraftDelete,
            Permission::DraftGraduate,
            Permission::ConversationLock,
        ];

        let mut permissions: Vec<Permission> = match self {
            ProjectRole::Owner => Permission::ALL.to_vec(),
            ProjectRole::Administrator => VIEWER
                .into_iter()
                .chain(CONTRIBUTOR)
                .chain(ADMINISTRATOR)
                .collect(),
            ProjectRole::Contributor => VIEWER.into_iter().chain(CONTRIBUTOR).collect(),
            ProjectRole::Viewer => VIEWER.to_vec(),
        };
        permissions.sort();
        permissions.dedup();
        permissions
    }
}

impl fmt::Display for ProjectRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A team or organization membership role (SAS-FR-EPXN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Administrator,
    Member,
    Viewer,
}

impl MemberRole {
    /// Every membership role.
    pub const ALL: [MemberRole; 4] = [
        MemberRole::Owner,
        MemberRole::Administrator,
        MemberRole::Member,
        MemberRole::Viewer,
    ];

    /// The name the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberRole::Owner => "owner",
            MemberRole::Administrator => "administrator",
            MemberRole::Member => "member",
            MemberRole::Viewer => "viewer",
        }
    }

    /// Reads a role by name and refuses an unknown one (SAS-FR-EPXN).
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        MemberRole::ALL
            .into_iter()
            .find(|role| role.as_str() == text)
            .ok_or_else(|| DomainError::InvalidRole {
                role: text.to_string(),
            })
    }
}

impl fmt::Display for MemberRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAS-FR-IJRO: the fifteen permissions, and no other name.
    #[test]
    fn the_permission_set_is_the_one_the_specification_names() {
        let names: Vec<&str> = Permission::ALL.iter().map(|item| item.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "project.read",
                "project.update",
                "project.manage_acl",
                "draft.read",
                "draft.create",
                "draft.update",
                "draft.archive",
                "draft.delete",
                "draft.graduate",
                "conversation.read",
                "conversation.create",
                "conversation.append",
                "conversation.update",
                "conversation.resolve",
                "conversation.lock",
            ]
        );
        assert!(matches!(
            Permission::parse("project.destroy"),
            Err(DomainError::InvalidPermission { .. })
        ));
    }

    // SAS-FR-HXAP: the role-to-permission mapping the contract surface tabulates.
    #[test]
    fn the_role_bundles_match_the_published_mapping() {
        assert_eq!(ProjectRole::Owner.permissions().len(), 15);
        assert_eq!(ProjectRole::Viewer.permissions(), {
            let mut expected = vec![
                Permission::ProjectRead,
                Permission::DraftRead,
                Permission::ConversationRead,
            ];
            expected.sort();
            expected
        });
        assert_eq!(ProjectRole::Contributor.permissions().len(), 10);
        assert_eq!(ProjectRole::Administrator.permissions().len(), 15);
        // The bundles nest: every viewer permission is a contributor permission.
        for permission in ProjectRole::Viewer.permissions() {
            assert!(ProjectRole::Contributor.permissions().contains(&permission));
        }
        for permission in ProjectRole::Contributor.permissions() {
            assert!(ProjectRole::Administrator
                .permissions()
                .contains(&permission));
        }
    }

    // SAS-FR-HXAP, SAS-FR-EPXN: an unknown role is refused rather than ignored.
    #[test]
    fn an_unknown_role_is_refused() {
        assert!(matches!(
            ProjectRole::parse("editor"),
            Err(DomainError::InvalidRole { .. })
        ));
        assert!(matches!(
            MemberRole::parse("editor"),
            Err(DomainError::InvalidRole { .. })
        ));
        assert_eq!(
            MemberRole::parse("member").expect("a known role"),
            MemberRole::Member
        );
    }

    // The administrator project role differs from the owner role by nothing but
    // its protection: SAS-FR-MWOD guards the owner grant, not its permissions.
    #[test]
    fn the_owner_and_the_administrator_hold_the_same_permissions() {
        assert_eq!(
            ProjectRole::Owner.permissions(),
            ProjectRole::Administrator.permissions()
        );
    }
}
