//! The strict validation of a whole project at a location
//! (`OSM-openspec-mode.md` OSM-FR-BXIF, OSM-FR-DXHP).
//!
//! `openspec validate --all` or `--specs` judges every item of its scope in
//! one process. The spec index reads the scope `all` (per
//! `OSI-openspec-spec-index.md` OSI-FR-DVLQ), and the review of a merge run
//! reads the scope `specs` (per `GRL-graduation-loop.md` GRL-FR-ZDNL).

use serde::{Deserialize, Serialize};

use super::{OpenSpec, OpenSpecReadError, ValidationIssue};
use crate::fs::RootFs;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

/// OSM contract surface `ValidationScope`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationScope {
    All,
    Specs,
}

impl ValidationScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationScope::All => "all",
            ValidationScope::Specs => "specs",
        }
    }

    fn flag(self) -> &'static str {
        match self {
            ValidationScope::All => "--all",
            ValidationScope::Specs => "--specs",
        }
    }
}

/// OSM contract surface `ProjectValidation`. The graduation loop carries it
/// whole in a task input, so its fields are in snake case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectValidation {
    /// Every item is valid; true where there is no item (OSM-FR-DXHP).
    pub valid: bool,
    pub items: Vec<ProjectValidationItem>,
}

/// One item of a `ProjectValidation`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectValidationItem {
    /// The change name or the capability id.
    pub id: String,
    /// `change` or `spec`.
    pub kind: String,
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

impl ProjectValidation {
    /// GRL-FR-ZFMZ: the id of the first spec item that is not valid.
    pub fn first_invalid_spec(&self) -> Option<&ProjectValidationItem> {
        self.items.iter().find(|item| item.kind == "spec" && !item.valid)
    }
}

// ---------------------------------------------------------------------------
// The CLI fields read (OSM contract surface, "CLI fields read")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ValidateAll {
    items: Vec<ValidateAllItem>,
}

#[derive(Deserialize)]
struct ValidateAllItem {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    valid: bool,
    #[serde(default)]
    issues: Vec<ValidationIssue>,
}

/// OSM-FR-BXIF, OSM-FR-DXHP: the answer of `validate --all` or `--specs`. An
/// item whose type is neither `change` nor `spec` does not decode.
pub fn parse_project_validation(bytes: &[u8]) -> Result<ProjectValidation, OpenSpecReadError> {
    let out: ValidateAll =
        serde_json::from_slice(bytes).map_err(|_| OpenSpecReadError::CliOutputUnreadable)?;
    let mut items = Vec::with_capacity(out.items.len());
    for item in out.items {
        if item.kind != "change" && item.kind != "spec" {
            return Err(OpenSpecReadError::CliOutputUnreadable);
        }
        items.push(ProjectValidationItem {
            id: item.id,
            kind: item.kind,
            valid: item.valid,
            issues: item.issues,
        });
    }
    Ok(ProjectValidation { valid: items.iter().all(|item| item.valid), items })
}

impl OpenSpec {
    /// OSM-FR-BXIF, OSM-FR-DXHP: `openspec validate --all|--specs --strict
    /// --json --no-interactive` in the working copy `at`, where an exit status
    /// of 1 with readable output says an item is not valid.
    pub fn validate_project<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        scope: ValidationScope,
    ) -> Result<ProjectValidation, OpenSpecReadError> {
        let result = self.ready_in(sink, buffer, active, at).and_then(|exe| {
            let args = ["validate", scope.flag(), "--strict", "--json", "--no-interactive"];
            let out = self
                .run_accepting(sink, buffer, &exe, "validate", &args, at.path(), &[0, 1])
                .map_err(OpenSpecReadError::from)?;
            parse_project_validation(&out)
        });
        match &result {
            Ok(validation) => logging::log_info(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec project validated",
                log_fields! {
                    "scope" => scope.as_str(),
                    "valid" => validation.valid,
                    "items" => validation.items.len(),
                    "invalid" => validation.items.iter().filter(|i| !i.valid).count(),
                },
            ),
            Err(error) => logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec read failed",
                log_fields! { "read" => "validate_project", "code" => error.code() },
            ),
        }
        result
    }
}
