//! The projects and their access-control grants.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-OKUC, SAS-FR-CGTM, SAS-FR-UPKA, SAS-FR-VNTC,
//! SAS-FR-MWOD, SAS-FR-RAKX, SAS-FR-EYUB, SAS-FR-DHLM, SAS-FR-AVDJ.

use crate::application::access::AccessService;
use crate::application::authority::Authority;
use crate::application::commands::{
    CreateGrant, CreateProject, Page, TransferOwnership, UpdateGrant, UpdateProject,
};
use crate::application::paginate;
use crate::application::ports::{Ports, Principal};
use crate::domain::error::DomainError;
use crate::domain::identity::TargetType;
use crate::domain::ids::{GrantId, ProjectId, UserId};
use crate::domain::permissions::{Permission, ProjectRole};
use crate::domain::project::{Grant, Project};
use crate::domain::validation;

/// The inbound port of the project and grant operations.
pub trait ProjectApi: Send + Sync {
    fn create_project(
        &self,
        principal: &Principal,
        command: CreateProject,
    ) -> Result<Project, DomainError>;
    fn read_project(&self, principal: &Principal, id: ProjectId) -> Result<Project, DomainError>;
    fn list_projects(&self, principal: &Principal, page: Page)
        -> Result<Vec<Project>, DomainError>;
    fn update_project(
        &self,
        principal: &Principal,
        id: ProjectId,
        command: UpdateProject,
    ) -> Result<Project, DomainError>;
    fn delete_project(
        &self,
        principal: &Principal,
        id: ProjectId,
        cascade: bool,
    ) -> Result<(), DomainError>;
    fn transfer_ownership(
        &self,
        principal: &Principal,
        id: ProjectId,
        command: TransferOwnership,
    ) -> Result<Project, DomainError>;
    fn effective_permissions(
        &self,
        principal: &Principal,
        id: ProjectId,
        user_id: Option<UserId>,
    ) -> Result<(UserId, Vec<Permission>), DomainError>;

    fn create_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateGrant,
    ) -> Result<Grant, DomainError>;
    fn read_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<Grant, DomainError>;
    fn list_grants(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Grant>, DomainError>;
    fn update_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
        command: UpdateGrant,
    ) -> Result<Grant, DomainError>;
    fn revoke_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<Grant, DomainError>;
    fn delete_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<(), DomainError>;
}

/// The project operations over the ports.
#[derive(Clone)]
pub struct ProjectService {
    ports: Ports,
    access: AccessService,
    authority: Authority,
}

impl ProjectService {
    /// The service over the given ports.
    pub fn new(ports: Ports) -> Self {
        ProjectService {
            access: AccessService::new(ports.clone()),
            authority: Authority::new(ports.clone()),
            ports,
        }
    }

    /// The grant, when it belongs to the project (SAS-FR-NKBC).
    fn grant_of(&self, project_id: ProjectId, id: GrantId) -> Result<Grant, DomainError> {
        let grant = self.ports.grants.get(id)?;
        if grant.project_id != project_id {
            return Err(DomainError::NotFound { resource: "grant" });
        }
        Ok(grant)
    }

    /// Writes the owner grant of a project (SAS-FR-MWOD).
    fn write_owner_grant(&self, project: &Project) -> Result<Grant, DomainError> {
        let id: GrantId = self.ports.resolve_id("id", None)?;
        let now = self.ports.clock.now();
        self.ports.grants.insert(
            id,
            Grant {
                id,
                project_id: project.id,
                target_type: TargetType::User,
                target_id: project.owner_id.raw(),
                role: ProjectRole::Owner,
                permissions: ProjectRole::Owner.permissions(),
                created_at: now.clone(),
                updated_at: now,
                revoked_at: None,
                revoked_by: None,
            },
        )
    }
}

impl ProjectApi for ProjectService {
    fn create_project(
        &self,
        principal: &Principal,
        command: CreateProject,
    ) -> Result<Project, DomainError> {
        let id: ProjectId = self.ports.resolve_id("id", command.id.as_deref())?;
        let display_name = validation::name("display_name", &command.display_name)?;
        let owner_id = match command.owner_id.as_deref() {
            None => principal.user_id,
            Some(text) => UserId::parse(text)?,
        };
        self.ports.users.get(owner_id)?;
        self.authority
            .require(principal, owner_id == principal.user_id, "project.create")?;

        let now = self.ports.clock.now();
        let project = self.ports.projects.insert(
            id,
            Project {
                id,
                display_name,
                owner_id,
                created_at: now.clone(),
                updated_at: now,
            },
        )?;

        // SAS-FR-MWOD: the owner is stored on the project and holds the owner
        // grant from the moment the project exists.
        self.write_owner_grant(&project)?;
        Ok(project)
    }

    fn read_project(&self, principal: &Principal, id: ProjectId) -> Result<Project, DomainError> {
        self.access
            .authorize(principal, id, Permission::ProjectRead)
    }

    fn list_projects(
        &self,
        principal: &Principal,
        page: Page,
    ) -> Result<Vec<Project>, DomainError> {
        let visible = self
            .ports
            .projects
            .list()
            .into_iter()
            .filter(|project| self.access.may_read(principal, project))
            .collect();
        paginate(visible, page)
    }

    fn update_project(
        &self,
        principal: &Principal,
        id: ProjectId,
        command: UpdateProject,
    ) -> Result<Project, DomainError> {
        self.access
            .authorize(principal, id, Permission::ProjectUpdate)?;

        let now = self.ports.clock.now();
        self.ports
            .projects
            .update(id, &mut |project: &mut Project| {
                if let Some(display_name) = command.display_name.as_deref() {
                    project.display_name = validation::name("display_name", display_name)?;
                }
                project.updated_at = now.clone();
                Ok(())
            })
    }

    fn delete_project(
        &self,
        principal: &Principal,
        id: ProjectId,
        cascade: bool,
    ) -> Result<(), DomainError> {
        // SAS-FR-DHLM: deleting a project is the owner's and the administrator's.
        let project = self
            .access
            .authorize(principal, id, Permission::ProjectRead)?;
        self.authority.require(
            principal,
            project.owner_id == principal.user_id,
            "project.delete",
        )?;

        let drafts: Vec<_> = self
            .ports
            .drafts
            .list()
            .into_iter()
            .filter(|draft| draft.project_id == id)
            .collect();
        let conversations: Vec<_> = self
            .ports
            .conversations
            .list()
            .into_iter()
            .filter(|conversation| conversation.project_id == id)
            .collect();

        // SAS-FR-AVDJ: the drafts and the conversations leave with the project
        // when the request asks for it, and hold it back when it does not.
        let holds_content = !drafts.is_empty() || !conversations.is_empty();
        if holds_content && !cascade {
            return Err(DomainError::Referenced {
                reason: "the project holds a draft or a conversation".to_string(),
            });
        }
        for draft in drafts {
            self.ports.drafts.remove(draft.id)?;
        }
        for conversation in conversations {
            self.ports.conversations.remove(conversation.id)?;
        }
        for grant in self.ports.grants.list() {
            if grant.project_id == id {
                self.ports.grants.remove(grant.id)?;
            }
        }
        self.ports.projects.remove(id).map(|_| ())
    }

    fn transfer_ownership(
        &self,
        principal: &Principal,
        id: ProjectId,
        command: TransferOwnership,
    ) -> Result<Project, DomainError> {
        let project = self
            .access
            .authorize(principal, id, Permission::ProjectRead)?;
        self.authority.require(
            principal,
            project.owner_id == principal.user_id,
            "project.transfer_ownership",
        )?;

        let new_owner = UserId::parse(&command.new_owner_id)?;
        self.ports.users.get(new_owner)?;
        let previous_owner = project.owner_id;

        let now = self.ports.clock.now();
        let moved = self
            .ports
            .projects
            .update(id, &mut |project: &mut Project| {
                project.owner_id = new_owner;
                project.updated_at = now.clone();
                Ok(())
            })?;

        // SAS-FR-RAKX: the owner grant moves with the ownership. The former
        // owner keeps no permission that no other grant supplies.
        let existing = self
            .ports
            .grants
            .list()
            .into_iter()
            .find(|grant| grant.project_id == id && grant.is_owner_grant(previous_owner));
        match existing {
            Some(grant) => {
                self.ports
                    .grants
                    .update(grant.id, &mut |grant: &mut Grant| {
                        grant.target_id = new_owner.raw();
                        grant.updated_at = now.clone();
                        Ok(())
                    })?;
            }
            None => {
                self.write_owner_grant(&moved)?;
            }
        }
        Ok(moved)
    }

    fn effective_permissions(
        &self,
        principal: &Principal,
        id: ProjectId,
        user_id: Option<UserId>,
    ) -> Result<(UserId, Vec<Permission>), DomainError> {
        let subject = user_id.unwrap_or(principal.user_id);
        let needed = if subject == principal.user_id {
            Permission::ProjectRead
        } else {
            Permission::ProjectManageAcl
        };
        let project = self.access.authorize(principal, id, needed)?;
        self.ports.users.get(subject)?;

        let permissions = if self
            .ports
            .users
            .get(subject)
            .is_ok_and(|user| user.is_administrator())
        {
            ProjectRole::Owner.permissions()
        } else {
            self.access.effective_permissions(subject, &project)
        };
        Ok((subject, permissions))
    }

    fn create_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateGrant,
    ) -> Result<Grant, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;

        let id: GrantId = self.ports.resolve_id("id", command.id.as_deref())?;
        let target_type = TargetType::parse(&command.target_type)?;
        let target_id = uuid::Uuid::parse_str(command.target_id.trim())
            .map_err(|_| DomainError::invalid_field("target_id", "the value is not a UUID"))?;
        let role = ProjectRole::parse(&command.role)?;
        if matches!(role, ProjectRole::Owner) {
            // SAS-FR-RAKX: the owner role is reached by a transfer of ownership.
            return Err(DomainError::OwnerProtected);
        }
        self.access.check_target_exists(target_type, target_id)?;

        // SAS-FR-VNTC: one project, one target, and one role hold one grant.
        let duplicate = self.ports.grants.list().into_iter().any(|grant| {
            grant.project_id == project_id
                && grant.target_type == target_type
                && grant.target_id == target_id
                && grant.role == role
        });
        if duplicate {
            return Err(DomainError::DuplicateGrant);
        }

        let now = self.ports.clock.now();
        self.ports.grants.insert(
            id,
            Grant {
                id,
                project_id,
                target_type,
                target_id,
                role,
                permissions: role.permissions(),
                created_at: now.clone(),
                updated_at: now,
                revoked_at: None,
                revoked_by: None,
            },
        )
    }

    fn read_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<Grant, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;
        self.grant_of(project_id, id)
    }

    fn list_grants(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Grant>, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;
        let grants = self
            .ports
            .grants
            .list()
            .into_iter()
            .filter(|grant| grant.project_id == project_id)
            .collect();
        paginate(grants, page)
    }

    fn update_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
        command: UpdateGrant,
    ) -> Result<Grant, DomainError> {
        let project = self
            .access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;
        let grant = self.grant_of(project_id, id)?;
        // SAS-FR-MWOD: the owner grant is neither reduced nor raised here.
        if grant.is_owner_grant(project.owner_id) {
            return Err(DomainError::OwnerProtected);
        }
        let role = ProjectRole::parse(&command.role)?;
        if matches!(role, ProjectRole::Owner) {
            return Err(DomainError::OwnerProtected);
        }

        // SAS-FR-VNTC: a replacement is an update of this grant, and it never
        // makes a second record with the same target and role.
        let duplicate = self.ports.grants.list().into_iter().any(|held| {
            held.id != id
                && held.project_id == project_id
                && held.target_type == grant.target_type
                && held.target_id == grant.target_id
                && held.role == role
        });
        if duplicate {
            return Err(DomainError::DuplicateGrant);
        }

        let now = self.ports.clock.now();
        self.ports.grants.update(id, &mut |grant: &mut Grant| {
            grant.role = role;
            grant.permissions = role.permissions();
            grant.updated_at = now.clone();
            Ok(())
        })
    }

    fn revoke_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<Grant, DomainError> {
        let project = self
            .access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;
        let grant = self.grant_of(project_id, id)?;
        if grant.is_owner_grant(project.owner_id) {
            return Err(DomainError::OwnerProtected);
        }

        let now = self.ports.clock.now();
        let actor = principal.user_id;
        self.ports.grants.update(id, &mut |grant: &mut Grant| {
            grant.revoked_at = Some(now.clone());
            grant.revoked_by = Some(actor);
            grant.updated_at = now.clone();
            Ok(())
        })
    }

    fn delete_grant(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        id: GrantId,
    ) -> Result<(), DomainError> {
        let project = self
            .access
            .authorize(principal, project_id, Permission::ProjectManageAcl)?;
        let grant = self.grant_of(project_id, id)?;
        if grant.is_owner_grant(project.owner_id) {
            return Err(DomainError::OwnerProtected);
        }
        self.ports.grants.remove(id).map(|_| ())
    }
}
