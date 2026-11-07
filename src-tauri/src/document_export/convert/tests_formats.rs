//! Tests of what each writer puts in its file (`DXP-document-export.md`
//! DXP-FR-CPPT, DXP-FR-YVIV, DXP-FR-ERZY, DXP-FR-RPPW, DXP-FR-IJOH).
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

// DXP-FR-YVIV: each break level has its own height in the DOCX: 12, 8, 5,
// and 3 body lines.
#[test]
fn docx_spacers_have_the_height_of_each_level() {
    let xml = docx_xml(
        "a\n\n<!-- % -->\nb\n\n<!-- %% -->\nc\n\n<!-- %%% -->\nd\n\n<!-- %%%% -->\ne\n",
        false,
    );
    for twips in [3036, 2024, 1265, 759] {
        assert!(xml.contains(&format!(r#"w:line="{twips}""#)), "no spacer of {twips}: {xml}");
    }
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
