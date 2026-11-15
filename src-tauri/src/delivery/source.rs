//! `DSR-delivery-source.md`: where the project's one delivery plan lives, and
//! the two moves of the plan between its homes.
//!
//! The source is the GitHub Project of `GPP-github-polling.md`, which
//! `GDP-github-delivery-plan.md` reads and writes, or one Flow file of the
//! project. The settings of [`crate::delivery::settings`] hold the choice and
//! the Type mapping. [`publish`] moves a Flow file to GitHub, [`export`] moves
//! the GitHub plan to a new Flow file, and [`milestones`] answers the
//! Dashboard's Delivery widget for either source.
//!
//! No log record of this module carries a token, a Type name, an item name, a
//! description, or a URL query (DSR-FR-LYMF): a record names the operation,
//! the outcome, the code, and counts.

pub mod export;
pub mod milestones;
pub mod publish;

use crate::project::OpenProject as _;
use crate::project_scope::events::EmitToProject;
use std::collections::HashSet;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::delivery::settings::{DeliverySettings, DeliverySourceChoice, IssueKind, ReadEntry};
use crate::delivery::DeliveryError;
use crate::fs::RootFs;
use crate::github_polling::delivery_access;
use crate::github_polling::GithubPollingConfiguration;
use crate::logging::{self, Domain, Fields, LogLevel};

/// DSR-FR-NFFP: a `file` choice whose artifact is not a Flow of the project.
pub const ERR_NOT_A_FLOW: &str = "not_a_flow";
/// DSR-FR-CPYM.
pub const ERR_INVALID_SETTINGS: &str = "invalid_delivery_settings";
/// DSR-FR-RCLA.
pub const ERR_NOT_FILE_SOURCE: &str = "not_file_source";
/// DSR-FR-OIAS, GDP-FR-QSHX.
pub const ERR_NOT_GITHUB_SOURCE: &str = "not_github_source";
/// DSR-FR-GXSE, DSR-FR-SKPX.
pub const ERR_FLOW_INVALID: &str = "flow_invalid";
/// DSR-FR-OIAS.
pub const ERR_PATH_EXISTS: &str = "path_exists";
/// The project settings file cannot be read (PSS-FR-10).
pub const ERR_SETTINGS_UNREADABLE: &str = "project_settings_unreadable";

/// DSR-FR-EGFF: the event each change of the source emits. Kebab-case,
/// because Tauri refuses a space in an event name at `emit`.
pub const DELIVERY_SOURCE_CHANGED: &str = "delivery-source-changed";

/// DSR-FR-ZKGG: why the source resolves to `none`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoneReason {
    NoSource,
    GithubProjectUnset,
    DeliveryFileMissing,
}

/// DSR-FR-EDHP: the selected Project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProject {
    pub node_id: String,
    pub title: String,
    pub owner_login: String,
    pub number: u64,
    /// The HTTPS address of the Project, from the read cache. It is empty
    /// until a read of the Project is cached.
    pub url: String,
}

/// DSR-FR-EDHP: the polling repository.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SourceRepository {
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// DSR-FR-AECK to DSR-FR-ZKGG: the resolved source, in the frontend shape
/// `DeliverySource` of `src/types/delivery.ts`.
// The answer of one command, made and sent once, so a box would save
// nothing that matters.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum DeliverySource {
    Github {
        project: SourceProject,
        repository: SourceRepository,
        configuration: GithubPollingConfiguration,
    },
    File {
        flow_id: String,
        path: String,
    },
    None {
        reason: NoneReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        flow_id: Option<String>,
    },
}

/// DSR-FR-AECK: the resolution of the choice, before the details of a
/// `github` source are read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    Github { project_node_id: String },
    File { flow_id: String },
    None { reason: NoneReason, flow_id: Option<String> },
}

/// DSR-FR-AECK, DSR-FR-EDHP, DSR-FR-RHUM, DSR-FR-ZKGG: resolve `choice`.
/// `project_node_id` is the selected GitHub Project, and `is_flow` tells
/// whether an artifact id names a `flow`-typed file. Pure.
pub fn resolve_choice(
    choice: &DeliverySourceChoice,
    project_node_id: Option<&str>,
    is_flow: impl Fn(&str) -> bool,
) -> Resolved {
    let project = project_node_id.filter(|p| !p.trim().is_empty());
    match choice {
        DeliverySourceChoice::Github => match project {
            Some(p) => Resolved::Github { project_node_id: p.trim().to_string() },
            None => Resolved::None { reason: NoneReason::GithubProjectUnset, flow_id: None },
        },
        DeliverySourceChoice::Automatic => match project {
            Some(p) => Resolved::Github { project_node_id: p.trim().to_string() },
            None => Resolved::None { reason: NoneReason::NoSource, flow_id: None },
        },
        DeliverySourceChoice::File { flow_id } => match is_flow(flow_id) {
            true => Resolved::File { flow_id: flow_id.clone() },
            false => Resolved::None {
                reason: NoneReason::DeliveryFileMissing,
                flow_id: Some(flow_id.clone()),
            },
        },
    }
}

/// DSR-FR-RHUM, DSR-FR-NFFP: whether `flow_id` names a file of the project
/// whose resolved type is `flow` (ASC-FR-06).
pub fn is_flow_file(root: &RootFs, flow_id: &str) -> bool {
    let Ok(path) = crate::fs::resolve_under(root.path(), flow_id) else { return false };
    root.file_info(&path).is_ok_and(|i| i.kind == crate::fs::EntryKind::File)
        && crate::artifacts::resolved_type(root, flow_id)
            == Some(crate::scanning::ArtifactType::Flow)
}

/// The stored delivery settings, with defaults applied.
pub fn load_settings(root: &RootFs) -> Result<DeliverySettings, DeliveryError> {
    crate::project_settings::load_delivery_settings_from(root)
        .map_err(|reason| DeliveryError::new(ERR_SETTINGS_UNREADABLE, reason))
}

/// DSR-FR-AECK: the settings and the resolution of their choice, read from
/// disk alone. It makes no request.
pub fn resolve_local(root: &RootFs) -> Result<(DeliverySettings, Resolved), DeliveryError> {
    let settings = load_settings(root)?;
    let polling = crate::project_settings::load_github_polling_settings_from(root)
        .map_err(|reason| DeliveryError::new(ERR_SETTINGS_UNREADABLE, reason))?;
    let resolved = resolve_choice(&settings.source, polling.project_node_id.as_deref(), |id| {
        is_flow_file(root, id)
    });
    Ok((settings, resolved))
}

/// DSR-FR-CPYM: the settings with each Type name trimmed, or
/// `invalid_delivery_settings`. No Type is checked against GitHub. Pure.
pub fn validate_settings(settings: &DeliverySettings) -> Result<DeliverySettings, DeliveryError> {
    let invalid = |message: &str| DeliveryError::new(ERR_INVALID_SETTINGS, message);
    let mut out = settings.clone();
    out.create_types.epic = out.create_types.epic.trim().to_string();
    out.create_types.feature = out.create_types.feature.trim().to_string();
    out.create_types.task = out.create_types.task.trim().to_string();
    out.create_types.defect = out.create_types.defect.trim().to_string();
    out.read_entries = out
        .read_entries
        .iter()
        .map(|e| ReadEntry { type_name: e.type_name.trim().to_string(), kind: e.kind })
        .collect();
    let create: Vec<&str> = IssueKind::ALL.iter().map(|k| out.create_types.of(*k)).collect();
    if create.iter().any(|n| n.is_empty())
        || out.read_entries.iter().any(|e| e.type_name.is_empty())
    {
        return Err(invalid("A Type name is empty."));
    }
    if create.iter().collect::<HashSet<_>>().len() != create.len() {
        return Err(invalid("Two kinds share one create Type."));
    }
    let mut seen = HashSet::new();
    for entry in &out.read_entries {
        if !seen.insert(entry.type_name.as_str()) {
            return Err(invalid("Two read entries share one Type."));
        }
        if create.contains(&entry.type_name.as_str()) {
            return Err(invalid("A read entry repeats a create Type."));
        }
    }
    if let DeliverySourceChoice::File { flow_id } = &mut out.source {
        *flow_id = flow_id.trim().to_string();
    }
    Ok(out)
}

/// DSR-FR-LYMF: one record of this module. Every record carries the domain
/// `backend`; a record about GitHub also carries `remote`.
pub(crate) fn log<R: Runtime>(
    app: &AppHandle<R>,
    level: LogLevel,
    remote: bool,
    message: &str,
    fields: Fields,
) {
    let domains: &[Domain] =
        if remote { &[Domain::Backend, Domain::Remote] } else { &[Domain::Backend] };
    logging::log(app, crate::delivery::github::buffer_of(app), level, domains, message, fields);
}

/// DSR-FR-LYMF, DSR-FR-CFQA: log a refusal and return it.
pub(crate) fn refuse<R: Runtime>(
    app: &AppHandle<R>,
    operation: &str,
    started: Instant,
    error: DeliveryError,
) -> DeliveryError {
    log(
        app,
        LogLevel::Warn,
        false,
        "delivery source operation refused",
        crate::log_fields! {
            "operation" => operation,
            "code" => &error.code,
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    error
}

/// DSR-FR-CFQA: the root of the open project, or `no_project_open`.
pub(crate) fn project_root<R: Runtime>(app: &AppHandle<R>) -> Result<RootFs, DeliveryError> {
    app.open_project_state()
        .and_then(|p| p.require_root().ok())
        .ok_or_else(DeliveryError::no_project)
}

/// DSR-FR-EDHP: the details of a `github` source. They come from the read
/// cache of the Project where one exists, so the answer needs no request;
/// otherwise the title is the one of the configuration, the owner, number,
/// and URL are empty, and the repository is resolved through GitHub
/// publication.
fn github_source<R: Runtime>(app: &AppHandle<R>, project_node_id: &str) -> DeliverySource {
    let configuration = delivery_access::configuration(app)
        .unwrap_or_else(|_| GithubPollingConfiguration::unchecked());
    let cache = app
        .open_project_state()
        .and_then(|p| crate::delivery::machine_store(&p).ok())
        .and_then(|store| {
            crate::delivery::github::cache::read_cache(
                app,
                crate::delivery::github::buffer_of(app),
                &store,
                project_node_id,
            )
            .ok()
            .flatten()
        });
    let project = SourceProject {
        node_id: project_node_id.to_string(),
        title: cache
            .as_ref()
            .map(|c| c.project.title.clone())
            .or_else(|| configuration.project_title.clone())
            .unwrap_or_default(),
        owner_login: cache.as_ref().map(|c| c.project.owner_login.clone()).unwrap_or_default(),
        number: cache.as_ref().map(|c| c.project.number).unwrap_or_default(),
        url: cache.as_ref().map(|c| c.project.url.clone()).unwrap_or_default(),
    };
    let repository = match &cache {
        Some(c) => SourceRepository {
            host: c.repository.host.clone(),
            owner: c.repository.owner.clone(),
            name: c.repository.name.clone(),
        },
        None => {
            let known = delivery_access::known_repository(app).or_else(|| {
                let token = delivery_access::project_token(app).ok();
                delivery_access::polling_repository(app, token.as_ref()).ok()
            });
            known
                .map(|r| SourceRepository { host: r.host, owner: r.owner, name: r.name })
                .unwrap_or_default()
        }
    };
    DeliverySource::Github { project, repository, configuration }
}

/// DSR-FR-AECK to DSR-FR-ZKGG: the resolved source of the open project.
pub fn resolve_source<R: Runtime>(app: &AppHandle<R>) -> Result<DeliverySource, DeliveryError> {
    let root = project_root(app)?;
    let (_, resolved) = resolve_local(&root)?;
    Ok(match resolved {
        Resolved::Github { project_node_id } => github_source(app, &project_node_id),
        Resolved::File { flow_id } => DeliverySource::File { path: flow_id.clone(), flow_id },
        Resolved::None { reason, flow_id } => DeliverySource::None { reason, flow_id },
    })
}

/// DSR-FR-EGFF: announce the resolved source.
pub(crate) fn announce<R: Runtime>(app: &AppHandle<R>) {
    match resolve_source(app) {
        Ok(source) => {
            let _ = app.emit_project(DELIVERY_SOURCE_CHANGED, source);
        }
        Err(e) => log(
            app,
            LogLevel::Warn,
            false,
            "delivery source change not announced",
            crate::log_fields! { "code" => &e.code },
        ),
    }
}

/// Store `settings` as they are. The callers validate them.
pub(crate) fn store_settings(
    root: &RootFs,
    settings: &DeliverySettings,
) -> Result<(), DeliveryError> {
    crate::project_settings::save_delivery_settings_to(root, settings)
        .map_err(|reason| DeliveryError::new(ERR_SETTINGS_UNREADABLE, reason))
}

/// DSR contract surface `"get delivery settings"`, with defaults applied.
pub fn get_settings_impl<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<DeliverySettings, DeliveryError> {
    let started = Instant::now();
    let op = "get_delivery_settings";
    let root = project_root(app).map_err(|e| refuse(app, op, started, e))?;
    let settings = load_settings(&root).map_err(|e| refuse(app, op, started, e))?;
    log(
        app,
        LogLevel::Debug,
        false,
        "delivery settings read",
        crate::log_fields! {
            "source" => settings.source.as_str(),
            "read_entries" => settings.read_entries.len(),
        },
    );
    Ok(settings)
}

/// DSR-FR-NFFP, DSR-FR-CPYM, DSR-FR-EGFF: validate and store the settings.
pub fn set_settings_impl<R: Runtime>(
    app: &AppHandle<R>,
    settings: DeliverySettings,
) -> Result<DeliverySettings, DeliveryError> {
    let started = Instant::now();
    let op = "set_delivery_settings";
    log(app, LogLevel::Debug, false, "delivery settings write started", Fields::new());
    let root = project_root(app).map_err(|e| refuse(app, op, started, e))?;
    let settings = validate_settings(&settings).map_err(|e| refuse(app, op, started, e))?;
    if let DeliverySourceChoice::File { flow_id } = &settings.source {
        if !is_flow_file(&root, flow_id) {
            let error =
                DeliveryError::new(ERR_NOT_A_FLOW, "The file is not a Flow of the project.");
            return Err(refuse(app, op, started, error));
        }
    }
    store_settings(&root, &settings).map_err(|e| refuse(app, op, started, e))?;
    log(
        app,
        LogLevel::Info,
        false,
        "delivery settings saved",
        crate::log_fields! {
            "source" => settings.source.as_str(),
            "read_entries" => settings.read_entries.len(),
            "untyped_set" => settings.untyped.is_some(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    announce(app);
    Ok(settings)
}

/// DSR contract surface `"get delivery source"`.
pub fn get_source_impl<R: Runtime>(app: &AppHandle<R>) -> Result<DeliverySource, DeliveryError> {
    let started = Instant::now();
    let op = "get_delivery_source";
    let source = resolve_source(app).map_err(|e| refuse(app, op, started, e))?;
    let kind = match &source {
        DeliverySource::Github { .. } => "github",
        DeliverySource::File { .. } => "file",
        DeliverySource::None { .. } => "none",
    };
    log(
        app,
        LogLevel::Debug,
        false,
        "delivery source resolved",
        crate::log_fields! { "kind" => kind, "ms" => started.elapsed().as_millis() as u64 },
    );
    Ok(source)
}

/// Run a command body off the window thread: a body can make GitHub requests.
pub(crate) async fn off_thread<T, F>(work: F) -> Result<T, DeliveryError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, DeliveryError> + Send + 'static,
{
    crate::project_scope::context::spawn_blocking(work).await.map_err(|_| {
        DeliveryError::new(
            crate::github_polling::records::ERR_GITHUB_UNREACHABLE,
            "The operation stopped unexpectedly.",
        )
    })?
}

/// DSR contract surface: `"get delivery settings"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn get_delivery_settings(app: AppHandle) -> Result<DeliverySettings, DeliveryError> {
    get_settings_impl(&app)
}

/// DSR contract surface: `"set delivery settings (settings)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn set_delivery_settings(
    app: AppHandle,
    settings: DeliverySettings,
) -> Result<DeliverySettings, DeliveryError> {
    off_thread(move || set_settings_impl(&app, settings)).await
}

/// DSR contract surface: `"get delivery source"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn get_delivery_source(app: AppHandle) -> Result<DeliverySource, DeliveryError> {
    off_thread(move || get_source_impl(&app)).await
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_publish;
