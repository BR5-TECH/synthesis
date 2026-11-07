//! Tests of the fonts and the typography of an export
//! (`DXP-document-export.md` DXP-FR-FNTP, DXP-FR-OSRE, DXP-FR-ODVI,
//! DXP-FR-RBAQ).

use std::io::Read;

use super::pdf::export_style;
use super::tests::{document_xml, render_ok, run};
use super::typography as ty;
use super::Format;

const SAMPLE: &str = "# Title

Body with `code` here.

## Second

### Third

#### Fourth

- one
- two

```
let x = 1;
```

> quoted

---

After.
";

fn docx_part(text: &str, part: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let (converted, _) = run(text, Format::Docx, false, &render_ok, dir.path());
    if part == "word/document.xml" {
        return document_xml(&converted.bytes);
    }
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(&converted.bytes[..])).expect("a zip");
    let mut file = archive.by_name(part).expect("the part");
    let mut xml = String::new();
    file.read_to_string(&mut xml).unwrap();
    xml
}

/// The paragraph of `document` whose text holds `text`.
fn paragraph_with<'a>(document: &'a str, text: &str) -> &'a str {
    let at = document.find(&format!(">{text}<")).unwrap_or_else(|| panic!("no {text:?}"));
    let start = document[..at].rfind("<w:p ").expect("a paragraph");
    let end = at + document[at..].find("</w:p>").expect("the paragraph end");
    &document[start..end]
}

// DXP-FR-OSRE, DXP-FR-ODVI: the document defaults are Arial for every script,
// 11 pt, a line height of 1.5, and 9 pt after a paragraph.
#[test]
fn docx_defaults_are_arial_at_the_body_size_and_spacing() {
    let styles = docx_part(SAMPLE, "word/styles.xml");
    let defaults = &styles[styles.find("<w:docDefaults>").expect("defaults")..];
    for attr in
        [r#"w:ascii="Arial""#, r#"w:hAnsi="Arial""#, r#"w:cs="Arial""#, r#"w:eastAsia="Arial""#]
    {
        assert!(defaults.contains(attr), "{attr} in {defaults}");
    }
    assert!(defaults.contains(r#"<w:sz w:val="22" />"#), "{defaults}");
    assert!(
        defaults
            .contains(r#"<w:spacing w:before="0" w:after="180" w:line="360" w:lineRule="auto" />"#),
        "{defaults}"
    );
    assert!(!styles.contains("Times New Roman"));
}

// DXP-FR-ODVI: each heading level has its size, its space before and after,
// and bold.
#[test]
fn docx_headings_have_their_size_and_space() {
    let document = docx_part(SAMPLE, "word/document.xml");
    let cases = [("Title", 41, 0, 240), ("Second", 32, 360, 180), ("Third", 26, 300, 120)];
    for (text, size, before, after) in cases {
        let p = paragraph_with(&document, text);
        assert!(p.contains(&format!(r#"<w:sz w:val="{size}" />"#)), "{text}: {p}");
        assert!(p.contains(&format!(r#"w:before="{before}" w:after="{after}""#)), "{text}: {p}");
        assert!(p.contains("<w:b />"), "{text} is bold: {p}");
    }
    let fourth = paragraph_with(&document, "Fourth");
    assert!(fourth.contains(r#"<w:sz w:val="24" />"#), "{fourth}");
}

// DXP-FR-RBAQ: 3 pt between list items and 9 pt after the list.
#[test]
fn docx_lists_have_a_gap_between_items_and_space_after() {
    let document = docx_part(SAMPLE, "word/document.xml");
    assert!(paragraph_with(&document, "one").contains(r#"w:after="60""#));
    assert!(paragraph_with(&document, "two").contains(r#"w:after="180""#));
}

// DXP-FR-RBAQ: a nested list does not end its parent list. Only the last
// item of the outer list has the space after the list.
#[test]
fn docx_a_nested_list_keeps_the_item_gap() {
    let document = docx_part("- outer\n  - inner\n- last\n", "word/document.xml");
    assert!(paragraph_with(&document, "outer").contains(r#"w:after="60""#));
    assert!(paragraph_with(&document, "inner").contains(r#"w:after="60""#));
    assert!(paragraph_with(&document, "last").contains(r#"w:after="180""#));
}

// DXP-FR-OSRE, DXP-FR-RBAQ: code block text is Courier New at 10 pt with a
// line height of 1.4. Inline code is Courier New at the size around it.
#[test]
fn docx_code_is_courier_new() {
    let document = docx_part(SAMPLE, "word/document.xml");
    let block = paragraph_with(&document, "let x = 1;");
    assert!(block.contains(r#"w:ascii="Courier New""#), "{block}");
    assert!(block.contains(r#"<w:sz w:val="20" />"#), "{block}");
    assert!(block.contains(r#"w:after="0" w:line="336""#), "{block}");
    let inline = paragraph_with(&document, "code");
    let run =
        &inline[inline.find(">code<").map(|i| inline[..i].rfind("<w:r>").unwrap()).unwrap()..];
    assert!(run.contains(r#"w:ascii="Courier New""#), "{run}");
    assert!(run.contains(r#"<w:sz w:val="22" />"#), "{run}");
}

// DXP-FR-RBAQ: a code block is followed by 9 pt of space, and a thematic
// break has 18 pt above and below it.
#[test]
fn docx_blocks_have_their_space_after() {
    let document = docx_part(SAMPLE, "word/document.xml");
    let after_table = &document[document.find("</w:tbl>").expect("a table")..];
    let gap = &after_table[..after_table.find("</w:p>").unwrap()];
    assert!(gap.contains(r#"w:after="0" w:line="180" w:lineRule="exact""#), "{gap}");
    let rule_at = document.find("<w:pBdr><w:bottom").expect("a rule");
    let rule = &document[document[..rule_at].rfind("<w:p ").unwrap()..];
    let rule = &rule[..rule.find("</w:p>").unwrap()];
    assert!(rule.contains(r#"w:before="360" w:after="360""#), "{rule}");
    assert!(rule.contains("<w:bottom"), "the rule is a bottom border: {rule}");
    assert!(!rule.contains("<w:top"), "the rule has no other border: {rule}");
    let quote = paragraph_with(&document, "quoted");
    assert!(quote.contains("<w:left"), "a quote has its bar: {quote}");
    assert!(!quote.contains("<w:top"), "a quote has no box: {quote}");
    assert!(quote.contains("<w:i />"), "quoted text is italic: {quote}");
}

// DXP-FR-ODVI, DXP-FR-RBAQ: the PDF style carries the same sizes and spaces.
#[test]
fn pdf_style_has_the_export_typography() {
    let style = export_style(std::path::Path::new("."));
    assert_eq!(style.paragraph.font_size_pt, ty::BODY_PT);
    assert_eq!(style.paragraph.line_height, ty::BODY_LINE_HEIGHT);
    assert_eq!(style.paragraph.margin_after_pt, 9.0);
    let sizes: Vec<_> = style.headings.iter().map(|h| h.font_size_pt).collect();
    assert_eq!(sizes, vec![20.5, 16.0, 13.0, 12.0, 11.0, 11.0]);
    let spaces: Vec<_> =
        style.headings.iter().take(3).map(|h| (h.margin_before_pt, h.margin_after_pt)).collect();
    assert_eq!(spaces, vec![(0.0, 12.0), (18.0, 9.0), (15.0, 6.0)]);
    assert!(style.headings.iter().all(|h| h.is_bold()));
    for list in [&style.list_ordered, &style.list_unordered, &style.list_task] {
        assert_eq!(list.item_spacing_tight_pt, 3.0);
        assert_eq!(list.block.margin_after_pt, 9.0);
    }
    assert_eq!(style.code_block.font_size_pt, 10.0);
    assert_eq!(style.code_block.line_height, 1.4);
    assert_eq!(style.code_block.margin_after_pt, 9.0);
    assert_eq!(style.table.margin_after_pt, 9.0);
    assert_eq!(style.blockquote.margin_after_pt, 9.0);
    assert_eq!(style.horizontal_rule.margin_before_pt, 18.0);
    assert_eq!(style.horizontal_rule.margin_after_pt, 18.0);
}

/// The names of the fonts a PDF embeds.
fn embedded_fonts(pdf: &[u8]) -> std::collections::BTreeSet<String> {
    let doc = pdf_extract::Document::load_mem(pdf).expect("a readable PDF");
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").and_then(|t| t.as_name()).is_ok_and(|n| n == b"Font"))
        .filter_map(|d| d.get(b"BaseFont").and_then(|n| n.as_name()).ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

// DXP-FR-FNTP: the PDF embeds a face for regular, bold, and italic body text
// and one for code, so bold and italic are real faces.
#[test]
fn pdf_embeds_the_bold_italic_and_code_faces() {
    let dir = tempfile::tempdir().unwrap();
    let text = "Plain, **bold**, and *italic* text with `code`.\n";
    let (converted, _) = run(text, Format::Pdf, false, &render_ok, dir.path());
    let fonts = embedded_fonts(&converted.bytes);
    assert!(fonts.len() >= 4, "regular, bold, italic, and code faces: {fonts:?}");

    let (plain, _) = run("Plain text.\n", Format::Pdf, false, &render_ok, dir.path());
    assert_eq!(embedded_fonts(&plain.bytes).len(), 1, "a plain paragraph needs one face");
}

// DXP-FR-FNTP: a code block with non-ASCII text shows its characters.
#[test]
fn pdf_code_shows_non_ascii_characters() {
    let dir = tempfile::tempdir().unwrap();
    let text = "```\nlet arrow = \"→\"; // für\n```\n";
    let (converted, _) = run(text, Format::Pdf, false, &render_ok, dir.path());
    let extracted = pdf_extract::extract_text_from_mem(&converted.bytes).expect("a readable PDF");
    assert!(extracted.contains('→'), "{extracted}");
    assert!(extracted.contains("für"), "{extracted}");
}

// DXP-FR-IJOH: a table and a code block span the text width, with one grid
// column per table column, so every reader of the file lays them out alike.
#[test]
fn docx_tables_span_the_text_width() {
    let document =
        docx_part("| a | b |\n|---|---|\n| 1 | 2 |\n\n```\ncode\n```\n", "word/document.xml");
    let widths: Vec<&str> = document.matches(r#"<w:tblW w:w="8640" w:type="dxa" />"#).collect();
    assert_eq!(widths.len(), 2, "{document}");
    assert_eq!(document.matches(r#"<w:gridCol w:w="4320" w:type="dxa" />"#).count(), 2);
    assert_eq!(document.matches(r#"<w:gridCol w:w="8640" w:type="dxa" />"#).count(), 1);
    assert!(!document.contains(r#"w:color="000000""#), "borders are light: {document}");
}
