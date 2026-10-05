//! Where a proposed change belongs in the prompt (`DCP-draft-change-proposals.md`).
//!
//! A hunk names **the text it changes** rather than a position in the file
//! (DCP-FR-HRQN). That is what lets the author keep typing while a proposal
//! stands: the text a hunk points at moves down the file, and the hunk still
//! finds it. It is also what makes hunks decidable in **any order** — accepting
//! one hunk rewrites the prompt underneath the others, and a content anchor
//! survives that where a line range could not (DCP-FR-VZTK).
//!
//! Resolution is deliberately **exact**. There is no whitespace tolerance and no
//! patch-style fuzz, because fuzz works by dropping context until something
//! matches, and a forced match applies an agent's edit to text the agent never
//! read. A hunk that cannot be placed exactly is **lost** (DCP-FR-BMLX), which is
//! a state the author clears by rejecting it.
//!
//! Nothing here touches disk or Tauri: it is pure over `&str`, so it unit-tests
//! without a runtime.

/// Which way a hunk changes the text it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkKind {
    /// New text, inserted between `lead` and `trail`.
    Add,
    /// `before` removed.
    Del,
    /// `before` replaced by `after`.
    Replace,
}

/// The content anchor a hunk carries, with a positional hint that is only ever a
/// hint (DCP-FR-HRQN).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct HunkAnchor {
    /// Exact prompt text immediately before the changed text, bounded.
    #[serde(default)]
    pub lead: String,
    /// Exact prompt text immediately after the changed text, bounded.
    #[serde(default)]
    pub trail: String,
    /// Byte offset into the base as it stood when the proposal was recorded.
    #[serde(default)]
    pub hint_start: usize,
    #[serde(default)]
    pub hint_end: usize,
}

/// Where a hunk applies in the prompt as it stands, or that it no longer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// The byte range the hunk replaces. `start == end` for an insertion.
    Resolved { start: usize, end: usize },
    /// The text this hunk changes is no longer in the prompt (DCP-FR-BMLX).
    Lost,
}

impl Resolution {
    pub fn range(&self) -> Option<(usize, usize)> {
        match *self {
            Resolution::Resolved { start, end } => Some((start, end)),
            Resolution::Lost => None,
        }
    }

    pub fn is_lost(&self) -> bool {
        matches!(self, Resolution::Lost)
    }
}

/// How much context either side of a change an anchor carries. Bounded so a
/// record stays a record: a hunk in a file of repeated boilerplate is
/// disambiguated by 200 characters or it is refused at record time as ambiguous
/// (DCP-FR-JGCD).
pub const CONTEXT_BOUND: usize = 200;

/// The one normalisation both sides of a resolution go through. It is the
/// convention the save path persists (DRS-FR-12), so a hunk composed against a
/// file read with one line ending resolves against the same file written with
/// another.
pub fn normalise_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Take at most `CONTEXT_BOUND` bytes of `text` ending at `at`, cut back to a
/// character boundary.
pub fn lead_context(text: &str, at: usize) -> String {
    lead_context_from(text, at, 0)
}

/// The lead context, and never any text below `floor`.
///
/// `floor` is where the change before this one ends. Context that reached past
/// it would quote text **another change of the same proposal is going to
/// rewrite**, and this change would then be lost the moment that one was
/// accepted — which is exactly what "decidable in any order" (DCP-FR-VZTK)
/// forbids. Clipping here is what makes each change's context describe only
/// text no other change touches.
pub fn lead_context_from(text: &str, at: usize, floor: usize) -> String {
    let at = clamp_boundary(text, at);
    let floor = clamp_boundary(text, floor.min(at));
    let mut start = at.saturating_sub(CONTEXT_BOUND).max(floor);
    while start < at && !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..at].to_string()
}

/// Take at most `CONTEXT_BOUND` bytes of `text` starting at `at`, cut forward to
/// a character boundary.
pub fn trail_context(text: &str, at: usize) -> String {
    trail_context_to(text, at, usize::MAX)
}

/// The trail context, and never any text above `ceiling`.
///
/// `ceiling` is where the change after this one begins. See `lead_context_from`
/// for why that bound is what keeps the changes decidable in any order.
pub fn trail_context_to(text: &str, at: usize, ceiling: usize) -> String {
    let at = clamp_boundary(text, at);
    let ceiling = clamp_boundary(text, ceiling.max(at));
    let mut end = (at + CONTEXT_BOUND).min(text.len()).min(ceiling);
    while end > at && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[at..end].to_string()
}

fn clamp_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The occurrence of `needle` in `haystack` whose start lies nearest `hint`.
///
/// Nearest rather than first, because a prompt that repeats a phrase would
/// otherwise resolve every hunk to the first copy of it.
pub fn find_nearest(haystack: &str, needle: &str, hint: usize) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack
        .match_indices(needle)
        .map(|(at, _)| at)
        .min_by_key(|at| at.abs_diff(hint))
}

fn abuts_lead(text: &str, start: usize, lead: &str) -> bool {
    lead.is_empty() || text[..start].ends_with(lead)
}

fn abuts_trail(text: &str, end: usize, trail: &str) -> bool {
    trail.is_empty() || text[end..].starts_with(trail)
}

/// Resolve one hunk against the prompt **as it stands** (DCP-FR-VZTK).
///
/// `text` and `before` must both already be newline-normalised. The steps are
/// tried in order and the first that answers wins:
///
/// 1. **Unmoved** — the hint still holds `before`, with `lead` and `trail` either
///    side. One slice, and the overwhelmingly common case.
/// 2. **Shifted** — the nearest occurrence of `lead + before + trail`.
/// 3. **Context partly eroded** — the nearest occurrence of `before` alone,
///    accepted only where `lead` *or* `trail` still abuts it. One side is enough
///    to disambiguate; neither is not.
///
/// Anything else is `Lost`.
pub fn resolve(text: &str, kind: HunkKind, before: &str, anchor: &HunkAnchor) -> Resolution {
    if kind == HunkKind::Add {
        return resolve_insertion(text, anchor);
    }
    if before.is_empty() {
        return Resolution::Lost;
    }

    // 1. Unmoved.
    let (hs, he) = (anchor.hint_start, anchor.hint_end);
    if he <= text.len()
        && hs <= he
        && text.is_char_boundary(hs)
        && text.is_char_boundary(he)
        && &text[hs..he] == before
        && abuts_lead(text, hs, &anchor.lead)
        && abuts_trail(text, he, &anchor.trail)
    {
        return Resolution::Resolved { start: hs, end: he };
    }

    // 2. Shifted: the whole anchored run moved.
    let run = format!("{}{}{}", anchor.lead, before, anchor.trail);
    if let Some(at) = find_nearest(text, &run, anchor.hint_start) {
        let start = at + anchor.lead.len();
        return Resolution::Resolved { start, end: start + before.len() };
    }

    // 3. Context partly eroded: one side still holds.
    if let Some(start) = find_nearest_where(text, before, anchor.hint_start, |t, s, e| {
        (!anchor.lead.is_empty() && abuts_lead(t, s, &anchor.lead))
            || (!anchor.trail.is_empty() && abuts_trail(t, e, &anchor.trail))
    }) {
        return Resolution::Resolved { start, end: start + before.len() };
    }

    Resolution::Lost
}

/// An insertion names no text of its own, so it resolves on its context alone:
/// between `lead` and `trail`, else after `lead`, else before `trail`.
fn resolve_insertion(text: &str, anchor: &HunkAnchor) -> Resolution {
    let (lead, trail) = (anchor.lead.as_str(), anchor.trail.as_str());

    if !lead.is_empty() && !trail.is_empty() {
        let run = format!("{lead}{trail}");
        if let Some(at) = find_nearest(text, &run, anchor.hint_start) {
            let at = at + lead.len();
            return Resolution::Resolved { start: at, end: at };
        }
    }
    if !lead.is_empty() {
        if let Some(at) = find_nearest(text, lead, anchor.hint_start) {
            let at = at + lead.len();
            return Resolution::Resolved { start: at, end: at };
        }
    }
    if !trail.is_empty() {
        if let Some(at) = find_nearest(text, trail, anchor.hint_start) {
            return Resolution::Resolved { start: at, end: at };
        }
    }
    // A prompt that is empty is the one place an insertion with no context lands.
    if lead.is_empty() && trail.is_empty() && text.is_empty() {
        return Resolution::Resolved { start: 0, end: 0 };
    }
    Resolution::Lost
}

fn find_nearest_where(
    haystack: &str,
    needle: &str,
    hint: usize,
    accept: impl Fn(&str, usize, usize) -> bool,
) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack
        .match_indices(needle)
        .map(|(at, _)| at)
        .filter(|&at| accept(haystack, at, at + needle.len()))
        .min_by_key(|at| at.abs_diff(hint))
}

/// How many times `before` occurs in `text`. Record-time validation refuses a
/// hunk that names text occurring more than once (DCP-FR-JGCD).
pub fn occurrences(text: &str, before: &str) -> usize {
    if before.is_empty() {
        return 0;
    }
    text.match_indices(before).count()
}

/// Replace one resolved range. The caller has already resolved, so this is the
/// whole of what an acceptance does to the prompt's bytes (DCP-FR-12).
pub fn splice(text: &str, start: usize, end: usize, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..start]);
    out.push_str(replacement);
    out.push_str(&text[end..]);
    out
}

/// Do two resolved ranges cover any of the same text? Two hunks that overlap are
/// refused at record time, which is what makes the rest decidable in any order
/// (DCP-FR-JGCD).
pub fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    let (a0, a1) = a;
    let (b0, b1) = b;
    if a0 == a1 && b0 == b1 {
        return a0 == b0;
    }
    a0 < b1 && b0 < a1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(lead: &str, trail: &str, hint_start: usize, hint_end: usize) -> HunkAnchor {
        HunkAnchor {
            lead: lead.to_string(),
            trail: trail.to_string(),
            hint_start,
            hint_end,
        }
    }

    // DCP-FR-VZTK: the hint still holds the text, so one slice answers.
    #[test]
    fn resolves_at_the_hint_when_nothing_moved() {
        let text = "alpha BETA gamma";
        let a = anchor("alpha ", " gamma", 6, 10);
        assert_eq!(
            resolve(text, HunkKind::Replace, "BETA", &a),
            Resolution::Resolved { start: 6, end: 10 }
        );
    }

    // DCP-FR-VZTK: text inserted above the hunk moves it, and the anchored run finds it.
    #[test]
    fn resolves_after_the_author_typed_above_it() {
        let text = "NEW LINE\nalpha BETA gamma";
        let a = anchor("alpha ", " gamma", 6, 10);
        let (s, e) = resolve(text, HunkKind::Replace, "BETA", &a).range().unwrap();
        assert_eq!(&text[s..e], "BETA");
        assert_eq!(s, 15);
    }

    // DCP-FR-VZTK: the nearest occurrence wins, so a repeated phrase resolves to the right copy.
    #[test]
    fn picks_the_occurrence_nearest_the_hint() {
        let text = "x TARGET y ...... x TARGET y";
        let a = anchor("x ", " y", 19, 25);
        let (s, _) = resolve(text, HunkKind::Replace, "TARGET", &a).range().unwrap();
        assert_eq!(s, 20, "the second copy is nearer the hint than the first");
    }

    // DCP-FR-VZTK: one side of the context is enough to place a hunk.
    #[test]
    fn resolves_on_one_side_of_context_when_the_other_is_gone() {
        let text = "alpha BETA REWRITTEN-TAIL";
        let a = anchor("alpha ", " gamma", 6, 10);
        let (s, e) = resolve(text, HunkKind::Replace, "BETA", &a).range().unwrap();
        assert_eq!(&text[s..e], "BETA");
    }

    // DCP-FR-BMLX: neither side abuts, so the hunk is lost rather than forced.
    #[test]
    fn is_lost_when_neither_side_of_the_context_survives() {
        let text = "completely different prose with BETA buried in it";
        let a = anchor("alpha ", " gamma", 6, 10);
        assert!(resolve(text, HunkKind::Replace, "BETA", &a).is_lost());
    }

    // DCP-FR-BMLX: the text it changes is gone entirely.
    #[test]
    fn is_lost_when_the_text_it_changes_is_gone() {
        let text = "alpha gamma";
        let a = anchor("alpha ", " gamma", 6, 10);
        assert!(resolve(text, HunkKind::Replace, "BETA", &a).is_lost());
    }

    // DCP-FR-BMLX: lost is derived, so undoing the edit resolves the hunk again.
    #[test]
    fn resolves_again_once_the_author_undoes_the_edit_that_lost_it() {
        let a = anchor("alpha ", " gamma", 6, 10);
        assert!(resolve("alpha gamma", HunkKind::Replace, "BETA", &a).is_lost());
        assert!(!resolve("alpha BETA gamma", HunkKind::Replace, "BETA", &a).is_lost());
    }

    // DCP-FR-HRQN: an insertion names no text, so it lands between its context.
    #[test]
    fn an_insertion_lands_between_its_context() {
        let text = "one\ntwo\n";
        let a = anchor("one\n", "two\n", 4, 4);
        assert_eq!(
            resolve(text, HunkKind::Add, "", &a),
            Resolution::Resolved { start: 4, end: 4 }
        );
    }

    // DCP-FR-VZTK: an insertion whose trailing context has gone still lands after its lead.
    #[test]
    fn an_insertion_falls_back_to_its_lead_alone() {
        let text = "one\nrewritten\n";
        let a = anchor("one\n", "two\n", 4, 4);
        assert_eq!(
            resolve(text, HunkKind::Add, "", &a),
            Resolution::Resolved { start: 4, end: 4 }
        );
    }

    // DCP-FR-VZTK: both texts are newline-normalised before anything is compared.
    #[test]
    fn normalises_line_endings_before_resolving() {
        assert_eq!(normalise_newlines("a\r\nb\rc"), "a\nb\nc");
        let text = normalise_newlines("alpha\r\nBETA\r\ngamma");
        let a = anchor("alpha\n", "\ngamma", 6, 10);
        assert!(!resolve(&text, HunkKind::Replace, "BETA", &a).is_lost());
    }

    // DCP-FR-12: an acceptance replaces the resolved range and nothing else.
    #[test]
    fn splice_replaces_only_the_resolved_range() {
        assert_eq!(splice("alpha BETA gamma", 6, 10, "DELTA"), "alpha DELTA gamma");
        assert_eq!(splice("one\ntwo\n", 4, 4, "mid\n"), "one\nmid\ntwo\n");
        assert_eq!(splice("alpha BETA gamma", 5, 10, ""), "alpha gamma");
    }

    // DCP-FR-JGCD: ambiguity is counted at record time rather than guessed at.
    #[test]
    fn counts_occurrences_for_the_ambiguity_refusal() {
        assert_eq!(occurrences("a X b X c", "X"), 2);
        assert_eq!(occurrences("a X b", "X"), 1);
        assert_eq!(occurrences("a b", "X"), 0);
        assert_eq!(occurrences("a b", ""), 0);
    }

    // DCP-FR-JGCD: overlapping hunks are what the record-time refusal catches.
    #[test]
    fn detects_overlapping_ranges() {
        assert!(ranges_overlap((0, 10), (5, 15)));
        assert!(ranges_overlap((5, 15), (0, 10)));
        assert!(!ranges_overlap((0, 5), (5, 10)), "touching is not overlapping");
        assert!(!ranges_overlap((0, 5), (10, 15)));
        assert!(!ranges_overlap((3, 3), (7, 7)), "two insertions at different points");
        assert!(ranges_overlap((3, 3), (3, 3)), "two insertions at one point");
    }

    // DCP-FR-HRQN: context is bounded, and never cuts a character in half.
    //
    // A three-byte character, deliberately: `CONTEXT_BOUND` is 200, which is a
    // multiple of one and of two, so a one- or two-byte character lands on a
    // boundary by arithmetic and the code that walks back to one is never
    // reached. Anything but a boundary here is a panic on the slice.
    #[test]
    fn context_is_bounded_and_lands_on_character_boundaries() {
        let text = "日".repeat(400);
        assert_ne!(CONTEXT_BOUND % "日".len(), 0, "the bound must fall mid-character");
        let lead = lead_context(&text, text.len());
        let trail = trail_context(&text, 0);
        assert!(lead.len() <= CONTEXT_BOUND && trail.len() <= CONTEXT_BOUND);
        assert!(text.ends_with(&lead) && text.starts_with(&trail));

        // And the same where the bound is a neighbour's range rather than
        // `CONTEXT_BOUND`, which is the cut a proposal of several changes makes.
        // The offsets are deliberately mid-character on both sides.
        let clipped_lead = lead_context_from(&text, 301, 250);
        let clipped_trail = trail_context_to(&text, 301, 350);
        assert!(text.contains(&clipped_lead) && text.contains(&clipped_trail));
        // Clipped to the neighbour rather than to `CONTEXT_BOUND`, and still on
        // character boundaries — which is the whole point of the clip: a change
        // must not quote text a neighbouring change is going to rewrite.
        assert!(clipped_lead.len() < CONTEXT_BOUND && clipped_lead.len() >= 48);
        assert!(clipped_trail.len() < CONTEXT_BOUND && clipped_trail.len() >= 45);
    }

    // DCP-FR-VZTK: a hint pointing past the end of a shortened prompt is only a hint.
    #[test]
    fn a_hint_past_the_end_of_the_text_still_resolves() {
        let text = "alpha BETA gamma";
        let a = anchor("alpha ", " gamma", 9000, 9004);
        assert!(!resolve(text, HunkKind::Replace, "BETA", &a).is_lost());
    }
}
