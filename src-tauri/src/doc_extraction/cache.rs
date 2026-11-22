//! CMS-FR-AAMO: the document cache of the application session.
//!
//! It holds the outcome of each extraction, keyed by the SHA-256 digest of the
//! document bytes. All projects share it, and it is never written to disk. A
//! fixed total size bound removes the least recently used entry first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::{ExtractError, ExtractedDocument};

/// CMS-FR-AAMO: the total size bound of the cache.
pub const CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;

/// What the cache keeps for one digest: the blocks, or the failure, so a
/// failed document is not extracted again (CMS-FR-RQTB).
pub type Outcome = Result<Arc<ExtractedDocument>, ExtractError>;

/// The size the cache counts for a failure.
const FAILURE_FOOTPRINT: usize = 64;

struct Entry {
    outcome: Outcome,
    footprint: usize,
    last_used: u64,
}

/// A bounded map from digest to outcome, least recently used first out.
pub struct DocumentCache {
    entries: HashMap<String, Entry>,
    max_bytes: usize,
    bytes: usize,
    clock: u64,
}

impl DocumentCache {
    pub fn new(max_bytes: usize) -> DocumentCache {
        DocumentCache { entries: HashMap::new(), max_bytes, bytes: 0, clock: 0 }
    }

    /// The outcome kept for `digest`, which becomes the most recently used.
    pub fn get(&mut self, digest: &str) -> Option<Outcome> {
        self.clock += 1;
        let clock = self.clock;
        let entry = self.entries.get_mut(digest)?;
        entry.last_used = clock;
        Some(entry.outcome.clone())
    }

    /// Keeps `outcome` for `digest`, then removes the least recently used
    /// entries until the total is within the bound. An entry larger than the
    /// bound on its own is not kept.
    pub fn put(&mut self, digest: &str, outcome: Outcome) {
        let footprint = match &outcome {
            Ok(document) => document.footprint(),
            Err(_) => FAILURE_FOOTPRINT,
        } + digest.len();
        if let Some(old) = self.entries.remove(digest) {
            self.bytes -= old.footprint;
        }
        if footprint > self.max_bytes {
            return;
        }
        self.clock += 1;
        self.entries
            .insert(digest.to_string(), Entry { outcome, footprint, last_used: self.clock });
        self.bytes += footprint;
        while self.bytes > self.max_bytes {
            let Some(oldest) =
                self.entries.iter().min_by_key(|(_, e)| e.last_used).map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes -= entry.footprint;
            }
        }
    }

    /// Whether `digest` has an entry, without a change to its use order.
    pub fn contains(&self, digest: &str) -> bool {
        self.entries.contains_key(digest)
    }

    /// The total size the entries count.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

fn shared() -> &'static Mutex<DocumentCache> {
    static CACHE: OnceLock<Mutex<DocumentCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(DocumentCache::new(CACHE_MAX_BYTES)))
}

/// The outcome the session cache keeps for `digest`.
pub fn lookup(digest: &str) -> Option<Outcome> {
    shared().lock().unwrap_or_else(|p| p.into_inner()).get(digest)
}

/// Keeps `outcome` for `digest` in the session cache.
pub fn store(digest: &str, outcome: Outcome) {
    shared().lock().unwrap_or_else(|p| p.into_inner()).put(digest, outcome);
}

/// The cached outcome for `digest`, or the outcome of `extract` stored first.
/// The lock is not held while `extract` runs, so a long extraction does not
/// stop a read of another document.
pub fn get_or_extract(
    digest: &str,
    extract: impl FnOnce() -> Result<ExtractedDocument, ExtractError>,
) -> Outcome {
    if let Some(outcome) = lookup(digest) {
        return outcome;
    }
    let outcome = extract().map(Arc::new);
    // An interrupted extraction depends on the load of the machine, so a later
    // read tries again.
    if !matches!(outcome, Err(ExtractError::Interrupted(_))) {
        store(digest, outcome.clone());
    }
    outcome
}

/// Empties the session cache, so a test reaches the path of a cache miss.
#[cfg(test)]
pub fn clear_for_test() {
    let mut cache = shared().lock().unwrap_or_else(|p| p.into_inner());
    *cache = DocumentCache::new(CACHE_MAX_BYTES);
}
