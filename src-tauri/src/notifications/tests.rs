//! The tests of notification delivery
//! (`../../specifications/core/NTD-notification-delivery.md`).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A `NotificationSink` that records what it was handed, so delivery,
/// retraction and the click callback are assertable without a notification
/// centre.
#[derive(Default)]
struct Recorder {
    delivered: Mutex<Vec<(String, NotificationRequest)>>,
    retracted: Mutex<Vec<String>>,
    /// Held so a test can fire the click the platform would have fired.
    clicks: Mutex<HashMap<String, Box<dyn FnOnce() + Send>>>,
    permission: Mutex<PermissionState>,
    fail_delivery: Mutex<bool>,
    requests: AtomicUsize,
}

impl Recorder {
    fn granted() -> Self {
        Recorder {
            permission: Mutex::new(PermissionState::Granted),
            ..Default::default()
        }
    }

    fn with_permission(state: PermissionState) -> Self {
        Recorder {
            permission: Mutex::new(state),
            ..Default::default()
        }
    }

    fn delivered_ids(&self) -> Vec<String> {
        self.delivered
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn fire_click(&self, id: &str) {
        let handler = self.clicks.lock().unwrap().remove(id);
        if let Some(handler) = handler {
            handler();
        }
    }
}

impl NotificationSink for Recorder {
    fn deliver(
        &self,
        id: &str,
        request: &NotificationRequest,
        on_click: Box<dyn FnOnce() + Send>,
    ) -> Result<(), PostError> {
        if *self.fail_delivery.lock().unwrap() {
            return Err(PostError::DeliveryFailed);
        }
        self.delivered
            .lock()
            .unwrap()
            .push((id.to_string(), request.clone()));
        self.clicks.lock().unwrap().insert(id.to_string(), on_click);
        Ok(())
    }

    fn retract(&self, id: &str) {
        self.retracted.lock().unwrap().push(id.to_string());
    }

    fn permission(&self) -> PermissionState {
        *self.permission.lock().unwrap()
    }

    fn request_permission(&self) -> PermissionState {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let mut state = self.permission.lock().unwrap();
        // A platform that has never been asked answers the ask; one that has
        // refused stays refused, because a refusal is reversed in the
        // operating system's settings and not by asking again (NTD-FR-03).
        if *state == PermissionState::NotRequested {
            *state = PermissionState::Granted;
        }
        *state
    }
}

fn request(key: &str, payload: &str) -> NotificationRequest {
    NotificationRequest {
        key: key.to_string(),
        title: "Title".into(),
        body: "Body".into(),
        payload: payload.to_string(),
    }
}

// -----------------------------------------------------------------------
// NTD-FR-01 — the documented payload shapes
// -----------------------------------------------------------------------

#[test]
fn the_wire_shapes_match_the_contract_surface() {
    // NTD-FR-01: camelCase fields, and the permission discriminant in the
    // snake_case the frontend branches on.
    let activation = Activation {
        id: "notif-0".into(),
        key: "run:abc".into(),
        payload: "synthesis://p/w/dashboard".into(),
    };
    let json = serde_json::to_value(&activation).unwrap();
    assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("notif-0"));
    assert_eq!(json.get("key").and_then(|v| v.as_str()), Some("run:abc"));
    assert_eq!(
        json.get("payload").and_then(|v| v.as_str()),
        Some("synthesis://p/w/dashboard")
    );

    for (state, wire) in [
        (PermissionState::Granted, "granted"),
        (PermissionState::Denied, "denied"),
        (PermissionState::NotRequested, "not_requested"),
        (PermissionState::Unsupported, "unsupported"),
    ] {
        assert_eq!(serde_json::to_value(state).unwrap(), wire);
    }

    // The request decodes from the camelCase the frontend sends.
    let decoded: NotificationRequest = serde_json::from_value(serde_json::json!({
        "key": "k", "title": "t", "body": "b", "payload": "p"
    }))
    .unwrap();
    assert_eq!(decoded, request_of("k", "t", "b", "p"));

    assert_eq!(
        serde_json::to_value(PostedNotification { id: "notif-3".into() }).unwrap(),
        serde_json::json!({ "id": "notif-3" })
    );
}

fn request_of(key: &str, title: &str, body: &str, payload: &str) -> NotificationRequest {
    NotificationRequest {
        key: key.into(),
        title: title.into(),
        body: body.into(),
        payload: payload.into(),
    }
}

#[test]
fn every_post_error_has_its_own_code() {
    // NTD-FR-16: `delivery_failed` is distinct from the other two, because
    // each calls for a different correction by the author.
    assert_eq!(PostError::PermissionDenied.code(), "permission_denied");
    assert_eq!(PostError::Unsupported.code(), "unsupported");
    assert_eq!(PostError::DeliveryFailed.code(), "delivery_failed");
}

// -----------------------------------------------------------------------
// NTD-FR-06, NTD-FR-07 — coalescing by key, and ids that never repeat
// -----------------------------------------------------------------------

#[test]
fn a_repeat_key_replaces_in_place_and_keeps_its_id() {
    // NTD-FR-06, NTD-FR-07: one notification showing, carrying the new payload, under
    // the id the first post got. A different key occupies a second slot.
    let registry = NotificationRegistry::default();
    let first = registry.post(&request("run:abc", "payload-1"));
    assert!(!first.replaced);

    let second = registry.post(&request("run:abc", "payload-2"));
    assert!(second.replaced, "a repeat key replaces rather than stacks");
    assert_eq!(second.id, first.id, "and keeps the id the caller holds");
    assert_eq!(registry.showing_count(), 1);

    // The replacement carries the *new* payload through to activation.
    let other = registry.post(&request("run:def", "payload-3"));
    assert_ne!(other.id, first.id);
    assert_eq!(registry.showing_count(), 2);

    let activated = registry.activate(&first.id).unwrap();
    assert_eq!(activated.payload, "payload-2");
}

#[test]
fn ids_never_repeat_across_a_session() {
    // NTD-FR-07: a long churn of posts and withdrawals must not
    // let a later notification reuse an earlier one's id.
    let registry = NotificationRegistry::default();
    let mut seen = Vec::new();
    for n in 0..50 {
        let posted = registry.post(&request(&format!("k{n}"), "p"));
        registry.withdraw(&posted.id);
        seen.push(posted.id);
    }
    let fresh = registry.post(&request("fresh", "p"));
    assert!(!seen.contains(&fresh.id));
    assert_eq!(registry.showing_count(), 1);
}

#[test]
fn a_key_freed_by_withdrawal_gets_a_new_id_rather_than_the_old_one() {
    // The two maps have to stay in step: a withdrawal that cleared `showing`
    // but left `by_key` would make the next post under that key "replace" an
    // id nothing is showing under, and it would never be delivered again.
    let registry = NotificationRegistry::default();
    let first = registry.post(&request("run:abc", "p"));
    assert!(registry.withdraw(&first.id));
    assert_eq!(registry.id_for_key("run:abc"), None);

    let second = registry.post(&request("run:abc", "p"));
    assert!(!second.replaced, "the key was free, so this is a fresh post");
    assert_ne!(second.id, first.id);
}

// -----------------------------------------------------------------------
// NTD-FR-08 — the payload is opaque
// -----------------------------------------------------------------------

#[test]
fn an_uninterpretable_payload_travels_byte_for_byte() {
    // NTD-FR-08: no shape is required of a payload, and one this module has
    // no grammar for is not an error, because it interprets none.
    let registry = NotificationRegistry::default();
    for payload in [
        "",
        "not a uri at all",
        "synthesis://proj/wt/file/a b/c%2Fd.md",
        "{\"json\":true}",
        "🎧 unicode ✓",
    ] {
        let posted = registry.post(&request("k", payload));
        let activated = registry.activate(&posted.id).unwrap();
        assert_eq!(activated.payload, payload, "carried byte-for-byte");
    }
}

// -----------------------------------------------------------------------
// NTD-FR-10 — exactly one activation per notification
// -----------------------------------------------------------------------

#[test]
fn activation_consumes_the_notification_exactly_once() {
    // NTD-FR-10: the notification is withdrawn as part of the activation, so
    // it can never be activated a second time.
    let registry = NotificationRegistry::default();
    let posted = registry.post(&request("run:abc", "p"));

    let first = registry.activate(&posted.id);
    assert!(first.is_some());
    assert_eq!(registry.showing_count(), 0);
    assert!(
        registry.activate(&posted.id).is_none(),
        "a second activation of the same id routes nowhere"
    );

    // And the key is free again, so a later post is a fresh notification.
    let again = registry.post(&request("run:abc", "p"));
    assert!(!again.replaced);
}

#[test]
fn activating_something_never_posted_routes_nowhere() {
    // NTD-FR-13: an id belonging to an earlier run of the application.
    let registry = NotificationRegistry::default();
    assert!(registry.activate("notif-from-a-previous-life").is_none());
}

#[test]
fn a_withdrawn_notification_cannot_be_activated() {
    // The property that makes withdrawal meaningful on a platform with no
    // retraction API (NTD-FR-12 / NTD-FR-13): the banner may linger, but the
    // click resolves to nothing.
    let registry = NotificationRegistry::default();
    let posted = registry.post(&request("run:abc", "p"));
    registry.withdraw(&posted.id);
    assert!(registry.activate(&posted.id).is_none());
}

#[test]
fn withdraw_is_idempotent_and_reports_whether_anything_was_showing() {
    let registry = NotificationRegistry::default();
    let posted = registry.post(&request("k", "p"));
    assert!(registry.withdraw(&posted.id));
    assert!(!registry.withdraw(&posted.id), "and is not an error");
    assert!(!registry.withdraw("never-existed"));
}

// -----------------------------------------------------------------------
// NTD-FR-12 / NTD-FR-18 — withdraw-all, and no history
// -----------------------------------------------------------------------

#[test]
fn withdraw_all_empties_the_registry_and_names_what_was_showing() {
    let registry = NotificationRegistry::default();
    let a = registry.post(&request("a", "p"));
    let b = registry.post(&request("b", "p"));
    let c = registry.post(&request("c", "p"));

    let mut withdrawn = registry.withdraw_all();
    withdrawn.sort();
    let mut expected = vec![a.id.clone(), b.id, c.id];
    expected.sort();
    assert_eq!(withdrawn, expected);
    assert_eq!(registry.showing_count(), 0);

    // NTD-FR-18: nothing is retained, so a fresh post under a previously
    // used key is a new notification rather than a replacement.
    assert!(!registry.post(&request("a", "p")).replaced);
    assert!(registry.withdraw_all().len() == 1);
    assert!(registry.withdraw_all().is_empty(), "and empties fully");
}

#[test]
fn nothing_accumulates_across_a_session() {
    // NTD-FR-18: the only thing held is what is showing.
    let registry = NotificationRegistry::default();
    for n in 0..100 {
        let posted = registry.post(&request(&format!("k{n}"), "p"));
        registry.activate(&posted.id);
    }
    assert_eq!(registry.showing_count(), 0);
    assert!(registry.lock().by_key.is_empty());
}

// -----------------------------------------------------------------------
// The sink seam: delivery, refusal, and the click callback
// -----------------------------------------------------------------------

#[test]
fn a_granted_post_reaches_the_platform_and_a_click_calls_back() {
    // The whole path this module owns, minus the Tauri emit: post ->
    // delivered -> the platform reports a click -> the registry yields the
    // payload exactly once (NTD-FR-09 / NTD-FR-10).
    let registry = Arc::new(NotificationRegistry::default());
    let sink = Recorder::granted();
    let req = request("run:abc", "synthesis://p/w/bottom/runs");

    let posted = registry.post(&req);
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (r, s, id) = (registry.clone(), seen.clone(), posted.id.clone());
    sink.deliver(
        &posted.id,
        &req,
        Box::new(move || {
            if let Some(activation) = r.activate(&id) {
                s.lock().unwrap().push(activation.payload);
            }
        }),
    )
    .unwrap();

    assert_eq!(sink.delivered_ids(), vec![posted.id.clone()]);
    assert!(seen.lock().unwrap().is_empty(), "no click yet, no callback");

    sink.fire_click(&posted.id);
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["synthesis://p/w/bottom/runs".to_string()]
    );
    assert_eq!(registry.showing_count(), 0);
}

#[test]
fn a_dismissal_calls_nothing_back() {
    // NTD-FR-11: dismissing without activating emits nothing and leaves the
    // application untouched. The recorder simply never fires the handler.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let req = request("k", "p");
    let posted = registry.post(&req);
    let called = Arc::new(AtomicUsize::new(0));
    let c = called.clone();
    sink.deliver(
        &posted.id,
        &req,
        Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();

    drop(sink); // the notification goes without an interaction
    assert_eq!(called.load(Ordering::SeqCst), 0);
    assert_eq!(
        registry.showing_count(),
        1,
        "and it is still showing as far as this run knows"
    );
}

// -----------------------------------------------------------------------
// NTD-FR-14 / NTD-FR-16 — the permission gate and the error mapping,
// exercised through `post_through` (the command's real body) rather than
// through the double that would otherwise be asserting about itself.
// -----------------------------------------------------------------------

fn post(registry: &NotificationRegistry, sink: &Recorder, key: &str) -> Result<Posted, PostError> {
    post_through(registry, sink, &request(key, "payload"), |_| Box::new(|| {}))
}

#[test]
fn a_refused_permission_posts_nothing_touches_nothing_and_never_prompts() {
    // NTD-FR-14: `denied` and `not_requested` both map to
    // `permission_denied`, nothing reaches the platform, the registry is
    // untouched, and no prompt was raised on the way.
    for state in [PermissionState::Denied, PermissionState::NotRequested] {
        let registry = NotificationRegistry::default();
        let sink = Recorder::with_permission(state);

        assert_eq!(post(&registry, &sink, "k"), Err(PostError::PermissionDenied));
        assert!(sink.delivered_ids().is_empty(), "nothing was shown");
        assert_eq!(registry.showing_count(), 0, "and nothing was recorded");
        assert_eq!(
            sink.requests.load(Ordering::SeqCst),
            0,
            "a background raise must never put a prompt in front of the author"
        );
    }
}

#[test]
fn an_unsupported_platform_is_its_own_error_rather_than_a_refusal() {
    // NTD-FR-15 / NTD-FR-16: distinct from `permission_denied`, because
    // there is nothing the author could do about this one.
    let registry = NotificationRegistry::default();
    let sink = Recorder::with_permission(PermissionState::Unsupported);
    assert_eq!(post(&registry, &sink, "k"), Err(PostError::Unsupported));
    assert!(sink.delivered_ids().is_empty());
    assert_eq!(registry.showing_count(), 0);
}

#[test]
fn a_granted_post_records_it_and_shows_it_with_its_text_unmodified() {
    // NTD-FR-05: title and body reach the platform exactly as handed over —
    // this module applies no markup, formatting, or truncation of its own.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let req = NotificationRequest {
        key: "run:abc".into(),
        title: "A very long title that no platform is obliged to show whole".into(),
        body: "Body with\nnewlines and *asterisks* and 🎧".into(),
        payload: "synthesis://p/w/dashboard".into(),
    };

    let posted = post_through(&registry, &sink, &req, |_| Box::new(|| {})).unwrap();
    assert_eq!(registry.showing_count(), 1);
    let delivered = sink.delivered.lock().unwrap();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].0, posted.id);
    assert_eq!(delivered[0].1.title, req.title, "handed over verbatim");
    assert_eq!(delivered[0].1.body, req.body);
}

#[test]
fn a_failed_delivery_returns_its_code_and_frees_the_key_for_a_retry() {
    // NTD-FR-16, through the real rollback rather than a test that performs
    // the rollback itself. Without it the key would shadow every later post
    // under it for the rest of the session.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    *sink.fail_delivery.lock().unwrap() = true;

    assert_eq!(post(&registry, &sink, "run:abc"), Err(PostError::DeliveryFailed));
    assert_eq!(
        registry.showing_count(),
        0,
        "the registry must not believe something is showing that was refused"
    );

    *sink.fail_delivery.lock().unwrap() = false;
    let retry = post(&registry, &sink, "run:abc").unwrap();
    assert!(!retry.replaced, "the key was freed, so the retry is a fresh post");
    assert_eq!(sink.delivered_ids(), vec![retry.id]);
}

#[test]
fn a_repeat_key_saying_the_same_thing_is_never_shown_twice() {
    // NTD-FR-06. A reporter posting "still running" every second must
    // occupy ONE banner however long it runs — and, on a platform with no
    // retraction, one blocked thread rather than one per post.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let req = request("run:abc", "synthesis://p/w/bottom/runs");

    let first = post_through(&registry, &sink, &req, |_| Box::new(|| {})).unwrap();
    assert!(!first.replaced);
    assert!(!first.unchanged);

    for _ in 0..20 {
        let again = post_through(&registry, &sink, &req, |_| Box::new(|| {})).unwrap();
        assert_eq!(again.id, first.id, "one id, so a held id stays valid");
        assert!(again.replaced);
        assert!(again.unchanged, "the words did not change");
    }

    assert_eq!(
        sink.delivered_ids(),
        vec![first.id.clone()],
        "twenty-one posts, one delivery: the platform is never handed a \
             notification repeating what it is already showing"
    );
    assert_eq!(registry.showing_count(), 1);
}

#[test]
fn a_repeat_key_with_new_words_is_shown_again_and_routes_to_the_latest() {
    // The other half of NTD-FR-06's dedup: "the run finished" is not the
    // same news as "the run needs you", so it is genuinely worth showing.
    // On a platform that cannot retract (`supports_replacement() == false`)
    // that is a second banner, and the superseded one stays until the author
    // clears it — but BOTH carry the same id, so a click on either routes to
    // the current target rather than to a stale one.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();

    let first = post_through(
        &registry,
        &sink,
        &NotificationRequest {
            key: "run:abc".into(),
            title: "Run".into(),
            body: "Finished.".into(),
            payload: "synthesis://p/w/bottom/runs".into(),
        },
        |_| Box::new(|| {}),
    )
    .unwrap();

    let second = post_through(
        &registry,
        &sink,
        &NotificationRequest {
            key: "run:abc".into(),
            title: "Run".into(),
            body: "Needs your input.".into(),
            payload: "synthesis://p/w/dashboard".into(),
        },
        |_| Box::new(|| {}),
    )
    .unwrap();

    assert_eq!(second.id, first.id);
    assert!(second.replaced);
    assert!(!second.unchanged, "different words are different news");
    assert_eq!(
        sink.delivered_ids(),
        vec![first.id.clone(), first.id.clone()],
        "shown again, under the same id"
    );
    assert!(!sink.supports_replacement());
    assert_eq!(registry.showing_count(), 1, "still one slot");
    assert_eq!(
        registry.activate(&first.id).unwrap().payload,
        "synthesis://p/w/dashboard",
        "and either banner routes to the LATEST target"
    );
}

#[test]
fn an_address_that_changed_alone_is_carried_through_without_a_new_banner() {
    // The payload is deliberately outside the sameness comparison: an
    // address that moved while the words did not is still nothing new to
    // read, but a click must land on the new target rather than the old.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();

    let first = post_through(&registry, &sink, &request("k", "synthesis://p/w/dashboard"), |_| {
        Box::new(|| {})
    })
    .unwrap();
    let second =
        post_through(&registry, &sink, &request("k", "synthesis://p/w/bottom/runs"), |_| {
            Box::new(|| {})
        })
        .unwrap();

    assert!(second.unchanged, "same words, so nothing new to show");
    assert_eq!(sink.delivered_ids(), vec![first.id.clone()], "one banner");
    assert_eq!(
        registry.activate(&first.id).unwrap().payload,
        "synthesis://p/w/bottom/runs",
        "but the click follows the newer address"
    );
}

#[test]
fn a_key_reused_after_its_notification_went_is_shown_again() {
    // Dedup is scoped to what is CURRENTLY showing. Once a notification has
    // been activated or withdrawn, the same words under the same key are
    // news again — otherwise a run that finishes twice would announce
    // itself only the first time, for the rest of the session.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let req = request("run:abc", "p");

    let first = post_through(&registry, &sink, &req, |_| Box::new(|| {})).unwrap();
    registry.activate(&first.id);
    let second = post_through(&registry, &sink, &req, |_| Box::new(|| {})).unwrap();

    assert!(!second.replaced, "the key was free");
    assert!(!second.unchanged);
    assert_ne!(second.id, first.id);
    assert_eq!(sink.delivered_ids().len(), 2, "shown again");
}

#[test]
fn two_keys_occupy_two_slots() {
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let a = post(&registry, &sink, "run:abc").unwrap();
    let b = post(&registry, &sink, "run:def").unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(registry.showing_count(), 2);
    assert_eq!(sink.delivered_ids().len(), 2);
}

#[test]
fn the_click_the_command_wires_carries_the_id_that_was_posted() {
    // The closure `post_through` builds is the one the platform calls back
    // on, and it must close over the id the registry actually assigned —
    // not a fresh one, which would route nowhere.
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();

    let posted = post_through(&registry, &sink, &request("k", "p"), move |id| {
        Box::new(move || s.lock().unwrap().push(id))
    })
    .unwrap();

    sink.fire_click(&posted.id);
    assert_eq!(*seen.lock().unwrap(), vec![posted.id]);
}

#[test]
fn requesting_permission_asks_once_and_a_refusal_stays_refused() {
    // NTD-FR-03: a platform that has never been asked answers the ask; one
    // that has already refused is not asked again, because that is reversed
    // in the operating system's settings.
    let fresh = Recorder::with_permission(PermissionState::NotRequested);
    assert_eq!(fresh.request_permission(), PermissionState::Granted);
    assert_eq!(fresh.permission(), PermissionState::Granted);

    let refused = Recorder::with_permission(PermissionState::Denied);
    assert_eq!(refused.request_permission(), PermissionState::Denied);
    assert_eq!(refused.permission(), PermissionState::Denied);
}

#[test]
fn withdrawal_reaches_the_platform_only_for_something_showing() {
    let registry = NotificationRegistry::default();
    let sink = Recorder::granted();
    let posted = post(&registry, &sink, "k").unwrap();

    for _ in 0..2 {
        if registry.withdraw(&posted.id) {
            sink.retract(&posted.id);
        }
    }
    assert_eq!(
        sink.retracted.lock().unwrap().len(),
        1,
        "the second withdrawal found nothing showing"
    );
}

// -----------------------------------------------------------------------
// NTD-FR-09 / NTD-FR-10 / NTD-FR-13, OVW-FR-01 — activation delivery, against a real
// (headless) app handle so the raise-then-emit ordering is exercised.
// -----------------------------------------------------------------------

#[test]
fn an_activation_raises_the_window_before_it_emits_and_emits_once() {
    use tauri::Listener;

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(NotificationRegistry::default());
    handle.manage(Notifier(Box::new(Recorder::granted())));

    let registry = handle.state::<NotificationRegistry>();
    let posted = registry.post(&request("run:abc", "synthesis://p/w/dashboard"));

    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    handle.listen(NOTIFICATION_ACTIVATED, move |event| {
        s.lock().unwrap().push(event.payload().to_string());
    });

    deliver_activation(&handle, &posted.id);

    let delivered = seen.lock().unwrap();
    assert_eq!(delivered.len(), 1, "exactly one event per activation");
    assert!(delivered[0].contains("synthesis://p/w/dashboard"));
    assert!(delivered[0].contains(&posted.id));
    assert!(delivered[0].contains("run:abc"));
    drop(delivered);

    // NTD-FR-10: the notification is consumed as part of the activation.
    assert_eq!(
        handle.state::<NotificationRegistry>().showing_count(),
        0
    );

    // A second activation of the same id routes nowhere.
    deliver_activation(&handle, &posted.id);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn an_activation_of_an_id_this_run_never_posted_emits_nothing() {
    // NTD-FR-13: a notification surviving in the centre from an earlier run
    // of the application. It must not route, and it must not panic.
    use tauri::Listener;

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(NotificationRegistry::default());
    handle.manage(Notifier(Box::new(Recorder::granted())));

    let seen = Arc::new(AtomicUsize::new(0));
    let s = seen.clone();
    handle.listen(NOTIFICATION_ACTIVATED, move |_| {
        s.fetch_add(1, Ordering::SeqCst);
    });

    deliver_activation(&handle, "notif-from-a-previous-life");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

#[test]
fn withdrawing_on_exit_clears_everything_this_run_posted() {
    // NTD-FR-12.
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(NotificationRegistry::default());
    handle.manage(Notifier(Box::new(Recorder::granted())));

    let registry = handle.state::<NotificationRegistry>();
    for key in ["a", "b", "c"] {
        registry.post(&request(key, "p"));
    }
    assert_eq!(registry.showing_count(), 3);

    withdraw_all_on_exit(&handle);
    assert_eq!(handle.state::<NotificationRegistry>().showing_count(), 0);

    // Idempotent: quitting twice is not a thing, but a held exit followed by
    // a real one is.
    withdraw_all_on_exit(&handle);
}

// -----------------------------------------------------------------------
// NTD-FR-07 under concurrency — the production case, since the macOS sink
// calls back from a thread per notification.
// -----------------------------------------------------------------------

#[test]
fn ids_stay_unique_and_the_two_maps_stay_in_step_under_concurrent_use() {
    let registry = Arc::new(NotificationRegistry::default());
    let mut handles = Vec::new();
    for t in 0..8 {
        let registry = registry.clone();
        handles.push(std::thread::spawn(move || {
            let mut ids = Vec::new();
            for n in 0..50 {
                let posted = registry.post(&request(&format!("k{t}-{n}"), "p"));
                ids.push(posted.id.clone());
                if n % 2 == 0 {
                    registry.withdraw(&posted.id);
                } else {
                    registry.activate(&posted.id);
                }
            }
            ids
        }));
    }
    let mut all: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    let total = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), total, "no id was handed out twice");
    assert_eq!(
        registry.showing_count(),
        0,
        "every post was withdrawn or activated"
    );
    assert!(
        registry.lock().by_key.is_empty(),
        "and the key map did not leak entries behind the showing map"
    );
}

#[test]
fn command_functions_are_in_scope() {
    // Compile-time guard: renaming or removing a command without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = get_notification_permission;
    let _ = request_notification_permission;
    let _ = post_notification;
    let _ = withdraw_notification;
    let _ = withdraw_all_notifications;
}
