//! The outbound ports of the application layer.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-IRWO, SAS-FR-VBQJ, SAS-FR-HEIV, SAS-FR-XUFA.
//!
//! Every entity is reached through one repository port. The V1 adapter holds
//! the records in memory; a durable adapter replaces it behind the same port
//! and changes neither the application services nor the REST contract.

use std::sync::Arc;

use uuid::Uuid;

use crate::domain::clock::Clock;
use crate::domain::content::{Conversation, Draft};
use crate::domain::error::DomainError;
use crate::domain::identity::{Device, Invitation, Membership, Organization, Team, User};
use crate::domain::ids::{
    ConversationId, DeviceId, DraftId, GrantId, IdSource, InvitationId, MembershipId,
    OrganizationId, ProjectId, TeamId, UserId,
};
use crate::domain::project::{Grant, Project};

/// The store of one kind of record.
///
/// The port is deliberately narrow. A query that filters records is written in
/// the application layer over [`Repository::list`], so a durable adapter
/// implements six operations rather than one per query.
pub trait Repository<K, V>: Send + Sync
where
    K: Copy,
    V: Clone,
{
    /// Writes a record that no other record's identifier holds.
    ///
    /// SAS-FR-OZET: an identifier another record holds is a `duplicate_id`
    /// refusal, and the store is left as it was.
    fn insert(&self, id: K, value: V) -> Result<V, DomainError>;

    /// The record, or nothing.
    fn find(&self, id: K) -> Option<V>;

    /// The record, or a `not_found` refusal (SAS-FR-NKBC).
    fn get(&self, id: K) -> Result<V, DomainError>;

    /// Changes one record under the store's own lock.
    ///
    /// SAS-FR-HEIV, SAS-FR-XUFA: the change is applied whole while no other
    /// writer holds the record, and a closure that refuses leaves the record as
    /// it was, so a revision counter and a message sequence never repeat.
    fn update(
        &self,
        id: K,
        change: &mut dyn FnMut(&mut V) -> Result<(), DomainError>,
    ) -> Result<V, DomainError>;

    /// Removes the record, or refuses when no record holds the identifier.
    fn remove(&self, id: K) -> Result<V, DomainError>;

    /// Every record, in insertion order.
    fn list(&self) -> Vec<V>;
}

/// Every outbound port the application services hold.
#[derive(Clone)]
pub struct Ports {
    pub users: Arc<dyn Repository<UserId, User>>,
    pub teams: Arc<dyn Repository<TeamId, Team>>,
    pub organizations: Arc<dyn Repository<OrganizationId, Organization>>,
    pub memberships: Arc<dyn Repository<MembershipId, Membership>>,
    pub invitations: Arc<dyn Repository<InvitationId, Invitation>>,
    pub devices: Arc<dyn Repository<DeviceId, Device>>,
    pub projects: Arc<dyn Repository<ProjectId, Project>>,
    pub grants: Arc<dyn Repository<GrantId, Grant>>,
    pub drafts: Arc<dyn Repository<DraftId, Draft>>,
    pub conversations: Arc<dyn Repository<ConversationId, Conversation>>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
}

impl Ports {
    /// The identifier a create request supplies, or one that is generated
    /// (SAS-FR-OZET).
    pub fn resolve_id<T>(&self, field: &str, supplied: Option<&str>) -> Result<T, DomainError>
    where
        T: From<Uuid>,
    {
        match supplied {
            None => Ok(T::from(self.ids.next())),
            Some(text) => Uuid::parse_str(text.trim())
                .map(T::from)
                .map_err(|_| DomainError::invalid_field(field, "the value is not a UUID")),
        }
    }
}

/// Who a request acts as (SAS-FR-XRPD).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Principal {
    /// The persisted user the credential resolved to.
    pub user_id: UserId,
    /// Whether the user bypasses every access-control check.
    pub is_administrator: bool,
}

impl Principal {
    /// A principal that holds no bypass.
    pub fn member(user_id: UserId) -> Self {
        Principal {
            user_id,
            is_administrator: false,
        }
    }

    /// The administrator principal.
    pub fn administrator(user_id: UserId) -> Self {
        Principal {
            user_id,
            is_administrator: true,
        }
    }
}
