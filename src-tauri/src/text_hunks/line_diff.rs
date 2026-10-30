//! A complete new text, expressed as the changes it makes (PCP-FR-CNVT).
//!
//! A caller that gives the whole new text of a file still gets a review hunk by
//! hunk: the two texts are compared line by line, and each run of changed
//! lines becomes one anchored change. The changes carry the bounded text either
//! side of them, clipped to the unchanged text between two changes, so each one
//! is placed and decided without the others (DCP-FR-VZTK).

use super::anchors::{lead_context_from, normalise_newlines, trail_context_to, HunkKind};
use super::hunks::ProposedHunk;

/// The most cells the line comparison fills. A larger comparison is one change
/// over the whole changed middle, which is still correct, only coarser.
const MAX_CELLS: usize = 2_000_000;

/// One run of changed lines: the lines `old` removes from the current text and
/// the lines `new` puts in their place, as line indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    old: (usize, usize),
    new: (usize, usize),
}

/// The changes that turn `current` into `next`, in document order.
///
/// Empty where the two texts are the same after line endings are normalised.
/// The caller places the result with `hunks::plan` and falls back to one
/// whole-document change where that placement refuses.
pub fn line_changes(current: &str, next: &str) -> Vec<ProposedHunk> {
    let current = normalise_newlines(current);
    let next = normalise_newlines(next);
    let old: Vec<&str> = current.split_inclusive('\n').collect();
    let new: Vec<&str> = next.split_inclusive('\n').collect();
    let regions = regions(&old, &new);
    // The byte offset at which each line of the current text starts, and one
    // more entry for the end of the text.
    let mut starts = Vec::with_capacity(old.len() + 1);
    let mut at = 0;
    for line in &old {
        starts.push(at);
        at += line.len();
    }
    starts.push(at);

    let mut out = Vec::with_capacity(regions.len());
    for (index, region) in regions.iter().enumerate() {
        let floor = if index == 0 { 0 } else { starts[regions[index - 1].old.1] };
        let ceiling = regions.get(index + 1).map_or(current.len(), |r| starts[r.old.0]);
        let added: String = new[region.new.0..region.new.1].concat();
        let (start, end) = (starts[region.old.0], starts[region.old.1]);
        if start < end {
            let kind = if added.is_empty() { HunkKind::Del } else { HunkKind::Replace };
            out.push(ProposedHunk {
                kind,
                before: Some(current[start..end].to_string()),
                after: (!added.is_empty()).then_some(added),
                after_text: None,
                lead: non_empty(lead_context_from(&current, start, floor)),
                trail: non_empty(trail_context_to(&current, end, ceiling)),
                note: None,
            });
        } else if region.old.0 > 0 {
            // An insertion follows the line before it.
            let line = starts[region.old.0 - 1];
            out.push(ProposedHunk {
                kind: HunkKind::Add,
                before: None,
                after: Some(added),
                after_text: Some(current[line..start].to_string()),
                lead: non_empty(lead_context_from(&current, line, floor)),
                trail: non_empty(trail_context_to(&current, start, ceiling)),
                note: None,
            });
        } else if region.old.0 < old.len() {
            // An insertion at the start of the text follows nothing, so it
            // replaces the first line with the new lines and that line.
            let first = starts[1];
            out.push(ProposedHunk {
                kind: HunkKind::Replace,
                before: Some(current[..first].to_string()),
                after: Some(format!("{added}{}", &current[..first])),
                after_text: None,
                lead: None,
                trail: non_empty(trail_context_to(&current, first, ceiling.max(first))),
                note: None,
            });
        } else {
            // The current text is empty: the change fills it.
            return super::hunks::whole_document(&current, &next);
        }
    }
    out
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

/// The runs of changed lines between `old` and `new`, in order. Two runs are
/// always apart by at least one unchanged line.
fn regions(old: &[&str], new: &[&str]) -> Vec<Region> {
    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let a = &old[prefix..old.len() - suffix];
    let b = &new[prefix..new.len() - suffix];
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    let whole = Region { old: (prefix, prefix + a.len()), new: (prefix, prefix + b.len()) };
    if a.is_empty() || b.is_empty() || a.len().saturating_mul(b.len()) > MAX_CELLS {
        return vec![whole];
    }

    // Longest common subsequence of the changed middle, filled from the end so
    // the walk below reads forward.
    let width = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }

    let mut out: Vec<Region> = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<Region> = None;
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            if let Some(region) = open.take() {
                out.push(region);
            }
            i += 1;
            j += 1;
            continue;
        }
        let region = open
            .get_or_insert(Region { old: (prefix + i, prefix + i), new: (prefix + j, prefix + j) });
        if j < b.len() && (i == a.len() || table[i * width + j + 1] >= table[(i + 1) * width + j]) {
            j += 1;
            region.new.1 = prefix + j;
        } else {
            i += 1;
            region.old.1 = prefix + i;
        }
    }
    if let Some(region) = open {
        out.push(region);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_hunks::hunks::{apply_all, plan};

    fn applied(current: &str, next: &str) -> (usize, String) {
        let changes = line_changes(current, next);
        let mut n = 0;
        let doc = plan(current, &changes, || {
            n += 1;
            format!("h{n}")
        })
        .expect("the changes are placed");
        (changes.len(), apply_all(current, &doc).expect("the changes apply"))
    }

    // PCP-FR-CNVT: each run of changed lines is its own change, and the changes
    // together give the new text.
    #[test]
    fn separate_edits_become_separate_changes() {
        let current = "# Title\n\nOne.\nTwo.\nThree.\nFour.\nFive.\n";
        let next = "# Title\n\nOne!\nTwo.\nThree.\nFour.\nFive?\nSix.\n";
        let (count, text) = applied(current, next);
        assert_eq!(count, 2);
        assert_eq!(text, next);
    }

    // PCP-FR-CNVT: an insertion, a deletion, and an insertion at the start each
    // apply to the text the caller gave.
    #[test]
    fn insertions_and_deletions_apply() {
        let current = "alpha\nbeta\ngamma\ndelta\n";
        for next in [
            "intro\nalpha\nbeta\ngamma\ndelta\n",
            "alpha\nbeta\nnew\ngamma\ndelta\n",
            "alpha\ngamma\ndelta\n",
            "alpha\nbeta\ngamma\ndelta\nomega",
            "",
        ] {
            let (_, text) = applied(current, next);
            assert_eq!(text, next);
        }
    }

    // PCP-FR-CNVT: the same text gives no change, and line endings alone are
    // no change.
    #[test]
    fn the_same_text_gives_no_change() {
        assert!(line_changes("a\nb\n", "a\nb\n").is_empty());
        assert!(line_changes("a\r\nb\r\n", "a\nb\n").is_empty());
    }

    // PCP-FR-CNVT: an empty current text is filled by one insertion.
    #[test]
    fn an_empty_text_is_filled_by_one_change() {
        let changes = line_changes("", "new\n");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, HunkKind::Add);
    }
}
