//! One change, read whole or validated, at a location (`OSM-openspec-mode.md`
//! OSM-FR-MFVN, OSM-FR-DDHL, OSM-FR-IHUO, OSM-FR-BCJO, OSM-FR-LLIO,
//! OSM-FR-NMHS, OSM-FR-PYHP, OSM-FR-APVF, OSM-FR-XDRN, OSM-FR-MBTI,
//! OSM-FR-KWWM, OSM-FR-GMKV).
//!
//! A location is the active worktree, the working copy of a stream, or, for
//! the graduation loop alone, a review checkout. Whether the mode is active is
//! decided on the active worktree for each of them (OSM-FR-DDHL); the CLI runs
//! in the working copy of the location (OSM-FR-HHUD).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{detection, reads, OpenSpec, OpenSpecReadError, Requirement, Scenario};
use crate::fs::{EntryKind, RootFs};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

/// OSM-FR-MFVN: where a change is read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeLocation {
    ActiveWorktree,
    Stream {
        #[serde(rename = "streamId")]
        stream_id: String,
    },
    /// The graduation loop alone: no Tauri command accepts it.
    #[serde(skip)]
    ReviewCheckout {
        root: PathBuf,
    },
}

impl ChangeLocation {
    /// The kind as a stable word, for a log record.
    pub fn kind(&self) -> &'static str {
        match self {
            ChangeLocation::ActiveWorktree => "active_worktree",
            ChangeLocation::Stream { .. } => "stream",
            ChangeLocation::ReviewCheckout { .. } => "review_checkout",
        }
    }
}

/// OSM-FR-LLIO: one file of an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeFile {
    pub path: String,
    pub text: Option<String>,
    pub unreadable: bool,
}

/// OSM-FR-LLIO: one artifact of the schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeArtifact {
    pub id: String,
    pub status: String,
    pub files: Vec<ChangeFile>,
}

/// OSM-FR-NMHS, OSM-FR-PYHP: one delta with the live requirement it changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDelta {
    pub capability: String,
    /// Whether the live specs of the working copy hold the capability.
    pub capability_exists: bool,
    pub operation: String,
    pub requirement: Option<Requirement>,
    pub rename: Option<Rename>,
    pub live: Option<Requirement>,
}

/// The two names of a `RENAMED` delta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    pub from: String,
    pub to: String,
}

/// OSM contract surface `ChangeDetail`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDetail {
    pub name: String,
    pub location: ChangeLocation,
    pub schema: String,
    pub is_complete: bool,
    pub completed_tasks: u64,
    pub total_tasks: u64,
    pub artifacts: Vec<ChangeArtifact>,
    pub deltas: Vec<ChangeDelta>,
}

/// OSM-FR-APVF: one issue of a validation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub level: String,
    pub path: String,
    pub message: String,
}

/// OSM contract surface `ChangeValidation`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeValidation {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

/// OSM-FR-IHUO: 1 to 64 characters of `a-z` and `0-9` in groups joined by one
/// `-`, and not `archive`.
pub fn valid_change_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "archive"
        && name.split('-').all(|group| {
            !group.is_empty() && group.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

// ---------------------------------------------------------------------------
// The CLI fields read (OSM contract surface, "CLI fields read")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusChange {
    #[serde(default)]
    schema_name: String,
    #[serde(default)]
    is_complete: bool,
    #[serde(default)]
    artifacts: Vec<StatusArtifact>,
    #[serde(default)]
    artifact_paths: HashMap<String, ArtifactPaths>,
}

#[derive(Deserialize)]
struct StatusArtifact {
    id: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactPaths {
    #[serde(default)]
    existing_output_paths: Vec<String>,
}

#[derive(Deserialize)]
struct ShowChange {
    #[serde(default)]
    deltas: Vec<ShowDelta>,
}

#[derive(Deserialize)]
struct ShowDelta {
    spec: String,
    operation: String,
    #[serde(default)]
    requirement: Option<ShowRequirement>,
    #[serde(default)]
    rename: Option<Rename>,
}

#[derive(Deserialize)]
struct ShowRequirement {
    #[serde(default)]
    name: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    scenarios: Vec<ShowScenario>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShowScenario {
    #[serde(default)]
    name: String,
    #[serde(default)]
    raw_text: String,
}

#[derive(Deserialize)]
struct ValidateOutput {
    items: Vec<ValidateItem>,
}

#[derive(Deserialize)]
struct ValidateItem {
    valid: bool,
    #[serde(default)]
    issues: Vec<ValidationIssue>,
}

fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, OpenSpecReadError> {
    serde_json::from_slice(bytes).map_err(|_| OpenSpecReadError::CliOutputUnreadable)
}

fn requirement_of(show: ShowRequirement) -> Requirement {
    Requirement {
        name: show.name,
        text: show.text,
        scenarios: show
            .scenarios
            .into_iter()
            .map(|s| Scenario { name: s.name, text: s.raw_text })
            .collect(),
    }
}

/// OSM-FR-APVF: the validation of the first item, which is the change named.
pub fn parse_validation(bytes: &[u8]) -> Result<ChangeValidation, OpenSpecReadError> {
    let out: ValidateOutput = decode(bytes)?;
    let item = out.items.into_iter().next().ok_or(OpenSpecReadError::CliOutputUnreadable)?;
    Ok(ChangeValidation { valid: item.valid, issues: item.issues })
}

/// OSM-FR-KWWM: the name of the newest folder `<YYYY-MM-DD>-<name>` of
/// `openspec/changes/archive/` in the working copy, by its date, or `None`
/// where none stands. A listing that fails reads as none.
pub fn archived_as(at: &RootFs, name: &str) -> Option<String> {
    let archive = at.path().join("openspec/changes/archive");
    let entries = at.list_dir(archive).ok()?;
    entries
        .into_iter()
        .filter(|entry| entry.kind == EntryKind::Dir)
        .filter(|entry| archive_folder_of(&entry.name, name))
        .map(|entry| entry.name)
        // The date leads the name and is of fixed width, so the text order is
        // the date order.
        .max()
}

/// OAR-FR-XTLB, OSM-FR-KWWM: whether `folder` is one path segment of the form
/// `<YYYY-MM-DD>-<name>`.
pub fn archive_folder_of(folder: &str, name: &str) -> bool {
    let Some(date) = folder.strip_suffix(name).and_then(|rest| rest.strip_suffix('-')) else {
        return false;
    };
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
}

/// OSM-FR-LLIO: a path the CLI reports, relative to the change folder, or
/// `None` where it lies outside that folder.
fn change_relative(path: &str, change_dir: &std::path::Path) -> Option<String> {
    let path = crate::changes::canonicalize_lenient(std::path::Path::new(path));
    let base = crate::changes::canonicalize_lenient(change_dir);
    path.strip_prefix(&base).ok().map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

// ---------------------------------------------------------------------------
// The seam
// ---------------------------------------------------------------------------

impl OpenSpec {
    /// OSM-FR-DDHL, OSM-FR-XSOC: the mode on the active worktree, then a usable
    /// CLI checked in the working copy of the location.
    fn ready_at<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        name: &str,
    ) -> Result<PathBuf, OpenSpecReadError> {
        let active = active.ok_or(OpenSpecReadError::NoOpenProject)?;
        if !Self::mode_active(Some(active)) {
            return Err(OpenSpecReadError::OpenSpecModeInactive);
        }
        if !valid_change_name(name) {
            return Err(OpenSpecReadError::InvalidChangeName);
        }
        match self.cli_state_in(sink, buffer, Some(at)) {
            super::OpenSpecCliState::Available { path, .. } => Ok(PathBuf::from(path)),
            other => Err(OpenSpecReadError::CliUnavailable(other)),
        }
    }

    /// OSM-FR-XSOC, OSM-FR-IHUO: an open project, an active mode, and a valid
    /// name, before any probe of the working copy.
    fn mode_and_name(&self, active: Option<&RootFs>, name: &str) -> Result<(), OpenSpecReadError> {
        let active = active.ok_or(OpenSpecReadError::NoOpenProject)?;
        if !Self::mode_active(Some(active)) {
            return Err(OpenSpecReadError::OpenSpecModeInactive);
        }
        if !valid_change_name(name) {
            return Err(OpenSpecReadError::InvalidChangeName);
        }
        Ok(())
    }

    /// OSM-FR-BCJO, OSM-FR-LLIO, OSM-FR-NMHS, OSM-FR-PYHP: one change, whole.
    pub fn read_change<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        location: ChangeLocation,
        name: &str,
    ) -> Result<ChangeDetail, OpenSpecReadError> {
        // OSM-FR-BCJO, OSM-FR-KWWM: the folder is checked before the CLI, so
        // a change that is not there, or that an archive moved, starts no
        // process.
        let rel = PathBuf::from("openspec/changes").join(name);
        let folder = || -> Result<(), OpenSpecReadError> {
            let is_folder = at
                .file_info(at.path().join(&rel))
                .map(|info| info.kind == EntryKind::Dir)
                .unwrap_or(false);
            if is_folder {
                return Ok(());
            }
            Err(match archived_as(at, name) {
                Some(archived_as) => OpenSpecReadError::ChangeArchived { archived_as },
                None => OpenSpecReadError::ChangeNotFound,
            })
        };
        let result = self
            .mode_and_name(active, name)
            .and_then(|()| folder())
            .and_then(|()| self.ready_at(sink, buffer, active, at, name));
        let result = result.and_then(|exe| {
            let status_args = ["status", "--change", name, "--json"];
            let status: StatusChange =
                decode(&self.run_logged(sink, buffer, &exe, "status", &status_args, at.path())?)?;
            let show_args =
                ["show", name, "--type", "change", "--json", "--deltas-only", "--no-interactive"];
            let show: ShowChange =
                decode(&self.run_logged(sink, buffer, &exe, "show", &show_args, at.path())?)?;
            let list_args = ["list", "--changes", "--json"];
            let list = self.run_logged(sink, buffer, &exe, "list", &list_args, at.path())?;
            let summary = reads::parse_changes(&list, b"{\"changes\":[]}")?
                .into_iter()
                .find(|c| c.name == name);
            let change_dir = at.path().join(&rel);
            let artifacts = status
                .artifacts
                .into_iter()
                .map(|a| {
                    let paths = status
                        .artifact_paths
                        .get(&a.id)
                        .map(|p| p.existing_output_paths.clone())
                        .unwrap_or_default();
                    let mut files: Vec<ChangeFile> = paths
                        .iter()
                        .filter_map(|p| change_relative(p, &change_dir))
                        .map(|path| {
                            let text = at.read_text(change_dir.join(&path)).ok();
                            ChangeFile { unreadable: text.is_none(), text, path }
                        })
                        .collect();
                    files.sort_by(|x, y| x.path.cmp(&y.path));
                    ChangeArtifact { id: a.id, status: a.status, files }
                })
                .collect();
            let deltas = self.deltas_with_live(sink, buffer, &exe, at, show.deltas)?;
            Ok(ChangeDetail {
                name: name.to_string(),
                location,
                schema: status.schema_name,
                is_complete: status.is_complete,
                completed_tasks: summary.as_ref().map_or(0, |s| s.completed_tasks),
                total_tasks: summary.as_ref().map_or(0, |s| s.total_tasks),
                artifacts,
                deltas,
            })
        });
        match &result {
            Ok(detail) => logging::log_info(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec change read",
                log_fields! {
                    "location" => detail.location.kind(),
                    "artifacts" => detail.artifacts.len(),
                    "deltas" => detail.deltas.len(),
                },
            ),
            Err(error) => logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec read failed",
                log_fields! { "read" => "read_change", "code" => error.code() },
            ),
        }
        result
    }

    /// OSM-FR-NMHS, OSM-FR-PYHP: each delta with the live requirement of its
    /// name, one `show` for each capability that the working copy holds.
    fn deltas_with_live<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        exe: &std::path::Path,
        at: &RootFs,
        deltas: Vec<ShowDelta>,
    ) -> Result<Vec<ChangeDelta>, OpenSpecReadError> {
        let mut live: HashMap<String, Option<Vec<Requirement>>> = HashMap::new();
        for delta in &deltas {
            if live.contains_key(&delta.spec) {
                continue;
            }
            let file = format!("{}/{}/{}", detection::SPECS_DIR, delta.spec, detection::SPEC_FILE);
            let held =
                reads::valid_capability_id(&delta.spec) && detection::is_regular_file(at, &file);
            let requirements = if held {
                let args =
                    ["show", delta.spec.as_str(), "--type", "spec", "--json", "--no-interactive"];
                let out = self.run_logged(sink, buffer, exe, "show", &args, at.path())?;
                Some(reads::parse_capability(&out)?.requirements)
            } else {
                None
            };
            live.insert(delta.spec.clone(), requirements);
        }
        Ok(deltas
            .into_iter()
            .map(|delta| {
                let held = live.get(&delta.spec).cloned().flatten();
                let wanted = match (&delta.rename, &delta.requirement) {
                    (Some(rename), _) => Some(rename.from.clone()),
                    (None, Some(requirement)) => Some(requirement.name.clone()),
                    (None, None) => None,
                };
                let live = match (&held, wanted) {
                    (Some(requirements), Some(name)) => requirements
                        .iter()
                        .find(|r| r.name.trim().eq_ignore_ascii_case(name.trim()))
                        .cloned(),
                    _ => None,
                };
                ChangeDelta {
                    capability_exists: held.is_some(),
                    capability: delta.spec,
                    operation: delta.operation,
                    requirement: if delta.rename.is_some() {
                        None
                    } else {
                        delta.requirement.map(requirement_of)
                    },
                    rename: delta.rename,
                    live,
                }
            })
            .collect())
    }

    /// OSM-FR-APVF, OSM-FR-XDRN: `openspec validate` of one change, where an
    /// exit status of 1 with readable output says the change is not valid.
    pub fn validate_change<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        name: &str,
    ) -> Result<ChangeValidation, OpenSpecReadError> {
        let result = self.ready_at(sink, buffer, active, at, name).and_then(|exe| {
            let args =
                ["validate", name, "--type", "change", "--strict", "--json", "--no-interactive"];
            let out = self
                .run_accepting(sink, buffer, &exe, "validate", &args, at.path(), &[0, 1])
                .map_err(OpenSpecReadError::from)?;
            parse_validation(&out)
        });
        match &result {
            Ok(validation) => logging::log_info(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec change validated",
                log_fields! { "valid" => validation.valid, "issues" => validation.issues.len() },
            ),
            Err(error) => logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec read failed",
                log_fields! { "read" => "validate_change", "code" => error.code() },
            ),
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Locations and the Tauri command
// ---------------------------------------------------------------------------

/// OSM-FR-MFVN: the working copy of a location, governed by the application's
/// own instance, which allows `short_data_dir()` (FSA-FR-21).
pub fn working_copy<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    location: &ChangeLocation,
) -> Result<RootFs, OpenSpecReadError> {
    use tauri::Manager;
    let active = || {
        app.try_state::<crate::project::ProjectState>()
            .and_then(|p| p.require_root().ok())
            .ok_or(OpenSpecReadError::NoOpenProject)
    };
    let shared = || {
        app.try_state::<crate::fs::FsAccessState>()
            .and_then(|state| state.get())
            .ok_or(OpenSpecReadError::NoOpenProject)
    };
    match location {
        ChangeLocation::ActiveWorktree => active(),
        ChangeLocation::Stream { stream_id } => {
            let stream = crate::streams::stream_by_id(app, stream_id)
                .ok_or(OpenSpecReadError::UnknownStream)?;
            Ok(RootFs::new(stream.worktree(), shared()?))
        }
        ChangeLocation::ReviewCheckout { root } => Ok(RootFs::new(root.clone(), shared()?)),
    }
}

/// OSM-FR-MBTI: `"read openspec change"`, on a blocking thread.
#[tauri::command]
pub async fn read_openspec_change<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    location: ChangeLocation,
    name: String,
) -> Result<ChangeDetail, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let seam =
            app.try_state::<OpenSpec>().ok_or_else(|| "openspec_mode_inactive".to_string())?;
        let active =
            app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
        let at = working_copy(&app, &location).map_err(|e| e.code().to_string())?;
        // OSM-FR-GMKV: `change_archived` carries its archive folder.
        seam.read_change(&app, &logging::BUFFER, active.as_ref(), &at, location, &name)
            .map_err(|e| e.wire_code())
    })
    .await
    .map_err(|e| format!("openspec change read failed: {e}"))?
}
