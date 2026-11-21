//! Reads the facts that [`super::targets`] decides on, from the open project and
//! the machine. Every read is best effort: a store that cannot be read makes
//! the kinds that depend on it unavailable, never the whole listing fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tauri::{AppHandle, Manager, Runtime};

use super::model::*;
use super::targets::{Environment, WorktreeFact};
use crate::agentic::{AgenticIntegrations, IntegrationState, VendorKind};
use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;
use crate::project_scope::ProjectStores;

/// The project of the running code: its content root and its key.
pub struct ProjectFacts {
    pub root: crate::fs::RootFs,
    pub project_key: String,
}

pub fn project<R: Runtime>(app: &AppHandle<R>) -> Result<ProjectFacts, TerminalError> {
    let state = app.project_store::<ProjectState>();
    let root =
        state.require_root().map_err(|e| TerminalError::with_message("unknown_target", e))?;
    Ok(ProjectFacts { root, project_key: state.slot_key() })
}

/// The worktrees of the project, with the stream each one belongs to.
/// `None` where the enumeration fails: then no target is known to be gone.
pub fn worktrees<R: Runtime>(app: &AppHandle<R>, root: &Path) -> Option<Vec<WorktreeFact>> {
    let mut context = crate::worktree::context_for(root).ok()?;
    crate::streams::decorate_worktrees(app, &mut context.worktrees);
    context
        .worktrees
        .into_iter()
        .map(|entry| WorktreeFact {
            path: entry.path,
            name: entry.name,
            branch: entry.branch,
            is_active: entry.is_active,
            is_missing: entry.is_missing,
            stream: entry.stream.map(|s| (s.stream_id, s.stream_name, s.busy_run_id)),
        })
        .collect::<Vec<_>>()
        .into()
}

/// TRS-FR-LUUX: the run that holds the direct-run queue of a worktree.
pub fn busy_of_worktree<R: Runtime>(app: &AppHandle<R>) -> impl Fn(&str) -> Option<String> {
    let queue = crate::graduation::project_queue(app);
    move |path: &str| {
        let queue = queue.as_ref()?;
        let key = crate::graduation::worktree_queue_key(path);
        crate::graduation::holder_of_queue(queue, &key).map(|run| run.id.clone())
    }
}

/// The facts of [`Environment`] for the open project.
pub fn environment<R: Runtime>(app: &AppHandle<R>, facts: &ProjectFacts) -> Environment {
    let root = &facts.root;
    let agent_environment =
        crate::project_settings::agent_environment::load_agent_environment_from(root);
    let devcontainer_project = agent_environment.source
        == crate::project_settings::agent_environment::AgentEnvironmentSource::Devcontainer;
    let store = app.try_state::<GlobalSettingsStore>();
    let ai = app.try_state::<AgenticIntegrations>();
    let docker_verified =
        store.as_ref().is_some_and(|s| crate::docker::resolve_docker_backend(s).is_ok());
    let devcontainer_ready = devcontainer_project
        && store.as_ref().is_some_and(|s| {
            s.load_devcontainer_cli().ok().is_some_and(|record| {
                crate::devcontainer::cli::resolve_devcontainer_cli(&record).is_ok()
            })
        })
        && crate::devcontainer::state_of(root).config_exists;
    let work_vendor = match (&store, &ai) {
        (Some(store), Some(ai)) => {
            match crate::agentic::resolve_agentic_invocation(
                store,
                ai,
                &facts.project_key,
                Some("work"),
            ) {
                Ok(crate::agentic::AgenticInvocation::Cli { vendor, .. }) => {
                    CliVendor::from_id(&vendor)
                }
                _ => None,
            }
        }
        _ => None,
    };
    let vendor_images: BTreeSet<CliVendor> = CliVendor::ALL
        .into_iter()
        .filter(|vendor| {
            crate::project_settings::resolve_project_vendor_image(root, vendor.id()).is_ok()
        })
        .collect();
    let verified_vendors: BTreeMap<CliVendor, String> = match (&store, &ai) {
        (Some(store), Some(ai)) => crate::agentic::list_integrations_impl(store, ai)
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.kind == VendorKind::Cli && i.state == IntegrationState::Verified)
            .filter_map(|i| Some((CliVendor::from_id(&i.vendor)?, i.binary_path?)))
            .collect(),
        _ => BTreeMap::new(),
    };
    let effective_microsandbox = !devcontainer_project
        && store.as_ref().is_some_and(|s| {
            let Ok(resolved) = crate::isolation::resolve_isolation(s) else { return false };
            let dind =
                crate::project_settings::docker_in_docker::load_docker_in_docker_settings_from(
                    root,
                );
            matches!(
                crate::isolation::effective_isolation(
                    resolved,
                    dind.enabled,
                    dind.under_microsandbox
                ),
                crate::isolation::ResolvedIsolation::Microsandbox(_)
            )
        });
    Environment {
        devcontainer_project,
        docker_verified,
        devcontainer_ready,
        work_vendor,
        vendor_images,
        verified_vendors,
        effective_microsandbox,
    }
}
