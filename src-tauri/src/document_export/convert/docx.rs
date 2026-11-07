//! The `docx` writer of an export, built on `docx-rs` (DXP-FR-CPPT).
//!
//! It writes the block model as Word paragraphs and tables. A top-level
//! alignment is the paragraph alignment of a text block and the table
//! alignment of a box (DXP-FR-ERZY). A spacer is an empty paragraph of exact
//! height (DXP-FR-YVIV).
//!
//! The document names Arial for the body and Courier New for code
//! (DXP-FR-OSRE). The sizes and the spaces come from `super::typography`
//! (DXP-FR-ODVI, DXP-FR-RBAQ).

use docx_rs::{
    AlignmentType, BorderType, Docx, Hyperlink, HyperlinkType, LineSpacing, LineSpacingType,
    Paragraph, ParagraphBorder, ParagraphBorderPosition, ParagraphBorders, ParagraphChild, Pic,
    Run, RunFonts, Shading, Table, TableAlignmentType, TableBorder, TableBorderPosition, TableCell,
    TableCellMargins, TableRow, WidthType,
};

use super::model::{plain_text, Align, Block, Document, Inline, ListItem};
use super::typography as ty;
use super::{Assets, Picture};

/// DXP-FR-OSRE: the body font and the code font.
const BODY_FONT: &str = "Arial";
const MONO: &str = "Courier New";
/// The colour of the light lines: table borders, the quote bar, and a rule.
const LINE_COLOR: &str = "D0D0D0";
/// The colour of quoted text.
const MUTED_COLOR: &str = "666666";
/// The fill of a code block, a code span, and a table header.
const SUNKEN_FILL: &str = "F3F3F3";
/// The text width of the default page, in EMU (6 inches).
const TEXT_WIDTH_EMU: u32 = 5_486_400;
/// EMU per pixel at 96 dpi.
const EMU_PER_PX: u32 = 9_525;
const INDENT_TWIPS: i32 = 360;

/// The body text size, in half-points.
fn body_half_points() -> usize {
    ty::half_points(ty::BODY_PT)
}

/// DXP-FR-YVIV: one body line height, in twentieths of a point.
fn body_line_twips() -> i32 {
    (ty::BODY_PT * ty::BODY_LINE_HEIGHT * 20.0).round() as i32
}

/// The spacing of a paragraph: the space before and after it, in points, and
/// its line height as a multiple of the font size.
fn spacing(before_pt: f32, after_pt: f32, line: f32) -> LineSpacing {
    LineSpacing::new()
        .line_rule(LineSpacingType::Auto)
        .line(ty::auto_line(line))
        .before(ty::twips(before_pt))
        .after(ty::twips(after_pt))
}

/// An empty paragraph of exact height, with no space before or after it.
fn gap(height_twips: i32) -> Paragraph {
    let spacing =
        LineSpacing::new().line_rule(LineSpacingType::Exact).line(height_twips).before(0).after(0);
    Paragraph::new().line_spacing(spacing)
}

fn fonts(name: &str) -> RunFonts {
    RunFonts::new().ascii(name).hi_ansi(name).cs(name).east_asia(name)
}

/// A run of body text. Each run names its font and size, because some
/// readers of `.docx` files ignore the document defaults.
fn body_run(text: &str) -> Run {
    Run::new().add_text(text).fonts(fonts(BODY_FONT)).size(body_half_points())
}

/// A paragraph of body text, with its spacing stated on the paragraph.
fn body_paragraph() -> Paragraph {
    Paragraph::new().line_spacing(spacing(0.0, ty::PARAGRAPH_AFTER_PT, ty::BODY_LINE_HEIGHT))
}

/// The text width of the page, in twentieths of a point.
fn text_width_twips() -> usize {
    TEXT_WIDTH_EMU as usize / 635
}

/// The widths of the columns of a table at the text width, in twentieths of a
/// point, in proportion to `shares`.
fn grid(shares: &[usize]) -> Vec<usize> {
    let total: usize = shares.iter().sum::<usize>().max(1);
    shares.iter().map(|share| text_width_twips() * share / total).collect()
}

/// DXP-FR-IJOH: a table at the text width, with light borders and padded
/// cells, as the editor shows it. `columns` is its grid (`grid`); each reader
/// of the file sizes the columns from it.
fn boxed(table: Table, columns: Vec<usize>) -> Table {
    let positions = [
        TableBorderPosition::Top,
        TableBorderPosition::Left,
        TableBorderPosition::Bottom,
        TableBorderPosition::Right,
        TableBorderPosition::InsideH,
        TableBorderPosition::InsideV,
    ];
    let table = positions.into_iter().fold(table, |t, position| {
        t.set_border(TableBorder::new(position).color(LINE_COLOR).size(4))
    });
    table
        .width(text_width_twips(), WidthType::Dxa)
        .set_grid(columns)
        .margins(TableCellMargins::new().margin(60, 120, 60, 120))
}

/// A border set that holds `border` alone. `docx-rs` starts a paragraph's
/// borders as a box on all four sides, so the set starts empty.
fn only(border: ParagraphBorder) -> ParagraphBorders {
    ParagraphBorders::with_empty().set(border)
}

/// A quoted paragraph: grey italic text behind a light bar, as in the editor.
fn quoted(mut p: Paragraph) -> Paragraph {
    p.children = std::mem::take(&mut p.children)
        .into_iter()
        .map(|child| match child {
            ParagraphChild::Run(run) => {
                ParagraphChild::Run(Box::new(run.italic().color(MUTED_COLOR)))
            }
            other => other,
        })
        .collect();
    let bar = ParagraphBorder::new(ParagraphBorderPosition::Left)
        .val(BorderType::Single)
        .size(12)
        .space(8)
        .color(LINE_COLOR);
    p.property = std::mem::take(&mut p.property).set_borders(only(bar));
    p
}

/// DXP-FR-OSRE / DXP-FR-ODVI: the document defaults. Every paragraph is set
/// in Arial at the body size, with the body line height and the space after a
/// paragraph, unless it states its own spacing.
fn new_document() -> Docx {
    Docx::new()
        .default_fonts(fonts(BODY_FONT))
        .default_size(body_half_points())
        .default_line_spacing(spacing(0.0, ty::PARAGRAPH_AFTER_PT, ty::BODY_LINE_HEIGHT))
}

/// DXP-FR-CPPT: the bytes of the `.docx` file of `doc`.
pub fn write(doc: &Document, assets: &Assets) -> Result<Vec<u8>, String> {
    let mut docx = new_document();
    for top in &doc.blocks {
        for part in block_parts(&top.block, top.align, 0, assets) {
            docx = match part {
                Part::Paragraph(p) => docx.add_paragraph(*p),
                // DXP-FR-RBAQ: a table has no space after it of its own, so
                // an empty paragraph of that height follows it.
                Part::Table(t) => {
                    docx.add_table(*t).add_paragraph(gap(ty::twips(ty::BLOCK_AFTER_PT) as i32))
                }
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
            let look = ty::heading(*level);
            let size = ty::half_points(look.size_pt);
            let style = RunStyle { bold: true, size: Some(size), ..RunStyle::default() };
            let p = Paragraph::new().keep_next(true).line_spacing(spacing(
                look.before_pt,
                look.after_pt,
                ty::HEADING_LINE_HEIGHT,
            ));
            let p = add_inlines(p, inlines, style, assets);
            vec![para(indent(aligned(p, align)))]
        }
        Block::Paragraph(inlines) => {
            let p = add_inlines(body_paragraph(), inlines, RunStyle::default(), assets);
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
                    vec![para(indent(aligned(body_paragraph().add_run(run), align)))]
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
                Part::Paragraph(p) => para(quoted(*p)),
                other => other,
            })
            .collect(),
        Block::Table { aligns, header, rows } => {
            vec![Part::Table(Box::new(table(aligns, header, rows, align, assets)))]
        }
        Block::Rule => {
            // A thematic break is a light line across the text width, as in
            // the editor: the bottom border of an empty paragraph.
            let line = ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                .val(BorderType::Single)
                .size(6)
                .space(1)
                .color(LINE_COLOR);
            let space = LineSpacing::new()
                .line_rule(LineSpacingType::Exact)
                .line(20)
                .before(ty::twips(ty::RULE_SPACE_PT))
                .after(ty::twips(ty::RULE_SPACE_PT));
            let mut p = Paragraph::new().line_spacing(space);
            p.property = std::mem::take(&mut p.property).set_borders(only(line));
            vec![para(p)]
        }
        Block::Spacer { lines } => vec![para(gap(body_line_twips() * (*lines as i32)))],
        Block::Html(text) => text
            .lines()
            .map(|line| {
                let run = mono(Run::new().add_text(line)).size(ty::half_points(ty::CODE_PT));
                let p = Paragraph::new().add_run(run).line_spacing(spacing(
                    0.0,
                    0.0,
                    ty::CODE_LINE_HEIGHT,
                ));
                para(indent(aligned(p, align)))
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
                    let p = Paragraph::new().add_run(body_run(&marker));
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
            let p = Paragraph::new().add_run(body_run(&marker));
            parts.push(para(p.indent(Some((depth + 1) * INDENT_TWIPS), None, None, None)));
        }
    }
    // DXP-FR-RBAQ: the gap between items, and the space after the list. A
    // nested list ends with the gap of an item, as it continues its parent.
    let last = parts.iter().rposition(|p| matches!(p, Part::Paragraph(_)));
    for (i, part) in parts.iter_mut().enumerate() {
        if let Part::Paragraph(p) = part {
            let after = if Some(i) == last && depth == 0 {
                ty::BLOCK_AFTER_PT
            } else {
                ty::LIST_ITEM_GAP_PT
            };
            let spaced =
                std::mem::take(p.as_mut()).line_spacing(spacing(0.0, after, ty::BODY_LINE_HEIGHT));
            **p = spaced;
        }
    }
    parts
}

/// DXP-FR-ERZY: a code block is a box, so an alignment moves the box and the
/// code text stays left-aligned.
fn code_box(text: &str, align: Align) -> Part {
    let mut cell = TableCell::new()
        .shading(Shading::new().fill(SUNKEN_FILL))
        .width(text_width_twips(), WidthType::Dxa);
    for line in text.split('\n') {
        let run = mono(Run::new().add_text(line)).size(ty::half_points(ty::CODE_PT));
        let p = Paragraph::new().add_run(run).line_spacing(cell_spacing(ty::CODE_LINE_HEIGHT));
        cell = cell.add_paragraph(p);
    }
    let table = boxed(Table::new(vec![TableRow::new(vec![cell])]), grid(&[1]));
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
    let count = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
    let columns = grid(&vec![1; count]);
    let cell = |inlines: &[Inline], col: usize, bold: bool| {
        let style = RunStyle { bold, ..RunStyle::default() };
        let p = Paragraph::new().line_spacing(cell_spacing(ty::BODY_LINE_HEIGHT));
        let p = add_inlines(p, inlines, style, assets);
        let p = aligned(p, aligns.get(col).copied().unwrap_or(Align::Left));
        let width = columns.get(col).copied().unwrap_or(columns[0]);
        let cell = TableCell::new().add_paragraph(p).width(width, WidthType::Dxa);
        if bold {
            cell.shading(Shading::new().fill(SUNKEN_FILL))
        } else {
            cell
        }
    };
    let mut table_rows =
        vec![TableRow::new(header.iter().enumerate().map(|(i, c)| cell(c, i, true)).collect())];
    for row in rows {
        table_rows
            .push(TableRow::new(row.iter().enumerate().map(|(i, c)| cell(c, i, false)).collect()));
    }
    table_align(boxed(Table::new(table_rows), columns.clone()), align)
}

/// DXP-FR-RPPW: the key in bold and the value in normal weight.
fn metadata_table(rows: &[(String, String)]) -> Table {
    let columns = grid(&[3, 7]);
    let rows = rows
        .iter()
        .map(|(key, value)| {
            let cell_paragraph =
                || Paragraph::new().line_spacing(cell_spacing(ty::BODY_LINE_HEIGHT));
            let key_cell = TableCell::new()
                .width(columns[0], WidthType::Dxa)
                .shading(Shading::new().fill(SUNKEN_FILL))
                .add_paragraph(cell_paragraph().add_run(body_run(key).bold()));
            let mut value_cell = TableCell::new().width(columns[1], WidthType::Dxa);
            for line in value.split('\n') {
                value_cell = value_cell.add_paragraph(cell_paragraph().add_run(body_run(line)));
            }
            TableRow::new(vec![key_cell, value_cell])
        })
        .collect();
    boxed(Table::new(rows), columns)
}

#[derive(Clone, Copy, Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    size: Option<usize>,
}

/// The spacing of a paragraph in a table cell: no space before or after it,
/// so a cell holds its lines and the cell padding alone.
fn cell_spacing(line: f32) -> LineSpacing {
    spacing(0.0, 0.0, line)
}

fn styled(text: &str, style: RunStyle) -> Run {
    // DXP-FR-RBAQ: inline code has the size of the text around it.
    let size = style.size.unwrap_or_else(body_half_points);
    let mut run = Run::new().add_text(text).fonts(fonts(BODY_FONT)).size(size);
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
        run = mono(run).shading(Shading::new().fill(SUNKEN_FILL));
    }
    run
}

/// DXP-FR-OSRE: code text is set in Courier New, for every script.
fn mono(run: Run) -> Run {
    run.fonts(fonts(MONO))
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
