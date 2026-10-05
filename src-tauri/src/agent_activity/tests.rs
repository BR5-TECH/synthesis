//! `AGV-agent-activity.md` — the store's own behaviour, with no Tauri handle
//! and no filesystem: what is asserted here is sequencing, bounding, and paging,
//! which is the whole of what a consumer depends on.

use super::*;

fn event(kind: &str, summary: &str) -> AgentActivityEvent {
    AgentActivityEvent {
        at: "2026-08-16T10:00:00Z".to_string(),
        channel: "stdout",
        kind: match kind {
            "message" => "message",
            "tool_call" => "tool_call",
            other => Box::leak(other.to_string().into_boxed_str()),
        },
        summary: summary.to_string(),
        payload: format!("{{\"summary\":\"{summary}\"}}"),
        payload_truncated: false,
    }
}

/// AGV-FR-09: sequences are the store's, ascend within a run, and are
/// independent between runs.
#[test]
fn sequences_ascend_within_a_run_and_start_again_for_another() {
    let store = ActivityStore::new();

    let (first, state) = store.append("run-a", event("message", "one")).expect("recorded");
    let (second, _) = store.append("run-a", event("message", "two")).expect("recorded");
    let (other, other_state) = store.append("run-b", event("message", "one")).expect("recorded");

    assert_eq!(first.seq, 1);
    assert_eq!(second.seq, 2);
    assert_eq!(state.total, 1);
    // A second run numbers from its own beginning: the cursor a consumer holds
    // is meaningless outside the run it came from, which is why every read names
    // one.
    assert_eq!(other.seq, 1);
    assert_eq!(other_state.run_id, "run-b");
    assert_eq!(store.read("run-a", None, None).latest_seq, 2);
    assert_eq!(store.read("run-b", None, None).latest_seq, 1);
}

/// AGV-FR-10: with no cursor a reader gets the newest page, so a panel opened on
/// a run that has been going for an hour starts at the end.
#[test]
fn a_read_with_no_cursor_takes_the_newest_records() {
    let store = ActivityStore::new();
    for i in 1..=10 {
        store.append("run", event("message", &format!("line {i}")));
    }

    let page = store.read("run", None, Some(3));

    assert_eq!(page.records.len(), 3);
    assert_eq!(page.records[0].seq, 8);
    assert_eq!(page.records[2].seq, 10);
    // Ascending whichever cursor produced them.
    assert!(page.records.windows(2).all(|w| w[0].seq < w[1].seq));
    assert_eq!(page.total, 10);
    assert_eq!(page.latest_seq, 10);
}

/// AGV-FR-10: with a cursor a reader gets the delta, oldest first — which is
/// what the appended event tells it to ask for.
#[test]
fn a_read_after_a_cursor_takes_what_followed_it() {
    let store = ActivityStore::new();
    for i in 1..=10 {
        store.append("run", event("message", &format!("line {i}")));
    }

    let page = store.read("run", Some(7), None);

    assert_eq!(
        page.records.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![8, 9, 10]
    );
    // A cursor at the newest record returns nothing rather than repeating it.
    assert!(store.read("run", Some(10), None).records.is_empty());
    // A cursor past the newest is positional rather than an error.
    assert!(store.read("run", Some(99), None).records.is_empty());
}

/// AGV-FR-13: a run nobody has recorded anything for reads as empty rather than
/// failing, because a panel opens on a run before its first line arrives.
#[test]
fn an_unknown_run_reads_as_empty() {
    let store = ActivityStore::new();

    let page = store.read("never-seen", None, None);

    assert!(page.records.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.dropped, 0);
    assert_eq!(page.latest_seq, 0);
}

/// AGV-FR-05: the memory bound drops the oldest records and counts them, and
/// `total` keeps counting what was appended.
#[test]
fn the_memory_bound_drops_the_oldest_and_says_how_many() {
    let store = ActivityStore::new();
    let appended = MEMORY_EVENTS_PER_RUN + 50;
    for i in 1..=appended {
        store.append("run", event("message", &format!("line {i}")));
    }

    let page = store.read("run", None, Some(MAX_PAGE));

    assert_eq!(page.total, appended as u64);
    assert_eq!(page.dropped, 50);
    assert_eq!(page.latest_seq, appended as u64);
    // The newest survive; the oldest are the ones that went.
    assert_eq!(page.records.last().unwrap().seq, appended as u64);
    assert!(page.records.iter().all(|r| r.seq > 50));
}

/// AGV-FR-05: the byte bound holds as well as the count bound, so a few very
/// large records cost memory rather than a few thousand small ones.
#[test]
fn the_byte_bound_holds_before_the_count_bound_is_reached() {
    let store = ActivityStore::new();
    let big = "x".repeat(1024 * 1024);
    for i in 1..=16 {
        let mut e = event("message", &format!("line {i}"));
        e.payload = big.clone();
        store.append("run", e);
    }

    let page = store.read("run", None, Some(MAX_PAGE));

    // Sixteen megabytes were appended into an eight megabyte bound, and the
    // count bound was nowhere near reached.
    assert!(page.records.len() < 16, "records = {}", page.records.len());
    assert!(page.dropped > 0);
    assert_eq!(page.total, 16);
    // Never emptied: the newest record is kept whatever it weighs, because a
    // bound that can discard the line a reader is waiting for is worse than one
    // that is briefly exceeded.
    assert!(!page.records.is_empty());
}

/// AGV-FR-05 — one record larger than the whole byte bound is still kept.
///
/// The branch the bound's own guard exists for, and the only one that decides
/// what happens when the newest record is itself the thing that will not fit. A
/// bound that can discard the line a reader is waiting for is worse than one
/// briefly exceeded — so the record stays and everything else goes.
#[test]
fn the_newest_record_is_kept_even_where_it_alone_outweighs_the_bound() {
    let store = ActivityStore::new();
    store.append("run", event("message", "a small one"));
    let mut huge = event("message", "the big one");
    huge.payload = "x".repeat(MEMORY_BYTES_PER_RUN + 1024 * 1024);

    store.append("run", huge);

    let page = store.read("run", None, Some(MAX_PAGE));
    assert_eq!(page.records.len(), 1, "the store emptied itself");
    assert_eq!(page.records[0].summary, "the big one");
    assert_eq!(page.dropped, 1);
    assert_eq!(page.total, 2);
}

/// AGV-FR-10: a page is bounded whatever was asked for.
#[test]
fn a_page_is_bounded_however_large_a_limit_is_asked_for() {
    let store = ActivityStore::new();
    for i in 1..=(MAX_PAGE + 100) {
        store.append("run", event("message", &format!("line {i}")));
    }

    assert_eq!(store.read("run", None, Some(usize::MAX)).records.len(), MAX_PAGE);
    // A limit of zero is a reader asking for nothing useful; one record is the
    // smallest answer that still moves a cursor.
    assert_eq!(store.read("run", None, Some(0)).records.len(), 1);
}

/// AGV-FR-06, AGV-FR-09: the run bound reclaims the records of the least recently
/// appended-to run, and never those of the run still being appended to.
#[test]
fn the_run_bound_reclaims_the_least_recently_appended_run() {
    let store = ActivityStore::new();
    for i in 0..RUNS_IN_MEMORY {
        store.append(&format!("run-{i}"), event("message", "hello"));
    }
    // Touch the oldest so it is no longer the oldest.
    store.append("run-0", event("message", "again"));

    store.append("run-new", event("message", "hello"));

    // `run-1` was the least recently appended to and is the one whose records
    // went.
    assert!(store.read("run-1", None, None).records.is_empty());
    assert_eq!(store.read("run-0", None, None).records.len(), 2);
    assert_eq!(store.read("run-new", None, None).records.len(), 1);
}

/// AGV-FR-09 — a run whose records were reclaimed keeps its sequence.
///
/// The failure this prevents is silent and permanent. A consumer holds a cursor;
/// a reclaimed run that numbered from one again would report a newest sequence
/// *below* that cursor, and every consumer that asks "is there anything after
/// what I hold" would be told no — for the rest of the run. The panel simply
/// stops updating, with nothing anywhere saying why.
#[test]
fn a_reclaimed_run_continues_its_numbering_rather_than_starting_again() {
    let store = ActivityStore::new();
    for i in 1..=5 {
        store.append("run", event("message", &format!("line {i}")));
    }
    // Push it past the records bound with other runs' traffic.
    for i in 0..RUNS_IN_MEMORY {
        store.append(&format!("other-{i}"), event("message", "hello"));
    }
    let reclaimed = store.read("run", None, None);
    assert!(reclaimed.records.is_empty(), "the run was not reclaimed");
    // What it said is still counted, even where it is no longer held.
    assert_eq!(reclaimed.total, 5);
    assert_eq!(reclaimed.latest_seq, 5);

    let (record, state) = store.append("run", event("message", "later")).expect("recorded");

    assert_eq!(record.seq, 6, "the run started numbering again");
    assert_eq!(state.total, 6);
    assert!(state.latest_seq > reclaimed.latest_seq);
}

/// AGV-FR-12: forgetting a run leaves nothing of it, and a line still in flight
/// from its own container cannot bring it back.
///
/// The window is real rather than theoretical: a discard is permitted while a
/// run is still working, cancellation only sets a flag the loop notices on its
/// next poll, and the container's pipes go on being drained until the child
/// actually dies. Every line written in that window arrives *after* the author
/// threw the run away.
#[test]
fn forgetting_a_run_leaves_nothing_of_it_and_nothing_brings_it_back() {
    let store = ActivityStore::new();
    store.append("run", event("message", "one"));
    store.append("run", event("message", "two"));

    store.forget("run");

    let page = store.read("run", None, None);
    assert!(page.records.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.latest_seq, 0);

    // The late line: refused rather than recorded, and refused in a way the
    // caller can see, so nothing downstream announces a run that is gone.
    assert!(store.append("run", event("message", "three")).is_none());
    assert_eq!(store.read("run", None, None).total, 0);
    // And it is still forgotten however many arrive.
    for _ in 0..5 {
        assert!(store.append("run", event("message", "late")).is_none());
    }
    assert!(store.read("run", None, None).records.is_empty());
}

/// AGV-FR-12 — a forgotten run does not stop other runs being recorded, and
/// forgetting one leaves the others exactly as they were.
#[test]
fn forgetting_one_run_touches_no_other() {
    let store = ActivityStore::new();
    store.append("kept", event("message", "one"));
    store.append("thrown", event("message", "one"));

    store.forget("thrown");

    let (record, state) = store.append("kept", event("message", "two")).expect("recorded");
    assert_eq!(record.seq, 2);
    assert_eq!(state.total, 2);
    assert_eq!(store.read("kept", None, None).records.len(), 2);
}

/// AGV-FR-02, AGV-FR-04: every field of an event survives the store unaltered. The store
/// masks nothing and rewrites nothing — the executor did that already, and a
/// second opinion here would be a second place for the rule to be wrong.
#[test]
fn a_record_carries_exactly_what_the_event_carried() {
    let store = ActivityStore::new();
    let mut e = event("tool_call", "Read(/workspace/spec.md)");
    e.channel = "stderr";
    e.payload = "verbatim · sk-a…9f2x".to_string();
    e.payload_truncated = true;

    let (record, _) = store.append("run", e).expect("recorded");

    assert_eq!(record.channel, "stderr");
    assert_eq!(record.kind, "tool_call");
    assert_eq!(record.summary, "Read(/workspace/spec.md)");
    assert_eq!(record.payload, "verbatim · sk-a…9f2x");
    assert!(record.payload_truncated);
    assert_eq!(record.at, "2026-08-16T10:00:00Z");
    assert_eq!(store.read("run", None, None).records[0], record);
}

/// AGV-FR-01 — the command exists, is registered, and reads through the same
/// store the sink writes into.
///
/// Registration is asserted against `lib.rs` itself: an unregistered command
/// fails at `invoke` time rather than at compile time, so a reference to the
/// function alone would hold for a command no frontend could ever call.
#[test]
fn the_command_is_registered_and_reads_what_a_sink_wrote() {
    let _ = read_agent_activity;
    let lib = include_str!("../lib.rs");
    assert!(
        lib.contains("agent_activity::read_agent_activity"),
        "the command is not in `generate_handler!`"
    );
    // The canonical name list lives beside `lib.rs` rather than in it.
    let names = include_str!("../command_names.rs");
    assert!(
        names.contains("\"read_agent_activity\""),
        "the command is not in the registered-name list"
    );
    // And the store the command reads is the one the application manages.
    assert!(
        lib.contains("agent_activity::ActivityStore::new()"),
        "no store is managed, so every read would answer as an unknown run"
    );
}

/// AGV-FR-06 / AGV-FR-12 — discarding runs costs a bounded amount of memory
/// rather than a growing one.
///
/// Including a run discarded before it recorded anything, which was never in the
/// eviction order to begin with and would otherwise stay for ever.
#[test]
fn forgotten_runs_stay_inside_the_tracking_bound() {
    let store = ActivityStore::new();

    for i in 0..(RUNS_TRACKED * 2) {
        store.forget(&format!("never-ran-{i}"));
    }

    assert!(
        store.tracked_runs() <= RUNS_TRACKED,
        "tombstones accumulated: {} entries",
        store.tracked_runs()
    );

    // And a tombstone is the first thing reclaimed rather than the last, because
    // it is what nobody can be reading.
    let store = ActivityStore::new();
    store.append("live", event("message", "one"));
    store.forget("thrown");
    for i in 0..(RUNS_IN_MEMORY - 2) {
        store.append(&format!("other-{i}"), event("message", "hello"));
    }
    assert_eq!(store.read("live", None, None).records.len(), 1);
}

/// AGV-FR-01 — two runs recorded at once stay separate under concurrency.
///
/// The store is reached from one thread per running container, and the two
/// mutexes it holds are taken separately; what must never happen is one run's
/// record landing under another's identity, or a sequence being handed out twice.
#[test]
fn concurrent_runs_never_mix_or_reuse_a_sequence() {
    use std::sync::Arc;

    let store = Arc::new(ActivityStore::new());
    let mut threads = Vec::new();
    for run in 0..4 {
        let store = Arc::clone(&store);
        threads.push(std::thread::spawn(move || {
            for i in 0..200 {
                store.append(&format!("run-{run}"), event("message", &format!("r{run} line {i}")));
            }
        }));
    }
    for thread in threads {
        thread.join().expect("a thread");
    }

    for run in 0..4 {
        let page = store.read(&format!("run-{run}"), None, Some(MAX_PAGE));
        assert_eq!(page.total, 200, "run-{run} lost or gained records");
        assert_eq!(page.latest_seq, 200);
        let seqs: Vec<u64> = page.records.iter().map(|r| r.seq).collect();
        let mut sorted = seqs.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(seqs.len(), sorted.len(), "a sequence was handed out twice");
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "records are out of order");
        // Every record belongs to the run it was read from.
        assert!(page
            .records
            .iter()
            .all(|record| record.summary.starts_with(&format!("r{run} "))));
    }
}

// ---------------------------------------------------------------------------
// AGV-FR-07 … AGV-FR-03 — the sink: the file behind a run, and its attribution
// ---------------------------------------------------------------------------

use std::sync::Arc;
use tempfile::TempDir;

/// A sink over a temporary directory, with the store it writes into.
///
/// Built against a real `FsAccess` rooted in that directory, because the file is
/// written through `FSA-filesystem-access.md` like every other backend write and
/// a double would be asserting something else.
struct SinkFixture {
    _dir: TempDir,
    /// Held for the life of the fixture: the sink emits through this app's
    /// handle, and a handle whose app has dropped is a handle to nothing.
    _app: tauri::App<tauri::test::MockRuntime>,
    root: PathBuf,
    store: Arc<ActivityStore>,
    sink: RunActivitySink<tauri::test::MockRuntime>,
}

impl SinkFixture {
    fn new(run_id: &str, store: ActivityStore) -> SinkFixture {
        let dir = TempDir::new().expect("a temp directory");
        let root = dir.path().canonicalize().expect("a real path");
        let app = tauri::test::mock_app();
        let fs = Arc::new(
            crate::fs::FsAccess::builder()
                .allow_root(&root)
                .build()
                .expect("an instance"),
        );
        let store = Arc::new(store);
        let sink = RunActivitySink::rooted_at(
            &app.handle().clone(),
            run_id,
            Arc::clone(&store),
            Some(fs),
            Some(root.join("agent-activity")),
        );
        SinkFixture {
            _dir: dir,
            _app: app,
            root,
            store,
            sink,
        }
    }

    fn file(&self, run_id: &str) -> PathBuf {
        self.root.join("agent-activity").join(format!("{run_id}.jsonl"))
    }

    /// Every record the file holds, in order.
    fn written(&self, run_id: &str) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(self.file(run_id)).unwrap_or_default();
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("one JSON document per line"))
            .collect()
    }
}

/// AGV-FR-07 — the file holds every record, including the ones memory dropped.
///
/// This is what makes the memory bound affordable: a reader watching live sees
/// the newest, and the account of the whole run survives the bound, the session,
/// and the relaunch.
#[test]
fn the_file_holds_every_record_including_those_memory_dropped() {
    let f = SinkFixture::new("run-file", ActivityStore::new());

    // Large enough that the byte bound reclaims some, without writing thousands
    // of records to reach the count bound.
    let mut appended = Vec::new();
    for i in 1..=12 {
        let mut e = event("message", &format!("line {i}"));
        e.payload = format!("{i}:{}", "p".repeat(1024 * 1024));
        appended.push(e.summary.clone());
        f.sink.activity(e);
    }

    let page = f.store.read("run-file", None, Some(MAX_PAGE));
    assert!(page.dropped > 0, "nothing was dropped, so nothing is proven");
    assert!(page.records.len() < 12);

    let written = f.written("run-file");
    assert_eq!(written.len(), 12, "the file lost what memory dropped");
    for (i, record) in written.iter().enumerate() {
        assert_eq!(record["seq"], (i + 1) as u64);
        assert_eq!(record["summary"], appended[i]);
        assert_eq!(record["channel"], "stdout");
    }
    // And it parses back into the record it was written from.
    let back: ActivityRecord =
        serde_json::from_value(written[0].clone()).expect("an activity record");
    assert_eq!(back.seq, 1);
    assert!(!back.payload_truncated);
}

/// AGV-FR-08 — a run whose file cannot be written records and streams anyway.
///
/// The file is the durable copy of something already held and already announced.
/// A full disk, an unwritable directory, or no resolvable root are each a reason
/// to lose the copy and none of them is a reason to stop an agent.
#[test]
fn a_run_whose_file_cannot_be_written_is_recorded_anyway() {
    // No `FsAccess` at all: the case where the application could not resolve a
    // place to write.
    let app = tauri::test::mock_app();
    let store = Arc::new(ActivityStore::new());
    let sink = RunActivitySink::rooted_at(
        &app.handle().clone(),
        "run-nofile",
        Arc::clone(&store),
        None,
        None,
    );

    sink.activity(event("message", "said anyway"));

    let page = store.read("run-nofile", None, None);
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].summary, "said anyway");

    // And a root that cannot be created — a file sitting where the directory
    // must go — is the same outcome rather than a failure that reaches anybody.
    let dir = TempDir::new().expect("a temp directory");
    let root = dir.path().canonicalize().expect("a real path");
    std::fs::write(root.join("agent-activity"), b"not a directory").expect("a file");
    let fs = Arc::new(
        crate::fs::FsAccess::builder()
            .allow_root(&root)
            .build()
            .expect("an instance"),
    );
    let store = Arc::new(ActivityStore::new());
    let sink = RunActivitySink::rooted_at(
        &app.handle().clone(),
        "run-blocked",
        Arc::clone(&store),
        Some(fs),
        Some(root.join("agent-activity")),
    );

    sink.activity(event("message", "still said"));

    assert_eq!(store.read("run-blocked", None, None).total, 1);
}

/// AGV-FR-07 — the file bound says once that it stopped short, and the run goes
/// on being recorded.
#[test]
fn the_file_bound_is_announced_once_and_stops_nothing() {
    // Small enough that a handful of records crosses it.
    let f = SinkFixture::new("run-bound", ActivityStore::with_file_bound(2048));

    for i in 1..=20 {
        let mut e = event("message", &format!("line {i}"));
        e.payload = "q".repeat(400);
        f.sink.activity(e);
    }

    let written = f.written("run-bound");
    let notices: Vec<_> = written
        .iter()
        .filter(|record| record["kind"] == "error")
        .collect();
    assert_eq!(notices.len(), 1, "the bound was announced {} times", notices.len());
    assert!(notices[0]["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("byte bound"));
    assert!(written.len() < 20, "the bound did not hold");

    // Everything is still in memory and still readable: the file stopping short
    // is the file's limit rather than the run's.
    let page = f.store.read("run-bound", None, Some(MAX_PAGE));
    assert_eq!(page.total, 20);
    assert_eq!(page.records.len(), 20);
    assert_eq!(page.latest_seq, 20);
}

/// AGV-FR-03 — a sink records under the run it was built for and no other.
///
/// The executor is handed a sink and never a run identifier, so this binding is
/// the whole of the attribution: a sink that ignored it would put every run's
/// narration in one place with nothing to separate them.
#[test]
fn a_sink_records_under_the_run_it_was_built_for() {
    let f = SinkFixture::new("run-mine", ActivityStore::new());

    f.sink.activity(event("message", "mine"));

    assert_eq!(f.store.read("run-mine", None, None).total, 1);
    assert_eq!(f.store.read("run-other", None, None).total, 0);
    assert!(f.file("run-mine").exists());
    assert!(!f.file("run-other").exists());
}

/// AGV-FR-12 — a sink still delivering when its run is discarded records
/// nothing, and writes nothing.
///
/// The window is the one a discard actually happens in: a run is discarded while
/// it works, cancellation only sets a flag, and the container's pipes go on being
/// drained until the child dies.
#[test]
fn a_sink_whose_run_was_discarded_records_and_writes_nothing_further() {
    let f = SinkFixture::new("run-thrown", ActivityStore::new());
    f.sink.activity(event("message", "before"));
    assert_eq!(f.written("run-thrown").len(), 1);

    f.store.forget("run-thrown");
    f.sink.activity(event("message", "after"));

    assert_eq!(f.store.read("run-thrown", None, None).total, 0);
    // The file keeps what was already written — it is the only account of what
    // the agent did to a directory that may still hold its edits — and gains
    // nothing after the discard.
    let written = f.written("run-thrown");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["summary"], "before");
}
