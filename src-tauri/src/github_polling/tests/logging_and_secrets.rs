//! What the module logs and what it never carries (GPP-FR-WKZF,
//! GPP-FR-PUXT, GPP-FR-WOIM).

use super::super::claims;
use super::*;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};

fn records(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap()
        .records
}

static GPP_POLL_LOG: LogBuffer = LogBuffer::new();
static GPP_CLAIM_LOG: LogBuffer = LogBuffer::new();

fn failures(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    records(buffer)
        .into_iter()
        .filter(|r| r.message == "github polling operation failed")
        .collect()
}

/// GPP-FR-WKZF / GPP-FR-PUXT: a failed poll and a failed claim each emit
/// exactly one failure record with the operation, the code, and the issue
/// number where one applies; no record holds the secret or an issue body.
#[test]
fn every_failure_is_one_log_record_naming_operation_code_and_issue() {
    let h = Harness::with_buffer(&GPP_POLL_LOG);
    h.select(Some(PROJECT));
    *h.fake.items.lock().unwrap() = Err(ERR_GITHUB_UNREACHABLE.into());
    super::super::poll_impl(&h.handle()).unwrap();
    let poll = failures(&GPP_POLL_LOG);
    assert_eq!(poll.len(), 1);
    assert_eq!(poll[0].level, LogLevel::Warn);
    assert!(poll[0].domains.contains(&Domain::Backend));
    assert_eq!(poll[0].fields["operation"], "poll");
    assert_eq!(poll[0].fields["code"], ERR_GITHUB_UNREACHABLE);
    assert!(poll[0].fields.get("issue").is_none());

    let h = Harness::with_buffer(&GPP_CLAIM_LOG);
    h.select(Some(PROJECT));
    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    *h.fake.set_status_error.lock().unwrap() = Some(ERR_REQUEST_FAILED.into());
    let refused = super::super::claim_impl(&h.handle(), 4).unwrap_err();
    assert_eq!(refused, ERR_STATUS_UPDATE_FAILED);
    let claim = failures(&GPP_CLAIM_LOG);
    assert_eq!(claim.len(), 1);
    assert_eq!(claim[0].fields["operation"], "claim");
    assert_eq!(claim[0].fields["code"], ERR_STATUS_UPDATE_FAILED);
    assert_eq!(claim[0].fields["issue"], 4);

    for buffer in [&GPP_POLL_LOG, &GPP_CLAIM_LOG] {
        let rendered = serde_json::to_string(&records(buffer)).unwrap();
        assert!(!rendered.contains(SECRET), "a record holds the secret");
        assert!(!rendered.contains("Body of task"), "a record holds an issue body");
        assert!(!rendered.contains("Bearer"));
    }
}

/// GPP-FR-WOIM: no secret is stored, copied, or returned in the view, the
/// pending claims, the shadow draft's record, or an error.
#[test]
fn the_secret_reaches_no_state_record_or_error() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let repository = repo();
    let fake = FakeProjects::with_items(vec![ready(4), ready(5)]);
    fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    fake.hold_issue(6, "Ready", Some("Task"), "OPEN");
    *fake.set_status_error.lock().unwrap() = None;

    let found = poll_once(&root, &fake, SECRET, &repository, PROJECT).unwrap();
    let mut slot = SessionSlot::default();
    let key = key(&dir);
    let ticket = match slot.begin_poll(&key, PROJECT, slot.generation()) {
        PollStart::Started(t) => t,
        PollStart::AlreadyRunning => unreachable!(),
    };
    slot.settle_poll(&ticket, Ok(found), "2026-10-02T10:00:00Z");
    let outcome = claims::claim(&context(&root, &repository), &fake, 4, "2026-10-02T10:01:00Z", &[]).unwrap();
    *fake.set_status_error.lock().unwrap() = Some(ERR_REQUEST_FAILED.into());
    let refused = claims::claim(&context(&root, &repository), &fake, 6, "2026-10-02T10:02:00Z", &[]).unwrap_err();

    let view = view::build_view(
        &slot,
        &key,
        settings(Some(PROJECT)),
        crate::project_settings::load_github_pending_claims_from(&root),
        Vec::new(),
    );
    let mut everything = serde_json::to_string(&view).unwrap();
    everything.push_str(&refused.code);
    everything.push_str(&std::fs::read_to_string(dir.path().join(".synthesis/local.toml")).unwrap());
    let draft = crate::drafts::read_draft_record(&root, &outcome.result.draft_id).unwrap();
    everything.push_str(&serde_json::to_string(&draft).unwrap());
    assert!(!everything.contains(SECRET), "the secret leaked");
    assert!(!everything.contains("Bearer"));
}
