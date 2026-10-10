//! Every desktop other than macOS: delivery through `notify-rust`, with no
//! activation channel (`NTD-notification-delivery.md`).
//!
//! `notify-rust`'s `wait_for_action` is XDG-only and its Windows path returns no
//! handle, so `on_click` is dropped here and a notification posted on these
//! platforms shows but is never routed from.

use super::{NotificationRequest, NotificationSink, PermissionState, PostError};

/// The real notification centre. See the module documentation.
pub struct OsSink;

impl NotificationSink for OsSink {
    fn deliver(
        &self,
        _id: &str,
        request: &NotificationRequest,
        _on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError> {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&request.title);
        // NTD-FR-05: Windows has a subtitle line of its own. XDG has none, so
        // the subtitle goes on the first line of the body (NTD-FR-FVPT).
        #[cfg(target_os = "windows")]
        notification.subtitle(&request.subtitle).body(&request.body);
        #[cfg(not(target_os = "windows"))]
        notification.body(&super::platform::fold_subtitle(
            &request.subtitle,
            &request.body,
        ));
        notification
            .show()
            .map(|_| ())
            .map_err(|_| PostError::DeliveryFailed)
    }

    fn retract(&self, _id: &str) {}

    fn permission(&self) -> PermissionState {
        PermissionState::Granted
    }

    fn request_permission(&self) -> PermissionState {
        self.permission()
    }
}
