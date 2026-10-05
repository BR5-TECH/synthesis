//! Snippet bounding around the match (SCC-FR-07).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// Snippet bounding (SCC-FR-07)
// -----------------------------------------------------------------------

#[test]
fn a_snippet_is_a_window_around_the_match_not_the_head_of_the_line() {
    // The behaviour the overlay depends on: a snippet is one clipped line,
    // so a match that sits late in an ordinary source line must still be
    // near the START of what is rendered. Anchoring on the line instead
    // would show the user a result whose reason for matching is off-screen.
    let line = format!(
        "{}needle{}",
        "lead ".repeat(40),   // 200 chars before the match
        " and some trailing context after it",
    );
    let snippet = snippet_for(&line, 200, 6);

    assert!(snippet.contains("needle"), "{snippet}");
    let at = snippet.find("needle").expect("the match is in the snippet");
    let before = snippet[..at].chars().count();
    assert!(
        before <= SNIPPET_LEAD_CHARS + 1, // +1 for the leading ellipsis
        "the match sits {before} chars in; it must stay near the head so a \
         clipped single-line render still shows it"
    );
    assert!(snippet.starts_with('…'), "the cut is marked: {snippet}");
    // The budget is spent on what FOLLOWS the match, which is the context
    // that actually explains the hit.
    assert!(snippet.contains("and some trailing context"), "{snippet}");
}

#[test]
fn a_match_at_the_end_of_a_long_line_is_not_pushed_out_of_view() {
    // Filling the window backwards from the end of the line would put the
    // match at the far right — exactly what the anchoring exists to avoid.
    // A shorter snippet with the match visible is the better answer.
    let line = format!("{}needle", "x".repeat(500));
    let snippet = snippet_for(&line, 500, 6);

    let at = snippet.find("needle").expect("the match is in the snippet");
    let before = snippet[..at].chars().count();
    assert!(
        before <= SNIPPET_LEAD_CHARS + 1,
        "the match is {before} chars into the snippet: {snippet}"
    );
    assert!(snippet.ends_with("needle"), "{snippet}");
}

#[test]
fn a_snippet_is_bounded_and_always_retains_the_match() {
    let short = "a short line with needle in it";
    assert_eq!(snippet_for(short, 18, 6), short, "a short line is verbatim");

    // A match far along a very long line must survive the bounding.
    let long = format!("{}needle{}", "x".repeat(4000), "y".repeat(4000));
    let snippet = snippet_for(&long, 4000, 6);
    assert!(
        snippet.chars().count() <= SNIPPET_MAX_CHARS + 2,
        "bounded (plus at most two ellipses), got {}",
        snippet.chars().count()
    );
    assert!(snippet.contains("needle"), "the match is always retained: {snippet}");

    // A match at the very start keeps its leading context rather than
    // producing a window that begins before the line.
    let head = format!("needle{}", "z".repeat(4000));
    assert!(snippet_for(&head, 0, 6).starts_with("needle"));
}

#[test]
fn a_case_insensitive_match_offset_survives_multibyte_lowercasing() {
    // `İ` (U+0130) lowercases to two chars, so a byte offset taken in the
    // lowered copy does not index the original. Reusing it naively would
    // slice mid-character and panic — on a real file, in a worker thread.
    let line = "İİİ needle here";
    let matcher = Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap();
    let (start, len) = matcher.find(line).expect("the literal is present");
    assert_eq!(len, 6);
    let start_chars = line[..start].chars().count();
    assert_eq!(&line[start..start + 6], "needle");
    assert_eq!(snippet_for(line, start_chars, len), line);
}
