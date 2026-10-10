//! The pure decisions behind the platform sinks (`NTD-notification-delivery.md`).
//!
//! Everything here compiles on every platform and touches no notification
//! centre, so each decision the macOS sink takes — whether this run may reach
//! the centre at all, which platform identifiers belong to this run, what a
//! platform status code means — is tested directly rather than through a
//! notification centre that a test runner does not have.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{PermissionState, PostError};

/// The prefix of every platform identifier this application gives a
/// notification.
const IDENTIFIER_PREFIX: &str = "synthesis";

/// NTD-FR-15: whether this process runs inside an application bundle.
///
/// The bundle identifier alone is not proof: Tauri embeds an `Info.plist` in a
/// development binary, so a bare `target/debug/synthesis` reports the real
/// identifier too. On macOS the notification API ends a process that has no
/// bundle, so the path must also be an `.app` directory.
pub fn is_app_bundle(bundle_path: &str, identifier: Option<&str>) -> bool {
    let has_identifier = identifier.is_some_and(|id| !id.trim().is_empty());
    let path = bundle_path.trim_end_matches('/');
    let is_app = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(stem, ext)| !stem.is_empty() && ext.eq_ignore_ascii_case("app"));
    has_identifier && is_app
}

/// NTD-FR-13: a token unique to this run of the application.
///
/// The process id and the start time together: a process id alone is reused by
/// the operating system, and a later run must never take an earlier run's
/// notification for its own.
pub fn run_token() -> &'static str {
    static TOKEN: LazyLock<String> = LazyLock::new(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("{:x}{:x}", std::process::id(), nanos)
    });
    TOKEN.as_str()
}

/// The identifier the platform holds for the notification `id` of the run that
/// `token` names. One `id` always gives one identifier, so a post that
/// supersedes a showing notification replaces it in place (NTD-FR-06).
pub fn platform_identifier(token: &str, id: &str) -> String {
    format!("{IDENTIFIER_PREFIX}.{token}.{id}")
}

/// NTD-FR-13: the notification id inside `identifier`, only when the run that
/// `token` names posted it. An identifier from an earlier run, from another
/// application, or of an unknown shape gives `None`.
pub fn own_id<'a>(token: &str, identifier: &'a str) -> Option<&'a str> {
    let rest = identifier.strip_prefix(IDENTIFIER_PREFIX)?.strip_prefix('.')?;
    let id = rest.strip_prefix(token)?.strip_prefix('.')?;
    (!id.is_empty()).then_some(id)
}

/// NTD-FR-02: the disposition a platform authorisation status gives.
///
/// The values are those of `UNAuthorizationStatus`. Provisional and ephemeral
/// permission let this application post, so they read as `granted`. A value
/// this code does not know reads as `denied`, because posting against an
/// unknown status is not safe.
pub fn permission_from_status(status: i64) -> PermissionState {
    match status {
        0 => PermissionState::NotRequested,
        1 => PermissionState::Denied,
        2..=4 => PermissionState::Granted,
        _ => PermissionState::Denied,
    }
}

/// NTD-FR-02 / NTD-FR-15: the disposition a settings read gives. A read the
/// centre did not answer in time reads as `unsupported`: no state is assumed,
/// and a centre that does not answer is not one this module can reach.
pub fn permission_from_answer(status: Option<i64>) -> PermissionState {
    status.map_or(PermissionState::Unsupported, permission_from_status)
}

/// What `request_notification_permission` does with the state it reads first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestDecision {
    /// Show the platform prompt.
    Ask,
    /// Answer with this state and show nothing.
    Answer(PermissionState),
}

/// NTD-FR-03: only a state nobody asked for yet shows a prompt. A refusal is
/// reversed in the operating system's settings, not by asking again.
pub fn request_decision(current: PermissionState) -> RequestDecision {
    match current {
        PermissionState::NotRequested => RequestDecision::Ask,
        other => RequestDecision::Answer(other),
    }
}

/// NTD-FR-03: the whole request. `read` asks the platform for its state, and
/// `ask` shows the platform prompt. The prompt shows only for a state nobody
/// asked for, and the answer is the state read after it — never what the
/// prompt itself reported.
pub fn request_flow(
    mut read: impl FnMut() -> PermissionState,
    ask: impl FnOnce(),
) -> PermissionState {
    match request_decision(read()) {
        RequestDecision::Answer(state) => state,
        RequestDecision::Ask => {
            ask();
            read()
        }
    }
}

/// The `UNErrorCodeNotificationsNotAllowed` code of `UNErrorDomain`.
const NOTIFICATIONS_NOT_ALLOWED: i64 = 1;

/// NTD-FR-16: the post error a platform delivery error gives. Only the
/// platform's own "not allowed" is a permission refusal; every other error is a
/// failed delivery.
pub fn post_error_from(domain: &str, code: i64) -> PostError {
    if domain == "UNErrorDomain" && code == NOTIFICATIONS_NOT_ALLOWED {
        PostError::PermissionDenied
    } else {
        PostError::DeliveryFailed
    }
}

/// NTD-FR-16: the outcome of a post from the centre's answer. `None` is a
/// centre that did not answer in time; `Some(None)` is a post it accepted.
pub fn post_outcome(answer: Option<Option<(String, i64)>>) -> Result<(), PostError> {
    match answer {
        Some(None) => Ok(()),
        Some(Some((domain, code))) => Err(post_error_from(&domain, code)),
        None => Err(PostError::DeliveryFailed),
    }
}

/// The action the author took on a notification, as the centre reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseAction {
    /// A click on the notification itself.
    Open,
    /// The platform's own dismiss gesture.
    Dismiss,
    /// Any other action. This application offers none.
    Other,
}

/// What this run does with a response the centre reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponsePlan<'a> {
    /// NTD-FR-09 / NTD-FR-10: run the route of this run's notification `id`.
    Open(&'a str),
    /// NTD-FR-11 / NTD-FR-06: forget this run's notification `id`, and do
    /// nothing the author can see.
    Forget(&'a str),
    /// NTD-FR-13: a notification of an earlier run, or of an unknown shape.
    /// It routes nowhere.
    Ignore,
}

/// NTD-FR-09 / NTD-FR-11 / NTD-FR-13: decide what a response does. The run is
/// decided first, so no action on an earlier run's notification reaches a
/// route of this run.
pub fn plan_response<'a>(
    token: &str,
    identifier: &'a str,
    action: ResponseAction,
) -> ResponsePlan<'a> {
    match (own_id(token, identifier), action) {
        (None, _) => ResponsePlan::Ignore,
        (Some(id), ResponseAction::Open) => ResponsePlan::Open(id),
        (Some(id), ResponseAction::Dismiss) => ResponsePlan::Forget(id),
        (Some(_), ResponseAction::Other) => ResponsePlan::Ignore,
    }
}

/// NTD-FR-FVPT: the body on a platform with no subtitle line — the subtitle on
/// the first line, the body on the next. An empty part adds no line.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn fold_subtitle(subtitle: &str, body: &str) -> String {
    match (subtitle.is_empty(), body.is_empty()) {
        (true, _) => body.to_string(),
        (false, true) => subtitle.to_string(),
        (false, false) => format!("{subtitle}\n{body}"),
    }
}

/// The click handlers of the notifications this run shows, by platform
/// identifier.
///
/// NTD-FR-10: a handler is given out once at most, so a second activation of
/// one notification finds nothing to run.
#[derive(Default)]
pub struct ClickRoutes {
    routes: Mutex<HashMap<String, Box<dyn FnOnce() + Send>>>,
}

impl ClickRoutes {
    /// Keep `route` for `identifier`. A later store for the same identifier
    /// replaces the earlier route, as a superseding post replaces the
    /// notification (NTD-FR-06).
    pub fn store(&self, identifier: String, route: Box<dyn FnOnce() + Send>) {
        self.lock().insert(identifier, route);
    }

    /// Remove and give out the route for `identifier`.
    pub fn take(&self, identifier: &str) -> Option<Box<dyn FnOnce() + Send>> {
        self.lock().remove(identifier)
    }

    /// Drop the route for `identifier`, if there is one.
    pub fn forget(&self, identifier: &str) {
        self.lock().remove(identifier);
    }

    /// How many routes are held. Tests only.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// A poisoned map must not stop notifications: a notification is
    /// peripheral to everything it reports on.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Box<dyn FnOnce() + Send>>> {
        self.routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
