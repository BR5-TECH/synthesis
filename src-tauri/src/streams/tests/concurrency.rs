//! How many streams may hold a run at one time (the claim of `StreamState`).

use super::*;
use crate::project_settings::GraduationConcurrency;

// WKS-FR-NDSV — one run of a stream at a time, and the project's limit
// ---------------------------------------------------------------------------

// WKS-FR-NDSV: one run of a stream works at a time.
#[test]
fn one_run_of_a_stream_works_at_a_time() {
    let state = StreamState::default();
    assert!(state.claim("w1", "run-a", GraduationConcurrency::limited(4)));
    assert!(!state.claim("w1", "run-b", GraduationConcurrency::limited(4)), "a second run is refused");
    assert_eq!(state.holder_of("w1").as_deref(), Some("run-a"));

    state.release("w1");
    assert!(state.claim("w1", "run-b", GraduationConcurrency::limited(4)));
}

// WKS-FR-NDSV: streams work at the same time up to the project's limit.
#[test]
fn streams_work_at_the_same_time_up_to_the_projects_limit() {
    let state = StreamState::default();
    assert!(state.claim("w1", "run-a", GraduationConcurrency::limited(2)));
    assert!(state.claim("w2", "run-b", GraduationConcurrency::limited(2)));
    assert!(!state.claim("w3", "run-c", GraduationConcurrency::limited(2)), "the limit holds");
    assert_eq!(state.working_count(), 2);

    state.release("w1");
    assert!(state.claim("w3", "run-c", GraduationConcurrency::limited(2)));
}

// WKS-FR-NDSV: the resting limit is one, so nothing works beside anything else
// until the author asks for it.
#[test]
fn a_limit_of_one_lets_only_one_stream_work() {
    let state = StreamState::default();
    assert!(state.claim("w1", "run-a", GraduationConcurrency::limited(1)));
    assert!(!state.claim("w2", "run-b", GraduationConcurrency::limited(1)));
}

// WKS-FR-NDSV, GRD-FR-KPET: under `unlimited` no project cap refuses a claim of a
// free stream.
#[test]
fn an_unlimited_project_refuses_no_free_stream() {
    let state = StreamState::default();
    for n in 0..20 {
        assert!(state.claim(&format!("w{n}"), &format!("run-{n}"), GraduationConcurrency::Unlimited));
    }
    assert!(!state.claim("w0", "run-x", GraduationConcurrency::Unlimited), "one run per stream still holds");
}
