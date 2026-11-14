//! The project's delivery: where the plan lives, and the per-machine state
//! around it.
//!
//! - [`settings`] is the model of the delivery settings
//!   (`DSR-delivery-source.md` DSR-FR-HOEW, DSR-FR-ZJMC). The project-public
//!   store keeps them (`crate::project_settings::delivery_settings`,
//!   PSS-FR-KZGG).
//! - [`source`] resolves the delivery source and moves the plan between its
//!   homes (`DSR-delivery-source.md`).
//! - [`github`] reads and writes the plan of a GitHub Project
//!   (`GDP-github-delivery-plan.md`).
//! - [`machine_state`] is the per-machine state of
//!   `DMS-delivery-machine-state.md`: the layout of each plan, the overlay of
//!   a GitHub plan, and the publish journal of a Flow file. All of it is in the
//!   repository store (`RMS-repository-machine-storage.md` RMS-FR-OCPB), so
//!   Git never sees it.

use serde::Serialize;

use crate::flow_validation::ValidationReport;

pub mod github;
pub mod machine_state;
pub mod settings;
pub mod source;

/// RMS-FR-OCPB: the folder of the repository store that holds the delivery
/// records.
pub const DELIVERY_DIRNAME: &str = "delivery";

/// DMS-FR-IHML: no project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// DMS-FR-IHML, RMS-FR-HAJC: the repository store is not available.
pub const ERR_STORE_UNAVAILABLE: &str = crate::repository_store::ERR_STORE_UNAVAILABLE;
/// DMS-FR-TJSJ: a scope, a position, or a size that is not valid.
pub const ERR_INVALID_LAYOUT: &str = "invalid_layout";
/// DMS-FR-QFEA: an overlay that breaks a field rule of FGV.
pub const ERR_INVALID_OVERLAY: &str = "invalid_overlay";

/// The typed error of the delivery operations (DMS, and later DSR and GDP).
/// The serde form is the frontend's `DeliveryError` of `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryError {
    pub code: String,
    pub message: String,
    /// The violations of a refused document or overlay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ValidationReport>,
    /// DSR-FR-ULON, GDP-FR-MSVS: the tracker items a failed call created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_urls: Option<Vec<String>>,
}

impl DeliveryError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into(), report: None, created_urls: None }
    }

    pub fn no_project() -> Self {
        Self::new(ERR_NO_PROJECT_OPEN, "No project is open.")
    }

    pub fn store_unavailable() -> Self {
        Self::new(ERR_STORE_UNAVAILABLE, "The machine store of this repository is not available.")
    }

    pub fn with_report(mut self, report: ValidationReport) -> Self {
        self.report = Some(report);
        self
    }
}

/// DMS-FR-IHML: the repository store of the open project, or the typed
/// refusal. No project is `no_project_open`; a store that cannot be resolved
/// or created is `store_unavailable`.
pub fn machine_store(
    project: &crate::project::ProjectState,
) -> Result<crate::fs::RootFs, DeliveryError> {
    if project.root().is_none() {
        return Err(DeliveryError::no_project());
    }
    project.require_store().map_err(|_| DeliveryError::store_unavailable())
}
