//! Tests of the comment rules of the block model (`DXP-document-export.md`
//! DXP-FR-ANXU, DXP-FR-SRHN, DXP-FR-BXIO, DXP-FR-DJZM).

use super::model::{parse, strip_comments, Block, Document, Inline};

/// The plain text of each top-level block, for blocks that hold text.
fn texts(doc: &Document) -> Vec<String> {
    doc.blocks
        .iter()
        .map(|top| match &top.block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                super::model::plain_text(inlines)
            }
            Block::Html(text) => format!("html:{text}"),
            other => format!("{other:?}"),
        })
        .collect()
}

// DXP-FR-BXIO: an `<!--` with no `-->` keeps the text from it onward.
#[test]
fn an_unclosed_comment_keeps_its_text() {
    assert_eq!(strip_comments("a <!-- b --> c"), "a  c");
    assert_eq!(strip_comments("a <!-- open\nmore"), "a <!-- open\nmore");
    assert_eq!(strip_comments("<!-- x --> y <!-- open"), " y <!-- open");
}

// DXP-FR-BXIO: an unclosed comment at the start of a line drops no text.
#[test]
fn an_unclosed_comment_at_line_start_drops_no_text() {
    let doc = parse("Before.\n\n<!-- never closed\nstill text\n\nNext paragraph.\n");
    let all = texts(&doc).join("\n");
    assert!(all.contains("Before."), "{all}");
    assert!(all.contains("never closed"), "{all}");
    assert!(all.contains("still text"), "{all}");
    assert!(all.contains("Next paragraph."), "{all}");
}

// DXP-FR-BXIO, DXP-FR-SRHN: an unclosed inline comment swallows nothing of
// its own paragraph or of the paragraphs after it.
#[test]
fn an_unclosed_inline_comment_keeps_the_paragraphs_after_it() {
    let doc = parse("Some text <!-- open\nand more\n\nNext paragraph.\n\n# Heading\n\nThird.\n");
    let all = texts(&doc);
    assert_eq!(all.len(), 4, "{all:?}");
    assert!(all[0].contains("Some text"), "{all:?}");
    assert!(all[0].contains("and more"), "{all:?}");
    assert_eq!(all[1], "Next paragraph.");
    assert_eq!(all[2], "Heading");
    assert_eq!(all[3], "Third.");
}

// DXP-FR-SRHN: a closed comment that continues on more lines of a paragraph
// is absent as a whole, and the text around it stays.
#[test]
fn a_closed_multi_line_inline_comment_is_absent() {
    let doc = parse("Start <!-- one\ntwo\nthree --> end.\n\nNext.\n");
    let all = texts(&doc);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[0].starts_with("Start"), "{all:?}");
    assert!(all[0].ends_with("end."), "{all:?}");
    assert!(!all[0].contains("two"), "{all:?}");
    assert_eq!(all[1], "Next.");
}

// DXP-FR-SRHN: a comment in a table cell ends with its cell.
#[test]
fn a_comment_in_a_table_cell_ends_with_the_cell() {
    let doc = parse("| a | b |\n|---|---|\n| x <!-- note --> | y |\n\nAfter.\n");
    let Block::Table { rows, .. } = &doc.blocks[0].block else {
        panic!("a table: {:?}", doc.blocks[0].block);
    };
    assert_eq!(super::model::plain_text(&rows[0][0]).trim(), "x");
    assert_eq!(super::model::plain_text(&rows[0][1]), "y");
    assert_eq!(texts(&doc)[1], "After.");
}

// DXP-FR-DJZM: the text after a comment on its line is a paragraph, not raw
// HTML.
#[test]
fn text_after_a_comment_on_its_line_is_a_paragraph() {
    let doc = parse("<!-- note --> Some *text*\n\nNext.\n");
    assert_eq!(doc.blocks.len(), 2, "{:?}", doc.blocks);
    assert_eq!(
        doc.blocks[0].block,
        Block::Paragraph(vec![
            Inline::Text("Some ".into()),
            Inline::Emphasis(vec![Inline::Text("text".into())]),
        ])
    );
    assert_eq!(texts(&doc)[1], "Next.");
    // Raw HTML after a comment stays raw HTML (DXP-FR-YRVE).
    let doc = parse("<!-- note --><div>box</div>\n");
    assert_eq!(doc.blocks[0].block, Block::Html("<div>box</div>".into()));
}
