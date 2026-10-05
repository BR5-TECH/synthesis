//! Timestamps and identifiers of a note.

use super::*;

// ---------------------------------------------------------------------------
// Timestamps (NTC-FR-03) and ids (NTC-FR-02)
// ---------------------------------------------------------------------------

/// Render a Unix instant as an RFC 3339 UTC timestamp, `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Fixed-width and always UTC, which is what lets the list commands order notes
/// by comparing `updated_at` as a string (NTC-FR-09) rather than parsing every
/// one on every load.
pub fn format_rfc3339_utc(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let secs_of_day = unix_seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (h, min, s) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
}

/// Days since the Unix epoch -> proleptic Gregorian year/month/day (Howard
/// Hinnant's `civil_from_days`). Exact for every representable date, which
/// matters because a timestamp is what orders the panel.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The current instant as an RFC 3339 UTC timestamp.
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_rfc3339_utc(secs)
}

/// Render a Unix instant as an RFC 3339 UTC timestamp with millisecond
/// precision, `YYYY-MM-DDTHH:MM:SS.mmmZ`.
///
/// The finer-grained sibling of [`format_rfc3339_utc`], for
/// `../core/LGC-logging.md`: a diagnostic log orders records that arrive inside
/// the same second, and a second-granularity stamp renders a burst as a column
/// of identical times.
pub fn format_rfc3339_millis_utc(unix_millis: i64) -> String {
    let seconds = unix_millis.div_euclid(1_000);
    let millis = unix_millis.rem_euclid(1_000);
    let base = format_rfc3339_utc(seconds);
    // `format_rfc3339_utc` always ends in `Z`, so the fraction is spliced in
    // ahead of it rather than reformatted from scratch.
    format!("{}.{:03}Z", &base[..base.len() - 1], millis)
}

/// The current instant as an RFC 3339 UTC timestamp with millisecond precision
/// (LGC-FR-06).
pub fn now_rfc3339_millis() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    format_rfc3339_millis_utc(millis)
}

/// Render a filesystem instant as an RFC 3339 UTC timestamp with millisecond
/// precision.
///
/// What every filesystem-derived activity time in the application is reported
/// as — a draft's prompt activity (`DRS-draft-storage.md` DRS-FR-41) and an
/// artifact's source-file modification time (`PST-project-storage.md`
/// PST-FR-31). Milliseconds rather than seconds because both are used to order
/// a list, and two files written inside the same second must not read as the
/// same instant.
///
/// An instant before the Unix epoch — which a filesystem can report for a file
/// whose time has been set by hand — renders as the epoch rather than failing:
/// the caller wants an ordering key, and refusing one would drop the entry.
pub fn format_rfc3339_millis_from(time: SystemTime) -> String {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    format_rfc3339_millis_utc(millis)
}

/// Monotonic within the process, so two notes created in the same millisecond
/// still get distinct ids.
static ID_COUNTER: AtomicU32 = AtomicU32::new(0);

/// NTC-FR-02: an opaque id, derived from neither the body nor the scope, so
/// editing or moving a note changes neither its id nor its filename.
///
/// The millisecond clock keeps ids roughly ordered (pleasant in a `git diff` of
/// the notes folder); the process counter separates same-millisecond creations;
/// and the hasher's per-instance random seed separates two people creating a
/// note in their own clone of the project at the same moment, which is what
/// keeps notes from colliding when their branches merge.
pub fn new_note_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(millis);
    hasher.write_u32(counter);
    let entropy = hasher.finish() as u32;
    format!("{millis:011x}-{counter:04x}-{entropy:08x}")
}

/// Reject an id that is not a bare, filename-safe token before it is joined into
/// a path. `resolve_under` already refuses to escape the root (NTC-FR-13), but a
/// separator or a dot segment would still name a file outside the notes folder
/// *inside* the project, so it is rejected outright rather than resolved.
pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
