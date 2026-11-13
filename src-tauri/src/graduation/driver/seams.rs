//! The seams the loop is driven through (GRL-FR-UVJP, GXD-FR-DRFF).
//!
//! Split out of one module file that had grown past five thousand lines.
//! What the loop does and what it hands an agent are unchanged; the module
//! root re-exports every name, so no caller's path moved.

use super::*;
use crate::tools::agent_exec::image_probe::{ImageProbeOutcome, ImageProbeRequest};

// ---------------------------------------------------------------------------
// The seams (GRL-FR-UVJP, GXD-FR-DRFF)
// ---------------------------------------------------------------------------

/// What one dispatch returns, boxed so the seam is object-safe.
///
/// `execute_agent_cli` is `async`, and an `async fn` in a trait returns an
/// anonymous `impl Future` that cannot be named behind a `dyn`. Erasing it here
/// is what lets the loop hold `&dyn GraduationDispatch` rather than being
/// generic over its executor at every call site.
pub type DispatchFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<AgentExecution, AgentExecutionError>> + Send + 'a>,
>;

/// What one image probe returns, boxed as [`DispatchFuture`] is.
pub type ProbeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<Output = Result<ImageProbeOutcome, AgentExecutionError>>
            + Send
            + 'a,
    >,
>;

/// GXD-FR-DRFF: the single seam through which a graduation reaches an execution
/// agent, in the substitution sense as well as the routing one.
///
/// The loop is handed an implementation rather than constructing one.
/// [`AgentCliDispatch`] is what production binds, at the one place a run is
/// handed to the loop; a build under test binds a scripted implementation that
/// returns any envelope of GXD-FR-LBYG or any outcome of GXD-FR-XEUX with no
/// integration resolved, no credential readable, and no container runtime
/// present. No command, project setting, or provider record selects between
/// them, so a substitution is a test's act rather than a configuration.
///
/// Generic over the Tauri runtime rather than over the whole loop: `R` is a
/// parameter of the *trait*, so `dyn GraduationDispatch<R>` is still a trait
/// object for whichever concrete runtime the caller has.
pub trait GraduationDispatch<R: tauri::Runtime>: Send + Sync {
    fn dispatch_graduation_turn<'a>(
        &'a self,
        app: &'a tauri::AppHandle<R>,
        // Which project's integration to resolve (per AIC-FR-16). A fact of the
        // run rather than anything the seam chooses.
        project_key: &'a str,
        request: AgentExecutionRequest,
    ) -> DispatchFuture<'a>;

    /// OFP-FR-BZDP, EAC-FR-WYYZ: the image probe of an OpenSpec run, through
    /// `probe_agent_image`. It is not a turn (OFP-FR-JBSO).
    fn probe_graduation_image<'a>(
        &'a self,
        app: &'a tauri::AppHandle<R>,
        project_key: &'a str,
        request: ImageProbeRequest,
    ) -> ProbeFuture<'a>;

    /// GXD-FR-TBOW: the `review verdict` record of a review is durable, and the
    /// verdict has had no effect yet. The loop calls this on its own thread and
    /// waits for it.
    ///
    /// Production does nothing here. A scripted implementation uses it to land
    /// an act between the verdict and the apply, a moment no turn can reach
    /// (GTE-FR-KVYE).
    fn review_verdict_recorded(&self) {}
}

/// The production dispatch: one call to `../tools/EAC-execute-agent-cli.md`'s
/// internal `execute_agent_cli` API (GXD-FR-KXXB).
///
/// This type invokes no Docker command, no vendor CLI, and no Tauri command to
/// run an agent; supplies no vendor, model, reasoning effort, image, or
/// credential, there being no field of `AgentExecutionRequest` that could carry
/// one; and assembles no vendor or container argument.
pub struct AgentCliDispatch;

impl<R: tauri::Runtime> GraduationDispatch<R> for AgentCliDispatch {
    fn dispatch_graduation_turn<'a>(
        &'a self,
        app: &'a tauri::AppHandle<R>,
        project_key: &'a str,
        request: AgentExecutionRequest,
    ) -> DispatchFuture<'a> {
        Box::pin(async move {
            // Resolved here rather than by the loop, so the loop holds nothing
            // that could route around the seam.
            let parts = LaunchParts::resolve(app)?;
            let context = parts.context(project_key);
            AgentCliExecutor::default().execute_agent_cli(app, &context, request).await
        })
    }

    fn probe_graduation_image<'a>(
        &'a self,
        app: &'a tauri::AppHandle<R>,
        project_key: &'a str,
        request: ImageProbeRequest,
    ) -> ProbeFuture<'a> {
        Box::pin(async move {
            let parts = LaunchParts::resolve(app)?;
            let context = parts.context(project_key);
            AgentCliExecutor::default().probe_agent_image(app, &context, request).await
        })
    }
}

/// What a launch and a probe resolve from the application, so the two read
/// the same settings, the same integrations, and the same project image.
struct LaunchParts<'a> {
    store: tauri::State<'a, crate::global_settings::GlobalSettingsStore>,
    integrations: tauri::State<'a, crate::agentic::AgenticIntegrations>,
    fs: Arc<crate::fs::FsAccess>,
    docker_in_docker: crate::tools::agent_exec::DockerInDockerLaunch,
    images: crate::tools::agent_exec::ProjectVendorImages,
}

impl<'a> LaunchParts<'a> {
    /// Each absence maps onto the pre-launch error GXD-FR-XEUX already
    /// classifies it as, rather than onto a second vocabulary meaning the
    /// same thing.
    fn resolve<R: tauri::Runtime>(
        app: &'a tauri::AppHandle<R>,
    ) -> Result<Self, AgentExecutionError> {
        let store =
            app.try_state::<crate::global_settings::GlobalSettingsStore>().ok_or_else(|| {
                AgentExecutionError::IntegrationUnresolved("no_settings_store".to_string())
            })?;
        let integrations =
            app.try_state::<crate::agentic::AgenticIntegrations>().ok_or_else(|| {
                AgentExecutionError::IntegrationUnresolved("no_integrations".to_string())
            })?;
        let fs = crate::graduation::store_fs(app).map_err(|_| {
            AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Inaccessible)
        })?;
        // EAC-FR-38: the image every launch of this run is created from is
        // the one the open project commits for the vendor it resolves to,
        // read from the project's own committed store (PSS-FR-30) rather
        // than from anything this module holds. The **project's** root, not
        // the run's execution directory: the configuration is a property of
        // the project and travels with it, where the execution directory is
        // a copy this run made.
        let project_root = app
            .try_state::<crate::project::ProjectState>()
            .and_then(|state| state.require_root().ok())
            .ok_or_else(|| {
                AgentExecutionError::VendorImageUnresolved(
                    crate::project_settings::images::ERR_VENDOR_IMAGE_UNCONFIGURED.to_string(),
                )
            })?;
        // EAC-FR-DKRS: read from the project-local store at every launch, so
        // Continue uses the settings and the socket that stand now. The
        // application reads them; the model has no route to them.
        let (docker_in_docker, images) =
            crate::tools::agent_exec::project_launch_inputs(project_root);
        Ok(LaunchParts { store, integrations, fs, docker_in_docker, images })
    }

    fn context<'b>(&'b self, project_key: &'b str) -> LaunchContext<'b> {
        LaunchContext {
            store: &self.store,
            integrations: &self.integrations,
            project_key,
            fs: &self.fs,
            images: &self.images,
            docker_in_docker: self.docker_in_docker.clone(),
        }
    }
}
