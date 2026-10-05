//! The test scenarios of `specifications/core/SCC-search-command.md`.
//!
//! Each test runs a real search over a temporary project directory and reads
//! what the recording sink was handed. This module holds the sink and the run
//! helpers; the topic modules hold the tests.

use super::*;
use crate::scanning;
use std::collections::BTreeMap;
use std::path::Path;

/// One thing the sink was handed, in the order it was handed it.
///
/// A single ordered log rather than two lists, deliberately: SCC-FR-13 and
/// SCC-FR-14 are claims about the *interleaving* — "no `search results`
/// carries that id after its `search ended`" — and two separate `Vec`s make
/// that unobservable.
#[derive(Clone, Debug)]
enum Recorded {
    Batch(SearchResultsPayload),
    Ended(SearchEndedPayload),
}

/// A `SearchSink` that records everything it was handed, so streaming,
/// ordering, capping and termination are assertable without a Tauri runtime.
#[derive(Default)]
struct Recorder {
    log: Mutex<Vec<Recorded>>,
}

impl SearchSink for Recorder {
    fn results(&self, payload: &SearchResultsPayload) {
        self.log
            .lock()
            .unwrap()
            .push(Recorded::Batch(payload.clone()));
    }
    fn ended(&self, payload: &SearchEndedPayload) {
        self.log
            .lock()
            .unwrap()
            .push(Recorded::Ended(payload.clone()));
    }
}

impl Recorder {
    fn hits(&self) -> Vec<SearchHit> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Recorded::Batch(b) => Some(b.hits.clone()),
                Recorded::Ended(_) => None,
            })
            .flatten()
            .collect()
    }

    fn ordered_hits(&self) -> Vec<SearchHit> {
        let mut hits = self.hits();
        hits.sort_by_key(|h| h.ordinal);
        hits
    }

    /// SCC-FR-13: exactly one terminal event, and — the half two lists could
    /// not see — nothing for that id after it.
    fn end_reason(&self) -> EndReason {
        let log = self.log.lock().unwrap();
        let terminals: Vec<&SearchEndedPayload> = log
            .iter()
            .filter_map(|e| match e {
                Recorded::Ended(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(
            terminals.len(),
            1,
            "SCC-FR-13: exactly one terminal event per search, got {terminals:?}"
        );
        let terminal = terminals[0];
        let at = log
            .iter()
            .position(|e| matches!(e, Recorded::Ended(_)))
            .expect("the terminal event is in the log");
        for later in &log[at + 1..] {
            if let Recorded::Batch(batch) = later {
                assert_ne!(
                    batch.search_id, terminal.search_id,
                    "SCC-FR-13: no `search results` may carry an id after its \
                     `search ended`; a consumer keying by id has already \
                     discarded its accumulator"
                );
            }
        }
        terminal.reason
    }

    fn batch_count(&self) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Recorded::Batch(_)))
            .count()
    }
}

/// Run a search over a real project directory, to exhaustion unless the
/// scope caps it.
fn search(
    root: &crate::fs::RootFs,
    query: &str,
    mode: SearchMode,
    scope: SearchScope,
) -> (Recorder, EndReason) {
    let recorder = Recorder::default();
    let candidates = Arc::new(scanning::candidate_files(&scanning::scan(root)));
    let matcher = Arc::new(Matcher::compile(query, mode).expect("query compiles"));
    let reason = run_search(
        &recorder,
        root,
        candidates,
        matcher,
        scope,
        &Stop::new(),
        "search-test",
        &|_| {},
    );
    (recorder, reason)
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

mod contract;
mod candidates;
mod modes;
mod hits;
mod grouping;
mod ordering_and_cap;
mod cancellation;
mod content_limits;
mod read_only;
mod responsiveness;
mod snippets;
mod candidate_list;
mod wire_and_bounds;
mod command_level;
