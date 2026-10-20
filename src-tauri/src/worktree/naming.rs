//! Branch and worktree names that this application makes
//! (`WTC-worktree-context.md` WTC-FR-XLXY, WTC-FR-AGQQ).
//!
//! One rule for every name an author types: a new worktree's branch, its
//! directory, its Git registration, and a work stream's slug
//! (`WKS-work-streams.md` WKS-FR-KDXF).

/// WTC-FR-XLXY: `raw` as a branch name. The result is lower case and holds
/// only `a-z`, `0-9`, `_`, `-` and `/`. Every other character becomes `-`, a
/// run of `-` or of `/` becomes one, and no segment is empty or starts or ends
/// with `-`. Empty when `raw` holds no character that a branch can keep.
pub fn normalize_branch_name(raw: &str) -> String {
    normalize(raw, true)
}

/// WTC-FR-AGQQ: `raw` as a worktree name. The rule of [`normalize_branch_name`],
/// and `/` also becomes `-`, so the result is one path segment.
pub fn normalize_worktree_name(raw: &str) -> String {
    normalize(raw, false)
}

fn normalize(raw: &str, keep_slash: bool) -> String {
    let mapped: String = raw
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            '/' if keep_slash => '/',
            _ => '-',
        })
        .collect();
    mapped
        .split('/')
        .map(collapse_segment)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// One `/` segment with each run of `-` made one `-`, and no `-` at its ends.
fn collapse_segment(segment: &str) -> String {
    segment.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-")
}
