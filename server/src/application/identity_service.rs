//! The users, the teams, and the organizations.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-KDSM, SAS-FR-GVAB, SAS-FR-RHYT, SAS-FR-TWEL,
//! SAS-FR-OZET, SAS-FR-SGXQ, SAS-FR-TQIM.

use crate::application::authority::Authority;
use crate::application::commands::{CreateGroup, CreateUser, Page, UpdateGroup, UpdateUser};
use crate::application::paginate;
use crate::application::ports::{Ports, Principal};
use crate::domain::error::DomainError;
use crate::domain::identity::{
    IdentityClaim, Organization, Status, TargetType, Team, User, UserRole,
};
use crate::domain::ids::{OrganizationId, TeamId, UserId};
use crate::domain::validation;

/// The inbound port of the identity operations.
pub trait IdentityApi: Send + Sync {
    fn create_user(&self, principal: &Principal, command: CreateUser) -> Result<User, DomainError>;
    fn read_user(&self, principal: &Principal, id: UserId) -> Result<User, DomainError>;
    fn list_users(&self, principal: &Principal, page: Page) -> Result<Vec<User>, DomainError>;
    fn update_user(
        &self,
        principal: &Principal,
        id: UserId,
        command: UpdateUser,
    ) -> Result<User, DomainError>;
    fn delete_user(&self, principal: &Principal, id: UserId) -> Result<(), DomainError>;

    fn create_team(&self, principal: &Principal, command: CreateGroup)
        -> Result<Team, DomainError>;
    fn read_team(&self, principal: &Principal, id: TeamId) -> Result<Team, DomainError>;
    fn list_teams(&self, principal: &Principal, page: Page) -> Result<Vec<Team>, DomainError>;
    fn update_team(
        &self,
        principal: &Principal,
        id: TeamId,
        command: UpdateGroup,
    ) -> Result<Team, DomainError>;
    fn delete_team(&self, principal: &Principal, id: TeamId) -> Result<(), DomainError>;

    fn create_organization(
        &self,
        principal: &Principal,
        command: CreateGroup,
    ) -> Result<Organization, DomainError>;
    fn read_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
    ) -> Result<Organization, DomainError>;
    fn list_organizations(
        &self,
        principal: &Principal,
        page: Page,
    ) -> Result<Vec<Organization>, DomainError>;
    fn update_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
        command: UpdateGroup,
    ) -> Result<Organization, DomainError>;
    fn delete_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
    ) -> Result<(), DomainError>;
}

/// The identity operations over the ports.
#[derive(Clone)]
pub struct IdentityService {
    ports: Ports,
    authority: Authority,
}

impl IdentityService {
    /// The service over the given ports.
    pub fn new(ports: Ports) -> Self {
        IdentityService {
            authority: Authority::new(ports.clone()),
            ports,
        }
    }

    /// The owner a create request names, which defaults to the caller.
    fn owner_of(
        &self,
        principal: &Principal,
        named: Option<&String>,
    ) -> Result<UserId, DomainError> {
        match named {
            None => Ok(principal.user_id),
            Some(text) => {
                let owner = UserId::parse(text)?;
                self.ports.users.get(owner)?;
                Ok(owner)
            }
        }
    }

    /// Refuses a claim another user already holds (SAS-FR-TWEL).
    fn check_claim_is_free(
        &self,
        claim: &IdentityClaim,
        except: Option<UserId>,
    ) -> Result<(), DomainError> {
        let taken = self.ports.users.list().into_iter().any(|user| {
            Some(user.id) != except && user.identity.as_ref().is_some_and(|held| held == claim)
        });
        if taken {
            Err(DomainError::invalid_field(
                "issuer",
                "another user holds that issuer and subject",
            ))
        } else {
            Ok(())
        }
    }
}

impl IdentityApi for IdentityService {
    fn create_user(&self, principal: &Principal, command: CreateUser) -> Result<User, DomainError> {
        self.authority.require(principal, false, "user.create")?;

        let id: UserId = self.ports.resolve_id("id", command.id.as_deref())?;
        let display_name = validation::name("display_name", &command.display_name)?;
        let email = command
            .email
            .as_deref()
            .map(|value| validation::email("email", value))
            .transpose()?;
        let role = match command.role.as_deref() {
            None | Some("member") => UserRole::Member,
            Some("administrator") => UserRole::Administrator,
            Some(other) => {
                return Err(DomainError::InvalidRole {
                    role: other.to_string(),
                })
            }
        };
        let identity = match (command.issuer.as_deref(), command.subject.as_deref()) {
            (None, None) => None,
            (Some(issuer), Some(subject)) => Some(IdentityClaim {
                issuer: validation::name("issuer", issuer)?,
                subject: validation::name("subject", subject)?,
            }),
            _ => {
                return Err(DomainError::invalid_field(
                    "issuer",
                    "an identity claim carries both an issuer and a subject",
                ))
            }
        };
        if let Some(claim) = identity.as_ref() {
            self.check_claim_is_free(claim, None)?;
        }

        let now = self.ports.clock.now();
        self.ports.users.insert(
            id,
            User {
                id,
                identity,
                display_name,
                email,
                role,
                status: Status::Active,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_user(&self, principal: &Principal, id: UserId) -> Result<User, DomainError> {
        let user = self.ports.users.get(id)?;
        self.authority
            .require(principal, principal.user_id == id, "user.read")?;
        Ok(user)
    }

    fn list_users(&self, principal: &Principal, page: Page) -> Result<Vec<User>, DomainError> {
        let all = self.ports.users.list();
        let visible = if principal.is_administrator {
            all
        } else {
            all.into_iter()
                .filter(|user| user.id == principal.user_id)
                .collect()
        };
        paginate(visible, page)
    }

    fn update_user(
        &self,
        principal: &Principal,
        id: UserId,
        command: UpdateUser,
    ) -> Result<User, DomainError> {
        self.ports.users.get(id)?;
        self.authority
            .require(principal, principal.user_id == id, "user.update")?;
        // The role and the status are the administrator's to change: a user
        // does not raise its own role.
        if (command.role.is_some() || command.status.is_some()) && !principal.is_administrator {
            return Err(DomainError::Forbidden {
                permission: "user.update_role".to_string(),
            });
        }

        let now = self.ports.clock.now();
        self.ports.users.update(id, &mut |user: &mut User| {
            if let Some(display_name) = command.display_name.as_deref() {
                user.display_name = validation::name("display_name", display_name)?;
            }
            if let Some(email) = command.email.as_deref() {
                user.email = Some(validation::email("email", email)?);
            }
            if let Some(role) = command.role.as_deref() {
                user.role = match role {
                    "member" => UserRole::Member,
                    "administrator" => UserRole::Administrator,
                    other => {
                        return Err(DomainError::InvalidRole {
                            role: other.to_string(),
                        })
                    }
                };
            }
            if let Some(status) = command.status.as_deref() {
                user.status = match status {
                    "active" => Status::Active,
                    "suspended" => Status::Suspended,
                    other => {
                        return Err(DomainError::invalid_field(
                            "status",
                            &format!("the value \"{other}\" is not active or suspended"),
                        ))
                    }
                };
            }
            user.updated_at = now.clone();
            Ok(())
        })
    }

    fn delete_user(&self, principal: &Principal, id: UserId) -> Result<(), DomainError> {
        self.ports.users.get(id)?;
        self.authority.require(principal, false, "user.delete")?;

        // SAS-FR-SGXQ: a user that owns a team, an organization, or a project
        // is not deleted while the reference stands.
        if self
            .ports
            .teams
            .list()
            .iter()
            .any(|team| team.owner_id == id)
        {
            return Err(DomainError::Referenced {
                reason: "the user owns a team".to_string(),
            });
        }
        if self
            .ports
            .organizations
            .list()
            .iter()
            .any(|organization| organization.owner_id == id)
        {
            return Err(DomainError::Referenced {
                reason: "the user owns an organization".to_string(),
            });
        }
        if self
            .ports
            .projects
            .list()
            .iter()
            .any(|project| project.owner_id == id)
        {
            return Err(DomainError::Referenced {
                reason: "the user owns a project".to_string(),
            });
        }

        self.ports.users.remove(id).map(|_| ())
    }

    fn create_team(
        &self,
        principal: &Principal,
        command: CreateGroup,
    ) -> Result<Team, DomainError> {
        let id: TeamId = self.ports.resolve_id("id", command.id.as_deref())?;
        let name = validation::name("name", &command.name)?;
        let owner_id = self.owner_of(principal, command.owner_id.as_ref())?;
        self.authority
            .require(principal, owner_id == principal.user_id, "team.create")?;

        let now = self.ports.clock.now();
        self.ports.teams.insert(
            id,
            Team {
                id,
                name,
                owner_id,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_team(&self, principal: &Principal, id: TeamId) -> Result<Team, DomainError> {
        let team = self.ports.teams.get(id)?;
        self.authority.require(
            principal,
            self.authority
                .belongs(principal.user_id, TargetType::Team, id.raw()),
            "team.read",
        )?;
        Ok(team)
    }

    fn list_teams(&self, principal: &Principal, page: Page) -> Result<Vec<Team>, DomainError> {
        let visible = self
            .ports
            .teams
            .list()
            .into_iter()
            .filter(|team| {
                principal.is_administrator
                    || self
                        .authority
                        .belongs(principal.user_id, TargetType::Team, team.id.raw())
            })
            .collect();
        paginate(visible, page)
    }

    fn update_team(
        &self,
        principal: &Principal,
        id: TeamId,
        command: UpdateGroup,
    ) -> Result<Team, DomainError> {
        self.ports.teams.get(id)?;
        self.authority.require(
            principal,
            self.authority
                .manages(principal.user_id, TargetType::Team, id.raw()),
            "team.update",
        )?;
        let owner_id = match command.owner_id.as_ref() {
            None => None,
            Some(text) => Some(self.owner_of(principal, Some(text))?),
        };

        let now = self.ports.clock.now();
        self.ports.teams.update(id, &mut |team: &mut Team| {
            if let Some(name) = command.name.as_deref() {
                team.name = validation::name("name", name)?;
            }
            if let Some(owner) = owner_id {
                team.owner_id = owner;
            }
            team.updated_at = now.clone();
            Ok(())
        })
    }

    fn delete_team(&self, principal: &Principal, id: TeamId) -> Result<(), DomainError> {
        let team = self.ports.teams.get(id)?;
        self.authority
            .require(principal, team.owner_id == principal.user_id, "team.delete")?;

        // SAS-FR-SGXQ: a team a grant addresses is not deleted.
        if self
            .ports
            .grants
            .list()
            .iter()
            .any(|grant| grant.target_type == TargetType::Team && grant.target_id == id.raw())
        {
            return Err(DomainError::Referenced {
                reason: "a project grant addresses the team".to_string(),
            });
        }

        // The memberships of a deleted team hold no target, so they leave with it.
        for membership in self.ports.memberships.list() {
            if membership.target_type == TargetType::Team && membership.target_id == id.raw() {
                self.ports.memberships.remove(membership.id)?;
            }
        }
        self.ports.teams.remove(id).map(|_| ())
    }

    fn create_organization(
        &self,
        principal: &Principal,
        command: CreateGroup,
    ) -> Result<Organization, DomainError> {
        let id: OrganizationId = self.ports.resolve_id("id", command.id.as_deref())?;
        let name = validation::name("name", &command.name)?;
        let owner_id = self.owner_of(principal, command.owner_id.as_ref())?;
        self.authority.require(
            principal,
            owner_id == principal.user_id,
            "organization.create",
        )?;

        let now = self.ports.clock.now();
        self.ports.organizations.insert(
            id,
            Organization {
                id,
                name,
                owner_id,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
    ) -> Result<Organization, DomainError> {
        let organization = self.ports.organizations.get(id)?;
        self.authority.require(
            principal,
            self.authority
                .belongs(principal.user_id, TargetType::Organization, id.raw()),
            "organization.read",
        )?;
        Ok(organization)
    }

    fn list_organizations(
        &self,
        principal: &Principal,
        page: Page,
    ) -> Result<Vec<Organization>, DomainError> {
        let visible = self
            .ports
            .organizations
            .list()
            .into_iter()
            .filter(|organization| {
                principal.is_administrator
                    || self.authority.belongs(
                        principal.user_id,
                        TargetType::Organization,
                        organization.id.raw(),
                    )
            })
            .collect();
        paginate(visible, page)
    }

    fn update_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
        command: UpdateGroup,
    ) -> Result<Organization, DomainError> {
        self.ports.organizations.get(id)?;
        self.authority.require(
            principal,
            self.authority
                .manages(principal.user_id, TargetType::Organization, id.raw()),
            "organization.update",
        )?;
        let owner_id = match command.owner_id.as_ref() {
            None => None,
            Some(text) => Some(self.owner_of(principal, Some(text))?),
        };

        let now = self.ports.clock.now();
        self.ports
            .organizations
            .update(id, &mut |organization: &mut Organization| {
                if let Some(name) = command.name.as_deref() {
                    organization.name = validation::name("name", name)?;
                }
                if let Some(owner) = owner_id {
                    organization.owner_id = owner;
                }
                organization.updated_at = now.clone();
                Ok(())
            })
    }

    fn delete_organization(
        &self,
        principal: &Principal,
        id: OrganizationId,
    ) -> Result<(), DomainError> {
        let organization = self.ports.organizations.get(id)?;
        self.authority.require(
            principal,
            organization.owner_id == principal.user_id,
            "organization.delete",
        )?;

        if self.ports.grants.list().iter().any(|grant| {
            grant.target_type == TargetType::Organization && grant.target_id == id.raw()
        }) {
            return Err(DomainError::Referenced {
                reason: "a project grant addresses the organization".to_string(),
            });
        }

        for membership in self.ports.memberships.list() {
            if membership.target_type == TargetType::Organization
                && membership.target_id == id.raw()
            {
                self.ports.memberships.remove(membership.id)?;
            }
        }
        self.ports.organizations.remove(id).map(|_| ())
    }
}
