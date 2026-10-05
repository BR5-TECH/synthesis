//! The polling session (GPP-FR-DATH, GPP-FR-XZTP, GPP-FR-ELWS, GPP-FR-GPYE,
//! GPP-FR-PUXT, GPP-FR-YROY, GPP-FR-RRPB, GPP-FR-ANDE, GPP-FR-EWLZ,
//! GPP-FR-IURX, GPP-FR-TYOV).

use super::*;

const NOW: &str = "2026-10-02T10:00:00Z";

fn started(slot: &mut SessionSlot, key: &SessionKey) -> PollTicket {
    let generation = { slot.enter(key); slot.generation() };
    match slot.begin_poll(key, PROJECT, generation) {
        PollStart::Started(ticket) => ticket,
        PollStart::AlreadyRunning => panic!("a poll was already running"),
    }
}

fn poll(slot: &mut SessionSlot, key: &SessionKey, numbers: &[u64]) -> Settled {
    let ticket = started(slot, key);
    slot.settle_poll(&ticket, Ok(success(numbers)), NOW)
}

fn numbers(settled: &Settled) -> Vec<u64> {
    match settled {
        Settled::Succeeded(new) => new.iter().map(|n| n.issue_number).collect(),
        other => panic!("not a success: {other:?}"),
    }
}

fn view_of(slot: &SessionSlot, key: &SessionKey) -> GithubPollingView {
    view::build_view(slot, key, settings(Some(PROJECT)), Vec::new(), Vec::new())
}

/// GPP-FR-GPYE: a successful poll replaces the rows, clears the stale mark and
/// the last error, and records the instant of success.
#[test]
fn a_successful_poll_replaces_the_snapshot() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    poll(&mut slot, &key, &[1, 2]);
    let ticket = started(&mut slot, &key);
    slot.settle_poll(&ticket, Err(PollFailure::code(ERR_GITHUB_UNREACHABLE)), NOW);
    assert!(view_of(&slot, &key).stale);

    let ticket = started(&mut slot, &key);
    slot.settle_poll(&ticket, Ok(success(&[3])), "2026-10-02T11:00:00Z");
    let view = view_of(&slot, &key);
    assert_eq!(view.tasks.iter().map(|t| t.issue_number).collect::<Vec<_>>(), vec![3]);
    assert!(!view.stale);
    assert_eq!(view.last_error_code, None);
    assert_eq!(view.last_error, None);
    assert_eq!(view.last_success_at.as_deref(), Some("2026-10-02T11:00:00Z"));
    assert_eq!(view.configuration.state, ConfigurationState::Valid);
    assert_eq!(view.configuration.project_title.as_deref(), Some("Roadmap"));
    assert_eq!(view.repository, Some(repo()));
}

/// GPP-FR-PUXT: a failed poll keeps the last rows, marks them stale, and
/// records the typed error with text. A configuration error becomes the
/// configuration state instead.
#[test]
fn a_failed_poll_keeps_the_rows_and_marks_them_stale() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    poll(&mut slot, &key, &[1, 2]);

    let ticket = started(&mut slot, &key);
    let settled = slot.settle_poll(&ticket, Err(PollFailure::code(ERR_REQUEST_FAILED)), NOW);
    assert_eq!(settled, Settled::Failed(ERR_REQUEST_FAILED.into()));
    let view = view_of(&slot, &key);
    assert_eq!(view.tasks.len(), 2);
    assert!(view.stale);
    assert_eq!(view.last_error_code.as_deref(), Some(ERR_REQUEST_FAILED));
    assert_eq!(view.last_error, Some(error_text(ERR_REQUEST_FAILED)));
    assert_eq!(view.last_success_at.as_deref(), Some(NOW));

    let ticket = started(&mut slot, &key);
    slot.settle_poll(
        &ticket,
        Err(PollFailure { code: ERR_STATUS_FIELD_MISSING.into(), project_title: Some("Roadmap".into()) }),
        NOW,
    );
    let view = view_of(&slot, &key);
    assert_eq!(view.configuration.state, ConfigurationState::Invalid);
    assert_eq!(view.configuration.error_code.as_deref(), Some(ERR_STATUS_FIELD_MISSING));
    assert!(view.configuration.error.is_some());
    assert_eq!(view.tasks.len(), 2, "the rows are kept");
}

/// GPP-FR-ANDE / GPP-FR-EWLZ: an invalid configuration refuses polls and
/// claims until the next settings write; no Project selected refuses with
/// `polling_unconfigured` and changes no row.
#[test]
fn an_invalid_or_missing_configuration_refuses_polls() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    poll(&mut slot, &key, &[1]);
    assert_eq!(
        slot.require_pollable(&key, &settings(None)),
        Err(ERR_UNCONFIGURED.to_string())
    );
    assert_eq!(view_of(&slot, &key).tasks.len(), 1, "no row changed");

    let generation = slot.generation();
    slot.record_configuration(
        &key,
        generation,
        PROJECT,
        GithubPollingConfiguration::invalid(ERR_PROJECT_UNAVAILABLE, None),
    );
    assert_eq!(
        slot.require_pollable(&key, &settings(Some(PROJECT))),
        Err(ERR_CONFIGURATION_INVALID.to_string())
    );
    // Still invalid after another read of the same settings.
    assert!(slot.is_invalid(&key, PROJECT));

    // The next settings write puts it back to unchecked, which polls.
    slot.settings_changed(&key);
    assert_eq!(slot.require_pollable(&key, &settings(Some(PROJECT))), Ok(PROJECT.to_string()));
    assert_eq!(view_of(&slot, &key).configuration.state, ConfigurationState::Unchecked);
}

/// GPP-FR-IURX: a write records the validation it ran; a `null` Project is
/// `unset`; a validation that started before a later write is ignored.
#[test]
fn a_settings_write_records_its_validation() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    assert_eq!(slot.configuration_for(&key, &settings(None)).state, ConfigurationState::Unset);
    assert_eq!(
        slot.configuration_for(&key, &settings(Some(PROJECT))).state,
        ConfigurationState::Unchecked
    );

    slot.settings_changed(&key);
    let generation = slot.generation();
    slot.record_configuration(&key, generation, PROJECT, GithubPollingConfiguration::valid("Roadmap"));
    let configuration = slot.configuration_for(&key, &settings(Some(PROJECT)));
    assert_eq!(configuration.state, ConfigurationState::Valid);
    assert_eq!(configuration.project_title.as_deref(), Some("Roadmap"));
    // Another Project selected on disk reads as unchecked.
    assert_eq!(
        slot.configuration_for(&key, &settings(Some("PVT_other"))).state,
        ConfigurationState::Unchecked
    );

    // A stale validation result is dropped.
    slot.settings_changed(&key);
    slot.record_configuration(&key, generation, PROJECT, GithubPollingConfiguration::valid("Old"));
    assert_eq!(
        slot.configuration_for(&key, &settings(Some(PROJECT))).state,
        ConfigurationState::Unchecked
    );
}

/// GPP-FR-XZTP: one poll at a time; a second call starts nothing and the view
/// says a poll runs.
#[test]
fn a_second_poll_while_one_runs_starts_nothing() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    let ticket = started(&mut slot, &key);
    let generation = slot.generation();
    assert_eq!(slot.begin_poll(&key, PROJECT, generation), PollStart::AlreadyRunning);
    assert!(view_of(&slot, &key).polling);
    slot.settle_poll(&ticket, Ok(success(&[1])), NOW);
    assert!(!view_of(&slot, &key).polling);
    let generation = slot.generation();
    assert!(matches!(slot.begin_poll(&key, PROJECT, generation), PollStart::Started(_)));
}

/// GPP-FR-ELWS / GPP-FR-RRPB: a poll whose project, worktree, or settings
/// changed while it ran is discarded and changes nothing.
#[test]
fn a_result_after_a_change_is_discarded() {
    let dir = project();
    let key = key(&dir);
    let other_worktree = SessionKey { project: key.project.clone(), worktree: "/elsewhere".into() };
    let other_project = SessionKey { project: "other".into(), worktree: key.worktree.clone() };

    // Settings changed.
    let mut slot = SessionSlot::default();
    poll(&mut slot, &key, &[1]);
    let ticket = started(&mut slot, &key);
    slot.settings_changed(&key);
    assert_eq!(slot.settle_poll(&ticket, Ok(success(&[1, 2])), NOW), Settled::Discarded);
    let view = view_of(&slot, &key);
    assert_eq!(view.tasks.len(), 1, "no row changed");
    assert!(!view.polling);

    // A failure after a change is discarded too: no stale mark.
    let ticket = started(&mut slot, &key);
    slot.settings_changed(&key);
    assert_eq!(
        slot.settle_poll(&ticket, Err(PollFailure::code(ERR_REQUEST_FAILED)), NOW),
        Settled::Discarded
    );
    assert!(!view_of(&slot, &key).stale);

    // Worktree and project changed.
    for changed in [other_worktree, other_project] {
        let mut slot = SessionSlot::default();
        let ticket = started(&mut slot, &key);
        slot.enter(&changed);
        assert_eq!(slot.settle_poll(&ticket, Ok(success(&[5])), NOW), Settled::Discarded);
        assert!(slot.session_for(&changed).unwrap().tasks.is_empty());
        assert!(slot.session_for(&changed).unwrap().reported.is_empty());
    }
}

/// GPP-FR-YROY / GPP-FR-RRPB: the first poll reports every eligible issue; a
/// later one reports only what is new; an issue is never reported twice in a
/// session, even when it leaves and comes back. Failures report nothing.
#[test]
fn the_notification_delta_is_new_issues_once_per_session() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    assert_eq!(numbers(&poll(&mut slot, &key, &[1, 2])), vec![1, 2]);
    assert_eq!(numbers(&poll(&mut slot, &key, &[1, 2, 3])), vec![3]);
    assert!(numbers(&poll(&mut slot, &key, &[1, 2, 3])).is_empty());
    // Issue 1 leaves the result and comes back: not reported again.
    assert!(numbers(&poll(&mut slot, &key, &[2, 3])).is_empty());
    assert!(numbers(&poll(&mut slot, &key, &[1, 2, 3])).is_empty());
    // A failed poll reports nothing and keeps the previous keys.
    let ticket = started(&mut slot, &key);
    assert_eq!(
        slot.settle_poll(&ticket, Err(PollFailure::code(ERR_GITHUB_UNREACHABLE)), NOW),
        Settled::Failed(ERR_GITHUB_UNREACHABLE.into())
    );
    assert_eq!(numbers(&poll(&mut slot, &key, &[1, 2, 3, 4])), vec![4]);
}

/// GPP-FR-DATH: a new session starts when the worktree changes or the project
/// closes, and it compares against an empty result again.
#[test]
fn a_new_session_starts_on_a_worktree_change_and_on_close() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    assert_eq!(numbers(&poll(&mut slot, &key, &[1])), vec![1]);

    let other = SessionKey { project: key.project.clone(), worktree: "/elsewhere".into() };
    assert_eq!(numbers(&poll(&mut slot, &other, &[1])), vec![1], "a new session");
    assert_eq!(numbers(&poll(&mut slot, &key, &[1])), vec![1], "and back is new again");

    slot.end();
    assert!(slot.session_for(&key).is_none());
    assert_eq!(numbers(&poll(&mut slot, &key, &[1])), vec![1], "reopened");
}

/// GPP-FR-TYOV: the payload carries each new issue's number and title, and an
/// empty list for every other cause.
#[test]
fn the_event_payload_names_new_issues() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    let Settled::Succeeded(new_issues) = poll(&mut slot, &key, &[7]) else { panic!() };
    let payload = serde_json::to_value(GithubPollingChanged { new_issues }).unwrap();
    assert_eq!(payload, serde_json::json!({ "newIssues": [{ "issueNumber": 7, "title": "Task 7" }] }));
    let empty = serde_json::to_value(GithubPollingChanged::default()).unwrap();
    assert_eq!(empty, serde_json::json!({ "newIssues": [] }));
    assert_eq!(GITHUB_POLLING_CHANGED, "github-polling-changed");
}

/// GPP-FR-QCAM / GPP-FR-IGER: the view drops a row that a claim made since the
/// poll now names, and lists the pending claims.
#[test]
fn the_view_excludes_rows_claimed_since_the_poll() {
    let dir = project();
    let key = key(&dir);
    let mut slot = SessionSlot::default();
    poll(&mut slot, &key, &[1, 2]);
    let view = view::build_view(&slot, &key, settings(Some(PROJECT)), vec![pending(1, None)], Vec::new());
    assert_eq!(view.tasks.iter().map(|t| t.issue_number).collect::<Vec<_>>(), vec![2]);
    assert_eq!(view.pending_claims, vec![pending(1, None)]);
    let json = serde_json::to_value(&view).unwrap();
    for field in [
        "settings", "configuration", "repository", "polling", "tasks", "stale", "lastErrorCode",
        "lastError", "lastSuccessAt", "pendingClaims", "shadows",
    ] {
        assert!(json.get(field).is_some(), "{field}");
    }
    assert_eq!(json["configuration"]["state"], "valid");
    assert_eq!(json["repository"], serde_json::json!({ "owner": "acme", "name": "widgets" }));
}

/// GPP-FR-DATH: a repository resolved under a key that is no longer the
/// session's is not remembered, and it does not reset the session.
#[test]
fn a_stale_key_does_not_reset_the_session() {
    let dir = project();
    let key = key(&dir);
    let other = SessionKey { project: "other".into(), worktree: "/elsewhere".into() };
    let mut slot = SessionSlot::default();
    poll(&mut slot, &other, &[1]);
    slot.remember_repository(&key, &repo());
    slot.keep_unsaved(&key, pending(4, None));
    assert!(slot.session_for(&key).is_none());
    let live = slot.session_for(&other).expect("the live session");
    assert_eq!(live.tasks.len(), 1);
    assert!(live.unsaved_claims.is_empty());
}
