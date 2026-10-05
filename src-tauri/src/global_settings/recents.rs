//! The pure rules of the recent-projects list and the small helpers the store
//! stamps its entries with (`specifications/core/GSS-global-settings-storage.md`
//! GSS-FR-05, GSS-FR-06, GSS-FR-07, GSS-FR-11, GSS-FR-12).
//!
//! One part of `global_settings.rs`, which re-exports every item here, so a
//! caller names them through that module as before.

use std::time::{SystemTime, UNIX_EPOCH};

use super::RecentProjectEntry;

/// Maximum number of *unpinned* recent-projects entries retained (GSS-FR-07).
/// Pinned entries are exempt from the cap.
pub const RECENT_UNPINNED_CAP: usize = 20;

/// Apply GSS-FR-05 (ordering), GSS-FR-06 (prune-missing), and GSS-FR-07
/// (cap-unpinned) to a raw entry list and return the visible, ordered list.
///
/// Pure and cap-parameterised so the capping behaviour is testable without
/// constructing `RECENT_UNPINNED_CAP + 1` entries.
///
/// 1. **Prune (GSS-FR-06):** drop entries that are unpinned *and* missing. A
///    pinned-but-missing entry is retained (flagged `missing`).
/// 2. **Order (GSS-FR-05):** pinned entries first, then most-recent-first
///    within each group (descending `last_opened_at`).
/// 3. **Cap (GSS-FR-07):** keep every pinned entry, and at most `cap` unpinned
///    entries — because the unpinned run is already most-recent-first, this
///    evicts the oldest unpinned entries.
pub fn ordered_visible_recents_with_cap(
    entries: &[RecentProjectEntry],
    cap: usize,
) -> Vec<RecentProjectEntry> {
    let mut kept: Vec<RecentProjectEntry> = entries
        .iter()
        .filter(|e| e.pinned || !e.missing)
        .cloned()
        .collect();

    // Pinned-first, then recency-desc. `last_opened_at` is ISO-8601 UTC, so a
    // reverse string comparison is a reverse-chronological comparison.
    //
    // INVARIANT: this MUST be the *stable* `sort_by` (not `sort_unstable_by`).
    // `record_recent_project` inserts the newest open at the front of the vec,
    // so when two entries share a timestamp (same wall-clock second) the stable
    // sort keeps the more-recently-recorded one ahead. An unstable sort would
    // break that tiebreak.
    kept.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.last_opened_at.cmp(&a.last_opened_at))
    });

    let mut out = Vec::with_capacity(kept.len());
    let mut unpinned_seen = 0usize;
    for entry in kept {
        if entry.pinned {
            out.push(entry);
        } else if unpinned_seen < cap {
            out.push(entry);
            unpinned_seen += 1;
        }
        // else: oldest unpinned beyond the cap — evicted (GSS-FR-07).
    }
    out
}

/// Convenience wrapper using the production cap (`RECENT_UNPINNED_CAP`).
pub fn ordered_visible_recents(entries: &[RecentProjectEntry]) -> Vec<RecentProjectEntry> {
    ordered_visible_recents_with_cap(entries, RECENT_UNPINNED_CAP)
}

/// Derive a stable registry id from an install source (GSS-FR-11 / GSS-FR-12).
///
/// Walking-skeleton heuristic: take the final path/URL segment and strip a
/// trailing `.git`. A real implementation would read a manifest; the id only
/// needs to be stable and human-recognisable for the skeleton.
pub fn derive_id_from_source(source: &str) -> String {
    let trimmed = source.trim().trim_end_matches('/');
    let tail = trimmed
        .rsplit(|c| c == '/' || c == ':' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or(trimmed);
    tail.strip_suffix(".git").unwrap_or(tail).to_string()
}

/// Current wall-clock time as an ISO-8601 UTC string (`YYYY-MM-DDTHH:MM:SSZ`).
/// Used to stamp the MRU `last_opened_at` on project open/create.
pub fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    unix_secs_to_iso8601(secs)
}

/// Convert Unix epoch seconds to an ISO-8601 UTC string. Pure (no dependency on
/// a date crate) so it is unit-testable against known vectors. Uses Howard
/// Hinnant's civil-from-days algorithm.
pub fn unix_secs_to_iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    // civil_from_days: days are counted from 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
