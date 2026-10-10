//! The tests of the pure platform decisions
//! (`../../specifications/core/NTD-notification-delivery.md`).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// NTD-FR-15: only an `.app` directory with an identifier can reach the centre.
// The development binary carries an embedded identifier, and must still read
// as unbundled.
#[test]
fn only_an_app_directory_with_an_identifier_is_a_bundle() {
    let id = Some("tech.br5.synthesis");
    let cases: &[(&str, Option<&str>, bool)] = &[
        ("/Applications/synthesis.app", id, true),
        ("/Applications/synthesis.app/", id, true),
        ("/Users/a/target/debug/bundle/macos/Synthesis.APP", id, true),
        ("/Users/a/develop/synthesis/src-tauri/target/debug", id, false),
        ("/Users/a/target/debug/synthesis", id, false),
        ("/Users/a/foo.app.d", id, false),
        ("/Users/a/foo.app/Contents", id, false),
        ("/Users/a/.app", id, false),
        ("", id, false),
        ("/Applications/synthesis.app", None, false),
        ("/Applications/synthesis.app", Some(""), false),
        ("/Applications/synthesis.app", Some("  "), false),
    ];
    for (path, identifier, expected) in cases {
        assert_eq!(
            is_app_bundle(path, *identifier),
            *expected,
            "path {path:?}, identifier {identifier:?}"
        );
    }
}

// NTD-FR-13: an identifier of this run gives back its id; one of an earlier
// run, of another application, or of an unknown shape gives nothing.
#[test]
fn only_this_runs_identifiers_give_back_an_id() {
    let token = "abc123";
    let identifier = platform_identifier(token, "notif-7");
    assert_eq!(identifier, "synthesis.abc123.notif-7");
    assert_eq!(own_id(token, &identifier), Some("notif-7"));

    let earlier = platform_identifier("abc122", "notif-7");
    assert_eq!(own_id(token, &earlier), None);
    // A token that only starts with this one is a different run.
    assert_eq!(own_id(token, "synthesis.abc1234.notif-7"), None);
    assert_eq!(own_id(token, "synthesis.abc123."), None);
    assert_eq!(own_id(token, "synthesis.abc123"), None);
    assert_eq!(own_id(token, "other.abc123.notif-7"), None);
    assert_eq!(own_id(token, "synthesisabc123.notif-7"), None);
    assert_eq!(own_id(token, ""), None);
}

// NTD-FR-13, NTD-FR-06: the token is stable within a run, and one id always
// gives the same identifier, so a superseding post replaces in place.
#[test]
fn the_run_token_is_stable_and_one_id_gives_one_identifier() {
    let token = run_token();
    assert!(!token.is_empty());
    assert!(!token.contains('.'), "a dot would break the identifier shape");
    assert_eq!(run_token(), token);
    assert_eq!(
        platform_identifier(token, "notif-0"),
        platform_identifier(run_token(), "notif-0")
    );
    let identifier = platform_identifier(token, "notif-0");
    assert_eq!(own_id(run_token(), &identifier), Some("notif-0"));
}

// NTD-FR-02: provisional and ephemeral permission let the application post. A
// status this code does not know reads as denied.
#[test]
fn every_platform_status_gives_its_disposition() {
    assert_eq!(permission_from_status(0), PermissionState::NotRequested);
    assert_eq!(permission_from_status(1), PermissionState::Denied);
    assert_eq!(permission_from_status(2), PermissionState::Granted);
    assert_eq!(permission_from_status(3), PermissionState::Granted);
    assert_eq!(permission_from_status(4), PermissionState::Granted);
    assert_eq!(permission_from_status(5), PermissionState::Denied);
    assert_eq!(permission_from_status(-1), PermissionState::Denied);
}

// NTD-FR-03: only a state nobody asked for shows a prompt.
#[test]
fn only_an_unrequested_state_shows_a_prompt() {
    assert_eq!(
        request_decision(PermissionState::NotRequested),
        RequestDecision::Ask
    );
    for state in [
        PermissionState::Granted,
        PermissionState::Denied,
        PermissionState::Unsupported,
    ] {
        assert_eq!(request_decision(state), RequestDecision::Answer(state));
    }
}

// NTD-FR-16, NTD-FR-14: only the platform's own "not allowed" is a refusal.
#[test]
fn only_the_not_allowed_error_is_a_permission_refusal() {
    assert_eq!(
        post_error_from("UNErrorDomain", 1),
        PostError::PermissionDenied
    );
    assert_eq!(post_error_from("UNErrorDomain", 2), PostError::DeliveryFailed);
    assert_eq!(post_error_from("NSCocoaErrorDomain", 1), PostError::DeliveryFailed);
    assert_eq!(post_error_from("", 0), PostError::DeliveryFailed);
}

// NTD-FR-FVPT: the subtitle goes on the first line, the body on the next, and
// an empty part adds no line.
#[test]
fn the_subtitle_folds_onto_the_first_line_of_the_body() {
    assert_eq!(fold_subtitle("acme", "Run finished."), "acme\nRun finished.");
    assert_eq!(fold_subtitle("", "Run finished."), "Run finished.");
    assert_eq!(fold_subtitle("acme", ""), "acme");
    assert_eq!(fold_subtitle("", ""), "");
    assert_eq!(
        fold_subtitle("acme", "line one\nline two"),
        "acme\nline one\nline two"
    );
}

fn counting_route(count: &Arc<AtomicUsize>) -> Box<dyn FnOnce() + Send> {
    let count = Arc::clone(count);
    Box::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    })
}

// NTD-FR-10: a route is given out once at most.
#[test]
fn a_route_is_given_out_once() {
    let routes = ClickRoutes::default();
    let count = Arc::new(AtomicUsize::new(0));
    routes.store("synthesis.t.notif-0".into(), counting_route(&count));

    let route = routes.take("synthesis.t.notif-0").expect("the route is held");
    route();
    assert!(routes.take("synthesis.t.notif-0").is_none());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(routes.len(), 0);
}

// NTD-FR-06: a later route for one identifier replaces the earlier one, so a
// click on a superseded notification runs only the current route.
#[test]
fn a_later_route_replaces_the_earlier_one() {
    let routes = ClickRoutes::default();
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    routes.store("synthesis.t.notif-0".into(), counting_route(&first));
    routes.store("synthesis.t.notif-0".into(), counting_route(&second));
    assert_eq!(routes.len(), 1);

    routes.take("synthesis.t.notif-0").unwrap()();
    assert_eq!(first.load(Ordering::SeqCst), 0);
    assert_eq!(second.load(Ordering::SeqCst), 1);
}

// NTD-FR-12, NTD-FR-16: a forgotten route runs never, and forgetting an
// identifier with no route does nothing.
#[test]
fn a_forgotten_route_never_runs() {
    let routes = ClickRoutes::default();
    let count = Arc::new(AtomicUsize::new(0));
    routes.store("synthesis.t.notif-0".into(), counting_route(&count));
    routes.store("synthesis.t.notif-1".into(), counting_route(&count));

    routes.forget("synthesis.t.notif-0");
    routes.forget("synthesis.t.unknown");
    assert!(routes.take("synthesis.t.notif-0").is_none());
    assert_eq!(routes.len(), 1, "the other route is untouched");
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

// NTD-FR-02, NTD-FR-15: a settings read the centre did not answer assumes no
// state; it reads as a centre this module cannot reach.
#[test]
fn an_unanswered_settings_read_is_unsupported() {
    assert_eq!(permission_from_answer(None), PermissionState::Unsupported);
    assert_eq!(permission_from_answer(Some(0)), PermissionState::NotRequested);
    assert_eq!(permission_from_answer(Some(2)), PermissionState::Granted);
}

/// Runs `request_flow` against reads that answer `reads` in turn, and reports
/// the result, how many reads it made, and how many prompts it showed.
fn run_request(reads: &[PermissionState]) -> (PermissionState, usize, usize) {
    let mut next = reads.iter().copied();
    let read_count = std::cell::Cell::new(0);
    let asks = std::cell::Cell::new(0);
    let result = request_flow(
        || {
            read_count.set(read_count.get() + 1);
            next.next().expect("no more reads expected")
        },
        || asks.set(asks.get() + 1),
    );
    (result, read_count.get(), asks.get())
}

// NTD-FR-03: an unrequested state shows the prompt once and answers with the
// state read after it.
#[test]
fn an_unrequested_state_prompts_once_and_answers_with_the_new_read() {
    assert_eq!(
        run_request(&[PermissionState::NotRequested, PermissionState::Granted]),
        (PermissionState::Granted, 2, 1)
    );
    // The author refused at the prompt, or the request failed: the read after
    // it says so.
    assert_eq!(
        run_request(&[PermissionState::NotRequested, PermissionState::Denied]),
        (PermissionState::Denied, 2, 1)
    );
}

// NTD-FR-03: every other state is answered as read, with no prompt.
#[test]
fn a_known_state_is_answered_without_a_prompt() {
    for state in [
        PermissionState::Granted,
        PermissionState::Denied,
        PermissionState::Unsupported,
    ] {
        assert_eq!(run_request(&[state]), (state, 1, 0), "state {state:?}");
    }
}

// NTD-FR-16: the centre's answer to a post gives its outcome, and a centre that
// does not answer gives a failed delivery.
#[test]
fn every_post_answer_gives_its_outcome() {
    assert_eq!(post_outcome(Some(None)), Ok(()));
    assert_eq!(
        post_outcome(Some(Some(("UNErrorDomain".into(), 1)))),
        Err(PostError::PermissionDenied)
    );
    assert_eq!(
        post_outcome(Some(Some(("UNErrorDomain".into(), 3)))),
        Err(PostError::DeliveryFailed)
    );
    assert_eq!(post_outcome(None), Err(PostError::DeliveryFailed));
}

// NTD-FR-09, NTD-FR-11, NTD-FR-13: a click on this run's notification opens
// it, a dismissal forgets it, and nothing on an earlier run's notification
// reaches this run.
#[test]
fn a_response_is_planned_by_run_first_then_by_action() {
    let token = "run1";
    let own = platform_identifier(token, "notif-3");
    let earlier = platform_identifier("run0", "notif-3");

    assert_eq!(
        plan_response(token, &own, ResponseAction::Open),
        ResponsePlan::Open("notif-3")
    );
    assert_eq!(
        plan_response(token, &own, ResponseAction::Dismiss),
        ResponsePlan::Forget("notif-3")
    );
    assert_eq!(
        plan_response(token, &own, ResponseAction::Other),
        ResponsePlan::Ignore
    );
    for action in [
        ResponseAction::Open,
        ResponseAction::Dismiss,
        ResponseAction::Other,
    ] {
        assert_eq!(plan_response(token, &earlier, action), ResponsePlan::Ignore);
        assert_eq!(
            plan_response(token, "com.other.app.notif-3", action),
            ResponsePlan::Ignore
        );
    }
}
