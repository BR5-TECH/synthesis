//! GDP-FR-PBSD, PPT-FR-UZEK, PPT-FR-WBYJ: the read cache after the reads of a
//! proposal. A read that finds what the cache holds changes nothing, and a
//! read or a write of the cache that fails never stops the proposal.

use std::sync::{Arc, Mutex};

use super::*;
use crate::delivery::github::tests::fake::{listen, PROJECT};
use crate::delivery::github::tests::wait_until;
use crate::delivery::github::GITHUB_DELIVERY_PLAN_CHANGED;

/// A field that no write of the cache keeps, because the record has no such
/// field. While it is in the file, no write replaced the file.
const UNTOUCHED: &str = "zzNoWriteSinceTheTest";

/// The event that ends a wait for the plan events. Tauri can queue an emit
/// that finds its listener table busy, and it delivers the queue in order on
/// the next emit. So when this event is seen, each earlier plan event is seen
/// too.
const FENCE: &str = "cache-reads-test-fence";

fn cache_file(f: &Fixture) -> std::path::PathBuf {
    f.plan.h.machine_store().path().join("delivery/github").join(format!("{PROJECT}.json"))
}

/// Put the marker field into the stored read cache.
fn mark_cache(f: &Fixture) {
    let path = cache_file(f);
    let mut stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    stored[UNTOUCHED] = json!(true);
    std::fs::write(&path, stored.to_string()).unwrap();
}

fn cache_marked(f: &Fixture) -> bool {
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(cache_file(f)).unwrap()).unwrap();
    stored.get(UNTOUCHED) == Some(&json!(true))
}

/// Wait until each plan event that was emitted before this call is seen. A
/// test calls it before it listens too, so an event of the fixture cannot reach
/// the new listener.
pub(super) fn fence(f: &Fixture) {
    use tauri::{Emitter, Listener};
    let seen = Arc::new(Mutex::new(false));
    let sink = Arc::clone(&seen);
    f.plan.app().listen(FENCE, move |_| *sink.lock().unwrap() = true);
    f.plan.app().emit(FENCE, ()).unwrap();
    wait_until("the fence event", || *seen.lock().unwrap());
}

/// Change issue 3 on GitHub, so a read of it differs from the read cache.
fn change_issue_on_github(f: &Fixture) {
    let mut fake = f.plan.h.fake.lock();
    let issue = fake.issues.iter_mut().find(|i| i.number == 3).unwrap();
    issue.body = "Body on GitHub".into();
    issue.updated_at = "2026-02-02T00:00:00Z".into();
}

/// The records of this process at `level` with `message`.
fn records(level: LogLevel, message: &str) -> Vec<crate::logging::LogRecord> {
    crate::logging::BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap()
        .records
        .into_iter()
        .filter(|r| r.level == level && r.message == message)
        .collect()
}

// GDP-FR-PBSD, PPT-FR-WBYJ: where the fresh read of each changed issue equals
// what the read cache holds, the cache file is not written again, and no
// "github delivery plan changed" event is emitted. The proposal is recorded.
#[test]
fn a_read_that_matches_the_cache_neither_writes_it_nor_announces_it() {
    let f = Fixture::new();
    mark_cache(&f);
    let cache = f.plan.h.cache();
    fence(&f);
    let seen = listen(&f.plan.app(), GITHUB_DELIVERY_PLAN_CHANGED);
    f.plan.h.fake.lock().calls.clear();
    f.rows(json!([modify("a", 3, json!({ "body": "A new body" }))])).unwrap();
    assert_eq!(f.proposals().len(), 1);
    assert!(f.plan.h.fake.calls().iter().any(|c| c == "read_issue 3"), "the cache read was made");
    fence(&f);
    assert!(seen.lock().unwrap().is_empty(), "{:?}", seen.lock().unwrap());
    assert!(cache_marked(&f), "the cache file was written again");
    assert_eq!(f.plan.h.cache(), cache);
    let cached = records(LogLevel::Debug, "plan issues cached after a proposal read");
    assert_eq!(cached.last().unwrap().fields["changed"], json!(false));
}

// GDP-FR-PBSD, PPT-FR-UZEK: where the read of one issue as the cache holds it
// fails, the proposal is still recorded with the values of the detail read,
// one WARN record names the code, and the other changed issue still goes into
// the cache, with one event.
#[test]
fn a_failed_cache_read_still_records_the_proposal_and_warns() {
    let f = Fixture::new();
    change_issue_on_github(&f);
    {
        let mut fake = f.plan.h.fake.lock();
        let issue = fake.issues.iter_mut().find(|i| i.number == 2).unwrap();
        issue.updated_at = "2026-02-03T00:00:00Z".into();
        fake.read_issue_error = Some((2, "github_unreachable".into()));
    }
    let before = f.plan.h.cache().unwrap().issue(2).unwrap().clone();
    fence(&f);
    let seen = listen(&f.plan.app(), GITHUB_DELIVERY_PLAN_CHANGED);
    let comments = f.plan.comments();
    let out = f
        .rows(json!([
            modify("a", 2, json!({ "title": "Renamed" })),
            modify("b", 3, json!({ "body": "A new body" })),
        ]))
        .unwrap();
    assert_eq!(out, json!({ "proposed": true, "rows": 2 }));
    assert!(f.proposed.load(Ordering::SeqCst));
    assert_eq!(f.plan.comments(), comments + 1);
    let rows = &f.proposals()[0].rows;
    assert_eq!(modify_fields(&rows[0]).base_updated_at, "2026-02-03T00:00:00Z");
    let three = modify_fields(&rows[1]);
    assert_eq!(three.base_updated_at, "2026-02-02T00:00:00Z");
    assert_eq!(three.changes[0].before, json!("Body on GitHub"));
    let cache = f.plan.h.cache().unwrap();
    assert_eq!(cache.issue(2).unwrap(), &before, "the issue that failed is as it was");
    assert_eq!(cache.issue(3).unwrap().body, "Body on GitHub");
    assert_eq!(cache.issue(3).unwrap().updated_at, "2026-02-02T00:00:00Z");
    fence(&f);
    assert_eq!(seen.lock().unwrap().len(), 1);
    let warned = records(LogLevel::Warn, "plan issue cache update skipped");
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].fields["code"], json!("github_unreachable"));
}

// GDP-FR-PBSD: where the write of the read cache fails, the proposal is still
// recorded with the fresh values, the cache stays as it was, no event is
// emitted, and one WARN record names the code.
#[cfg(unix)]
#[test]
fn a_failed_cache_write_still_records_the_proposal_and_warns() {
    use std::os::unix::fs::PermissionsExt;
    // Mode bits do not stop root or some container mounts, and then the write
    // cannot fail this way.
    if !crate::fs::permission_probe::write_bits_bind() {
        return;
    }
    let f = Fixture::new();
    change_issue_on_github(&f);
    let cache = f.plan.h.cache();
    fence(&f);
    let seen = listen(&f.plan.app(), GITHUB_DELIVERY_PLAN_CHANGED);
    let dir = cache_file(&f).parent().unwrap().to_path_buf();
    let restore = std::fs::metadata(&dir).unwrap().permissions();
    // Puts the mode back also when the call panics, so the temporary folder stays removable.
    struct Restore(std::path::PathBuf, std::fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, self.1.clone());
        }
    }
    let guard = Restore(dir.clone(), restore);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = f.rows(json!([modify("a", 3, json!({ "body": "A new body" }))]));
    drop(guard);
    assert_eq!(result.unwrap(), json!({ "proposed": true, "rows": 1 }));
    let row = f.only_row();
    assert_eq!(modify_fields(&row).base_updated_at, "2026-02-02T00:00:00Z");
    assert_eq!(f.plan.h.cache(), cache, "the cache is unchanged");
    fence(&f);
    assert!(seen.lock().unwrap().is_empty());
    let warned = records(LogLevel::Warn, "plan issue cache update failed");
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].fields["code"], json!("store_unavailable"));
}
