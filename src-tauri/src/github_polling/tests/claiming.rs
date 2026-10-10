//! Claiming a ready task (GPP-FR-IFVC, GPP-FR-ILGG, GPP-FR-DHQM,
//! GPP-FR-XPUO, GPP-FR-CWGH, GPP-FR-DGWL, GPP-FR-BSLI, GPP-FR-IGER,
//! GPP-FR-TOED, GPP-FR-KSMZ, GPP-FR-RAQP, GPP-FR-ZLBF, GPP-FR-CRWY).

use super::super::claims;
use super::*;
use crate::drafts::DraftStatus;
use crate::project_settings::{load_github_pending_claims_from, save_github_pending_claims_to};

const NOW: &str = "2026-10-02T09:30:00Z";

fn root_of(dir: &TempDir) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(dir.path())
}

fn draft_count(root: &crate::fs::RootFs) -> usize {
    crate::drafts::list_drafts_impl(root).drafts.len()
}

fn set_status_calls(fake: &FakeProjects) -> Vec<Call> {
    fake.calls().into_iter().filter(|c| matches!(c, Call::SetStatus { .. })).collect()
}

/// GPP-FR-IFVC / GPP-FR-ILGG / GPP-FR-DHQM / GPP-FR-XPUO / GPP-FR-CWGH: the
/// claim re-fetches, moves the item to In Progress, records the pending claim,
/// creates the shadow draft from the re-fetched title and body, writes its id
/// into the claim, and returns its id and name.
#[test]
fn a_claim_runs_its_steps_in_order() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");

    let outcome = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).expect("claimed");

    assert_eq!(
        fake.calls(),
        vec![
            Call::Shape(PROJECT.into()),
            Call::Fetch(4),
            Call::SetStatus { item: "ITEM_4".into(), option: "OPT_progress".into() },
        ]
    );
    assert!(outcome.created);
    assert_eq!(outcome.result.draft_name, "Task 4");
    let record = crate::drafts::read_draft_prompt(&root, &outcome.result.draft_id).unwrap();
    assert_eq!(record.content, "Body of task 4\nsecond line\n");
    assert_eq!(record.draft.status, DraftStatus::GithubShadow);
    assert_eq!(record.draft.github_issue, Some(link(4)));

    let stored = load_github_pending_claims_from(&root);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].draft_id.as_deref(), Some(outcome.result.draft_id.as_str()));
    assert_eq!(stored[0].issue_url, "https://github.com/acme/widgets/issues/4");
    assert_eq!(stored[0].project_node_id, PROJECT);
    assert_eq!(stored[0].claimed_at, NOW);
}

/// GPP-FR-IFVC: an issue that is no longer eligible when re-fetched is
/// refused with `task_not_ready`, and nothing changes.
#[test]
fn a_task_that_is_no_longer_ready_is_refused_and_nothing_changes() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(1, "In Progress", Some("Task"), "OPEN");
    fake.hold_issue(2, "Ready", Some("Bug"), "OPEN");
    fake.hold_issue(3, "Ready", Some("Task"), "CLOSED");
    for number in [1, 2, 3, 99] {
        let failure = claims::claim(&context(&root, &repository), &fake, number, NOW, &[]).unwrap_err();
        assert_eq!(failure.code, ERR_TASK_NOT_READY, "issue {number}");
    }
    assert!(set_status_calls(&fake).is_empty());
    assert!(load_github_pending_claims_from(&root).is_empty());
    assert_eq!(draft_count(&root), 0);
}

/// GPP-FR-ILGG / GPP-FR-IGER: a failed status update writes no pending claim,
/// creates no draft, and opens nothing.
#[test]
fn a_failed_status_update_writes_nothing() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    *fake.set_status_error.lock().unwrap() = Some(ERR_REQUEST_FAILED.into());
    let failure = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap_err();
    assert_eq!(failure.code, ERR_STATUS_UPDATE_FAILED);
    assert!(load_github_pending_claims_from(&root).is_empty());
    assert_eq!(draft_count(&root), 0);
    assert!(!dir.path().join(".synthesis/local.toml").exists());
}

/// GPP-FR-DHQM / GPP-FR-CWGH / GPP-FR-IGER: a draft that cannot be created
/// returns `shadow_draft_create_failed` and leaves the pending claim (written
/// first) and the GitHub status as they are.
#[test]
fn a_failed_draft_creation_keeps_the_pending_claim() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    // A file where the drafts root must be makes every creation fail.
    std::fs::write(dir.path().join(".synthesis/drafts"), "not a folder").unwrap();
    let failure = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap_err();
    assert_eq!(failure.code, ERR_SHADOW_CREATE_FAILED);
    let stored = load_github_pending_claims_from(&root);
    assert_eq!(stored, vec![GithubPendingClaim { claimed_at: NOW.into(), ..pending(4, None) }]);
    assert_eq!(set_status_calls(&fake).len(), 1, "the status update stands");

    // GPP-FR-TOED: the retry creates the missing draft and changes no status.
    std::fs::remove_file(dir.path().join(".synthesis/drafts")).unwrap();
    let retried = claims::retry(&root, SECRET, &repository, &fake, 4, &[]).expect("retried");
    assert!(retried.created);
    assert_eq!(set_status_calls(&fake).len(), 1, "no second status update");
    assert_eq!(
        load_github_pending_claims_from(&root)[0].draft_id.as_deref(),
        Some(retried.result.draft_id.as_str())
    );
}

/// GPP-FR-TOED: a retry re-fetches for the latest title and body, creates the
/// draft only where none names the issue, and never changes the status.
#[test]
fn a_retry_creates_only_what_is_missing() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "In Progress", Some("Task"), "OPEN");
    fake.issues.lock().unwrap()[0].title = "Renamed on GitHub".into();
    save_github_pending_claims_to(&root, &[pending(4, None)]).unwrap();

    let first = claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap();
    assert!(first.created);
    assert_eq!(first.result.draft_name, "Renamed on GitHub");

    // The claim names the draft now: a second retry reuses it.
    let second = claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap();
    assert!(!second.created);
    assert_eq!(second.result.draft_id, first.result.draft_id);

    // A claim that names no draft while a shadow names the issue reuses it.
    save_github_pending_claims_to(&root, &[pending(4, None)]).unwrap();
    let third = claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap();
    assert!(!third.created);
    assert_eq!(third.result.draft_id, first.result.draft_id);
    assert_eq!(draft_count(&root), 1);
    assert!(set_status_calls(&fake).is_empty());
    assert_eq!(fake.calls().iter().filter(|c| matches!(c, Call::Fetch(4))).count(), 3);
}

/// GPP-FR-KSMZ: a retry with no pending claim refuses `no_pending_claim`; a
/// re-fetch that fails refuses with its code and keeps the claim.
#[test]
fn a_retry_refuses_without_a_claim_and_keeps_it_on_failure() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    assert_eq!(
        claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap_err(),
        ERR_NO_PENDING_CLAIM
    );
    save_github_pending_claims_to(&root, &[pending(4, None)]).unwrap();
    *fake.fetch_error.lock().unwrap() = Some(ERR_GITHUB_UNREACHABLE.into());
    assert_eq!(
        claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap_err(),
        ERR_GITHUB_UNREACHABLE
    );
    assert_eq!(load_github_pending_claims_from(&root), vec![pending(4, None)]);
    assert_eq!(draft_count(&root), 0);
}

/// GPP-FR-XPUO: a claim reuses a shadow draft that already names the issue.
#[test]
fn a_claim_reuses_an_existing_shadow_draft() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let existing = crate::drafts::create_github_shadow_draft(&root, "Old title", "old", link(4)).unwrap();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    let outcome = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap();
    assert!(!outcome.created);
    assert_eq!(outcome.result.draft_id, existing.draft.id);
    assert_eq!(draft_count(&root), 1);
}

/// GPP-FR-RAQP: a claim against an issue that holds a pending claim refuses
/// `claim_pending` before any GitHub call, and a second claim or retry of the
/// same issue while one runs refuses `claim_in_progress`.
#[test]
fn claims_of_one_issue_do_not_overlap() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    save_github_pending_claims_to(&root, &[pending(4, None)]).unwrap();
    let failure = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap_err();
    assert_eq!(failure.code, ERR_CLAIM_PENDING);
    assert!(fake.calls().is_empty());

    let guards = ClaimGuards::default();
    let held = guards.acquire(IssueKey::new("github.com", "acme", "widgets", 4)).unwrap();
    assert_eq!(
        guards.acquire(IssueKey::new("github.com", "ACME", "Widgets", 4)).err(),
        Some(ERR_CLAIM_IN_PROGRESS.to_string())
    );
    assert!(guards.acquire(IssueKey::new("github.com", "acme", "widgets", 5)).is_ok(), "another issue runs");
    drop(held);
    assert!(guards.acquire(IssueKey::new("github.com", "acme", "widgets", 4)).is_ok(), "released");
}

/// GPP-FR-BSLI / GPP-FR-IGER: a pending claim survives a reload of the store
/// and is removed only by its acknowledgement.
#[test]
fn a_pending_claim_lasts_until_it_is_acknowledged() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    fake.hold_issue(5, "Ready", Some("Task"), "OPEN");
    claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap();
    claims::claim(&context(&root, &repository), &fake, 5, NOW, &[]).unwrap();

    // A restart reads the same store from disk.
    let reopened = crate::fs::RootFs::for_root(dir.path());
    assert_eq!(load_github_pending_claims_from(&reopened).len(), 2);

    claims::acknowledge(&reopened, &repository, 4).unwrap();
    let left = load_github_pending_claims_from(&reopened);
    assert_eq!(left.iter().map(|c| c.issue_number).collect::<Vec<_>>(), vec![5]);
    assert_eq!(
        claims::acknowledge(&reopened, &repository, 4),
        Err(ERR_NO_PENDING_CLAIM.to_string())
    );
}

/// GPP-FR-DGWL / GPP-FR-ZLBF: a claim starts no graduation; the shadow draft
/// stays available to graduate and the issue stays In Progress whatever the
/// dialog does.
#[test]
fn a_claim_starts_no_graduation_and_the_draft_stays_available() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::new();
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    let outcome = claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap();

    // Nothing but the draft exists: no run is attributed to it.
    let queue = crate::graduation::GraduationQueue::default();
    assert!(crate::graduation::draft_graduation(&queue, &outcome.result.draft_id).is_none());
    // A cancelled dialog changes nothing: the draft still reads github_shadow.
    assert_eq!(
        crate::drafts::resolved_shadow_status_for_test(&queue, &outcome.result.draft_id, DraftStatus::GithubShadow),
        DraftStatus::GithubShadow
    );
    assert!(crate::drafts::read_draft_prompt(&root, &outcome.result.draft_id).is_ok());
    // And the one status write moved the issue to In Progress, never back.
    assert_eq!(
        set_status_calls(&fake),
        vec![Call::SetStatus { item: "ITEM_4".into(), option: "OPT_progress".into() }]
    );
}

/// GPP-FR-CRWY: across a poll, a claim, a retry, and an acknowledgement, the
/// only GitHub write is one status update to `In Progress`. No Done, no close,
/// no edit of the title, body, labels, or comments.
#[test]
fn no_operation_closes_or_edits_an_issue() {
    let dir = project();
    let root = root_of(&dir);
    let repository = repo();
    let fake = FakeProjects::with_items(vec![ready(4)]);
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    poll_once(&root, &fake, SECRET, &repository, PROJECT).unwrap();
    claims::claim(&context(&root, &repository), &fake, 4, NOW, &[]).unwrap();
    claims::retry(&root, SECRET, &repository, &fake, 4, &[]).unwrap();
    claims::acknowledge(&root, &repository, 4).unwrap();
    let writes = set_status_calls(&fake);
    assert_eq!(writes.len(), 1);
    assert!(writes.iter().all(|c| matches!(c, Call::SetStatus { option, .. } if option == "OPT_progress")));
    // The trait offers no other write: every other recorded call is a read.
    assert!(fake.calls().iter().all(|c| matches!(
        c,
        Call::Shape(_) | Call::Items(_) | Call::Fetch(_) | Call::SetStatus { .. }
    )));
}
