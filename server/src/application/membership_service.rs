//! The memberships, the invitations, and the devices.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-MZQF, SAS-FR-EPXN, SAS-FR-YCWK, SAS-FR-ATLB,
//! SAS-FR-FQVS, SAS-FR-BFHN.

use crate::application::authority::Authority;
use crate::application::commands::{
    AcceptInvitation, CreateDevice, CreateInvitation, CreateMembership, MembershipFilter, Page,
    UpdateDevice, UpdateMembership,
};
use crate::application::paginate;
use crate::application::ports::{Ports, Principal};
use crate::domain::clock::Timestamp;
use crate::domain::error::DomainError;
use crate::domain::identity::{
    Device, DeviceType, Invitation, InvitationState, Membership, Status, TargetType,
};
use crate::domain::ids::{DeviceId, InvitationId, MembershipId, OrganizationId, TeamId, UserId};
use crate::domain::permissions::MemberRole;
use crate::domain::validation;

/// The inbound port of the membership, invitation, and device operations.
pub trait MembershipApi: Send + Sync {
    fn create_membership(
        &self,
        principal: &Principal,
        command: CreateMembership,
    ) -> Result<Membership, DomainError>;
    fn read_membership(
        &self,
        principal: &Principal,
        id: MembershipId,
    ) -> Result<Membership, DomainError>;
    fn list_memberships(
        &self,
        principal: &Principal,
        filter: MembershipFilter,
    ) -> Result<Vec<Membership>, DomainError>;
    fn update_membership(
        &self,
        principal: &Principal,
        id: MembershipId,
        command: UpdateMembership,
    ) -> Result<Membership, DomainError>;
    fn delete_membership(&self, principal: &Principal, id: MembershipId)
        -> Result<(), DomainError>;

    fn create_invitation(
        &self,
        principal: &Principal,
        command: CreateInvitation,
    ) -> Result<Invitation, DomainError>;
    fn read_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
    ) -> Result<Invitation, DomainError>;
    fn list_invitations(
        &self,
        principal: &Principal,
        page: Page,
    ) -> Result<Vec<Invitation>, DomainError>;
    fn accept_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
        command: AcceptInvitation,
    ) -> Result<Membership, DomainError>;
    fn revoke_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
    ) -> Result<Invitation, DomainError>;

    fn create_device(
        &self,
        principal: &Principal,
        command: CreateDevice,
    ) -> Result<Device, DomainError>;
    fn read_device(&self, principal: &Principal, id: DeviceId) -> Result<Device, DomainError>;
    fn list_devices(&self, principal: &Principal, page: Page) -> Result<Vec<Device>, DomainError>;
    fn update_device(
        &self,
        principal: &Principal,
        id: DeviceId,
        command: UpdateDevice,
    ) -> Result<Device, DomainError>;
    fn revoke_device(&self, principal: &Principal, id: DeviceId) -> Result<Device, DomainError>;
    fn delete_device(&self, principal: &Principal, id: DeviceId) -> Result<(), DomainError>;
}

/// The membership operations over the ports.
#[derive(Clone)]
pub struct MembershipService {
    ports: Ports,
    authority: Authority,
}

impl MembershipService {
    /// The service over the given ports.
    pub fn new(ports: Ports) -> Self {
        MembershipService {
            authority: Authority::new(ports.clone()),
            ports,
        }
    }

    /// Reads a target that a membership or an invitation addresses, and refuses
    /// one that does not exist (SAS-FR-MZQF).
    fn check_target(
        &self,
        target_type: TargetType,
        target_id: uuid::Uuid,
    ) -> Result<(), DomainError> {
        let exists = match target_type {
            TargetType::Team => self.ports.teams.find(TeamId(target_id)).is_some(),
            TargetType::Organization => self
                .ports
                .organizations
                .find(OrganizationId(target_id))
                .is_some(),
            TargetType::User => false,
        };
        if exists {
            Ok(())
        } else {
            Err(DomainError::NotFound {
                resource: match target_type {
                    TargetType::Organization => "organization",
                    _ => "team",
                },
            })
        }
    }

    /// Writes one active membership, and refuses a second one for that member
    /// and that target (SAS-FR-BFHN).
    fn write_membership(
        &self,
        id: MembershipId,
        member_id: UserId,
        target_type: TargetType,
        target_id: uuid::Uuid,
        role: MemberRole,
    ) -> Result<Membership, DomainError> {
        self.ports.users.get(member_id)?;
        self.check_target(target_type, target_id)?;

        let taken = self.ports.memberships.list().into_iter().any(|held| {
            held.member_id == member_id
                && held.target_type == target_type
                && held.target_id == target_id
                && held.is_active()
        });
        if taken {
            return Err(DomainError::DuplicateMembership);
        }

        let now = self.ports.clock.now();
        self.ports.memberships.insert(
            id,
            Membership {
                id,
                member_id,
                target_type,
                target_id,
                role,
                status: Status::Active,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }
}

impl MembershipApi for MembershipService {
    fn create_membership(
        &self,
        principal: &Principal,
        command: CreateMembership,
    ) -> Result<Membership, DomainError> {
        let id: MembershipId = self.ports.resolve_id("id", command.id.as_deref())?;
        let member_id = UserId::parse(&command.member_id)?;
        let target_type = TargetType::parse_membership_target(&command.target_type)?;
        let target_id = uuid::Uuid::parse_str(command.target_id.trim())
            .map_err(|_| DomainError::invalid_field("target_id", "the value is not a UUID"))?;
        let role = MemberRole::parse(&command.role)?;

        self.authority.require(
            principal,
            self.authority
                .manages(principal.user_id, target_type, target_id),
            "membership.create",
        )?;
        self.write_membership(id, member_id, target_type, target_id, role)
    }

    fn read_membership(
        &self,
        principal: &Principal,
        id: MembershipId,
    ) -> Result<Membership, DomainError> {
        let membership = self.ports.memberships.get(id)?;
        let allowed = membership.member_id == principal.user_id
            || self.authority.manages(
                principal.user_id,
                membership.target_type,
                membership.target_id,
            );
        self.authority
            .require(principal, allowed, "membership.read")?;
        Ok(membership)
    }

    fn list_memberships(
        &self,
        principal: &Principal,
        filter: MembershipFilter,
    ) -> Result<Vec<Membership>, DomainError> {
        let member = filter.member_id.as_deref().map(UserId::parse).transpose()?;
        let target_type = filter
            .target_type
            .as_deref()
            .map(TargetType::parse)
            .transpose()?;
        let target_id = filter
            .target_id
            .as_deref()
            .map(|text| {
                uuid::Uuid::parse_str(text.trim())
                    .map_err(|_| DomainError::invalid_field("target_id", "the value is not a UUID"))
            })
            .transpose()?;

        let visible = self
            .ports
            .memberships
            .list()
            .into_iter()
            .filter(|membership| member.is_none_or(|id| membership.member_id == id))
            .filter(|membership| target_type.is_none_or(|kind| membership.target_type == kind))
            .filter(|membership| target_id.is_none_or(|id| membership.target_id == id))
            .filter(|membership| {
                principal.is_administrator
                    || membership.member_id == principal.user_id
                    || self.authority.manages(
                        principal.user_id,
                        membership.target_type,
                        membership.target_id,
                    )
            })
            .collect();
        paginate(visible, filter.page)
    }

    fn update_membership(
        &self,
        principal: &Principal,
        id: MembershipId,
        command: UpdateMembership,
    ) -> Result<Membership, DomainError> {
        let membership = self.ports.memberships.get(id)?;
        self.authority.require(
            principal,
            self.authority.manages(
                principal.user_id,
                membership.target_type,
                membership.target_id,
            ),
            "membership.update",
        )?;

        let now = self.ports.clock.now();
        self.ports
            .memberships
            .update(id, &mut |membership: &mut Membership| {
                if let Some(role) = command.role.as_deref() {
                    membership.role = MemberRole::parse(role)?;
                }
                if let Some(status) = command.status.as_deref() {
                    membership.status = match status {
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
                membership.updated_at = now.clone();
                Ok(())
            })
    }

    fn delete_membership(
        &self,
        principal: &Principal,
        id: MembershipId,
    ) -> Result<(), DomainError> {
        let membership = self.ports.memberships.get(id)?;
        let allowed = membership.member_id == principal.user_id
            || self.authority.manages(
                principal.user_id,
                membership.target_type,
                membership.target_id,
            );
        self.authority
            .require(principal, allowed, "membership.delete")?;
        self.ports.memberships.remove(id).map(|_| ())
    }

    fn create_invitation(
        &self,
        principal: &Principal,
        command: CreateInvitation,
    ) -> Result<Invitation, DomainError> {
        let id: InvitationId = self.ports.resolve_id("id", command.id.as_deref())?;
        let target_type = TargetType::parse_membership_target(&command.target_type)?;
        let target_id = uuid::Uuid::parse_str(command.target_id.trim())
            .map_err(|_| DomainError::invalid_field("target_id", "the value is not a UUID"))?;
        let role = MemberRole::parse(&command.role)?;
        let expires_at = Timestamp(validation::name("expires_at", &command.expires_at)?);

        let invitee_user_id = command
            .invitee_user_id
            .as_deref()
            .map(UserId::parse)
            .transpose()?;
        let invitee_email = command
            .invitee_email
            .as_deref()
            .map(|value| validation::email("invitee_email", value))
            .transpose()?;
        if invitee_user_id.is_none() && invitee_email.is_none() {
            return Err(DomainError::invalid_field(
                "invitee_user_id",
                "an invitation names an invitee user or an invitee email",
            ));
        }
        if let Some(user_id) = invitee_user_id {
            self.ports.users.get(user_id)?;
        }

        self.check_target(target_type, target_id)?;
        self.authority.require(
            principal,
            self.authority
                .manages(principal.user_id, target_type, target_id),
            "invitation.create",
        )?;

        let now = self.ports.clock.now();
        self.ports.invitations.insert(
            id,
            Invitation {
                id,
                target_type,
                target_id,
                inviter_id: principal.user_id,
                invitee_user_id,
                invitee_email,
                role,
                expires_at,
                state: InvitationState::Pending,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
    ) -> Result<Invitation, DomainError> {
        let invitation = self.ports.invitations.get(id)?;
        let allowed = invitation.inviter_id == principal.user_id
            || invitation.invitee_user_id == Some(principal.user_id)
            || self.authority.manages(
                principal.user_id,
                invitation.target_type,
                invitation.target_id,
            );
        self.authority
            .require(principal, allowed, "invitation.read")?;
        Ok(invitation)
    }

    fn list_invitations(
        &self,
        principal: &Principal,
        page: Page,
    ) -> Result<Vec<Invitation>, DomainError> {
        let visible = self
            .ports
            .invitations
            .list()
            .into_iter()
            .filter(|invitation| {
                principal.is_administrator
                    || invitation.inviter_id == principal.user_id
                    || invitation.invitee_user_id == Some(principal.user_id)
                    || self.authority.manages(
                        principal.user_id,
                        invitation.target_type,
                        invitation.target_id,
                    )
            })
            .collect();
        paginate(visible, page)
    }

    fn accept_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
        command: AcceptInvitation,
    ) -> Result<Membership, DomainError> {
        let invitation = self.ports.invitations.get(id)?;
        let member_id = match (command.user_id.as_deref(), invitation.invitee_user_id) {
            (Some(text), _) => UserId::parse(text)?,
            (None, Some(user_id)) => user_id,
            (None, None) => principal.user_id,
        };
        self.authority.require(
            principal,
            member_id == principal.user_id,
            "invitation.accept",
        )?;

        // SAS-FR-ATLB: the state moves first. An invitation that is expired,
        // revoked, or accepted is refused here and writes no membership.
        let now = self.ports.clock.now();
        self.ports
            .invitations
            .update(id, &mut |invitation: &mut Invitation| {
                if !invitation.is_acceptable(&now) {
                    return Err(DomainError::InvitationNotPending);
                }
                invitation.state = InvitationState::Accepted;
                invitation.updated_at = now.clone();
                Ok(())
            })?;

        let membership_id: MembershipId = self.ports.resolve_id("id", None)?;
        let written = self.write_membership(
            membership_id,
            member_id,
            invitation.target_type,
            invitation.target_id,
            invitation.role,
        );

        // SAS-FR-XUFA: a membership that is refused leaves the invitation as it
        // was, so a refused acceptance may be repeated.
        if written.is_err() {
            let _ = self
                .ports
                .invitations
                .update(id, &mut |invitation: &mut Invitation| {
                    invitation.state = InvitationState::Pending;
                    Ok(())
                });
        }
        written
    }

    fn revoke_invitation(
        &self,
        principal: &Principal,
        id: InvitationId,
    ) -> Result<Invitation, DomainError> {
        let invitation = self.ports.invitations.get(id)?;
        self.authority.require(
            principal,
            invitation.inviter_id == principal.user_id
                || self.authority.manages(
                    principal.user_id,
                    invitation.target_type,
                    invitation.target_id,
                ),
            "invitation.revoke",
        )?;

        let now = self.ports.clock.now();
        self.ports
            .invitations
            .update(id, &mut |invitation: &mut Invitation| {
                if !matches!(invitation.state, InvitationState::Pending) {
                    return Err(DomainError::InvitationNotPending);
                }
                invitation.state = InvitationState::Revoked;
                invitation.updated_at = now.clone();
                Ok(())
            })
    }

    fn create_device(
        &self,
        principal: &Principal,
        command: CreateDevice,
    ) -> Result<Device, DomainError> {
        let id: DeviceId = self.ports.resolve_id("id", command.id.as_deref())?;
        let device_type = DeviceType::parse(&command.device_type)?;
        let display_name = validation::name("display_name", &command.display_name)?;
        let owner_id = match command.owner_id.as_deref() {
            None => principal.user_id,
            Some(text) => UserId::parse(text)?,
        };
        self.ports.users.get(owner_id)?;
        self.authority
            .require(principal, owner_id == principal.user_id, "device.create")?;

        let now = self.ports.clock.now();
        self.ports.devices.insert(
            id,
            Device {
                id,
                owner_id,
                device_type,
                display_name,
                created_at: now.clone(),
                last_seen_at: now,
                revoked_at: None,
            },
        )
    }

    fn read_device(&self, principal: &Principal, id: DeviceId) -> Result<Device, DomainError> {
        let device = self.ports.devices.get(id)?;
        self.authority.require(
            principal,
            device.owner_id == principal.user_id,
            "device.read",
        )?;
        Ok(device)
    }

    fn list_devices(&self, principal: &Principal, page: Page) -> Result<Vec<Device>, DomainError> {
        let visible = self
            .ports
            .devices
            .list()
            .into_iter()
            .filter(|device| principal.is_administrator || device.owner_id == principal.user_id)
            .collect();
        paginate(visible, page)
    }

    fn update_device(
        &self,
        principal: &Principal,
        id: DeviceId,
        command: UpdateDevice,
    ) -> Result<Device, DomainError> {
        let device = self.ports.devices.get(id)?;
        self.authority.require(
            principal,
            device.owner_id == principal.user_id,
            "device.update",
        )?;

        let now = self.ports.clock.now();
        self.ports.devices.update(id, &mut |device: &mut Device| {
            if let Some(display_name) = command.display_name.as_deref() {
                device.display_name = validation::name("display_name", display_name)?;
            }
            if command.seen == Some(true) {
                device.last_seen_at = now.clone();
            }
            Ok(())
        })
    }

    fn revoke_device(&self, principal: &Principal, id: DeviceId) -> Result<Device, DomainError> {
        let device = self.ports.devices.get(id)?;
        self.authority.require(
            principal,
            device.owner_id == principal.user_id,
            "device.revoke",
        )?;

        let now = self.ports.clock.now();
        self.ports.devices.update(id, &mut |device: &mut Device| {
            device.revoked_at = Some(now.clone());
            Ok(())
        })
    }

    fn delete_device(&self, principal: &Principal, id: DeviceId) -> Result<(), DomainError> {
        let device = self.ports.devices.get(id)?;
        self.authority.require(
            principal,
            device.owner_id == principal.user_id,
            "device.delete",
        )?;
        self.ports.devices.remove(id).map(|_| ())
    }
}
