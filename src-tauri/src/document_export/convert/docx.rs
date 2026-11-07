//! The `docx` writer of an export, built on `docx-rs` (DXP-FR-CPPT).
//!
//! It writes the block model as Word paragraphs and tables. A top-level
//! alignment is the paragraph alignment of a text block and the table
//! alignment of a box (DXP-FR-ERZY). A spacer is an empty paragraph of exact
//! height (DXP-FR-YVIV).

use docx_rs::{
    AlignmentType, Docx, Hyperlink, HyperlinkType, LineSpacing, LineSpacingType, Paragraph, Pic,
    Run, RunFonts, Shading, Table, TableAlignmentType, TableCell, TableRow, WidthType,
};

use super::model::{plain_text, Align, Block, Document, Inline, ListItem};
use super::{Assets, Picture};

/// The body text size, in half-points (11 pt).
const BODY_HALF_POINTS: usize = 22;
/// DXP-FR-YVIV: one body line height, in twentieths of a point (11 pt × 1.15).
const BODY_LINE_TWIPS: i32 = 253;
/// The text width of the default page, in EMU (6 inches).
const TEXT_WIDTH_EMU: u32 = 5_486_400;
/// EMU per pixel at 96 dpi.
const EMU_PER_PX: u32 = 9_525;
const INDENT_TWIPS: i32 = 360;
const MONO: &str = "Courier New";

/// DXP-FR-CPPT: the bytes of the `.docx` file of `doc`.
pub fn write(doc: &Document, assets: &Assets) -> Result<Vec<u8>, String> {
    let mut docx = Docx::new();
    for top in &doc.blocks {
        for part in block_parts(&top.block, top.align, 0, assets) {
            docx = match part {
                Part::Paragraph(p) => docx.add_paragraph(*p),
                Part::Table(t) => docx.add_table(*t),
            };
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    docx.build().pack(&mut out).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

enum Part {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

fn para(p: Paragraph) -> Part {
    Part::Paragraph(Box::new(p))
}

fn paragraph_align(align: Align) -> Option<AlignmentType> {
    match align {
        Align::Left => None,
        Align::Center => Some(AlignmentType::Center),
        Align::Right => Some(AlignmentType::Right),
    }
}

fn aligned(p: Paragraph, align: Align) -> Paragraph {
    match paragraph_align(align) {
        Some(a) => p.align(a),
        None => p,
    }
}

fn block_parts(block: &Block, align: Align, depth: i32, assets: &Assets) -> Vec<Part> {
    let indent = |p: Paragraph| {
        if depth > 0 {
            p.indent(Some(depth * INDENT_TWIPS), None, None, None)
        } else {
            p
        }
    };
    match block {
        Block::Heading { level, inlines } => {
            let size = match level {
                1 => 40,
                2 => 32,
                3 => 28,
                4 => 26,
                5 => 24,
                _ => 22,
            };
            let style = RunStyle { bold: true, size: Some(size), ..RunStyle::default() };
            let p = add_inlines(Paragraph::new().keep_next(true), inlines, style, assets);
            vec![para(indent(aligned(p, align)))]
        }
        Block::Paragraph(inlines) => {
            let p = add_inlines(Paragraph::new(), inlines, RunStyle::default(), assets);
            vec![para(indent(aligned(p, align)))]
        }
        Block::Code { text, .. } => vec![code_box(text, align)],
        Block::Diagram { source, png, .. } => match png {
            Some(bytes) => match super::diagrams::png_size(bytes) {
                Some((w, h)) => {
                    let picture = Picture {
                        png: bytes.clone(),
                        width: (w as f32 / super::diagrams::RASTER_SCALE) as u32,
                        height: (h as f32 / super::diagrams::RASTER_SCALE) as u32,
                    };
                    let run = Run::new().add_image(pic(&picture));
                    vec![para(indent(aligned(Paragraph::new().add_run(run), align)))]
                }
                None => vec![code_box(source, align)],
            },
            None => vec![code_box(source, align)],
        },
        Block::List { start, items } => list_parts(*start, items, align, depth, assets),
        Block::Quote(inner) => inner
            .iter()
            .flat_map(|b| block_parts(b, align, depth + 1, assets))
            .map(|part| match part {
                Part::Paragraph(p) => para(p.color("555555")),
                other => other,
            })
            .collect(),
        Block::Table { aligns, header, rows } => {
            vec![Part::Table(Box::new(table(aligns, header, rows, align, assets)))]
        }
        Block::Rule => {
            let run = Run::new().add_text("—".repeat(24)).color("999999");
            vec![para(Paragraph::new().add_run(run).align(AlignmentType::Center))]
        }
        Block::Spacer { lines } => {
            let height = BODY_LINE_TWIPS * (*lines as i32);
            let spacing = LineSpacing::new().line_rule(LineSpacingType::Exact).line(height);
            vec![para(Paragraph::new().line_spacing(spacing))]
        }
        Block::Html(text) => text
            .lines()
            .map(|line| {
                let run = mono(Run::new().add_text(line));
                para(indent(aligned(Paragraph::new().add_run(run), align)))
            })
            .collect(),
        Block::Metadata(rows) => vec![Part::Table(Box::new(metadata_table(rows)))],
    }
}

fn list_parts(
    start: Option<u64>,
    items: &[ListItem],
    align: Align,
    depth: i32,
    assets: &Assets,
) -> Vec<Part> {
    let mut parts = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let marker = match (item.task, start) {
            (Some(true), _) => "☑ ".to_string(),
            (Some(false), _) => "☐ ".to_string(),
            (None, Some(n)) => format!("{}. ", n + i as u64),
            (None, None) => "• ".to_string(),
        };
        let mut first = true;
        for block in &item.blocks {
            match block {
                Block::Paragraph(inlines) if first => {
                    let p = Paragraph::new().add_run(Run::new().add_text(marker.clone()));
                    let p = add_inlines(p, inlines, RunStyle::default(), assets).indent(
                        Some((depth + 1) * INDENT_TWIPS),
                        None,
                        None,
                        None,
                    );
                    parts.push(para(aligned(p, align)));
                }
                other => parts.extend(block_parts(other, align, depth + 1, assets)),
            }
            first = false;
        }
        if item.blocks.is_empty() {
            let p = Paragraph::new().add_run(Run::new().add_text(marker));
            parts.push(para(p.indent(Some((depth + 1) * INDENT_TWIPS), None, None, None)));
        }
    }
    parts
}

/// DXP-FR-ERZY: a code block is a box, so an alignment moves the box and the
/// code text stays left-aligned.
fn code_box(text: &str, align: Align) -> Part {
    let mut cell = TableCell::new().shading(Shading::new().fill("F3F3F3"));
    for line in text.split('\n') {
        cell = cell.add_paragraph(Paragraph::new().add_run(mono(Run::new().add_text(line))));
    }
    let table = Table::new(vec![TableRow::new(vec![cell])]);
    Part::Table(Box::new(table_align(table, align)))
}

fn table_align(table: Table, align: Align) -> Table {
    match align {
        Align::Left => table,
        Align::Center => table.align(TableAlignmentType::Center),
        Align::Right => table.align(TableAlignmentType::Right),
    }
}

fn table(
    aligns: &[Align],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    align: Align,
    assets: &Assets,
) -> Table {
    let cell = |inlines: &[Inline], col: usize, bold: bool| {
        let style = RunStyle { bold, ..RunStyle::default() };
        let p = add_inlines(Paragraph::new(), inlines, style, assets);
        let p = aligned(p, aligns.get(col).copied().unwrap_or(Align::Left));
        TableCell::new().add_paragraph(p)
    };
    let mut table_rows =
        vec![TableRow::new(header.iter().enumerate().map(|(i, c)| cell(c, i, true)).collect())];
    for row in rows {
        table_rows
            .push(TableRow::new(row.iter().enumerate().map(|(i, c)| cell(c, i, false)).collect()));
    }
    table_align(Table::new(table_rows), align)
}

/// DXP-FR-RPPW: the key in bold and the value in normal weight.
fn metadata_table(rows: &[(String, String)]) -> Table {
    let rows = rows
        .iter()
        .map(|(key, value)| {
            let key_cell = TableCell::new()
                .shading(Shading::new().fill("F3F3F3"))
                .add_paragraph(Paragraph::new().add_run(Run::new().add_text(key.as_str()).bold()));
            let mut value_cell = TableCell::new();
            for line in value.split('\n') {
                value_cell =
                    value_cell.add_paragraph(Paragraph::new().add_run(Run::new().add_text(line)));
            }
            TableRow::new(vec![key_cell, value_cell])
        })
        .collect();
    Table::new(rows).width(TEXT_WIDTH_EMU as usize / 635, WidthType::Dxa)
}

#[derive(Clone, Copy, Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    size: Option<usize>,
}

fn styled(text: &str, style: RunStyle) -> Run {
    let mut run = Run::new().add_text(text).size(style.size.unwrap_or(BODY_HALF_POINTS));
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if style.code {
        run = mono(run).shading(Shading::new().fill("F3F3F3"));
    }
    run
}

fn mono(run: Run) -> Run {
    run.fonts(RunFonts::new().ascii(MONO).hi_ansi(MONO))
}

fn add_inlines(
    mut p: Paragraph,
    inlines: &[Inline],
    style: RunStyle,
    assets: &Assets,
) -> Paragraph {
    for inline in inlines {
        p = match inline {
            Inline::Text(t) => p.add_run(styled(t, style)),
            Inline::Code(t) => p.add_run(styled(t, RunStyle { code: true, ..style })),
            Inline::Emphasis(c) => add_inlines(p, c, RunStyle { italic: true, ..style }, assets),
            Inline::Strong(c) => add_inlines(p, c, RunStyle { bold: true, ..style }, assets),
            Inline::Strike(c) => add_inlines(p, c, RunStyle { strike: true, ..style }, assets),
            Inline::Link { url, children } => {
                let run = styled(&plain_text(children), style).color("0563C1").underline("single");
                p.add_hyperlink(Hyperlink::new(url.as_str(), HyperlinkType::External).add_run(run))
            }
            Inline::Image { url, alt } => match assets.images.get(url) {
                Some(picture) => p.add_run(Run::new().add_image(pic(picture))),
                None => p.add_run(styled(&format!("[{alt}]"), RunStyle { italic: true, ..style })),
            },
            Inline::SoftBreak => p.add_run(styled(" ", style)),
            Inline::HardBreak => p.add_run(Run::new().add_break(docx_rs::BreakType::TextWrapping)),
        };
    }
    p
}

/// A picture at its natural size, scaled down to the text width.
fn pic(picture: &Picture) -> Pic {
    // u64, so a very wide picture can not overflow the EMU size.
    let mut w = u64::from(picture.width.max(1)) * u64::from(EMU_PER_PX);
    let mut h = u64::from(picture.height.max(1)) * u64::from(EMU_PER_PX);
    if w > u64::from(TEXT_WIDTH_EMU) {
        h = h * u64::from(TEXT_WIDTH_EMU) / w;
        w = u64::from(TEXT_WIDTH_EMU);
    }
    let w = u32::try_from(w).unwrap_or(TEXT_WIDTH_EMU);
    let h = u32::try_from(h).unwrap_or(u32::MAX);
    Pic::new_with_dimensions(picture.png.clone(), picture.width.max(1), picture.height.max(1))
        .size(w, h)
}
