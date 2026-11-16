//! The tests of the loads of a GitHub delivery plan
//! (`GDP-github-delivery-plan.md` GDP-FR-QSHX to GDP-FR-RJFS, GDP-FR-PIAQ,
//! GDP-FR-QHQR, GDP-FR-ZFQH), against a fake tracker adapter in a mock
//! application. No test reaches a network.

use std::sync::mpsc;

use serde_json::json;
use tauri::Manager;

mod edit_cases;
mod edit_refusals;
pub(crate) mod fake;
mod plan_cases;

use super::*;
use crate::delivery::settings::{DeliverySettings, DeliverySourceChoice};
use crate::logging::{LogBuffer, LogFilter};
use fake::*;

fn load(h: &Harness, refresh: bool) -> Result<GithubDeliveryPlan, DeliveryError> {
    load_plan_impl(&h.handle(), refresh)
}

fn reads(h: &Harness) -> usize {
    h.fake.calls().iter().filter(|c| c.starts_with("read_project")).count()
}

/// Wait until `done` holds: a state, not a time. The deadline only stops a
/// test that would never end; no result depends on it.
pub(crate) fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::park_timeout(std::time::Duration::from_millis(1));
    }
}

/// Write `read_at` into the stored read cache, so a test can see which read an
/// answer comes from without a clock.
fn mark_cache(h: &Harness, read_at: &str) {
    let path = h.store.path().join("delivery/github").join(format!("{PROJECT}.json"));
    let mut stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    stored["readAt"] = json!(read_at);
    std::fs::write(&path, stored.to_string()).unwrap();
}

const SENTINEL: &str = "2001-02-03T04:05:06Z";

/// GDP-FR-QSHX, GDP-FR-HNEW, GDP-FR-XWES: a load of a GitHub source answers
/// the plan as a version 3 Flow document with its tracker facts, states,
/// overlaps, and notices, in the frontend shape.
#[test]
fn a_load_answers_the_plan_in_the_frontend_shape() {
    let h = Harness::github_plan();
    h.fake.lock().board.push(("PR".into(), None, Board::PullRequest));
    h.fake.lock().board.push(("DRAFT".into(), None, Board::Draft));
    let plan = load(&h, true).expect("a plan");
    let value = serde_json::to_value(&plan).unwrap();
    for key in ["document", "tracker", "states", "overlaps", "notices", "readAt", "stale", "error"]
    {
        assert!(value.get(key).is_some(), "the answer has {key}");
    }
    assert_eq!(value["document"]["version"], 3);
    assert_eq!(value["document"]["name"], "Roadmap");
    let ids: Vec<&str> = plan.document.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["milestone-1", "issue-1", "issue-2", "issue-3"]);
    assert_eq!(
        value["notices"],
        json!({ "unmappedTypes": [], "misplaced": [],
                                          "omittedRelations": [], "skipped": 2 })
    );
    // FDL-FR-YIAZ through compute_delivery_state: one state per item.
    assert_eq!(plan.states.len(), 4);
    assert_eq!(value["states"][3]["itemId"], "issue-3");
    assert_eq!(value["states"][3]["state"], "not_started");
    assert_eq!(value["stale"], false);
    assert_eq!(value["error"], json!(null));
}

/// GDP-FR-QSHX: a source other than `github` is `not_github_source`, and no
/// request is made.
#[test]
fn another_source_is_refused() {
    let h = Harness::new();
    assert_eq!(load(&h, true).unwrap_err().code, ERR_NOT_GITHUB_SOURCE, "no Project");
    let h = Harness::github_plan();
    h.write("plans/a.flow", "");
    h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: "plans/a.flow".into() },
        ..DeliverySettings::default()
    });
    assert_eq!(load(&h, true).unwrap_err().code, ERR_NOT_GITHUB_SOURCE);
    assert!(h.fake.calls().is_empty(), "a refusal makes no request");
}

/// GDP-FR-QSHX, GPP-FR-DEZO: a read that finds the configuration not valid
/// records it and refuses with its error; a later load refuses with the same
/// error without a request.
#[test]
fn an_invalid_configuration_is_refused() {
    let h = Harness::github_plan();
    h.fake.lock().status_field = None;
    let refused = load(&h, true).unwrap_err();
    assert_eq!(refused.code, "status_field_missing");
    let configuration = crate::github_polling::delivery_access::configuration(&h.handle()).unwrap();
    assert_eq!(configuration.state, ConfigurationState::Invalid);
    let before = h.fake.calls().len();
    assert_eq!(load(&h, false).unwrap_err().code, "status_field_missing");
    assert_eq!(h.fake.calls().len(), before, "the second refusal makes no request");
}

/// GPP-FR-DEZO: a read that finds the configuration valid records it.
#[test]
fn a_valid_read_records_the_configuration() {
    let h = Harness::github_plan();
    load(&h, true).unwrap();
    let configuration = crate::github_polling::delivery_access::configuration(&h.handle()).unwrap();
    assert_eq!(configuration.state, ConfigurationState::Valid);
    assert_eq!(configuration.project_title.as_deref(), Some("Roadmap"));
}

/// GDP-FR-QSHX, GPP-FR-TYOV: a configuration that is `unchecked`, as after a
/// restart, does not refuse the load. The load that reads GitHub checks it,
/// records it as valid, and announces the change of the polling view.
#[test]
fn an_unchecked_configuration_is_checked_by_the_load() {
    let h = Harness::github_plan();
    let before = crate::github_polling::delivery_access::configuration(&h.handle()).unwrap();
    assert_eq!(before.state, ConfigurationState::Unchecked);
    let seen = listen(&h.handle(), crate::github_polling::GITHUB_POLLING_CHANGED);
    load(&h, true).expect("an unchecked configuration loads the plan");
    let after = crate::github_polling::delivery_access::configuration(&h.handle()).unwrap();
    assert_eq!(after.state, ConfigurationState::Valid);
    let payloads = seen.lock().unwrap().clone();
    assert!(!payloads.is_empty(), "the check announces the polling view");
    // GPP-FR-TYOV: a cause other than a poll reports no new issue.
    let payload: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
    assert_eq!(payload["newIssues"], json!([]), "{payload}");
}

/// GDP-FR-ZFQH, GDP-FR-QHQR: no open project is `no_project_open`, and the
/// refusal is logged.
#[test]
fn no_project_is_refused() {
    static LOG: LogBuffer = LogBuffer::new();
    let h = Harness::with_buffer(&LOG);
    h.close();
    assert_eq!(load(&h, false).unwrap_err().code, "no_project_open");
    let edit = edits::GithubDeliveryEdit::UnlinkDraft { item_id: "issue-1".into() };
    assert_eq!(edits::apply_edit_impl(&h.handle(), edit).unwrap_err().code, "no_project_open");
    let url = "https://github.com/acme/widgets/issues/1";
    assert_eq!(checked_tracker_url(&h.handle(), url).unwrap_err().code, "no_project_open");
    let text = records(&LOG);
    assert!(text.contains("github delivery plan load refused"));
    assert!(text.contains("github delivery edit refused"));
}

/// GDP-FR-YOSF, GDP-FR-URZY: a successful read stores what it read as the
/// read cache `delivery/github/<node>.json` in the repository store, and
/// nothing derived, and emits the change with the Project node ID.
#[test]
fn a_read_stores_the_read_cache_and_announces_it() {
    let h = Harness::github_plan();
    let events = listen(&h.handle(), GITHUB_DELIVERY_PLAN_CHANGED);
    load(&h, true).unwrap();
    let path = h.store.path().join("delivery/github").join(format!("{PROJECT}.json"));
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the cache file")).unwrap();
    assert_eq!(stored["project"]["title"], "Roadmap");
    assert_eq!(stored["project"]["items"].as_array().unwrap().len(), 3);
    assert!(stored.get("states").is_none() && stored.get("document").is_none());
    assert!(!h.dir.path().join(".synthesis/delivery").exists(), "nothing in the worktree");
    let seen = events.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&seen[0]).unwrap(),
        json!({ "projectNodeId": PROJECT })
    );

    // A second read replaces the cache.
    h.fake.lock().title = "Renamed".into();
    load(&h, true).unwrap();
    assert_eq!(h.cache().unwrap().project.title, "Renamed");
}

/// GDP-FR-LKXA, GDP-FR-WEFV: with `refresh` false a load answers from the
/// cache without a request, with the states of the current drafts; with no
/// cache it reads; with `refresh` true it reads. `readAt` is the read's.
#[test]
fn refresh_false_answers_from_the_cache() {
    let h = Harness::github_plan();
    let first = load(&h, false).unwrap();
    assert_eq!(reads(&h), 1, "no cache: a read");
    assert_eq!(first.read_at, h.cache().unwrap().read_at, "readAt is the read's");
    mark_cache(&h, SENTINEL);
    let cached = load(&h, false).unwrap();
    assert_eq!(reads(&h), 1, "a cache: no request");
    assert_eq!(cached.read_at, SENTINEL, "the answer comes from the cache");
    let fresh = load(&h, true).unwrap();
    assert_eq!(reads(&h), 2, "refresh: a read");
    assert_ne!(fresh.read_at, SENTINEL, "a refresh answers the new read");
}

/// GDP-FR-LKXA: an answer from the cache computes the states from the
/// drafts as they are now, so a change of a draft between two cached loads
/// changes the state, with no request.
#[test]
fn cached_loads_take_the_current_drafts() {
    let h = Harness::github_plan();
    load(&h, true).unwrap();
    let draft = crate::drafts::create_draft_impl(&h.root(), Some("Work"), None).unwrap();
    let id = draft.draft.id.clone();
    let link =
        edits::GithubDeliveryEdit::LinkDraft { item_id: "issue-3".into(), draft_id: id.clone() };
    edits::apply_edit_impl(&h.handle(), link).unwrap();
    let state = |p: &GithubDeliveryPlan, item: &str| {
        let s = p.states.iter().find(|s| s.item_id == item).unwrap();
        serde_json::to_value(s.state).unwrap()
    };
    let before = h.fake.calls().len();
    let drafting = load(&h, false).unwrap();
    assert_eq!(state(&drafting, "issue-3"), json!("drafting"));
    crate::drafts::set_draft_status_impl(&h.root(), &id, crate::drafts::DraftStatus::Archived)
        .unwrap();
    let dropped = load(&h, false).unwrap();
    assert_eq!(state(&dropped, "issue-3"), json!("dropped"));
    assert_eq!(state(&dropped, "issue-2"), json!("dropped"), "its container follows");
    assert_eq!(h.fake.calls().len(), before, "no request");
}

/// GDP-FR-GCJO: a failed read answers from the cache, marked stale, with the
/// typed error; with no cache it refuses with the error.
#[test]
fn a_failed_read_answers_stale_from_the_cache() {
    let h = Harness::github_plan();
    h.fake.lock().read_error = Some("github_unreachable".into());
    assert_eq!(load(&h, true).unwrap_err().code, "github_unreachable");
    h.fake.lock().read_error = None;
    let good = load(&h, true).unwrap();
    mark_cache(&h, SENTINEL);
    // A cut Project read is `github_request_failed` (GDP-FR-BGIT), and a
    // failed read of any code answers from the cache.
    for code in ["github_unreachable", "github_request_failed"] {
        h.fake.lock().read_error = Some(code.into());
        let stale = load(&h, true).unwrap();
        assert!(stale.stale);
        assert_eq!(stale.error.as_ref().map(|e| e.code.as_str()), Some(code));
        assert_eq!(stale.read_at, SENTINEL, "the tracker data is the cached read's");
        assert_eq!(stale.document, good.document);
        assert_eq!(h.cache().unwrap().read_at, SENTINEL, "a failed read stores nothing");
        // A request error is not a configuration finding: the recorded
        // configuration stays valid.
        let configuration =
            crate::github_polling::delivery_access::configuration(&h.handle()).unwrap();
        assert_eq!(configuration.state, ConfigurationState::Valid, "{code}");
    }
}

/// GDP-FR-QSHX, GDP-FR-GCJO: a configuration that is not valid refuses the
/// load also where a read cache exists; it gives no stale plan.
#[test]
fn an_invalid_configuration_refuses_also_with_a_cache() {
    let h = Harness::github_plan();
    load(&h, true).unwrap();
    assert!(h.cache().is_some());
    h.fake.lock().status_field = None;
    assert_eq!(load(&h, true).unwrap_err().code, "status_field_missing", "the read finds it");
    assert_eq!(load(&h, true).unwrap_err().code, "status_field_missing", "it is recorded");
    assert_eq!(load(&h, false).unwrap_err().code, "status_field_missing");
    let h2 = Harness::github_plan();
    load(&h2, true).unwrap();
    h2.fake.lock().read_error = Some("project_unavailable".into());
    assert_eq!(load(&h2, true).unwrap_err().code, "project_unavailable");
}

/// GDP-FR-LKXA, DMS-FR-ETLQ: a read cache that cannot be parsed reads as no
/// cache: the load reads GitHub, logs the record kind with no content, and
/// stores a new cache.
#[test]
fn a_corrupt_read_cache_reads_as_no_cache() {
    static LOG: LogBuffer = LogBuffer::new();
    let h = Harness::with_buffer(&LOG);
    h.select_project(Some(PROJECT));
    h.fake.board_issue(issue(1, Some("Task")), None);
    load(&h, true).unwrap();
    let path = h.store.path().join("delivery/github").join(format!("{PROJECT}.json"));
    std::fs::write(&path, "{ not json: Secret plan title").unwrap();
    let plan = load(&h, false).expect("a plan from a new read");
    assert_eq!(reads(&h), 2, "the corrupt cache is no cache");
    assert_eq!(plan.document.items.len(), 1);
    assert!(h.cache().is_some(), "the read stores a new cache");
    let text = records(&LOG);
    assert!(text.contains("delivery record is not readable and reads as empty"));
    assert!(text.contains("github_cache"));
    assert!(!text.contains("Secret plan title"));
}

/// GDP-FR-RJFS: a load that asks for a read while one runs waits for it and
/// returns its answer; one read reaches GitHub.
#[test]
fn a_waiting_load_takes_the_running_read() {
    let h = Harness::github_plan();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    h.fake.lock().pause = Some((entered_tx, release_rx));
    let first = {
        let app = h.handle();
        std::thread::spawn(move || load_plan_impl(&app, true))
    };
    entered_rx.recv().unwrap();
    let second = {
        let app = h.handle();
        std::thread::spawn(move || load_plan_impl(&app, true))
    };
    // Wait for the second load to reach the read lock: a state, not a time.
    let state = h.handle();
    let state = state.state::<GithubDeliveryState>();
    wait_until("the second load waits", || state.entered.load(Ordering::SeqCst) >= 2);
    release_tx.send(()).unwrap();
    let a = first.join().unwrap().unwrap();
    let b = second.join().unwrap().unwrap();
    assert_eq!(a.read_at, b.read_at);
    assert_eq!(reads(&h), 1, "one read reached GitHub");
}

/// GDP-FR-RJFS, GDP-FR-YOSF, GDP-FR-IRDD: an edit that finishes while a read
/// is paused keeps its cache write. The read holds the Project as it was
/// before the edit, so it does not replace the cache: it answers the newer
/// cache. A cached load and the next edit find the new item, and a read that
/// begins after the edit writes the cache again.
#[test]
fn an_edit_that_finishes_while_a_read_is_paused_keeps_its_cache_write() {
    let h = Harness::github_plan();
    load(&h, true).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    h.fake.lock().pause = Some((entered_tx, release_rx));
    let reading = {
        let app = h.handle();
        std::thread::spawn(move || load_plan_impl(&app, true))
    };
    entered_rx.recv().unwrap();
    let has = |p: &GithubDeliveryPlan, id: &str| p.document.items.iter().any(|i| i.id == id);
    let create = edits::GithubDeliveryEdit::CreateItem {
        item_kind: crate::flow_validation::model::Kind::Defect,
        name: "Crash".into(),
        container_id: Some("issue-2".into()),
        description: None,
        due: None,
        parent_id: None,
        milestone_id: None,
        labels: Vec::new(),
    };
    let created = edits::apply_edit_impl(&h.handle(), create).unwrap();
    assert!(has(&created, "issue-101"));
    release_tx.send(()).unwrap();
    let read = reading.join().unwrap().unwrap();
    assert!(has(&read, "issue-101"), "the older read answers the newer cache");
    assert!(h.cache().unwrap().issue(101).is_some(), "the edit's cache write stays");
    let cached = load(&h, false).unwrap();
    assert!(has(&cached, "issue-101"));
    let updated_at =
        cached.tracker.iter().find(|t| t.item_id == "issue-101").unwrap().updated_at.clone();
    let rename = edits::GithubDeliveryEdit::Rename {
        item_id: "issue-101".into(),
        name: "Renamed".into(),
        updated_at,
    };
    edits::apply_edit_impl(&h.handle(), rename).expect("the next edit finds the new item");
    mark_cache(&h, SENTINEL);
    load(&h, true).unwrap();
    assert_ne!(h.cache().unwrap().read_at, SENTINEL, "a later read writes the cache");
}

const OTHER: &str = "PVT_delivery2";

/// GPP-FR-OVOS, GDP-FR-LKXA: after the selected Project changes from P1 to
/// P2, a load with `refresh` false does not answer P1's cache: it reads P2
/// and stores P2's own cache. P1's cache stays as it was.
#[test]
fn a_changed_project_does_not_answer_the_cache_of_the_earlier_one() {
    let h = Harness::github_plan();
    h.fake.lock().title = "One".into();
    load(&h, true).unwrap();
    h.fake.lock().title = "Two".into();
    h.select_project(Some(OTHER));
    let plan = load(&h, false).unwrap();
    assert_eq!(reads(&h), 2, "P1's cache is not P2's");
    assert_eq!(plan.document.name.as_deref(), Some("Two"));
    assert!(h.fake.calls().contains(&format!("read_project {OTHER}")));
    let store = h.machine_store();
    let other = cache::read_cache(&h.handle(), &crate::logging::BUFFER, &store, OTHER).unwrap();
    assert_eq!(other.unwrap().project.title, "Two");
    assert_eq!(h.cache().unwrap().project.title, "One", "P1's cache is unchanged");
}

/// GDP-FR-RJFS, GPP-FR-OVOS: a load that waits for a running read takes its
/// answer only for the same Project. A load of P2 that waits for a read of P1
/// does not take P1's answer: it reads P2 itself.
#[test]
fn a_waiting_load_of_another_project_reads_its_own() {
    let h = Harness::github_plan();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    h.fake.lock().pause = Some((entered_tx, release_rx));
    let first = {
        let app = h.handle();
        std::thread::spawn(move || load_plan_impl(&app, true))
    };
    entered_rx.recv().unwrap();
    h.select_project(Some(OTHER));
    h.fake.lock().title = "Two".into();
    let second = {
        let app = h.handle();
        std::thread::spawn(move || load_plan_impl(&app, true))
    };
    let state = h.handle();
    let state = state.state::<GithubDeliveryState>();
    wait_until("the second load waits", || state.entered.load(Ordering::SeqCst) >= 2);
    release_tx.send(()).unwrap();
    let a = first.join().unwrap().unwrap();
    let b = second.join().unwrap().unwrap();
    assert_eq!(a.document.name.as_deref(), Some("Roadmap"), "the first read is P1's");
    assert_eq!(b.document.name.as_deref(), Some("Two"), "the second load reads P2");
    assert_eq!(reads(&h), 2);
    assert!(h.fake.calls().contains(&format!("read_project {OTHER}")));
}

/// GDP-FR-PIAQ: an HTTPS issue or milestone address of the polling
/// repository is opened; every other address is `tracker_url_refused`.
#[test]
fn only_tracker_addresses_of_the_repository_open() {
    let ok = |u: &str| is_tracker_url(u, "github.com", "acme", "widgets");
    assert!(ok("https://github.com/acme/widgets/issues/12"));
    assert!(ok("https://GitHub.com/Acme/Widgets/milestone/3"));
    for bad in [
        "http://github.com/acme/widgets/issues/12",
        "https://github.com/acme/other/issues/12",
        "https://evil.com/acme/widgets/issues/12",
        "https://github.com/acme/widgets/pull/12",
        "https://github.com/acme/widgets/issues/12?x=1",
        "https://github.com/acme/widgets/issues/12#c",
        "https://user@github.com/acme/widgets/issues/12",
        "https://github.com/acme/widgets/issues/012",
        "https://github.com/acme/widgets/issues/",
        "https://github.com/acme/widgets/issues/12/extra",
        "file:///etc/passwd",
    ] {
        assert!(!ok(bad), "{bad}");
    }
    let h = Harness::github_plan();
    let app = h.handle();
    assert!(checked_tracker_url(&app, "https://github.com/acme/widgets/issues/3").is_ok());
    let refused =
        checked_tracker_url(&app, "https://example.com/acme/widgets/issues/3").unwrap_err();
    assert_eq!(refused.code, ERR_TRACKER_URL_REFUSED);
}

/// GDP-FR-PIAQ: the address of the selected Project opens when it is the
/// same text as the stored Project URL, on the host of the polling
/// repository. Before a read is cached there is no stored URL, and the
/// address is refused. Every other address is still `tracker_url_refused`.
#[test]
fn the_address_of_the_selected_project_opens() {
    let ok = |u: &str| is_project_url(u, "github.com", PROJECT_URL);
    assert!(ok(PROJECT_URL));
    for bad in [
        "https://github.com/orgs/acme/projects/7/",
        "https://github.com/orgs/acme/projects/8",
        "https://github.com/orgs/acme/projects/7?x=1",
        "https://github.com/orgs/acme/projects/7#a",
        "http://github.com/orgs/acme/projects/7",
        "",
    ] {
        assert!(!ok(bad), "{bad}");
    }
    assert!(!is_project_url(PROJECT_URL, "github.com", ""), "no stored URL matches nothing");
    let other = "https://evil.com/orgs/acme/projects/7";
    assert!(!is_project_url(other, "github.com", other), "another host is refused");
    let spaced = "https://github.com/orgs/acme/projects/7 x";
    assert!(!is_project_url(spaced, "github.com", spaced));

    let h = Harness::github_plan();
    let app = h.handle();
    let refused = checked_tracker_url(&app, PROJECT_URL).unwrap_err();
    assert_eq!(refused.code, ERR_TRACKER_URL_REFUSED, "no read is cached yet");
    load(&h, true).unwrap();
    assert_eq!(checked_tracker_url(&app, PROJECT_URL).unwrap(), PROJECT_URL);
    let near = "https://github.com/orgs/acme/projects/77";
    assert_eq!(checked_tracker_url(&app, near).unwrap_err().code, ERR_TRACKER_URL_REFUSED);
    // Another selected Project has no cached URL: the earlier one's is refused.
    h.select_project(Some("PVT_delivery2"));
    let refused = checked_tracker_url(&app, PROJECT_URL).unwrap_err();
    assert_eq!(refused.code, ERR_TRACKER_URL_REFUSED);
}

fn records(buffer: &LogBuffer) -> String {
    let page = buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap();
    serde_json::to_string(&page.records).unwrap()
}

/// The records of `buffer`, oldest first.
fn record_list(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap()
        .records
}

/// The one record with `message`, or a failure that names it.
fn record<'a>(
    list: &'a [crate::logging::LogRecord],
    message: &str,
) -> &'a crate::logging::LogRecord {
    list.iter().find(|r| r.message == message).unwrap_or_else(|| panic!("no record {message:?}"))
}

/// GDP-FR-QHQR: each read and each edit logs its operation, outcome, counts,
/// and duration, and every error; no record carries the token, a title, a
/// body, a Type name, or a URL query.
#[test]
fn reads_and_edits_log_without_content() {
    static LOG: LogBuffer = LogBuffer::new();
    let h = Harness::with_buffer(&LOG);
    h.select_project(Some(PROJECT));
    let mut special = issue(1, Some("Spaceship"));
    special.title = "Secret plan title".into();
    special.body = "Private body text".into();
    special.url = "https://github.com/acme/widgets/issues/1?token=leak".into();
    h.fake.board_issue(special, None);
    h.fake.board_issue(issue(2, Some("Task")), None);
    load(&h, true).unwrap();
    let plan = load(&h, false).unwrap();
    let updated_at =
        plan.tracker.iter().find(|t| t.item_id == "issue-2").unwrap().updated_at.clone();
    let edit = edits::GithubDeliveryEdit::Rename {
        item_id: "issue-2".into(),
        name: "Renamed title".into(),
        updated_at,
    };
    edits::apply_edit_impl(&h.handle(), edit).unwrap();
    let stale_edit = edits::GithubDeliveryEdit::Rename {
        item_id: "issue-2".into(),
        name: "Second title".into(),
        updated_at: "old".into(),
    };
    edits::apply_edit_impl(&h.handle(), stale_edit).unwrap_err();
    h.fake.lock().read_error = Some("github_unreachable".into());
    load(&h, true).unwrap();
    h.close();
    load(&h, true).unwrap_err();

    let list = record_list(&LOG);
    let field = |r: &crate::logging::LogRecord, key: &str| r.fields.get(key).cloned();
    let started = record(&list, "github delivery plan load started");
    assert_eq!(started.level, LogLevel::Debug);
    assert_eq!(field(started, "refresh"), Some(json!(true)));
    let read = record(&list, "github delivery plan read");
    assert_eq!((read.level, field(read, "project_items")), (LogLevel::Info, Some(json!(2))));
    assert_eq!(field(read, "milestones"), Some(json!(0)));
    assert!(field(read, "ms").is_some(), "a duration is logged");
    let loaded = record(&list, "github delivery plan loaded");
    assert_eq!(field(loaded, "items"), Some(json!(1)), "the Spaceship Type maps to no kind");
    assert_eq!(field(loaded, "stale"), Some(json!(false)));
    assert!(field(loaded, "ms").is_some());
    let edit_started = record(&list, "github delivery edit started");
    assert_eq!(field(edit_started, "edit"), Some(json!("rename")));
    let applied = record(&list, "github delivery edit applied");
    assert_eq!((applied.level, field(applied, "edit")), (LogLevel::Info, Some(json!("rename"))));
    assert!(field(applied, "ms").is_some());
    let refused = record(&list, "github delivery edit refused");
    assert_eq!(
        (refused.level, field(refused, "code")),
        (LogLevel::Warn, Some(json!(ERR_CHANGED_ON_TRACKER)))
    );
    let failed = record(&list, "github delivery plan read failed");
    assert_eq!(
        (failed.level, field(failed, "code")),
        (LogLevel::Warn, Some(json!("github_unreachable")))
    );
    let stale = list
        .iter()
        .filter(|r| r.message == "github delivery plan loaded")
        .find(|r| field(r, "stale") == Some(json!(true)));
    assert!(stale.is_some(), "a stale answer is logged as stale");
    let no_project = record(&list, "github delivery plan load refused");
    assert_eq!(field(no_project, "code"), Some(json!("no_project_open")));

    let text = serde_json::to_string(&list).unwrap();
    for (index, marker) in [
        SECRET,
        "Secret plan title",
        "Private body text",
        "Spaceship",
        "Renamed title",
        "Second title",
        "token=leak",
    ]
    .iter()
    .enumerate()
    {
        assert!(!text.contains(marker), "a record holds sentinel {index}");
    }
}
