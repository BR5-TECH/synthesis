//! The builder seam, the Tauri event sink, and the preparation of the image
//! build (PSS-FR-26..PSS-FR-29).

use super::{image_builder, images};

/// The builder a build is performed through, held in state so a test swaps the
/// production one without touching a call site.
pub struct ImageBuilderSeam(pub std::sync::Arc<dyn images::ImageBuilder>);

impl Default for ImageBuilderSeam {
    fn default() -> Self {
        ImageBuilderSeam(std::sync::Arc::new(image_builder::RealImageBuilder))
    }
}

/// PSS-FR-27 / PSS-FR-28: the two events, emitted through the Tauri bus.
///
/// An emit that cannot be delivered is dropped rather than propagated, for the
/// same reason `ProgressSink` drops one: reporting must never block or fail the
/// build it is reporting on.
pub(super) struct AppBuildEvents<R: tauri::Runtime>(pub(super) tauri::AppHandle<R>);

impl<R: tauri::Runtime> images::BuildEvents for AppBuildEvents<R> {
    fn progress(&self, payload: images::ImageBuildProgress) {
        use tauri::Emitter;
        let _ = self.0.emit(images::PROJECT_IMAGE_BUILD_PROGRESS, payload);
    }

    fn finished(&self, payload: images::ImageBuildFinished) {
        use tauri::Emitter;
        let _ = self.0.emit(images::PROJECT_IMAGE_BUILD_FINISHED, payload);
    }
}

/// PSS-FR-26 / PSS-FR-BCTX: prepare the build of `vendor`'s configured image in
/// the project at `root`, through the verified backend of `store`. The root is
/// the active worktree root, and the context is the folder the entry names, or
/// that root by default. The command and the Docker-in-Docker acceptance test
/// (`../tools/EAC-execute-agent-cli.md` EAC-FR-UDQN) both prepare a build here.
pub fn prepare_project_vendor_image_build(
    root: &crate::fs::RootFs,
    store: &crate::global_settings::GlobalSettingsStore,
    vendor: &str,
    operation_id: &str,
    registry: &images::ImageBuildRegistry,
) -> Result<images::PreparedBuild, String> {
    let table = super::load_public(root)?;
    let entries = images::read_entries(&table);
    let project_root = root.path().to_path_buf();
    let context_root = match entries.get(vendor).and_then(|entry| entry.build_context.as_deref()) {
        Some(context) => images::validate_build_context(root, context),
        None => Ok(project_root.clone()),
    };
    images::prepare_build(
        entries.get(vendor),
        crate::docker::resolve_docker_backend(store),
        project_root,
        context_root,
        vendor,
        operation_id,
        registry,
    )
}
