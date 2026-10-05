//! Unit coverage for the slicing itself.

use super::*;

// ---------------------------------------------------------------------------
// Unit coverage for the slicing itself
// ---------------------------------------------------------------------------

#[test]
fn slicing_is_a_span_of_the_original_text() {
    assert_eq!(slice_lines("a\nb\nc\n", 0, None), "a\nb\nc\n");
    assert_eq!(slice_lines("a\nb\nc\n", 1, None), "b\nc\n");
    assert_eq!(slice_lines("a\nb\nc\n", 1, Some(1)), "b\n");
    assert_eq!(slice_lines("a\nb\nc\n", 3, None), "");
    assert_eq!(slice_lines("a\nb\nc\n", 9, Some(2)), "");
    assert_eq!(slice_lines("", 0, None), "");
    assert_eq!(slice_lines("", 0, Some(3)), "");
    assert_eq!(slice_lines("solo", 0, Some(5)), "solo");
    assert_eq!(slice_lines("a\r\nb\r\n", 1, Some(1)), "b\r\n");
    assert_eq!(slice_lines("a\r\nb\r\nc", 2, Some(1)), "c");
    // A lone CR is not a line break, so the whole text is one line.
    assert_eq!(slice_lines("a\rb\rc", 0, Some(1)), "a\rb\rc");
    assert_eq!(slice_lines("a\rb\rc", 1, None), "");
    assert_eq!(slice_lines("\n\n\n", 1, Some(1)), "\n");
    assert_eq!(slice_lines("a\nb\n", 1, Some(usize::MAX)), "b\n");
    assert_eq!(slice_lines("α\nβ\n", 1, Some(1)), "β\n");
    let huge = "x".repeat(200_000);
    assert_eq!(slice_lines(&huge, 0, Some(1)), huge);
    assert_eq!(slice_lines(&huge, 1, Some(1)), "");
}

#[test]
fn any_range_is_a_contiguous_span_that_reassembles_the_file() {
    // The property behind RFT-FR-10 and RFT-FR-11 together: a range is always a
    // contiguous piece of the original, and consecutive ranges rejoin into it
    // exactly — which is what "no marker, no reformatting" amounts to.
    for text in ["", "a", "a\n", "a\nb\nc", "a\r\nb\r\n", "α\nβ\nγ", "\n\n"] {
        let total = text.split_inclusive('\n').count();
        for cut in 0..=total + 1 {
            let head = slice_lines(text, 0, Some(cut.max(1)));
            let tail = slice_lines(text, cut.max(1), None);
            assert_eq!(
                format!("{head}{tail}"),
                text,
                "text {text:?} cut at {cut} must rejoin exactly",
            );
            assert!(text.contains(head), "the head is a span of the original");
        }
    }
}

#[test]
fn a_whole_file_read_returns_the_original_without_reslicing() {
    // RFT-FR-06: with both bounds absent the text is returned as-is, which is
    // what makes "no ceiling, no truncation, no elision" trivially true.
    let text = "α\nβ\nγ\n";
    assert!(std::ptr::eq(slice_lines(text, 0, None), text));
}
