//! The application layer: the inbound ports, the outbound ports, and the
//! services that hold the rules.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-VBQJ.

pub mod access;
pub mod authority;
pub mod commands;
pub mod content_service;
pub mod identity_service;
pub mod logging;
pub mod membership_service;
pub mod ports;
pub mod project_service;
pub mod startup;

use std::sync::Arc;

use crate::application::commands::Page;
use crate::domain::error::DomainError;
use crate::domain::validation;

/// The window a list route serves (SAS-FR-TQIM).
pub fn paginate<T>(items: Vec<T>, page: Page) -> Result<Vec<T>, DomainError> {
    let limit = validation::page_size(page.limit)?;
    let offset = page.offset.unwrap_or(0);
    Ok(items.into_iter().skip(offset).take(limit).collect())
}

/// Every inbound port an adapter holds.
#[derive(Clone)]
pub struct Application {
    pub identity: Arc<dyn identity_service::IdentityApi>,
    pub memberships: Arc<dyn membership_service::MembershipApi>,
    pub projects: Arc<dyn project_service::ProjectApi>,
    pub content: Arc<dyn content_service::ContentApi>,
}

impl Application {
    /// The services over the given ports.
    pub fn new(ports: ports::Ports) -> Self {
        Application {
            identity: Arc::new(identity_service::IdentityService::new(ports.clone())),
            memberships: Arc::new(membership_service::MembershipService::new(ports.clone())),
            projects: Arc::new(project_service::ProjectService::new(ports.clone())),
            content: Arc::new(content_service::ContentService::new(ports)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_the_window_the_request_asked_for() {
        let items: Vec<u32> = (0..10).collect();
        assert_eq!(
            paginate(items.clone(), Page::default()).expect("a page"),
            items
        );
        assert_eq!(
            paginate(
                items.clone(),
                Page {
                    limit: Some(3),
                    offset: Some(2)
                }
            )
            .expect("a page"),
            vec![2, 3, 4]
        );
        assert!(paginate(
            items,
            Page {
                limit: Some(10_000),
                offset: None
            }
        )
        .is_err());
    }
}
