//! The domain layer: the entities, their identifiers, and their rules.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-VBQJ.
//!
//! Nothing here names an HTTP type, an adapter type, or a store. The layer
//! holds no clock and no random source: a time and a generated identifier reach
//! it from the application layer.

pub mod clock;
pub mod content;
pub mod error;
pub mod identity;
pub mod ids;
pub mod permissions;
pub mod project;
pub mod validation;
