use super::*;

// CVL-FR-XMGI: the bound is the free room before the compaction mark —
// (three quarters of the budget less the estimate) × 4 characters.
#[test]
fn the_result_bound_is_the_free_room() {
    // 200,000 tokens: the mark is 150,000, so 150,000 free tokens.
    assert_eq!(result_bound(200_000, 0), 600_000);
    assert_eq!(result_bound(200_000, 100_000), 200_000);
    // A 1,000,000-token window with 50,000 tokens in use.
    assert_eq!(result_bound(1_000_000, 50_000), 2_800_000);
    // The whole part of three quarters.
    assert_eq!(result_bound(1_010, 0), 757 * 4);
}

// CVL-FR-XMGI: the bound is never below the minimum bound — the whole part of
// `context_size × 2 / 100 × 4` characters — also at or past the mark.
#[test]
fn the_result_bound_has_a_floor() {
    assert_eq!(minimum_bound(200_000), 16_000);
    assert_eq!(minimum_bound(1_010), 80);
    assert_eq!(minimum_bound(0), 0);
    // 4,000 free tokens give 16,000 characters, which is the floor exactly.
    assert_eq!(result_bound(200_000, 146_000), 16_000);
    // 3,999 free tokens fall below it.
    assert_eq!(result_bound(200_000, 146_001), 16_000);
    assert_eq!(result_bound(200_000, 145_999), 16_004);
    // At and past the mark.
    assert_eq!(result_bound(200_000, 150_000), 16_000);
    assert_eq!(result_bound(200_000, 900_000), 16_000);
    assert_eq!(result_bound(u64::MAX, u64::MAX), minimum_bound(u64::MAX));
}

/// Assert a cut result: at most `bound` characters, the start of `whole`,
/// and the line that names what it kept. Returns the kept characters.
fn assert_cut(
    text: &str,
    cut: Option<(usize, usize)>,
    whole: &str,
    bound: usize,
    hint: &str,
) -> usize {
    let (all, kept) = cut.expect("cut");
    assert_eq!(all, whole.chars().count());
    assert!(text.chars().count() <= bound, "{} > {bound}", text.chars().count());
    let start: String = whole.chars().take(kept).collect();
    assert_eq!(
        text,
        format!("{start}\n\n[The result was cut at {kept} of {all} characters.{hint}]")
    );
    kept
}

const FILE_HINT: &str = " Read the rest in parts with offset and limit.";

// CVL-FR-XMGI: a text at the bound stands as it is; one character more is cut
// and, with its line, stays within the bound.
#[test]
fn a_result_is_cut_only_above_the_bound() {
    let at = "a".repeat(1_000);
    assert_eq!(cap_result("search_specifications", at.clone(), 1_000, false), (at, None));
    let above = format!("{}b", "a".repeat(1_000));
    let (text, cut) = cap_result("search_specifications", above.clone(), 1_000, false);
    let kept = assert_cut(&text, cut, &above, 1_000, "");
    assert!(kept > 900 && !text.contains('b'));
}

// CVL-FR-ZJLK: the fixed line names the range arguments of the read tools,
// and no range for every other tool.
#[test]
fn the_cut_line_names_the_range_arguments_of_the_tool() {
    let whole = "z".repeat(1_000);
    for (tool, hint) in [
        (crate::tools::file_read::NAME, FILE_HINT),
        (crate::tools::draft_read::NAME, FILE_HINT),
        (
            crate::tools::document_get::NAME,
            " Read the rest in parts with line_offset and line_limit.",
        ),
        ("mcp__tracker__list", ""),
    ] {
        let (text, cut) = cap_result(tool, whole.clone(), 500, false);
        assert_cut(&text, cut, &whole, 500, hint);
    }
}

// CVL-FR-XMGI: the bound counts characters, so text in any script is cut
// where it reads.
#[test]
fn the_cut_counts_characters_not_bytes() {
    let whole = "ж".repeat(1_000);
    let (text, cut) = cap_result("read_file", whole.clone(), 500, false);
    assert_cut(&text, cut, &whole, 500, FILE_HINT);
}

// CVL-FR-UOKM: the result of `load_skill` is never cut.
#[test]
fn a_skill_is_never_cut() {
    let body = "s".repeat(1_000);
    assert_eq!(cap_result(crate::tools::skill_load::NAME, body.clone(), 10, false), (body, None));
}

// CVL-FR-QJLU, CVL-FR-ZJLK: a result cut a second time keeps the size of the
// whole result in its line and one line only; a cut result that fits a later
// bound stands as it was.
#[test]
fn a_second_cut_names_the_whole_result() {
    let whole = "q".repeat(10_000);
    let (first, _) = cap_result("read_file", whole.clone(), 4_000, false);
    let (same, none) = cap_result("read_file", first.clone(), 4_000, true);
    assert_eq!((same, none), (first.clone(), None));
    let (second, cut) = cap_result("read_file", first, 1_000, true);
    assert_cut(&second, cut, &whole, 1_000, FILE_HINT);
    assert_eq!(second.matches("[The result was cut at").count(), 1);
}

// CVL-FR-XMGI: outside a recovery, a text that ends with a line like the cut
// line is a result like any other: below the bound it stands byte for byte,
// and above it the line names its own size.
#[test]
fn a_text_that_looks_cut_is_read_as_it_is() {
    let looks = format!("{}\n\n[The result was cut at 5 of 99999 characters.]", "f".repeat(100));
    assert_eq!(cap_result("read_file", looks.clone(), 1_000, false), (looks.clone(), None));
    let (text, cut) = cap_result("read_file", looks.clone(), 100, false);
    assert_eq!(cut.unwrap().0, looks.chars().count());
    assert!(!text.contains("of 99999 characters"));
}
