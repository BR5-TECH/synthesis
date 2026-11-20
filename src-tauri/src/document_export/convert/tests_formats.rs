//! Tests of what each writer puts in its file (`DXP-document-export.md`
//! DXP-FR-CPPT, DXP-FR-YVIV, DXP-FR-SEDX, DXP-FR-RWWA, DXP-FR-ERZY,
//! DXP-FR-RPPW, DXP-FR-IJOH).
//!
//! A PDF is read back with `pdf_extract`, which the Documents collection uses
//! already, and a DOCX by its `word/document.xml`.

use super::tests::{document_xml, render_ok, run};
use super::Format;

/// The text of a PDF export of `text`.
fn pdf_text(text: &str, frontmatter: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let (converted, _) = run(text, Format::Pdf, frontmatter, &render_ok, dir.path());
    assert!(converted.bytes.starts_with(b"%PDF-"));
    pdf_extract::extract_text_from_mem(&converted.bytes).expect("the PDF reads back")
}

fn docx_xml(text: &str, frontmatter: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let (converted, _) = run(text, Format::Docx, frontmatter, &render_ok, dir.path());
    document_xml(&converted.bytes)
}

const FRONTMATTER: &str = "---\ntitle: Guide\ntags:\n  - alpha\n  - beta\n---\n# Body\n";

// DXP-FR-RPPW, DXP-FR-CPPT: the PDF metadata table has one row per key and no
// header row, and a list value keeps its lines.
#[test]
fn pdf_metadata_table_has_no_header_row_and_keeps_lines() {
    let text = pdf_text(FRONTMATTER, true);
    for needle in ["title", "Guide", "tags", "- alpha", "- beta", "Body"] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
    assert!(!text.contains("Key"), "no invented header row: {text}");
    assert!(!text.contains("Value"), "no invented header row: {text}");
    assert!(!text.contains("- alpha - beta"), "the list value keeps its line break: {text}");
}

// DXP-FR-RPPW: the DOCX metadata table has one row per key, the key in bold,
// and one paragraph per line of a list value.
#[test]
fn docx_metadata_table_has_a_row_per_key() {
    let xml = docx_xml(FRONTMATTER, true);
    assert_eq!(xml.matches("<w:tr>").count() + xml.matches("<w:tr ").count(), 2, "{xml}");
    assert!(xml.contains("- alpha") && xml.contains("- beta"));
    assert!(xml.contains("<w:b />") || xml.contains("<w:b/>"), "the key is bold");
    assert!(!xml.contains(">Key<"));
}

// DXP-FR-WAQF: the frontmatter is absent from both formats when it is not
// included.
#[test]
fn frontmatter_is_absent_when_not_included() {
    assert!(!pdf_text(FRONTMATTER, false).contains("alpha"));
    assert!(!docx_xml(FRONTMATTER, false).contains("alpha"));
}

// DXP-FR-YVIV, DXP-FR-ODVI: V2, V3, and V4 have their own heights in the
// DOCX: 10, 6, and 3 body lines of 11 pt × 1.5. V1 is no spacer.
#[test]
fn docx_spacers_have_the_height_of_each_level() {
    let xml = docx_xml(
        "a\n\n<!-- % -->\nb\n\n<!-- %% -->\nc\n\n<!-- %%% -->\nd\n\n<!-- %%%% -->\ne\n",
        false,
    );
    for twips in [3300, 1980, 990] {
        assert!(xml.contains(&format!(r#"w:line="{twips}""#)), "no spacer of {twips}: {xml}");
    }
    assert!(!xml.contains(r#"w:line="5280""#), "V1 is not a spacer: {xml}");
}

/// The number of page breaks in a DOCX body.
fn docx_page_breaks(xml: &str) -> usize {
    xml.matches(r#"w:type="page""#).count()
}

// DXP-FR-SEDX: each V1 break between blocks is one page break in the DOCX,
// also two in sequence.
#[test]
fn docx_v1_breaks_are_page_breaks() {
    assert_eq!(docx_page_breaks(&docx_xml("a\n\n<!-- % -->\nb\n", false)), 1);
    let two = docx_xml("a\n\n<!-- % -->\n<!-- % -->\nb\n\n<!-- % -->\nc\n", false);
    assert_eq!(docx_page_breaks(&two), 3, "{two}");
}

// DXP-FR-RWWA: a V1 break at the start or at the end of the DOCX makes no
// page break, and a document of V1 breaks alone has none.
#[test]
fn docx_v1_at_the_edges_makes_no_page_break() {
    let xml = docx_xml("<!-- % -->\n<!-- % -->\na\n\n<!-- % -->\n", false);
    assert_eq!(docx_page_breaks(&xml), 0, "{xml}");
    assert_eq!(docx_page_breaks(&docx_xml("<!-- % -->\n", false)), 0);
}

// DXP-FR-RWWA: with the frontmatter included, its table is a block before a
// leading V1, so that V1 makes a page break.
#[test]
fn docx_v1_after_the_metadata_table_is_a_page_break() {
    let xml = docx_xml("---\ntitle: T\n---\n<!-- % -->\nbody\n", true);
    assert_eq!(docx_page_breaks(&xml), 1, "{xml}");
}

/// The text of each page of a PDF export of `text`.
fn pdf_pages(text: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let (converted, _) = run(text, Format::Pdf, false, &render_ok, dir.path());
    pdf_extract::extract_text_from_mem_by_pages(&converted.bytes).expect("the PDF reads back")
}

// DXP-FR-SEDX: the block after a V1 break starts a new page of the PDF.
#[test]
fn pdf_v1_break_starts_a_new_page() {
    let pages = pdf_pages("First page.\n\n<!-- % -->\nSecond page.\n");
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert!(pages[0].contains("First page.") && !pages[0].contains("Second"), "{pages:?}");
    assert!(pages[1].contains("Second page."), "{pages:?}");
    assert!(!pages.concat().contains("pagebreak"), "{pages:?}");
}

// DXP-FR-RWWA: a V1 break at the start or at the end of the PDF adds no page.
#[test]
fn pdf_v1_at_the_edges_adds_no_page() {
    let pages = pdf_pages("<!-- % -->\nOnly page.\n\n<!-- % -->\n");
    assert_eq!(pages.len(), 1, "{pages:?}");
}

// DXP-FR-YVIV, DXP-FR-CPPT: a PDF spacer is empty space. Its marker text does
// not reach the page, and the text around it does.
#[test]
fn pdf_spacers_are_empty_space() {
    let text = pdf_text("Above.\n\n<!-- %% -->\nBelow.\n", false);
    assert!(text.contains("Above.") && text.contains("Below."), "{text}");
    assert!(!text.contains('%'), "{text}");
    assert!(!text.contains("<!--"), "{text}");
}

// DXP-FR-ERZY: the DOCX aligns a list and a quote that follow a marker.
#[test]
fn docx_aligns_a_list_and_a_quote() {
    let xml = docx_xml("<!-- center -->\n- centred item\n\n<!-- right -->\n> right quote\n", false);
    let center = xml.find("centred item").expect("the list");
    let right = xml.find("right quote").expect("the quote");
    let before_center = &xml[..center];
    let between = &xml[center..right];
    assert!(before_center.rfind(r#"w:val="center""#).is_some(), "{xml}");
    assert!(xml[..right].rfind(r#"w:val="right""#).is_some_and(|at| at > center), "{between}");
}

// DXP-FR-IJOH, DXP-FR-YRVE: a table of the body keeps its cells in both
// formats when the document also holds an aligned block.
#[test]
fn a_table_keeps_its_cells_beside_an_aligned_block() {
    let source =
        "<!-- center -->\nCentred words\n\n| Name | Count |\n|------|-------|\n| one  | 1     |\n";
    let text = pdf_text(source, false);
    for needle in ["Centred words", "Name", "Count", "one"] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
    let xml = docx_xml(source, false);
    assert!(xml.contains("<w:tbl>"));
    assert!(
        xml.contains("<w:tblBorders>") || xml.contains("w:tblBorders"),
        "the table has borders"
    );
}
