//! The macOS notification centre: `UNUserNotificationCenter`
//! (`NTD-notification-delivery.md`).
//!
//! The centre posts under the bundle's own identity, so every notification
//! carries the application's name and icon (NTD-FR-ZGHA). The centre is never
//! reached from a run outside an application bundle, because there it ends the
//! process; such a run reports `unsupported` instead (NTD-FR-15).
//!
//! A click reaches this module through a delegate that `install` registers
//! while the application starts (NTD-FR-DMYR). The delegate finds the click
//! handler by the notification's platform identifier, which carries a token of
//! this run, so a click on a notification of an earlier run finds nothing
//! (NTD-FR-13).

use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{LazyLock, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSArray, NSBundle, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationCategory, UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
    UNNotificationDismissActionIdentifier, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNNotificationSettings,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tauri::Manager;

use super::platform::{
    self, permission_from_answer, plan_response, platform_identifier, post_outcome,
    request_flow, run_token, ClickRoutes, ResponseAction, ResponsePlan,
};
use super::{NotificationRegistry, NotificationRequest, NotificationSink, PermissionState, PostError};
use crate::log_fields;
use crate::logging::{log_debug, log_info, log_warn, Domain, Fields, BUFFER};

/// The longest wait for the centre to answer a settings read or a post. The
/// centre answers in milliseconds; a centre that does not answer gives a
/// refused post rather than a held thread.
const ANSWER_LIMIT: Duration = Duration::from_secs(5);

/// The longest wait for the author to answer the permission prompt.
const PROMPT_LIMIT: Duration = Duration::from_secs(300);

/// NTD-FR-QSRQ: how the centre presents a notification while the application
/// is frontmost — the same banner and list entry as at any other time.
const FOREGROUND_PRESENTATION: UNNotificationPresentationOptions =
    UNNotificationPresentationOptions::Banner.union(UNNotificationPresentationOptions::List);

/// The category every notification carries. It asks the centre to report a
/// dismissal, so a dismissed notification stops being one that shows
/// (NTD-FR-06).
const CATEGORY: &str = "synthesis.notification";

/// NTD-FR-15: whether this run can reach the centre at all. Read once: the
/// bundle of a process does not change while it runs.
static BUNDLED: LazyLock<bool> = LazyLock::new(|| {
    let bundle = NSBundle::mainBundle();
    let path = bundle.bundlePath().to_string();
    let identifier = bundle.bundleIdentifier().map(|id| id.to_string());
    platform::is_app_bundle(&path, identifier.as_deref())
});

/// The click handlers of the notifications this run shows.
static ROUTES: LazyLock<ClickRoutes> = LazyLock::new(ClickRoutes::default);

/// The handle the log records of this module go through. Set by `install`;
/// before that, nothing here has anything to report.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

fn debug(message: &str, fields: Fields) {
    if let Some(app) = APP.get() {
        log_debug(app, &BUFFER, &[Domain::Backend], message, fields);
    }
}

fn info(message: &str, fields: Fields) {
    if let Some(app) = APP.get() {
        log_info(app, &BUFFER, &[Domain::Backend], message, fields);
    }
}

fn warn(message: &str, fields: Fields) {
    if let Some(app) = APP.get() {
        log_warn(app, &BUFFER, &[Domain::Backend], message, fields);
    }
}

/// The centre, or `None` where this run cannot reach it (NTD-FR-15). The call
/// is guarded against an Objective-C exception as well as by the bundle check,
/// because an exception that escapes ends the process.
fn center() -> Option<Retained<UNUserNotificationCenter>> {
    if !*BUNDLED {
        return None;
    }
    match objc2::exception::catch(UNUserNotificationCenter::currentNotificationCenter) {
        Ok(center) => Some(center),
        Err(_) => {
            warn("notification centre raised an exception", Fields::new());
            None
        }
    }
}

/// A one-element array of `identifier`, the shape every removal takes.
fn identifiers(identifier: &str) -> Retained<NSArray<NSString>> {
    NSArray::from_retained_slice(&[NSString::from_str(identifier)])
}

/// Remove the notification `identifier` from the centre, shown or waiting.
fn remove(center: &UNUserNotificationCenter, identifier: &str) {
    let ids = identifiers(identifier);
    center.removeDeliveredNotificationsWithIdentifiers(&ids);
    center.removePendingNotificationRequestsWithIdentifiers(&ids);
}

/// The domain and code of an `NSError` the centre handed back, if any.
fn error_parts(error: *mut NSError) -> Option<(String, i64)> {
    // SAFETY: the centre hands a valid `NSError` or null to its completion
    // handler, and the reference does not outlive the handler call.
    let error = unsafe { error.as_ref() }?;
    Some((error.domain().to_string(), error.code() as i64))
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Delegate` does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "SynthesisNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// NTD-FR-QSRQ: the same banner while the application is frontmost.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler.call((FOREGROUND_PRESENTATION,));
        }

        /// NTD-FR-09 / NTD-FR-10 / NTD-FR-11 / NTD-FR-13.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            // A panic must not unwind into the centre's own frames.
            let routed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                route_response(center, response)
            }));
            if routed.is_err() {
                warn("notification activation failed", Fields::new());
            }
            completion_handler.call(());
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: `NSObject`'s `init` is the designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// The action of `response`, in the terms of [`plan_response`].
fn response_action(response: &UNNotificationResponse) -> ResponseAction {
    let action = response.actionIdentifier();
    // SAFETY: the centre defines these constants for the life of the process.
    let (open, dismiss) = unsafe {
        (
            UNNotificationDefaultActionIdentifier,
            UNNotificationDismissActionIdentifier,
        )
    };
    if action.isEqualToString(open) {
        ResponseAction::Open
    } else if action.isEqualToString(dismiss) {
        ResponseAction::Dismiss
    } else {
        ResponseAction::Other
    }
}

/// Act on the response the author gave to a notification.
fn route_response(center: &UNUserNotificationCenter, response: &UNNotificationResponse) {
    let identifier = response.notification().request().identifier().to_string();
    match plan_response(run_token(), &identifier, response_action(response)) {
        ResponsePlan::Ignore => {
            // NTD-FR-13: a notification of an earlier run routes nowhere.
            debug(
                "notification response ignored",
                log_fields! { "identifier" => identifier.as_str() },
            );
            remove(center, &identifier);
        }
        ResponsePlan::Forget(id) => {
            // NTD-FR-11: a dismissal raises nothing and emits nothing. The
            // notification no longer shows, so the registry forgets it and a
            // later post with the same words shows again (NTD-FR-06).
            ROUTES.forget(&identifier);
            if let Some(app) = APP.get() {
                app.state::<NotificationRegistry>().withdraw(id);
            }
            debug(
                "notification dismissed",
                log_fields! { "notificationId" => id },
            );
        }
        ResponsePlan::Open(id) => {
            // NTD-FR-10: the route is given out once, and the notification
            // leaves the centre as part of the activation.
            let route = ROUTES.take(&identifier);
            remove(center, &identifier);
            let Some(route) = route else {
                debug(
                    "notification activation ignored",
                    log_fields! { "notificationId" => id, "reason" => "no route" },
                );
                return;
            };
            // The route raises a window and emits an event. It runs off the
            // centre's callback, so the centre is never held while a window
            // comes forward.
            let spawned = std::thread::Builder::new()
                .name("notification-activation".into())
                .spawn(route);
            if let Err(e) = spawned {
                warn(
                    "could not start the notification activation",
                    log_fields! { "error" => e.to_string() },
                );
            }
        }
    }
}

/// NTD-FR-DMYR: register the delegate before the application finishes its
/// start, and withdraw what an earlier run left in the centre.
pub fn install(app: &tauri::AppHandle) {
    let _ = APP.set(app.clone());
    let Some(center) = center() else {
        info(
            "notification centre unsupported",
            log_fields! { "reason" => "not an application bundle" },
        );
        return;
    };
    center.removeAllDeliveredNotifications();
    center.removeAllPendingNotificationRequests();
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(CATEGORY),
        &NSArray::<UNNotificationAction>::new(),
        &NSArray::<NSString>::new(),
        UNNotificationCategoryOptions::CustomDismissAction,
    );
    center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));
    let delegate = Delegate::new();
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // The centre holds its delegate weakly. The delegate must live as long as
    // the process, so this reference is never released.
    std::mem::forget(delegate);
    info("notification centre installed", Fields::new());
}

/// NTD-FR-02: read the authorisation status from the centre.
fn read_permission(center: &UNUserNotificationCenter) -> PermissionState {
    let (tx, rx) = mpsc::sync_channel::<i64>(1);
    let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: the centre hands a valid settings object for the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus().0 as i64;
        let _ = tx.try_send(status);
    });
    center.getNotificationSettingsWithCompletionHandler(&block);
    let answer = rx.recv_timeout(ANSWER_LIMIT).ok();
    if answer.is_none() {
        warn("notification settings read timed out", Fields::new());
    }
    permission_from_answer(answer)
}

/// The real notification centre. See the module documentation.
pub struct OsSink;

impl NotificationSink for OsSink {
    fn deliver(
        &self,
        id: &str,
        request: &NotificationRequest,
        on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError> {
        let center = center().ok_or(PostError::Unsupported)?;
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&request.title));
        content.setSubtitle(&NSString::from_str(&request.subtitle));
        content.setBody(&NSString::from_str(&request.body));
        content.setCategoryIdentifier(&NSString::from_str(CATEGORY));
        // NTD-FR-04: no sound is set, so the module plays none of its own.
        let identifier = platform_identifier(run_token(), id);
        let platform_request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&identifier),
            &content,
            None,
        );

        // The route is in place before the centre can show the notification,
        // so an immediate click finds it.
        ROUTES.store(identifier.clone(), on_click);
        let (tx, rx) = mpsc::sync_channel::<Option<(String, i64)>>(1);
        let block = RcBlock::new(move |error: *mut NSError| {
            let _ = tx.try_send(error_parts(error));
        });
        center.addNotificationRequest_withCompletionHandler(&platform_request, Some(&block));
        let answer = rx.recv_timeout(ANSWER_LIMIT).ok();
        match &answer {
            Some(None) => {}
            Some(Some((domain, code))) => warn(
                "notification centre refused a post",
                log_fields! { "notificationId" => id, "domain" => domain.as_str(), "code" => *code },
            ),
            None => warn(
                "notification post timed out",
                log_fields! { "notificationId" => id },
            ),
        }
        let Err(failure) = post_outcome(answer) else {
            return Ok(());
        };
        // NTD-FR-16: a failed post leaves nothing showing and nothing to route.
        ROUTES.forget(&identifier);
        remove(&center, &identifier);
        Err(failure)
    }

    fn retract(&self, id: &str) {
        let identifier = platform_identifier(run_token(), id);
        ROUTES.forget(&identifier);
        if let Some(center) = center() {
            remove(&center, &identifier);
        }
    }

    /// NTD-FR-06: one `id` gives one platform identifier, and the centre
    /// replaces a notification whose identifier it already shows.
    fn supports_replacement(&self) -> bool {
        true
    }

    fn permission(&self) -> PermissionState {
        match center() {
            Some(center) => read_permission(&center),
            None => PermissionState::Unsupported,
        }
    }

    fn request_permission(&self) -> PermissionState {
        let Some(center) = center() else {
            return PermissionState::Unsupported;
        };
        // NTD-FR-03: the answer is the state the centre reports after the
        // prompt.
        request_flow(
            || read_permission(&center),
            || ask_permission(&center),
        )
    }
}

/// Show the platform permission prompt and wait for the author's answer.
fn ask_permission(center: &UNUserNotificationCenter) {
    let (tx, rx) = mpsc::sync_channel::<Option<(String, i64)>>(1);
    let block = RcBlock::new(move |_granted: Bool, error: *mut NSError| {
        let _ = tx.try_send(error_parts(error));
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &block);
    match rx.recv_timeout(PROMPT_LIMIT) {
        Ok(Some((domain, code))) => warn(
            "notification permission request failed",
            log_fields! { "domain" => domain.as_str(), "code" => code },
        ),
        Ok(None) => {}
        Err(_) => warn("notification permission request timed out", Fields::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NTD-FR-QSRQ: a frontmost application gets the same banner and list
    // entry as a background one.
    #[test]
    fn the_foreground_presentation_is_a_banner_and_a_list_entry() {
        assert_eq!(
            FOREGROUND_PRESENTATION,
            UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List
        );
    }

    // NTD-FR-15: a test binary is not inside an application bundle, so the real
    // sink answers `unsupported` everywhere and never reaches the centre, which
    // would end this process.
    #[test]
    fn outside_a_bundle_the_real_sink_reports_unsupported() {
        assert!(!*BUNDLED);
        let sink = OsSink;
        assert_eq!(sink.permission(), PermissionState::Unsupported);
        assert_eq!(sink.request_permission(), PermissionState::Unsupported);
        let delivered = sink.deliver(
            "notif-unbundled",
            &NotificationRequest::default(),
            Box::new(|| {}),
        );
        assert_eq!(delivered, Err(PostError::Unsupported));
        assert!(ROUTES.take(&platform_identifier(run_token(), "notif-unbundled")).is_none());
        sink.retract("notif-unbundled");
        assert!(sink.supports_replacement());
    }
}
