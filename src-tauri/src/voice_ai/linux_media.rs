//! Microphone access on Linux (VOP-FR-MNTN).
//!
//! WebKitGTK denies each capture request unless the application answers its
//! `permission-request` signal, and the webview layer has no handler. This
//! grants an audio request from the application's own page and refuses the
//! rest. Other kinds of permission request stay with the default handler.

use tauri::Manager;
use webkit2gtk::glib::object::Cast;
use webkit2gtk::{
    PermissionRequestExt, SettingsExt, UserMediaPermissionRequest, UserMediaPermissionRequestExt,
    WebViewExt,
};

/// Whether a page URL is the application's own page.
fn is_own_page(uri: &str) -> bool {
    uri.starts_with("tauri://localhost") || uri.starts_with("http://localhost:1420")
}

pub fn install(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.with_webview(|platform| {
        let webview = platform.inner();
        if let Some(settings) = WebViewExt::settings(&webview) {
            settings.set_enable_media_stream(true);
        }
        webview.connect_permission_request(|webview, request| {
            let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() else {
                return false;
            };
            let own_page = webview.uri().is_some_and(|uri| is_own_page(uri.as_str()));
            if own_page && media.is_for_audio_device() && !media.is_for_video_device() {
                request.allow();
            } else {
                request.deny();
            }
            true
        });
    });
}

#[cfg(test)]
mod tests {
    use super::is_own_page;

    // VOP-FR-MNTN: only the application's own page gets the microphone.
    #[test]
    fn only_the_applications_own_page_is_own() {
        assert!(is_own_page("tauri://localhost/index.html"));
        assert!(is_own_page("http://localhost:1420/"));
        assert!(!is_own_page("https://example.com/"));
        assert!(!is_own_page("http://localhost:9999/"));
    }
}
