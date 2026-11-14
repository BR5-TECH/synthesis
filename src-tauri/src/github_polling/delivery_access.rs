//! What the delivery modules read from GitHub polling
//! (`DSR-delivery-source.md` DSR-FR-EDHP, `GDP-github-delivery-plan.md`
//! GDP-FR-QSHX, GDP-FR-ZGJB).
//!
//! The delivery plan of a GitHub source uses the selected Project, the polling
//! repository (GPP-FR-RGNM), the token of the project, and the configuration
//! state (GPP-FR-DEZO) of this module. These functions give them to the
//! delivery modules, so the delivery modules keep no second copy of any of
//! them. No function here makes a request, except the repository resolution of
//! GitHub publication where the session does not know the repository yet.

use tauri::{AppHandle, Runtime};

use crate::github_tokens::ProjectToken;

use super::records::{GithubPollingConfiguration, GithubPollingSettings, RepositoryRef};

/// GPP-FR-NLPG: the polling settings of the open project.
pub(crate) fn polling_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<GithubPollingSettings, String> {
    let ctx = super::context(app)?;
    crate::project_settings::load_github_polling_settings_from(&ctx.root)
}

/// GPP-FR-DEZO: the configuration state the polling view reports for the
/// stored settings. It reads memory alone and makes no request.
pub(crate) fn configuration<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<GithubPollingConfiguration, String> {
    let ctx = super::context(app)?;
    let settings = crate::project_settings::load_github_polling_settings_from(&ctx.root)?;
    Ok(super::state(app).slot().configuration_for(&ctx.key, &settings))
}

/// The generation of the polling session, read before a configuration check
/// starts, so a settings write that races the check discards its result
/// (GPP-FR-ELWS).
pub(crate) fn generation<R: Runtime>(app: &AppHandle<R>) -> u64 {
    let state = super::state(app);
    let mut slot = state.slot();
    if let Ok(ctx) = super::context(app) {
        slot.enter(&ctx.key);
    }
    slot.generation()
}

/// GPP-FR-DEZO: record what a read of the Project found about the
/// configuration, and announce the change of the polling view (GPP-FR-TYOV).
pub(crate) fn record_configuration<R: Runtime>(
    app: &AppHandle<R>,
    generation: u64,
    project_id: &str,
    configuration: GithubPollingConfiguration,
) {
    let Ok(ctx) = super::context(app) else { return };
    {
        let state = super::state(app);
        let mut slot = state.slot();
        slot.enter(&ctx.key);
        slot.record_configuration(&ctx.key, generation, project_id, configuration);
    }
    super::announce(app, Vec::new());
}

/// GTS-FR-13: the token of the open project. The secret is passed to the
/// tracker adapter and never stored, returned, or logged.
pub(crate) fn project_token<R: Runtime>(app: &AppHandle<R>) -> Result<ProjectToken, String> {
    let ctx = super::context(app)?;
    super::require_token(app, &ctx.project_key)
}

/// GPP-FR-RGNM: the polling repository. The session's repository is used
/// where it is known; otherwise GitHub publication resolves it.
pub(crate) fn polling_repository<R: Runtime>(
    app: &AppHandle<R>,
    token: Option<&ProjectToken>,
) -> Result<RepositoryRef, String> {
    let ctx = super::context(app)?;
    super::session_repository(app, &ctx, token)
}

/// GPP-FR-RGNM: the polling repository that the session already knows, or
/// `None`. It makes no request.
pub(crate) fn known_repository<R: Runtime>(app: &AppHandle<R>) -> Option<RepositoryRef> {
    let ctx = super::context(app).ok()?;
    let state = super::state(app);
    let slot = state.slot();
    slot.session_for(&ctx.key).and_then(|s| s.repository.clone())
}
