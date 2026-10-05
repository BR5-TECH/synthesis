//! Notification delivery (`specifications/core/NTD-notification-delivery.md`).
//!
//! Puts a short message in front of the author through the operating system's
//! own notification centre and reports back when they act on one. The module is
//! deliberately thin: it posts, replaces, withdraws, and reports activation, and
//! it decides nothing — whether a message is worth sending, what it should say,
//! and where the author should land are all `../ui/NTF-notifications.md`'s.
//!
//! Two seams make the whole thing testable without a Tauri runtime or a
//! notification centre, the same shape `progress.rs` uses:
//!
//! - [`NotificationRegistry`] holds what is showing and decides *what* happens,
//!   including the key coalescing of NTD-FR-06 and the exactly-once activation
//!   of NTD-FR-10. Its methods return the outcome rather than performing it.
//! - [`NotificationSink`] is the platform. `OsSink` implements it against the
//!   real centre; the tests use a recording one, so every id, coalescing,
//!   activation and withdrawal invariant is exercised without a window.
//!
//! **The payload is opaque** (NTD-FR-08). It is carried byte-for-byte from the
//! post to the activation and is never parsed, inspected, or acted on here — the
//! `synthesis://` grammar lives entirely in `src/state/notificationAddress.ts`.
//! Nothing here registers a URL scheme or protocol handler, so no process
//! outside this application can mint one or hand one in (NTD-FR-20).

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::logging::{log_debug, log_error, log_info, log_warn, Domain, BUFFER};
use crate::log_fields;

/// The event an activation is published on (NTD contract surface).
///
/// Kebab-case rather than the spec's abstract `"notification activated"`, for
/// the reason every other channel in this codebase is: Tauri validates event
/// names and rejects one containing a space on both sides, leaving the channel
/// silently dead while every test that mocks `listen` stays green. Matches the
/// constant in `src/events.ts` byte-for-byte, and pinned by
/// `every_event_name_is_one_tauri_will_actually_deliver` in `lib.rs`.
pub const NOTIFICATION_ACTIVATED: &str = "notification-activated";

// ---------------------------------------------------------------------------
// Wire shapes (NTD "Payload shapes")
// ---------------------------------------------------------------------------

/// The platform's disposition toward this application (NTD-FR-02).
///
/// `NotRequested` and `Denied` are distinct because they call for different
/// things from the author: one is a prompt this application can raise, the other
/// is a trip to the operating system's own settings, and `GLS-FR-26` renders a
/// different section for each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
    /// The honest default: until something has asked the platform, nothing is
    /// known, and assuming `Granted` would let a post be attempted against a
    /// centre that refuses it.
    #[default]
    NotRequested,
    Unsupported,
}

/// What a caller asks to have shown (NTD contract surface).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRequest {
    /// Caller-chosen identity of the thing being notified about. A post whose
    /// key matches one still showing replaces it in place (NTD-FR-06).
    pub key: String,
    pub title: String,
    pub body: String,
    /// Opaque; never parsed here (NTD-FR-08).
    pub payload: String,
}

/// What `post_notification` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostedNotification {
    pub id: String,
}

/// The `"notification activated"` payload (NTD-FR-09).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    pub id: String,
    pub key: String,
    pub payload: String,
}

/// Why a post produced nothing (NTD contract surface).
///
/// Serialised as the bare code, because the frontend branches on the string and
/// a prose message would make that a substring match. Each names a different
/// correction: `permission_denied` is answered by the author granting it,
/// `unsupported` by nothing at all, and `delivery_failed` by retrying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostError {
    PermissionDenied,
    Unsupported,
    DeliveryFailed,
}

impl PostError {
    pub fn code(self) -> &'static str {
        match self {
            PostError::PermissionDenied => "permission_denied",
            PostError::Unsupported => "unsupported",
            PostError::DeliveryFailed => "delivery_failed",
        }
    }
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

// ---------------------------------------------------------------------------
// The platform seam
// ---------------------------------------------------------------------------

/// Where a notification actually goes. Implemented by [`OsSink`] in production
/// and by a recording stub in the tests, so the registry's behaviour is
/// exercised without a notification centre.
pub trait NotificationSink: Send + Sync {
    /// Show `request` under `id`. `on_click` is invoked — on whatever thread the
    /// platform chooses — if and only if the author activates it.
    fn deliver(
        &self,
        id: &str,
        request: &NotificationRequest,
        on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError>;

    /// Best-effort removal from the notification centre.
    ///
    /// The *contract* withdrawal upholds is that an activation no longer routes,
    /// and that is the registry's doing rather than the platform's — see
    /// [`NotificationRegistry::withdraw`]. A platform with no retraction API
    /// implements this as a no-op.
    fn retract(&self, id: &str);

    /// The platform's disposition (NTD-FR-02).
    fn permission(&self) -> PermissionState;

    /// Ask the platform for permission (NTD-FR-03).
    fn request_permission(&self) -> PermissionState;

    /// Whether the platform can genuinely **replace** a showing notification in
    /// place, rather than merely being asked to show a second one (NTD-FR-06).
    ///
    /// macOS answers `false`: `mac-notification-sys` offers neither retraction
    /// nor a notification identity to reuse. This is why dedup happens a layer
    /// up, in [`post_through`]: a repeat post carrying the same words is never
    /// handed to the platform at all, so the common case — a reporter saying
    /// "still running" over and over — is one banner and one waiting thread. A
    /// post whose words genuinely changed IS shown again, and on a `false`
    /// platform the superseded banner stays until the author clears it. The
    /// registry invariant holds throughout — one slot, one id, the latest
    /// payload — so a click on either banner routes to the current target.
    fn supports_replacement(&self) -> bool {
        false
    }
}

/// The permission gate, the registry write, delivery, and the rollback — the
/// whole of what `post_notification` decides, with no Tauri types in it.
///
/// Extracted so those decisions are testable directly rather than through a
/// test double that re-implements them: asserting that a `Recorder` refuses
/// when its own permission field says to proves nothing about this function.
pub fn post_through(
    registry: &NotificationRegistry,
    sink: &dyn NotificationSink,
    request: &NotificationRequest,
    make_click: impl FnOnce(String) -> Box<dyn FnOnce() + Send>,
) -> Result<Posted, PostError> {
    // NTD-FR-14 / NTD-FR-15: decided before the registry is touched, so a
    // refused post leaves no trace and never prompts.
    match sink.permission() {
        PermissionState::Granted => {}
        PermissionState::Unsupported => return Err(PostError::Unsupported),
        _ => return Err(PostError::PermissionDenied),
    }

    let posted = registry.post(request);
    if posted.unchanged {
        // NTD-FR-06: the platform is never handed a notification that repeats
        // what it is already showing. The registry has taken the newer payload,
        // so the banner already up now routes to the current target and the
        // thread already waiting on it is the only one there needs to be.
        return Ok(posted);
    }
    let click = make_click(posted.id.clone());
    if let Err(error) = sink.deliver(&posted.id, request, click) {
        // The registry must not keep believing something is showing that the
        // platform refused, or its key would shadow every later post under that
        // key for the rest of the session — each one "replacing" a notification
        // nobody can see.
        registry.withdraw(&posted.id);
        return Err(error);
    }
    Ok(posted)
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// One notification currently believed to be showing.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Showing {
    key: String,
    payload: String,
    /// What the platform was last actually shown for this key. Held so a repeat
    /// post carrying the same words can be recognised as adding nothing and
    /// skipped, rather than becoming a second banner saying what the first one
    /// already says (NTD-FR-06).
    title: String,
    body: String,
}

#[derive(Default)]
struct Inner {
    /// Keyed by id. Bounded by what is actually showing; nothing accumulates
    /// across a session (NTD-FR-18).
    showing: HashMap<String, Showing>,
    /// NTD-FR-06: the id a key currently occupies, so a repeat post replaces in
    /// place rather than adding a second.
    by_key: HashMap<String, String>,
    /// NTD-FR-07: monotonic, never reset within a run, so no id is ever reused.
    next_id: u64,
}

/// What is showing right now. Held in memory only; nothing here is persisted and
/// there is no query for what was posted earlier in the session (NTD-FR-18).
#[derive(Default)]
pub struct NotificationRegistry {
    inner: Mutex<Inner>,
}

/// The outcome of a [`NotificationRegistry::post`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub id: String,
    /// True when this replaced a notification already showing under the same key
    /// (NTD-FR-06), in which case `id` is the one that notification already had.
    pub replaced: bool,
    /// True when the replacement says exactly what the showing one already says.
    ///
    /// The platform is not asked to show it again: a second banner repeating the
    /// first word for word tells the author nothing, and on a platform with no
    /// retraction it would also mean a second blocked thread waiting on a click
    /// nobody needs. A reporter that posts "still running" every second
    /// therefore occupies one banner and one thread however long it runs.
    pub unchanged: bool,
}

impl NotificationRegistry {
    /// NTD-FR-06 / NTD-FR-07: record a notification as showing and answer with
    /// the id it occupies.
    ///
    /// A key already showing keeps **its** id, so a caller holding one stays
    /// valid across a replacement — which is what lets a surface post repeatedly
    /// about one thing and still withdraw it later with the id it first got.
    pub fn post(&self, request: &NotificationRequest) -> Posted {
        let mut inner = self.lock();
        let showing = Showing {
            key: request.key.clone(),
            payload: request.payload.clone(),
            title: request.title.clone(),
            body: request.body.clone(),
        };
        if let Some(existing) = inner.by_key.get(&request.key).cloned() {
            // Compare against what is showing BEFORE overwriting it. The
            // payload is deliberately not part of the comparison: an address
            // that changed while the words did not is still nothing new to
            // read, and it is carried through so a click routes to the latest
            // target either way.
            let unchanged = inner
                .showing
                .get(&existing)
                .is_some_and(|prev| prev.title == showing.title && prev.body == showing.body);
            inner.showing.insert(existing.clone(), showing);
            return Posted {
                id: existing,
                replaced: true,
                unchanged,
            };
        }
        let id = format!("notif-{}", inner.next_id);
        inner.next_id += 1;
        inner.by_key.insert(request.key.clone(), id.clone());
        inner.showing.insert(id.clone(), showing);
        Posted {
            id,
            replaced: false,
            unchanged: false,
        }
    }

    /// Forget a notification, returning whether one was actually showing.
    /// Idempotent: an id naming nothing is not an error (NTD contract surface).
    ///
    /// This is what makes withdrawal *mean* something on a platform whose centre
    /// offers no retraction API. A banner may linger there, but the id is gone
    /// from the registry, so a later click resolves to nothing and routes
    /// nowhere — exactly what NTD-FR-13 requires of an activation the
    /// application cannot honour.
    pub fn withdraw(&self, id: &str) -> bool {
        let mut inner = self.lock();
        match inner.showing.remove(id) {
            Some(showing) => {
                // Only clear the key if it still points at *this* id.
                if inner.by_key.get(&showing.key).map(String::as_str) == Some(id) {
                    inner.by_key.remove(&showing.key);
                }
                true
            }
            None => false,
        }
    }

    /// NTD-FR-12: forget everything, returning the ids that were showing so the
    /// caller can ask the platform to retract each.
    pub fn withdraw_all(&self) -> Vec<String> {
        let mut inner = self.lock();
        inner.by_key.clear();
        inner.showing.drain().map(|(id, _)| id).collect()
    }

    /// NTD-FR-10: consume an activation, returning its payload exactly once.
    ///
    /// The notification is withdrawn as part of it, so a second activation of
    /// the same id — and an activation of one withdrawn, already activated, or
    /// belonging to an earlier run (NTD-FR-13) — answers `None` and routes
    /// nowhere.
    pub fn activate(&self, id: &str) -> Option<Activation> {
        let mut inner = self.lock();
        let showing = inner.showing.remove(id)?;
        if inner.by_key.get(&showing.key).map(String::as_str) == Some(id) {
            inner.by_key.remove(&showing.key);
        }
        Some(Activation {
            id: id.to_string(),
            key: showing.key,
            payload: showing.payload,
        })
    }

    /// How many notifications are currently showing. Tests and diagnostics only.
    pub fn showing_count(&self) -> usize {
        self.lock().showing.len()
    }

    /// The id a key currently occupies, if any. Tests only.
    #[cfg(test)]
    fn id_for_key(&self, key: &str) -> Option<String> {
        self.lock().by_key.get(key).cloned()
    }

    /// A poisoned registry must not take the application down with it: a
    /// notification is peripheral to everything it reports on.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// ---------------------------------------------------------------------------
// The production sink
// ---------------------------------------------------------------------------

/// The real notification centre.
///
/// Held as Tauri state alongside the registry so both the commands and the
/// application-exit hook can reach it.
pub struct OsSink;

#[cfg(target_os = "macos")]
impl NotificationSink for OsSink {
    fn deliver(
        &self,
        id: &str,
        request: &NotificationRequest,
        on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError> {
        let title = request.title.clone();
        let body = request.body.clone();
        let id = id.to_string();
        // `wait_for_click` makes `send()` block until the author interacts or
        // the banner goes, which is the only way this platform reports a click
        // at all — so every posted notification owns a thread for as long as it
        // is showing. Posting itself stays asynchronous (NTD-FR-17): the caller
        // gets its id back from `post_notification` without waiting on any of
        // this.
        std::thread::Builder::new()
            .name(format!("notification-{id}"))
            .spawn(move || {
                let mut notification = mac_notification_sys::Notification::default();
                notification
                    .title(title.as_str())
                    .message(body.as_str())
                    .wait_for_click(true);
                match notification.send() {
                    Ok(mac_notification_sys::NotificationResponse::Click) => on_click(),
                    // Every other response is a dismissal or an interaction this
                    // application offers no action for. NTD-FR-11: dismissing
                    // emits nothing and raises no window.
                    Ok(_) => {}
                    Err(_) => {}
                }
            })
            .map_err(|_| PostError::DeliveryFailed)?;
        Ok(())
    }

    fn retract(&self, _id: &str) {
        // `mac-notification-sys` exposes no retraction. The registry has already
        // forgotten the id by the time this is called, so the contract that
        // matters — that a click no longer routes — holds regardless; what may
        // linger is the banner in Notification Center.
    }

    /// NTD-FR-06 is only partially achievable here: with no retraction and no
    /// reusable notification identity, a replacement is a second banner. The
    /// registry still keeps one slot, one id, and the latest payload, so a click
    /// on either banner routes to the same, current target.
    fn supports_replacement(&self) -> bool {
        false
    }

    fn permission(&self) -> PermissionState {
        // This platform answers notification permission at the moment of
        // delivery rather than through a queryable API, so the honest report is
        // that the application has a working centre to post into. A refusal
        // surfaces as a delivery that shows nothing, which is why `GLS-FR-26`
        // states the disposition rather than promising it.
        PermissionState::Granted
    }

    fn request_permission(&self) -> PermissionState {
        self.permission()
    }
}

#[cfg(not(target_os = "macos"))]
impl NotificationSink for OsSink {
    fn deliver(
        &self,
        _id: &str,
        request: &NotificationRequest,
        _on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError> {
        // Delivery without an activation channel: `on_click` is dropped, never
        // invoked, so a notification posted here shows and is never routed from.
        let mut notification = notify_rust::Notification::new();
        notification.summary(&request.title).body(&request.body);
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

/// The sink the commands post through. Boxed so a test can hold a different one
/// behind the same Tauri state slot.
pub struct Notifier(pub Box<dyn NotificationSink>);

impl Default for Notifier {
    fn default() -> Self {
        Notifier(Box::new(OsSink))
    }
}

// ---------------------------------------------------------------------------
// Activation delivery (NTD-FR-09 / NTD-FR-10)
// ---------------------------------------------------------------------------

/// Raise the application's mounted window, then publish the activation.
///
/// The order is the contract (NTD-FR-09): the window is in front of the author
/// before any consumer routes into it, so a tab never opens into a window that
/// cannot be seen. Raising touches focus alone — the window's size, maximized
/// state, and full-screen presentation are all left as they were, and no
/// preference describing any of them is written (`SNV-FR-38`).
pub fn deliver_activation<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    let registry = app.state::<NotificationRegistry>();
    let Some(activation) = registry.activate(id) else {
        // NTD-FR-13: an id this run does not know — withdrawn, already
        // activated, or posted by an earlier run — routes nowhere at all.
        log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "notification activation ignored",
            log_fields! { "notificationId" => id, "reason" => "not showing" },
        );
        return;
    };

    // The main window when a project is open, the picker otherwise — whichever
    // is mounted (NTD-FR-09).
    let window = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    match window {
        Some(window) => {
            if let Err(e) = window.set_focus() {
                log_warn(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "could not raise the window for a notification activation",
                    log_fields! { "notificationId" => id, "error" => e.to_string() },
                );
            }
        }
        None => log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "notification activated with no window mounted",
            log_fields! { "notificationId" => id },
        ),
    }

    // The payload is opaque and may name a project path, so it is logged by
    // shape rather than by value (LGC-FR-16: nothing downstream redacts).
    log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "notification activated",
        log_fields! {
            "notificationId" => activation.id.as_str(),
            "key" => activation.key.as_str(),
            "payloadLength" => activation.payload.len() as i64
        },
    );
    if let Err(e) = app.emit(NOTIFICATION_ACTIVATED, &activation) {
        log_error(
            app,
            &BUFFER,
            &[Domain::Backend],
            "could not publish a notification activation",
            log_fields! { "notificationId" => activation.id.as_str(), "error" => e.to_string() },
        );
    }
}

/// NTD-FR-12: withdraw every notification this run posted, as the application
/// quits, so the centre holds none of them once the process is gone.
pub fn withdraw_all_on_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let registry = app.state::<NotificationRegistry>();
    let notifier = app.state::<Notifier>();
    let ids = registry.withdraw_all();
    for id in &ids {
        notifier.0.retract(id);
    }
    if !ids.is_empty() {
        log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "withdrew notifications on exit",
            log_fields! { "count" => ids.len() as i64 },
        );
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (NTD contract surface)
// ---------------------------------------------------------------------------

/// NTD-FR-02: the platform's current disposition. Prompts for nothing, persists
/// nothing, and never returns an error.
#[tauri::command]
pub fn get_notification_permission(notifier: State<'_, Notifier>) -> PermissionState {
    notifier.0.permission()
}

/// NTD-FR-03: the only operation here that can present a permission prompt.
#[tauri::command]
pub fn request_notification_permission(
    app: tauri::AppHandle,
    notifier: State<'_, Notifier>,
) -> PermissionState {
    let state = notifier.0.request_permission();
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "notification permission requested",
        log_fields! { "state" => format!("{state:?}") },
    );
    state
}

/// NTD-FR-06 / NTD-FR-14: show a notification, replacing one already showing
/// under the same key. Posts nothing while permission is anything other than
/// granted, and never prompts on its own.
#[tauri::command]
pub fn post_notification(
    request: NotificationRequest,
    app: tauri::AppHandle,
    registry: State<'_, NotificationRegistry>,
    notifier: State<'_, Notifier>,
) -> Result<PostedNotification, String> {
    let handle = app.clone();
    let posted = match post_through(&registry, notifier.0.as_ref(), &request, move |id| {
        Box::new(move || deliver_activation(&handle, &id))
    }) {
        Ok(posted) => posted,
        Err(error) => {
            // NTD-FR-14: a refusal is not an error the author asked a question
            // to receive, so it is recorded for the Logs panel and nowhere else.
            log_debug(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "notification not posted",
                log_fields! { "key" => request.key.as_str(), "error" => error.code() },
            );
            if error == PostError::DeliveryFailed {
                log_warn(
                    &app,
                    &BUFFER,
                    &[Domain::Backend],
                    "notification delivery failed",
                    log_fields! { "key" => request.key.as_str() },
                );
            }
            return Err(error.code().to_string());
        }
    };

    // Title and body are author-facing text a surface composed, and the payload
    // is an address: neither is logged by value. The key names *what* was
    // raised about, which is what a reader debugging a missing notification
    // needs (LGC-FR-16).
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "notification posted",
        log_fields! {
            "notificationId" => posted.id.as_str(),
            "key" => request.key.as_str(),
            "replaced" => posted.replaced,
            // Whether the platform was actually handed anything. A reader
            // asking "why did I not see that?" is otherwise looking at a line
            // that says the notification was posted.
            "shown" => !posted.unchanged
        },
    );
    Ok(PostedNotification { id: posted.id })
}

/// Idempotent: an id that names nothing showing is not an error.
#[tauri::command]
pub fn withdraw_notification(
    id: String,
    registry: State<'_, NotificationRegistry>,
    notifier: State<'_, Notifier>,
) {
    if registry.withdraw(&id) {
        notifier.0.retract(&id);
    }
}

/// NTD-FR-12: withdraw everything this run posted.
#[tauri::command]
pub fn withdraw_all_notifications(
    registry: State<'_, NotificationRegistry>,
    notifier: State<'_, Notifier>,
) {
    for id in registry.withdraw_all() {
        notifier.0.retract(&id);
    }
}

#[cfg(test)]
mod tests;
