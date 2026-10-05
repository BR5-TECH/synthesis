//! The text and its ranges (GDT-FR-NKKK, GDT-FR-HREW, GDT-FR-QTAA, GDT-FR-YXCH,
//! GDT-FR-IUFV, GDT-FR-RQZS, GDT-FR-CHWC).

use super::*;

// GDT-FR-NKKK, GDT-FR-CHWC: with no range the whole text comes back, with nothing
// added to it and no ceiling.
#[test]
fn the_whole_text_is_returned_by_default() {
    let (fixture, id) = with_document("# Title\r\n\r\nbody é ✓\n");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "# Title\r\n\r\nbody é ✓\n");

    let big = "line of text\n".repeat(300_000);
    let (fixture, id) = with_document(&big);
    assert_eq!(get(&fixture, args(&id)).unwrap().len(), big.len(), "no size ceiling");
}

// GDT-FR-NKKK: a changed file is visible on the next call, with no refresh in
// between.
#[test]
fn a_changed_file_is_visible_on_the_next_call() {
    let (fixture, id) = with_document("before");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "before");
    std::fs::write(fixture.root.join("refs/doc.md"), "after").unwrap();
    assert_eq!(get(&fixture, args(&id)).unwrap(), "after");
}

// GDT-FR-HREW: a PDF's text is the extracted text, never the bytes, both range
// forms apply to it, and a changed PDF gives the new text.
#[test]
fn a_pdf_returns_extracted_text_and_both_ranges_apply_to_it() {
    let fixture = DocFixture::new();
    let file = fixture.write("refs/paper.pdf", "first line\nsecond line\nthird line");
    fixture.select(SourceKind::Folder, "refs");
    let id = fixture.id_of("refs/paper.pdf");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "first line\nsecond line\nthird line");

    let mut bytes = args(&id);
    bytes.byte_offset = Some(11);
    bytes.byte_length = Some(11);
    assert_eq!(get(&fixture, bytes).unwrap(), "second line");
    let mut lines = args(&id);
    lines.line_offset = Some(2);
    assert_eq!(get(&fixture, lines).unwrap(), "third line");

    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&file, "replaced text").unwrap();
    assert_eq!(get(&fixture, args(&id)).unwrap(), "replaced text");
}

fn bytes(text: &str, offset: Option<i64>, length: Option<i64>) -> String {
    let (fixture, id) = with_document(text);
    let mut call = args(&id);
    call.byte_offset = offset;
    call.byte_length = length;
    get(&fixture, call).unwrap()
}

// GDT-FR-QTAA: a byte range is half-open over the UTF-8 text, with the documented
// defaults and clamps.
#[test]
fn a_byte_range_is_half_open_with_defaults_and_clamps() {
    let text = "0123456789";
    assert_eq!(bytes(text, Some(2), Some(3)), "234");
    assert_eq!(bytes(text, None, Some(4)), "0123", "offset defaults to 0");
    assert_eq!(bytes(text, Some(7), None), "789", "length defaults to the rest");
    assert_eq!(bytes(text, Some(-5), Some(2)), "01", "a negative offset is 0");
    assert_eq!(bytes(text, Some(3), Some(0)), "3", "a length below 1 is 1");
    assert_eq!(bytes(text, Some(3), Some(-9)), "3");
    assert_eq!(bytes(text, Some(8), Some(100)), "89", "past the end returns what exists");
    assert_eq!(bytes(text, Some(0), Some(i64::MAX)), text, "no overflow");
}

// GDT-FR-IUFV: an offset at or beyond the end, and an empty document, answer with
// empty text as a success.
#[test]
fn an_offset_at_or_past_the_end_is_empty_text() {
    assert_eq!(bytes("abc", Some(3), Some(1)), "");
    assert_eq!(bytes("abc", Some(99), None), "");
    assert_eq!(bytes("", Some(0), Some(5)), "");
    assert_eq!(bytes("", None, Some(5)), "");
    let (fixture, id) = with_document("");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "");
    let mut lines = args(&id);
    lines.line_offset = Some(0);
    assert_eq!(get(&fixture, lines).unwrap(), "");
}

// GDT-FR-YXCH: an edge inside a UTF-8 character widens outward, so the text is
// valid UTF-8 and holds every requested byte.
#[test]
fn a_byte_range_inside_a_character_widens_outward() {
    // `a` is 1 byte, `é` 2 bytes (1..3), `✓` 3 bytes (3..6), `😀` 4 bytes (6..10), `z` 1 byte.
    let text = "aé✓😀z";
    assert_eq!(text.len(), 11);
    // The start is inside `é`: it moves back to the first byte of `é`.
    assert_eq!(bytes(text, Some(2), Some(1)), "é");
    // The end is inside `✓`: it moves forward to the last byte of `✓`.
    assert_eq!(bytes(text, Some(0), Some(4)), "aé✓");
    // Both edges are inside characters.
    assert_eq!(bytes(text, Some(4), Some(3)), "✓😀");
    // A single byte in the middle of the emoji returns the whole emoji.
    assert_eq!(bytes(text, Some(8), Some(1)), "😀");
    // Edges on boundaries are not widened.
    assert_eq!(bytes(text, Some(1), Some(2)), "é");
    // Every possible range is valid UTF-8 holding the requested bytes.
    for offset in 0..text.len() as i64 {
        for length in 1..=text.len() as i64 {
            let got = slice_bytes(text, Some(offset), Some(length));
            let start = offset as usize;
            let end = (start + length as usize).min(text.len());
            let (got_start, got_end) = {
                let from = got.as_ptr() as usize - text.as_ptr() as usize;
                (from, from + got.len())
            };
            assert!(got_start <= start && got_end >= end, "{offset}+{length}: {got:?}");
            assert!(got_start == 0 || text.is_char_boundary(got_start));
        }
    }
}

fn lines(text: &str, offset: Option<i64>, limit: Option<i64>) -> String {
    let (fixture, id) = with_document(text);
    let mut call = args(&id);
    call.line_offset = offset;
    call.line_limit = limit;
    get(&fixture, call).unwrap()
}

// GDT-FR-RQZS: a line range is `line_limit` lines from the 0-based `line_offset`,
// keeps each line's terminator, and clamps as documented.
#[test]
fn a_line_range_keeps_terminators_and_clamps() {
    let text = "zero\none\r\ntwo\nthree";
    assert_eq!(lines(text, Some(1), Some(2)), "one\r\ntwo\n");
    assert_eq!(lines(text, None, Some(1)), "zero\n");
    assert_eq!(lines(text, Some(2), None), "two\nthree", "the rest, last line without a terminator");
    assert_eq!(lines(text, Some(-3), Some(1)), "zero\n", "a negative offset is 0");
    assert_eq!(lines(text, Some(1), Some(0)), "one\r\n", "a limit below 1 is 1");
    assert_eq!(lines(text, Some(2), Some(100)), "two\nthree");
    assert_eq!(lines(text, Some(4), Some(1)), "", "past the last line");
    assert_eq!(lines(text, Some(99), None), "");
}
