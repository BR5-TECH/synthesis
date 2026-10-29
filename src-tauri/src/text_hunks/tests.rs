//! Tests for the neutral hunk planning: `lead` and `trail`, the uniqueness rule,
//! and the bound (`PDC-propose-draft-changes-tool.md`,
//! `PPC-propose-prompt-changes-tool.md`).

use super::anchors::{self, find_once, HunkKind, Occurrence};
use super::hunks::{apply_all, plan, ContextSide, PlanError, ProposedHunk, LEAD_TRAIL_BOUND};

fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("h{n}")
    }
}

fn replace(before: &str, after: &str, lead: Option<&str>, trail: Option<&str>) -> ProposedHunk {
    ProposedHunk::from_fields(
        Some(before.into()),
        Some(after.into()),
        None,
        lead.map(Into::into),
        trail.map(Into::into),
        None,
    )
}

/// Plan `hunks` against `text` and apply them, as a whole-candidate build does.
fn build(text: &str, hunks: &[ProposedHunk]) -> Result<String, PlanError> {
    let doc = plan(text, hunks, ids())?;
    Ok(apply_all(text, &doc).expect("a planned document applies"))
}

const REPEATED: &str = "## Setup\nRun the tests.\n\n## Release\nRun the tests.\n";

// PDC-FR-WCXD, PPC-FR-RJCA: `lead` names which copy of a repeated text changes,
// and only `before` changes.
#[test]
fn a_lead_picks_one_copy_of_a_repeated_text() {
    let out = build(
        REPEATED,
        &[replace("Run the tests.", "Run the full suite.", Some("## Release\n"), None)],
    )
    .expect("planned");
    assert_eq!(out, "## Setup\nRun the tests.\n\n## Release\nRun the full suite.\n");
}

// PDC-FR-WCXD, PPC-FR-RJCA: `trail` does the same from the other side.
#[test]
fn a_trail_picks_one_copy_of_a_repeated_text() {
    let out = build(
        REPEATED,
        &[replace("Run the tests.", "Run the unit tests.", None, Some("\n\n## Release"))],
    )
    .expect("planned");
    assert_eq!(out, "## Setup\nRun the unit tests.\n\n## Release\nRun the tests.\n");
}

// PDC-FR-WCXD: a deletion with `lead` removes `before` alone and keeps `lead`.
#[test]
fn a_deletion_with_a_lead_removes_only_its_own_text() {
    let del = ProposedHunk::from_fields(
        Some("Run the tests.\n".into()),
        None,
        None,
        Some("## Release\n".into()),
        None,
        None,
    );
    assert_eq!(del.kind, HunkKind::Del);
    let out = build(REPEATED, &[del]).expect("planned");
    assert_eq!(out, "## Setup\nRun the tests.\n\n## Release\n");
}

// PDC-FR-WCXD: an insertion finds `lead` + `after_text` + `trail` and goes in
// after `after_text`; `lead` and `trail` stay as they are.
#[test]
fn an_insertion_stands_after_after_text_between_lead_and_trail() {
    let add = ProposedHunk::from_fields(
        None,
        Some("Check the logs.\n".into()),
        Some("Run the tests.\n".into()),
        Some("## Setup\n".into()),
        Some("\n## Release".into()),
        None,
    );
    assert_eq!(add.kind, HunkKind::Add);
    let out = build(REPEATED, &[add]).expect("planned");
    assert_eq!(out, "## Setup\nRun the tests.\nCheck the logs.\n\n## Release\nRun the tests.\n");
}

// PDC-FR-YSKV, PPC-FR-RXCS: a repeated text with neither `lead` nor `trail` is
// the not-unique refusal, and so is one whose context still repeats. Both name
// the hunk's place.
#[test]
fn a_repeated_text_is_refused_with_its_place_with_or_without_context() {
    let first = replace("## Setup", "## Install", None, None);
    let bare = replace("Run the tests.", "x", None, None);
    assert_eq!(build(REPEATED, &[first.clone(), bare]), Err(PlanError::Ambiguous(2)));
    let still = replace("Run the tests.", "x", Some("\n"), Some("\n"));
    assert_eq!(build(REPEATED, &[first, still]), Err(PlanError::Ambiguous(2)));
}

// PDC-FR-WCXD: a `lead` that is not next to the text means the run is not in the
// file, which is the not-found refusal.
#[test]
fn a_lead_that_does_not_abut_the_text_is_not_found() {
    let wrong = replace("Run the tests.", "x", Some("## Nowhere\n"), None);
    assert_eq!(build(REPEATED, &[wrong]), Err(PlanError::AnchorLost(1)));
}

// PDC-FR-WCXD: overlapping occurrences count, so a run that repeats over itself
// is not unique.
#[test]
fn overlapping_occurrences_count_as_more_than_one() {
    assert_eq!(build("ababa", &[replace("aba", "X", None, None)]), Err(PlanError::Ambiguous(1)));
    assert_eq!(anchors::occurrences("ababa", "aba"), 2);
    assert_eq!(find_once("ababa", "aba"), Occurrence::Many);
    assert_eq!(find_once("xabay", "aba"), Occurrence::Once(1));
    assert_eq!(find_once("xyz", "aba"), Occurrence::None);
    assert_eq!(find_once("xyz", ""), Occurrence::None);
}

// PDC-FR-DMTH, PPC-FR-RJCA: the bound is 200 characters, counted as characters
// and not bytes, and a longer `lead` or `trail` is refused with its place.
#[test]
fn lead_and_trail_hold_at_most_two_hundred_characters() {
    // 200 two-byte characters: 400 bytes and still within the bound.
    let lead = "é".repeat(LEAD_TRAIL_BOUND);
    let text = format!("{lead}target");
    let at_bound = replace("target", "done", Some(&lead), None);
    assert_eq!(build(&text, &[at_bound]).expect("at the bound"), format!("{lead}done"));

    let over = "é".repeat(LEAD_TRAIL_BOUND + 1);
    let text = format!("head {over}target{over}");
    let long_lead = replace("target", "done", Some(&over), None);
    assert_eq!(build(&text, &[long_lead]), Err(PlanError::ContextTooLong(1, ContextSide::Lead)));
    let first = replace("head", "HEAD", None, None);
    let long_trail = replace("target", "done", None, Some(&over));
    assert_eq!(
        build(&text, &[first, long_trail]),
        Err(PlanError::ContextTooLong(2, ContextSide::Trail)),
    );
    let both = replace("target", "done", Some(&over), Some(&over));
    assert_eq!(build(&text, &[both]), Err(PlanError::ContextTooLong(1, ContextSide::Both)));
}

// PDC-FR-DMTH, PPC-FR-KWQB: the side survives the colon of the revision's
// error code, and no other text reads as a side.
#[test]
fn a_context_side_reads_back_from_its_name() {
    for side in [ContextSide::Lead, ContextSide::Trail, ContextSide::Both] {
        assert_eq!(ContextSide::parse(side.as_str()), Some(side));
    }
    assert_eq!(ContextSide::parse("Lead"), None);
    assert_eq!(ContextSide::parse(""), None);
}

// PDC-FR-WCXD: line endings are normalised on both sides, so a `lead` read from
// a CRLF file finds its text.
#[test]
fn a_crlf_file_and_a_crlf_lead_resolve_after_normalisation() {
    let crlf = REPEATED.replace('\n', "\r\n");
    let out = build(
        &crlf,
        &[replace("Run the tests.", "Run the full suite.", Some("## Release\r\n"), None)],
    )
    .expect("planned");
    assert_eq!(out, "## Setup\nRun the tests.\n\n## Release\nRun the full suite.\n");
}

// DCP-FR-JGCD: two hunks whose runs differ but whose `before` ranges overlap are
// refused, and the refusal names the later one.
#[test]
fn hunks_that_overlap_are_refused_at_the_later_place() {
    let a = replace("Run the tests.", "x", Some("## Release\n"), None);
    let b = replace("the tests.\n", "y", Some("## Release\nRun "), None);
    assert_eq!(build(REPEATED, &[a, b]), Err(PlanError::Overlap(2)));
}

// DCP-FR-JGCD: the stored anchor is still computed from the prompt, bounded by
// the neighbours, and not copied from the `lead` the caller gave.
#[test]
fn the_stored_anchor_comes_from_the_prompt_and_not_from_the_argument() {
    let doc = plan(REPEATED, &[replace("Run the tests.", "x", Some("## Release\n"), None)], ids())
        .expect("planned");
    let anchor = &doc.hunks[0].anchor;
    assert_eq!(anchor.lead, "## Setup\nRun the tests.\n\n## Release\n");
    assert_eq!(anchor.trail, "\n");
    assert_eq!(&REPEATED[anchor.hint_start..anchor.hint_end], "Run the tests.");
}

fn insert(after_text: &str, after: &str) -> ProposedHunk {
    ProposedHunk::from_fields(None, Some(after.into()), Some(after_text.into()), None, None, None)
}

// DCP-FR-JGCD, PPC-FR-DLHS: an insertion that only its neighbour placed has no
// context left once the neighbour's text is cut away, so it is refused at its
// place instead of being recorded as a change that cannot be applied.
#[test]
fn an_insertion_with_no_context_left_is_refused_at_its_place() {
    let out = build("A\nB", &[replace("B", "C", None, None), insert("B", "D")]);
    assert_eq!(out, Err(PlanError::Overlap(2)));
}

// DCP-FR-JGCD, PPC-FR-DLHS: a `lead` that is the neighbour's own text is cut
// away, and the repeated text it named can then match the wrong copy. The
// change is refused at its place, and the first copy is never edited.
#[test]
fn a_lead_cut_away_by_its_neighbour_is_refused_at_its_place() {
    let out = build(
        "foo X foo",
        &[replace("X ", "LONGER TEXT ", None, None), replace("foo", "bar", Some("X "), None)],
    );
    assert_eq!(out, Err(PlanError::Overlap(2)));
}

// PDC-FR-DMTH, PPC-FR-KWQB: at the bound a context is accepted, and the side
// named is the side that is over it, whatever the other side holds.
#[test]
fn the_side_named_is_the_side_over_the_bound() {
    let at = "é".repeat(LEAD_TRAIL_BOUND);
    let over = "é".repeat(LEAD_TRAIL_BOUND + 1);
    let text = format!("target{at}");
    let trail_at_bound = replace("target", "done", None, Some(&at));
    assert_eq!(build(&text, &[trail_at_bound]).expect("at the bound"), format!("done{at}"));
    let trail_over = replace("target", "done", Some(&at), Some(&over));
    assert_eq!(build(&text, &[trail_over]), Err(PlanError::ContextTooLong(1, ContextSide::Trail)));
    let lead_over = replace("target", "done", Some(&over), Some(&at));
    assert_eq!(build(&text, &[lead_over]), Err(PlanError::ContextTooLong(1, ContextSide::Lead)));
}

// PDC-FR-DMTH: the bound is checked before the text is searched, so a long
// context is named even where `before` is not in the text.
#[test]
fn a_long_context_is_named_before_the_text_is_searched() {
    let over = "x".repeat(LEAD_TRAIL_BOUND + 1);
    let absent = replace("not in the text", "x", Some(&over), None);
    assert_eq!(build("some text", &[absent]), Err(PlanError::ContextTooLong(1, ContextSide::Lead)),);
}
