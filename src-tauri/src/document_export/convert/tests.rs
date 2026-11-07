//! Tests of the conversion (`DXP-document-export.md`).

use std::cell::RefCell;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::model::{classify_marker_line, parse, Align, Block, Inline, Marker};
use super::*;

fn render_ok(_: &str) -> Option<Vec<u8>> {
    let image = image::RgbaImage::new(40, 20);
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn render_fail(_: &str) -> Option<Vec<u8>> {
    None
}

fn no_image(_: &str) -> Option<Vec<u8>> {
    None
}

/// Convert `text` with the given renderer, writing pictures into `dir`.
fn run(
    text: &str,
    format: Format,
    frontmatter: bool,
    render: &dyn Fn(&str) -> Option<Vec<u8>>,
    dir: &Path,
) -> (Converted, Vec<(Stage, usize, usize)>) {
    let write_file = |name: &str, bytes: &[u8]| -> Option<PathBuf> {
        let path = dir.join(name);
        std::fs::write(&path, bytes).ok()?;
        Some(path)
    };
    let input = Input {
        text,
        format,
        include_frontmatter: frontmatter,
        load_image: &no_image,
        render_diagram: render,
        pictures_dir: dir,
        write_file: &write_file,
    };
    let events = RefCell::new(Vec::new());
    let converted =
        convert(&input, &mut |s, d, t| events.borrow_mut().push((s, d, t))).expect("converts");
    (converted, events.into_inner())
}

/// The `word/document.xml` of a `.docx`.
fn document_xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a zip");
    let mut file = archive.by_name("word/document.xml").expect("document.xml");
    let mut xml = String::new();
    file.read_to_string(&mut xml).unwrap();
    xml
}

// DXP-FR-YVIV: the four levels of a vertical break and their heights.
#[test]
fn vertical_breaks_become_spacers_of_each_level() {
    assert_eq!(classify_marker_line("<!-- % -->"), Some(Marker::Break(12)));
    assert_eq!(classify_marker_line("<!--%%-->"), Some(Marker::Break(8)));
    assert_eq!(classify_marker_line("  <!-- %%% -->  "), Some(Marker::Break(5)));
    assert_eq!(classify_marker_line("<!-- %%%%%% -->"), Some(Marker::Break(3)));
    assert_eq!(classify_marker_line("<!-- % x -->"), None);
    let doc = parse("one\n\n<!-- %% -->\ntwo\n");
    assert_eq!(doc.blocks[1].block, Block::Spacer { lines: 8 });
}

// DXP-FR-ERZY: a marker aligns the top-level block on the very next line only.
#[test]
fn alignment_marker_aligns_only_the_next_block() {
    let doc = parse("<!-- center -->\n# Title\n\nplain\n\n<!-- right -->\n\nnot aligned\n");
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(doc.blocks[0].align, Align::Center);
    assert!(matches!(doc.blocks[0].block, Block::Heading { level: 1, .. }));
    assert_eq!(doc.blocks[1].align, Align::Left);
    assert_eq!(doc.blocks[2].align, Align::Left);
    let doc = parse("<!-- right -->\nright text\n");
    assert_eq!(doc.blocks[0].align, Align::Right);
}

// DXP-FR-ANXU: other comments are dropped, and a comment in code stays code.
#[test]
fn other_comments_are_dropped_and_code_comments_stay() {
    let doc = parse("<!-- a note -->\n\ntext <!-- inline --> more\n\n`<!-- code -->`\n\n```\n<!-- block -->\n```\n");
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(doc.blocks[0].block, Block::Paragraph(vec![Inline::Text("text  more".into())]));
    assert_eq!(doc.blocks[1].block, Block::Paragraph(vec![Inline::Code("<!-- code -->".into())]));
    assert_eq!(doc.blocks[2].block, Block::Code { language: None, text: "<!-- block -->".into() });
}

// DXP-FR-RPPW: the frontmatter is a metadata table when it is included.
#[test]
fn frontmatter_renders_as_a_metadata_table_when_included() {
    let text = "---\ntitle: \"Guide\"\ntags:\n  - a\n  - b\n---\n# Body\n";
    let doc = document_of(text, true);
    assert_eq!(
        doc.blocks[0].block,
        Block::Metadata(
            vec![("title".into(), "Guide".into()), ("tags".into(), "- a\n- b".into()),]
        )
    );
    assert!(matches!(doc.blocks[1].block, Block::Heading { .. }));
}

// DXP-FR-WAQF: no table when it is not included, and a non-mapping is code.
#[test]
fn frontmatter_is_omitted_or_shown_as_text() {
    let text = "---\ntitle: Guide\n---\n# Body\n";
    let doc = document_of(text, false);
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0].block, Block::Heading { .. }));
    let doc = document_of("---\n- one\n- two\n---\nBody\n", true);
    assert_eq!(doc.blocks[0].block, Block::Code { language: None, text: "- one\n- two".into() });
    let doc = document_of("# No frontmatter\n", true);
    assert_eq!(doc.blocks.len(), 1);
}

// DXP-FR-RKJC, DXP-FR-LSYA: a diagram becomes an image, and the stages and the
// diagram counts are reported in order.
#[test]
fn diagrams_render_and_progress_follows_them() {
    let dir = tempfile::tempdir().unwrap();
    let text = "```mermaid\ngraph TD; A-->B\n```\n\n```mermaid\ngraph TD; C-->D\n```\n";
    let (converted, events) = run(text, Format::Docx, false, &render_ok, dir.path());
    assert!(converted.failed_diagrams.is_empty());
    assert_eq!(converted.diagram_count, 2);
    assert_eq!(
        events,
        vec![
            (Stage::Diagrams, 0, 2),
            (Stage::Diagrams, 1, 2),
            (Stage::Diagrams, 2, 2),
            (Stage::Converting, 0, 0),
        ]
    );
    let xml = document_xml(&converted.bytes);
    assert!(xml.contains("<pic:pic"), "the diagram is a picture");
    assert!(!xml.contains("A--&gt;B"), "its source is gone");
}

// DXP-FR-IOFM: a failed diagram stays as code and is reported by its index.
#[test]
fn a_failed_diagram_stays_as_code_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let text = "```mermaid\nnot a diagram\n```\n";
    let (converted, _) = run(text, Format::Docx, false, &render_fail, dir.path());
    assert_eq!(converted.failed_diagrams, vec![1]);
    assert!(document_xml(&converted.bytes).contains("not a diagram"));
}

// DXP-FR-RKJC: merman renders a real flowchart to a PNG, in the backend.
#[test]
fn merman_renders_a_flowchart_to_png() {
    let png = diagrams::render_png("flowchart TD\n  A[Start] --> B[Done]").expect("renders");
    assert!(diagrams::png_size(&png).is_some());
    assert!(diagrams::render_png("   ").is_none());
}

// DXP-FR-ERZY, DXP-FR-YVIV, DXP-FR-CPPT: a DOCX carries the alignment and the
// exact height of a spacer.
#[test]
fn docx_carries_alignment_and_spacers() {
    let dir = tempfile::tempdir().unwrap();
    let text = "<!-- center -->\nCentred words\n\n<!-- % -->\n<!-- right -->\nRight words\n";
    let (converted, _) = run(text, Format::Docx, false, &render_ok, dir.path());
    let xml = document_xml(&converted.bytes);
    assert!(xml.contains("Centred words"));
    assert!(
        xml.contains(r#"<w:jc w:val="center" />"#) || xml.contains(r#"<w:jc w:val="center"/>"#)
    );
    assert!(xml.contains(r#"w:val="right""#));
    assert!(xml.contains(r#"w:line="3036""#), "a V1 break is 12 body lines");
}

// DXP-FR-YRVE: the constructs of the body reach the DOCX.
#[test]
fn docx_keeps_the_body_constructs() {
    let dir = tempfile::tempdir().unwrap();
    let text = "# Head\n\n*em* **strong** ~~gone~~ `code` [link](https://x.test)\n\n- one\n- [x] two\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n";
    let (converted, _) = run(text, Format::Docx, false, &render_ok, dir.path());
    let xml = document_xml(&converted.bytes);
    for needle in ["Head", "strong", "gone", "code", "link", "one", "two", "quoted", "<w:tbl>"] {
        assert!(xml.contains(needle), "missing {needle}");
    }
}

// DXP-FR-CPPT, DXP-FR-ERZY, DXP-FR-YVIV: a PDF is written with aligned blocks,
// spacers, a diagram, and a metadata table.
#[test]
fn pdf_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let text = "---\ntitle: Guide\n---\n<!-- center -->\n# Title\n\n<!-- %% -->\nBody text.\n\n```mermaid\ngraph TD; A-->B\n```\n";
    let (converted, _) = run(text, Format::Pdf, true, &render_ok, dir.path());
    assert!(converted.bytes.starts_with(b"%PDF-"));
}
