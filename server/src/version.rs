//! The build version the service reports.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-14, BMS-FR-15.

/// The version that `build.rs` resolved when the crate was compiled.
///
/// BMS-FR-15: the value is compiled in. The running process reads no file, runs
/// no Git command, and reads no environment variable to learn it, so the image
/// — which holds neither a Git executable nor a checkout — reports the value
/// that was chosen when it was built.
pub const BUILD_VERSION: &str = env!("SYNTHESIS_SERVER_VERSION");
