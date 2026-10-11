//! The builder seam and the Tauri event sink of the image build (PSS-FR-26..PSS-FR-29).

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
