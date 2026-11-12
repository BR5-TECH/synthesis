//! The OpenSpec spec index of the specification map
//! (`OSI-openspec-spec-index.md`).
//!
//! In OpenSpec mode `"load specification map"` answers the project's own live
//! capabilities and requirements, in groups by the first segment of the
//! capability id, with a chip for each active change in the active worktree
//! and on every live stream, and with one strict validation as the
//! completeness of each requirement. Outside the mode it answers `demo`, and
//! the UI shows its demo index. The load reads through the seam of this
//! module, writes nothing, and keeps no result between two loads.

use std::time::Instant;

use serde::Serialize;

use super::{ChangeLocation, OpenSpec, OpenSpecReadError, ValidationScope};
use crate::fs::{EntryKind, RootFs};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

mod build;

pub use build::{build_index, IndexCounts, LocationRead, ROOT_GROUP_ID, ROOT_GROUP_LABEL};

#[cfg(test)]
mod tests;

/// OSI-FR-PJJO.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";

/// OSI contract surface `SpecificationMapLoad`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SpecificationMapLoad {
    /// OSI-FR-OSWD: the UI shows its demo index.
    Demo,
    Openspec {
        index: Box<OpenSpecMapIndex>,
    },
}

/// OSI contract surface `OpenSpecMapIndex`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecMapIndex {
    pub levels: Vec<LevelName>,
    pub roots: Vec<IndexNode>,
    /// OSI-FR-JIJM: always empty.
    pub dependencies: Vec<serde_json::Value>,
    /// OSI-FR-OIOB: `valid`, `invalid`, or `unavailable`.
    pub validation: &'static str,
    /// OSI-FR-OIOB: the failure of a validation that is `unavailable`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_failure: Option<super::OpenSpecFailure>,
    /// OSI-FR-DENT.
    pub unread_locations: u64,
}

/// OSI-FR-XUJG: the names of one level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LevelName {
    pub plural: String,
    pub singular: String,
}

/// OSI contract surface `IndexNode`: a group or a capability node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexNode {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub children: IndexChildren,
    /// OSI-FR-VOOS: present and true on a planned capability node alone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub planned: bool,
    /// OSI-FR-SBVF: the issues of a capability, absent where none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<MapIssue>,
    /// OSI-FR-ALQI: the change chips of a capability, absent where none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<ChangeChip>,
}

/// The children of a group (capability nodes) or of a capability
/// (requirement nodes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum IndexChildren {
    Nodes(Vec<IndexNode>),
    Specs(Vec<SpecNode>),
}

/// OSI contract surface `SpecNode`: one requirement (OSI-FR-SEYB).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecNode {
    pub code: String,
    pub badge: String,
    pub path: String,
    pub label: String,
    pub summary: String,
    pub requirements: u64,
    pub scenarios: u64,
    /// OSI-FR-OJRE: `built` or `gap`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<MapIssue>,
}

/// OSI contract surface `ChangeChip`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeChip {
    pub change: String,
    pub location: ChipLocation,
    /// OSI-FR-GRCV: the active worktree location alone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<MapIssue>,
}

/// OSI-FR-ALQI: where a chip's change stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChipLocation {
    ActiveWorktree,
    Stream {
        #[serde(rename = "streamId")]
        stream_id: String,
        #[serde(rename = "streamName")]
        stream_name: String,
    },
}

/// OSI contract surface `MapIssue`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MapIssue {
    pub level: String,
    pub message: String,
}

/// One location of OSI-FR-QELX with its working copy, or `None` where its
/// working copy cannot be governed, which reads as a location that cannot be
/// read (OSI-FR-DENT).
pub struct IndexLocation {
    pub location: ChipLocation,
    pub at: Option<RootFs>,
    /// OSI-FR-GRCV: the active worktree is this location's working copy.
    pub validated: bool,
}

/// Run `work` over each item with at most four threads at the same time, and
/// give the results in the order of the items. The seam's own slots bound the
/// CLI processes (OSM-FR-QHRV).
fn in_parallel<T: Sync, U: Send>(items: &[T], work: impl Fn(&T) -> U + Sync) -> Vec<U> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<Option<U>>> =
        std::sync::Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..items.len().min(super::cli::MAX_CONCURRENT) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(item) = items.get(i) else {
                    break;
                };
                let value = work(item);
                results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(value);
            });
        }
    });
    results.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().flatten().collect()
}

impl OpenSpec {
    /// OSI-FR-SJNC, OSI-FR-DVLQ, OSI-FR-QELX, OSI-FR-DENT, OSI-FR-OIOB: the
    /// reads of one load, and the index built from them. A read of the live
    /// capabilities that fails fails the load with its code.
    pub fn load_spec_index<S: LogSink + Clone + Send + Sync + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: &RootFs,
        locations: &[IndexLocation],
    ) -> Result<OpenSpecMapIndex, OpenSpecReadError> {
        let listed = self.list_capabilities(sink, buffer, Some(active))?;
        let read = in_parallel(&listed, |capability| {
            self.read_capability(sink, buffer, Some(active), &capability.id)
        });
        let capabilities = read.into_iter().collect::<Result<Vec<_>, _>>()?;
        // OSI-FR-OIOB: a validation that fails is no failure of the load.
        let validation =
            self.validate_project(sink, buffer, Some(active), active, ValidationScope::All);
        let validation_failure = validation.as_ref().err().map(OpenSpecReadError::to_failure);
        let validation = validation.ok();
        let reads = in_parallel(locations, |location| LocationRead {
            location: location.location.clone(),
            validated: location.validated,
            changes: location
                .at
                .as_ref()
                .and_then(|at| self.list_changes_at(sink, buffer, Some(active), at).ok()),
        });
        let mut index = build_index(&capabilities, validation.as_ref(), &reads);
        index.validation_failure = validation_failure;
        Ok(index)
    }
}

/// OSI-FR-QELX: the active worktree and the working copy of each live stream
/// of the project. An active worktree that is the working copy of a stream is
/// that stream's location alone.
fn index_locations<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    active: &RootFs,
) -> Vec<IndexLocation> {
    let mut locations = Vec::new();
    let active_stream = crate::streams::stream_at_path(app, active.path());
    if active_stream.is_none() {
        locations.push(IndexLocation {
            location: ChipLocation::ActiveWorktree,
            at: Some(active.clone()),
            validated: true,
        });
    }
    for stream in crate::streams::live_streams(app) {
        let location = ChangeLocation::Stream { stream_id: stream.id.clone() };
        // OSI-FR-DENT: a working copy whose folder is gone cannot be read,
        // and costs no process.
        let at = super::change::working_copy(app, &location)
            .ok()
            .filter(|at| at.file_info(at.path()).is_ok_and(|info| info.kind == EntryKind::Dir));
        let validated = active_stream.as_ref().is_some_and(|own| own.id == stream.id);
        locations.push(IndexLocation {
            location: ChipLocation::Stream { stream_id: stream.id, stream_name: stream.name },
            at,
            validated,
        });
    }
    locations
}

/// OSI-FR-OSWD, OSI-FR-PJJO: the whole load, on the calling thread.
pub fn load_specification_map_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<SpecificationMapLoad, Box<super::OpenSpecFailure>> {
    use tauri::Manager;
    let started = Instant::now();
    let active =
        app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
    let outcome = match active {
        None => Err(Box::new(super::OpenSpecFailure::new(ERR_NO_PROJECT_OPEN))),
        Some(active) if !OpenSpec::mode_active(Some(&active)) => Ok(SpecificationMapLoad::Demo),
        Some(active) => {
            let locations = index_locations(app, &active);
            match app.try_state::<OpenSpec>() {
                Some(seam) => seam.load_spec_index(app, &logging::BUFFER, &active, &locations),
                None => {
                    OpenSpec::default().load_spec_index(app, &logging::BUFFER, &active, &locations)
                }
            }
            .map(|index| SpecificationMapLoad::Openspec { index: Box::new(index) })
            .map_err(|error| Box::new(error.to_failure()))
        }
    };
    let millis = started.elapsed().as_millis() as u64;
    match &outcome {
        Ok(SpecificationMapLoad::Demo) => logging::log_info(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "specification map load answered the demo index",
            log_fields! { "source" => "demo", "durationMs" => millis },
        ),
        Ok(SpecificationMapLoad::Openspec { index }) => {
            let counts = index.counts();
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend],
                "specification map loaded",
                log_fields! {
                    "source" => "openspec",
                    "groups" => counts.groups,
                    "capabilities" => counts.capabilities,
                    "requirements" => counts.requirements,
                    "chips" => counts.chips,
                    "issues" => counts.issues,
                    "unreadLocations" => index.unread_locations,
                    "validation" => index.validation,
                    "durationMs" => millis,
                },
            )
        }
        Err(failure) => logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "specification map load failed",
            log_fields! { "code" => failure.code.clone(), "durationMs" => millis },
        ),
    }
    outcome
}

/// OSI-FR-VTAD: `"load specification map"`, on a blocking thread. Read-only.
#[tauri::command]
pub async fn load_specification_map<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<SpecificationMapLoad, Box<super::OpenSpecFailure>> {
    tauri::async_runtime::spawn_blocking(move || load_specification_map_now(&app))
        .await
        .map_err(|_| Box::new(super::OpenSpecFailure::new(super::failure::CODE_FAILED)))?
}
