//! The administrator the service reconciles when it starts.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-DTXV, SAS-FR-NWQE, SAS-FR-PDNU.

use crate::application::ports::Ports;
use crate::domain::error::DomainError;
use crate::domain::identity::{Status, User, UserRole};
use crate::domain::ids::UserId;

/// The display name the reconciled administrator carries.
pub const ADMINISTRATOR_DISPLAY_NAME: &str = "Administrator";

/// Creates or reconciles the administrator user (SAS-FR-DTXV).
///
/// The operation is idempotent: repeated starts with one configured identifier
/// create one user. A record that exists is moved back to the administrator
/// role and the active status, so an in-memory store that lost the record — or
/// a record that was changed — is restored rather than duplicated.
pub fn reconcile_administrator(
    ports: &Ports,
    administrator_id: UserId,
) -> Result<User, DomainError> {
    let now = ports.clock.now();

    if ports.users.find(administrator_id).is_some() {
        return ports
            .users
            .update(administrator_id, &mut |user: &mut User| {
                user.role = UserRole::Administrator;
                user.status = Status::Active;
                user.updated_at = now.clone();
                Ok(())
            });
    }

    ports.users.insert(
        administrator_id,
        User {
            id: administrator_id,
            identity: None,
            display_name: ADMINISTRATOR_DISPLAY_NAME.to_string(),
            // SAS-FR-NWQE: the token is never written onto the record, and the
            // administrator holds no profile the operator did not supply.
            email: None,
            role: UserRole::Administrator,
            status: Status::Active,
            created_at: now.clone(),
            updated_at: now,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::memory::memory_ports;

    fn administrator_id() -> UserId {
        UserId(uuid::Uuid::from_u128(42))
    }

    // SAS-FR-DTXV: repeated starts with one identifier create one user.
    #[test]
    fn the_administrator_is_created_once_and_reconciled_after() {
        let ports = memory_ports();
        let first = reconcile_administrator(&ports, administrator_id()).expect("the first start");
        let second = reconcile_administrator(&ports, administrator_id()).expect("the second start");

        assert_eq!(first.id, second.id);
        assert_eq!(ports.users.list().len(), 1);
        assert!(second.is_administrator());
    }

    // SAS-FR-DTXV: a record that lost the administrator role is restored.
    #[test]
    fn the_administrator_role_is_restored() {
        let ports = memory_ports();
        reconcile_administrator(&ports, administrator_id()).expect("the first start");
        ports
            .users
            .update(administrator_id(), &mut |user: &mut User| {
                user.role = UserRole::Member;
                user.status = Status::Suspended;
                Ok(())
            })
            .expect("the record is changed");

        let reconciled = reconcile_administrator(&ports, administrator_id()).expect("the restart");
        assert!(reconciled.is_administrator());
        assert_eq!(reconciled.status, Status::Active);
    }

    // SAS-FR-PDNU: a new store holds no record, and the administrator is written again.
    #[test]
    fn a_new_store_reconciles_the_administrator_again() {
        let first = memory_ports();
        reconcile_administrator(&first, administrator_id()).expect("the first process");
        let second = memory_ports();
        assert!(second.users.list().is_empty());
        reconcile_administrator(&second, administrator_id()).expect("the second process");
        assert_eq!(second.users.list().len(), 1);
    }

    // SAS-FR-NWQE: no credential is written onto the record.
    #[test]
    fn the_administrator_record_holds_no_credential() {
        let ports = memory_ports();
        let administrator = reconcile_administrator(&ports, administrator_id()).expect("the start");
        let json = serde_json::to_string(&administrator).expect("the record serializes");
        assert!(!json.to_lowercase().contains("token"), "{json}");
        assert!(!json.to_lowercase().contains("bearer"), "{json}");
    }
}
