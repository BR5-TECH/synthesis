//! The claim commands driven through a mock application (GPP-FR-ANDE,
//! GPP-FR-IFVC, GPP-FR-IGER, GPP-FR-DHQM, GPP-FR-TOED, GPP-FR-BSLI,
//! GPP-FR-RAQP, GPP-FR-CWGH).

use tauri::Listener;

use super::super::{acknowledge_impl, claim_impl, poll_impl, retry_impl};
use super::*;
use crate::project_settings::load_github_pending_claims_from;

fn writes(h: &Harness) -> usize {
    h.calls_matching(|c| matches!(c, Call::SetStatus { .. }))
}

fn fetches(h: &Harness) -> usize {
    h.calls_matching(|c| matches!(c, Call::Fetch(_)))
}

fn draft_count(h: &Harness) -> usize {
    crate::drafts::list_drafts_impl(&h.root()).drafts.len()
}

/// GPP-FR-ANDE: with an invalid configuration, a claim and a retry refuse
/// with `polling_configuration_invalid` and read or write nothing on GitHub.
#[test]
fn an_invalid_configuration_refuses_claims_and_retries() {
    let h = Harness::new();
    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    *h.fake.shape.lock().unwrap() = Err(ERR_PROJECT_UNAVAILABLE.into());
    assert_eq!(h.select(Some(PROJECT)).configuration.state, ConfigurationState::Invalid);
    h.fake.calls.lock().unwrap().clear();
    crate::project_settings::save_github_pending_claims_to(&h.root(), &[pending(5, None)]).unwrap();

    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_CONFIGURATION_INVALID);
    assert_eq!(retry_impl(&h.handle(), 5).unwrap_err(), ERR_CONFIGURATION_INVALID);
    assert_eq!(fetches(&h), 0);
    assert_eq!(writes(&h), 0);
    assert!(h.fake.calls().is_empty());
}

/// GPP-FR-ANDE: a claim whose Project read finds a configuration error makes
/// the configuration invalid, so the next poll refuses.
#[test]
fn a_claim_that_finds_a_configuration_error_stops_the_next_poll() {
    let h = Harness::new();
    *h.fake.shape.lock().unwrap() = Err(ERR_GITHUB_UNREACHABLE.into());
    assert_eq!(h.select(Some(PROJECT)).configuration.state, ConfigurationState::Unchecked);
    let mut no_ready = valid_shape();
    no_ready.status_field.as_mut().unwrap().options.retain(|o| o.name != "Ready");
    *h.fake.shape.lock().unwrap() = Ok(no_ready);
    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_READY_OPTION_MISSING);
    assert_eq!(poll_impl(&h.handle()).unwrap_err(), ERR_CONFIGURATION_INVALID);
    assert_eq!(h.view().configuration.error_code.as_deref(), Some(ERR_READY_OPTION_MISSING));
}

/// GPP-FR-IFVC: a re-fetch failure, an issue that is not in the selected
/// Project, and an issue that is Ready only in another Project each change
/// nothing.
#[test]
fn a_claim_that_cannot_confirm_the_task_changes_nothing() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    *h.fake.fetch_error.lock().unwrap() = Some(ERR_GITHUB_UNREACHABLE.into());
    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_GITHUB_UNREACHABLE);
    *h.fake.fetch_error.lock().unwrap() = None;

    h.fake.hold_issue(5, "Ready", Some("Task"), "OPEN");
    h.fake.issues.lock().unwrap()[0].project_items.clear();
    assert_eq!(claim_impl(&h.handle(), 5).unwrap_err(), ERR_TASK_NOT_READY);

    h.fake.hold_issue(6, "Ready", Some("Task"), "OPEN");
    h.fake.issues.lock().unwrap()[1].project_items[0].project_id = "PVT_other".into();
    assert_eq!(claim_impl(&h.handle(), 6).unwrap_err(), ERR_TASK_NOT_READY);

    assert_eq!(writes(&h), 0);
    assert!(load_github_pending_claims_from(&h.root()).is_empty());
    assert!(h.view().pending_claims.is_empty());
    assert_eq!(draft_count(&h), 0);
}

/// GPP-FR-IGER / GPP-FR-DHQM / GPP-FR-TOED / GPP-FR-BSLI: a pending claim the
/// disk refuses is held in memory and shown, so Retry is offered; the retry
/// works from it, and the acknowledgement clears it. A malformed `local.toml`
/// is refused rather than wiped.
#[test]
fn a_claim_the_disk_refuses_is_held_in_memory() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    let local = h.dir.path().join(".synthesis/local.toml");
    std::fs::write(&local, "this is = = not toml").unwrap();

    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_PENDING_CLAIM_WRITE_FAILED);
    assert_eq!(writes(&h), 1, "the issue is In Progress");
    assert_eq!(std::fs::read_to_string(&local).unwrap(), "this is = = not toml", "not wiped");
    let view = h.view();
    assert_eq!(view.pending_claims.len(), 1);
    assert_eq!(view.pending_claims[0].issue_number, 4);
    assert_eq!(view.pending_claims[0].draft_id, None);
    assert_eq!(draft_count(&h), 0);
    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_CLAIM_PENDING);

    // The retry creates the draft from the held claim; the disk still refuses,
    // so the claim stays held, now naming the draft. The draft reaches the
    // panel and the commit all the same.
    let changed: Arc<Mutex<usize>> = Arc::default();
    let sink = changed.clone();
    h.app.listen(crate::drafts::DRAFTS_CHANGED, move |_| *sink.lock().unwrap() += 1);
    let retried = retry_impl(&h.handle(), 4).unwrap();
    assert_eq!(*changed.lock().unwrap(), 1, "drafts changed was announced");
    assert_eq!(draft_count(&h), 1);
    assert_eq!(
        h.view().pending_claims[0].draft_id.as_deref(),
        Some(retried.draft_id.as_str())
    );

    // Once the disk takes it, the claim moves to disk.
    std::fs::remove_file(&local).unwrap();
    let again = retry_impl(&h.handle(), 4).unwrap();
    assert_eq!(again.draft_id, retried.draft_id);
    assert_eq!(load_github_pending_claims_from(&h.root()).len(), 1);
    assert_eq!(writes(&h), 1, "no retry changed the status");

    acknowledge_impl(&h.handle(), 4).unwrap();
    assert!(h.view().pending_claims.is_empty());
    assert_eq!(acknowledge_impl(&h.handle(), 4).unwrap_err(), ERR_NO_PENDING_CLAIM);
}

/// GPP-FR-BSLI: an acknowledgement clears a claim held only in memory.
#[test]
fn an_acknowledgement_clears_a_claim_held_in_memory() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    std::fs::write(h.dir.path().join(".synthesis/local.toml"), "= broken").unwrap();
    claim_impl(&h.handle(), 4).unwrap_err();
    assert_eq!(h.view().pending_claims.len(), 1);
    acknowledge_impl(&h.handle(), 4).unwrap();
    assert!(h.view().pending_claims.is_empty());
}

/// GPP-FR-TOED: a retry needs no current selection: it links the draft to
/// the Project the claim recorded, after the selection was cleared or
/// changed.
#[test]
fn a_retry_works_after_the_selection_changed() {
    let h = Harness::new();
    h.fake.hold_issue(4, "In Progress", Some("Task"), "OPEN");
    crate::project_settings::save_github_pending_claims_to(&h.root(), &[pending(4, None)]).unwrap();

    h.select(None);
    let cleared = retry_impl(&h.handle(), 4).unwrap();
    let link = crate::drafts::github_issue_link(&h.root(), &cleared.draft_id).unwrap();
    assert_eq!(link.project_node_id, PROJECT);

    crate::project_settings::save_github_pending_claims_to(&h.root(), &[pending(5, None)]).unwrap();
    h.fake.hold_issue(5, "In Progress", Some("Task"), "OPEN");
    h.select(Some("PVT_other"));
    let changed = retry_impl(&h.handle(), 5).unwrap();
    let link = crate::drafts::github_issue_link(&h.root(), &changed.draft_id).unwrap();
    assert_eq!(link.project_node_id, PROJECT);
    assert_eq!(writes(&h), 0);
}

/// GPP-FR-RAQP: while a claim of an issue runs, a second claim and a retry of
/// the same issue refuse `claim_in_progress`, and GitHub sees one status
/// update.
#[test]
fn a_claim_in_flight_refuses_a_second_one() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *h.fake.pause.lock().unwrap() = Some((entered_tx, release_rx));

    let handle = h.handle();
    let first = std::thread::spawn(move || claim_impl(&handle, 4));
    entered_rx.recv().unwrap();
    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_CLAIM_IN_PROGRESS);
    assert_eq!(retry_impl(&h.handle(), 4).unwrap_err(), ERR_CLAIM_IN_PROGRESS);
    release_tx.send(()).unwrap();
    assert!(first.join().unwrap().is_ok());
    assert_eq!(writes(&h), 1);
}
