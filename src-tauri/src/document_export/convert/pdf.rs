//! The `pdf` writer of an export, built on `markdown2pdf` (DXP-FR-CPPT).
//!
//! The writer builds the token stream of `markdown2pdf` from the block model
//! and renders it with the crate's own renderer, so the marker rules the model
//! applied reach the PDF unchanged.
//!
//! The PDF is set in the bundled font files, and the export measures with the
//! same files (DXP-FR-FNTP). The sizes and the spaces come from
//! `super::typography` (DXP-FR-ODVI, DXP-FR-RBAQ).
//!
//! `markdown2pdf` has no alignment of its own for one block. The text of a
//! centred or right-aligned paragraph, heading, list, or quote is therefore
//! broken into lines here and moved into place (`super::pdf_lines`,
//! DXP-FR-ERZY), so no other block changes its look (DXP-FR-IJOH). A spacer is
//! an empty image of the height of the break (DXP-FR-YVIV).

use std::path::{Path, PathBuf};

use markdown2pdf::fonts::{FontConfig, FontSource};
use markdown2pdf::markdown::{TableAlignment, TableCell, Token};
use markdown2pdf::styling::{
    FontWeight, Orientation, PageSize, ResolvedBlock, ResolvedList, ResolvedStyle, Sides,
    TextAlignment,
};

use super::model::{Align, Block, Document, Inline, ListItem};
use super::pdf_lines::{self, Face, Fonts, Frame};
use super::typography as ty;
use super::Assets;

/// One millimetre in points.
const MM_TO_PT: f32 = 72.0 / 25.4;

/// The width of the page body in points, as `markdown2pdf` lays it out.
fn body_width(style: &ResolvedStyle) -> f32 {
    let (w, h) = match style.page.size {
        PageSize::A4 => (210.0, 297.0),
        PageSize::Letter => (216.0, 279.4),
        PageSize::Legal => (216.0, 355.6),
        PageSize::A3 => (297.0, 420.0),
        PageSize::A5 => (148.0, 210.0),
        PageSize::Custom { width_mm, height_mm } => (width_mm, height_mm),
    };
    let width = match style.page.orientation {
        Orientation::Portrait => w,
        Orientation::Landscape => h,
    };
    let margins = &style.page.margins_mm;
    ((width - margins.left.max(1.0) - margins.right.max(1.0)) * MM_TO_PT).max(10.0)
}

fn sides(s: &Sides<f32>) -> f32 {
    s.left + s.right
}

/// The look of the list items of one kind.
struct ListLook {
    /// The frame of a list at the left margin, before any marker.
    frame: Frame,
    bullet: String,
    gap: f32,
    indent: f32,
}

/// The frames the aligned lines are broken for (DXP-FR-ERZY).
struct Layout {
    paragraph: Frame,
    headings: Vec<Frame>,
    quote: Frame,
    unordered: ListLook,
    ordered: ListLook,
    task: ListLook,
}

impl Layout {
    fn of(style: &ResolvedStyle) -> Layout {
        let body = body_width(style);
        let code = &style.code_inline;
        let frame = |block: &ResolvedBlock, width: f32| Frame {
            width,
            size: block.font_size_pt,
            // DXP-FR-RBAQ: `markdown2pdf` sets inline code at the size of
            // its block, so the lines are measured at that size too.
            code_size: block.font_size_pt,
            code_pad: (code.padding.left + code.padding.right) / 2.0,
            base: Face { bold: block.is_bold(), italic: block.is_italic(), mono: false },
        };
        let paragraph_sides = sides(&style.paragraph.padding);
        let look = |list: &ResolvedList| ListLook {
            frame: frame(&list.block, body),
            bullet: list.bullet.clone(),
            gap: list.bullet_gap_pt,
            indent: list.indent_per_level_pt,
        };
        Layout {
            paragraph: frame(&style.paragraph, body - paragraph_sides),
            headings: style.headings.iter().map(|h| frame(h, body - sides(&h.padding))).collect(),
            quote: frame(
                &style.blockquote,
                body - sides(&style.blockquote.padding) - paragraph_sides,
            ),
            unordered: look(&style.list_unordered),
            ordered: look(&style.list_ordered),
            task: look(&style.list_task),
        }
    }

    /// The look of an item and the text of its marker, as `markdown2pdf`
    /// writes it: the bullet, the number, or the task box, then spaces.
    fn marker(&self, task: Option<bool>, number: Option<usize>) -> (&ListLook, String) {
        match (task, number) {
            (Some(done), _) => (&self.task, if done { "[x] " } else { "[ ] " }.to_string()),
            (None, Some(n)) => {
                let template = self.ordered.bullet.trim();
                let text = if template.contains('1') {
                    format!("{}  ", template.replacen('1', &n.to_string(), 1))
                } else if template.is_empty() {
                    format!("{n}.  ")
                } else {
                    format!("{n}{template}  ")
                };
                (&self.ordered, text)
            }
            (None, None) => {
                let glyph = self.unordered.bullet.trim();
                let glyph = if glyph.is_empty() { "\u{2022}" } else { glyph };
                (&self.unordered, format!("{glyph}  "))
            }
        }
    }
}

/// The least width of the text that `markdown2pdf` keeps beside a marker.
const MIN_TEXT_PT: f32 = 10.0;

/// The resolution at which `markdown2pdf` places an image.
const IMAGE_DPI: f32 = 300.0;

/// The CSS pixel a natural size is counted in.
const CSS_DPI: f32 = 96.0;

/// A project image is at its natural size in CSS pixels.
const IMAGE_SCALE: f32 = IMAGE_DPI / CSS_DPI;

/// A diagram PNG is already `RASTER_SCALE` times its natural size.
const DIAGRAM_SCALE: f32 = IMAGE_DPI / CSS_DPI / super::diagrams::RASTER_SCALE;

/// The longest side `markdown2pdf` keeps; it shrinks a larger image itself.
const MAX_SIDE_PX: f32 = 4000.0;

/// The PNG of `png` scaled by `scale`, capped at `MAX_SIDE_PX`, or `None`
/// where no scale is needed or the PNG does not decode.
fn at_page_scale(png: &[u8], scale: f32) -> Option<Vec<u8>> {
    if (scale - 1.0).abs() < 0.01 {
        return None;
    }
    let decoded = image::load_from_memory(png).ok()?;
    let longest = decoded.width().max(decoded.height()) as f32;
    let scale = scale.min(MAX_SIDE_PX / longest.max(1.0));
    let width = ((decoded.width() as f32) * scale).round().max(1.0) as u32;
    let height = ((decoded.height() as f32) * scale).round().max(1.0) as u32;
    let resized = decoded.resize_exact(width, height, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    resized.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Writes one picture file and returns its path. The export owns the folder.
pub type FileWriter<'a> = &'a dyn Fn(&str, &[u8]) -> Option<PathBuf>;

/// The style of every PDF export.
pub(crate) fn export_style(folder: &Path) -> ResolvedStyle {
    let mut style = ResolvedStyle::default();
    style.security.image_root = Some(folder.to_path_buf());
    style.security.allow_absolute_image_paths = true;
    style.security.allow_remote_images = false;
    // DXP-FR-ERZY: only a marker aligns a heading, as in the editor.
    for heading in &mut style.headings {
        heading.text_align = TextAlignment::Left;
    }
    apply_typography(&mut style);
    style
}

/// DXP-FR-ODVI / DXP-FR-RBAQ: the sizes and the spaces of the export.
fn apply_typography(style: &mut ResolvedStyle) {
    let body = |block: &mut ResolvedBlock| {
        block.font_size_pt = ty::BODY_PT;
        block.line_height = ty::BODY_LINE_HEIGHT;
    };
    body(&mut style.paragraph);
    style.paragraph.margin_before_pt = 0.0;
    style.paragraph.margin_after_pt = ty::PARAGRAPH_AFTER_PT;
    for (level, heading) in style.headings.iter_mut().enumerate() {
        let look = ty::HEADINGS[level];
        heading.font_size_pt = look.size_pt;
        heading.line_height = ty::HEADING_LINE_HEIGHT;
        heading.font_weight = FontWeight::Bold;
        heading.margin_before_pt = look.before_pt;
        heading.margin_after_pt = look.after_pt;
    }
    for list in [&mut style.list_ordered, &mut style.list_unordered, &mut style.list_task] {
        body(&mut list.block);
        list.block.margin_before_pt = 0.0;
        list.block.margin_after_pt = ty::BLOCK_AFTER_PT;
        list.item_spacing_tight_pt = ty::LIST_ITEM_GAP_PT;
        list.item_spacing_loose_pt = ty::LIST_ITEM_GAP_PT;
    }
    body(&mut style.blockquote);
    style.blockquote.margin_before_pt = 0.0;
    style.blockquote.margin_after_pt = ty::BLOCK_AFTER_PT;
    style.code_block.font_size_pt = ty::CODE_PT;
    style.code_block.line_height = ty::CODE_LINE_HEIGHT;
    style.code_block.margin_before_pt = 0.0;
    style.code_block.margin_after_pt = ty::BLOCK_AFTER_PT;
    for cell in [&mut style.table.header, &mut style.table.cell] {
        body(cell);
    }
    style.table.margin_before_pt = 0.0;
    style.table.margin_after_pt = ty::BLOCK_AFTER_PT;
    style.horizontal_rule.margin_before_pt = ty::RULE_SPACE_PT;
    style.horizontal_rule.margin_after_pt = ty::RULE_SPACE_PT;
}

/// The token stream of `doc` for `style`.
pub(crate) fn tokens(
    doc: &Document,
    assets: &Assets,
    style: &ResolvedStyle,
    write_file: FileWriter<'_>,
) -> Vec<Token> {
    let body_line_pt = style.paragraph.font_size_pt * style.paragraph.line_height;
    let mut writer = Writer {
        assets,
        write_file,
        body_line_pt,
        files: 0,
        fonts: Fonts::load(),
        layout: Layout::of(style),
    };
    let mut tokens = Vec::new();
    for top in &doc.blocks {
        writer.top_block(&top.block, top.align, &mut tokens);
    }
    tokens
}

/// DXP-FR-CPPT: the bytes of the `.pdf` file of `doc`. `folder` is where
/// `write_file` puts the pictures, and the only folder the renderer reads.
pub fn write(
    doc: &Document,
    assets: &Assets,
    folder: &Path,
    write_file: FileWriter<'_>,
) -> Result<Written, String> {
    let style = export_style(folder);
    let tokens = tokens(doc, assets, &style, write_file);
    let config = font_config(write_file);
    let regular_faces_only = !has_font_files(&config);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        markdown2pdf::render::render_to_bytes(tokens, style, Some(&config))
    }));
    match outcome {
        Ok(Ok(bytes)) => Ok(Written { bytes, regular_faces_only }),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the PDF renderer stopped".to_string()),
    }
}

/// The bytes of a PDF, and whether it is set in the regular faces alone
/// because a font file did not write (DXP-FR-FNTP).
pub struct Written {
    pub bytes: Vec<u8>,
    pub regular_faces_only: bool,
}

/// Whether `config` sets the PDF in the font files of the export folder.
pub(super) fn has_font_files(config: &FontConfig) -> bool {
    matches!(config.default_font_source, Some(FontSource::File(_)))
}

/// DXP-FR-FNTP: the body is set in the bundled Liberation Sans and code in the
/// bundled Liberation Mono. No font of the host is looked up.
///
/// `markdown2pdf` finds the bold and italic faces only beside a font file, so
/// the files are written into the folder of the export first. Where a file
/// does not write, the PDF is set in the regular faces alone.
pub(super) fn font_config(write_file: FileWriter<'_>) -> FontConfig {
    let mut written = None;
    let mut mono = None;
    let mut all = true;
    for (name, bytes) in pdf_lines::FONT_FILES {
        match write_file(name, bytes) {
            Some(path) if name == pdf_lines::SANS_REGULAR_FILE => written = Some(path),
            Some(path) if name == pdf_lines::MONO_REGULAR_FILE => mono = Some(path),
            Some(_) => {}
            None => all = false,
        }
    }
    let (body, code) = match (all, written, mono) {
        (true, Some(body), Some(code)) => (FontSource::File(body), FontSource::File(code)),
        _ => {
            (FontSource::bytes(pdf_lines::SANS_REGULAR), FontSource::bytes(pdf_lines::MONO_REGULAR))
        }
    };
    FontConfig::new().with_default_font_source(body).with_code_font_source(code)
}

struct Writer<'a> {
    assets: &'a Assets,
    write_file: FileWriter<'a>,
    body_line_pt: f32,
    files: usize,
    fonts: Fonts,
    layout: Layout,
}

fn paragraph_end(out: &mut Vec<Token>) {
    out.push(Token::Newline);
    out.push(Token::Newline);
}

fn table_align(align: Align) -> TableAlignment {
    match align {
        Align::Left => TableAlignment::Left,
        Align::Center => TableAlignment::Center,
        Align::Right => TableAlignment::Right,
    }
}

/// The paragraph of a list item or a quote that takes the alignment.
fn aligned_paragraph(block: &Block) -> Option<&[Inline]> {
    match block {
        Block::Paragraph(inlines) if !image_only(inlines) => Some(inlines),
        _ => None,
    }
}

impl Writer<'_> {
    fn picture_path(&mut self, png: &[u8]) -> Option<String> {
        self.files += 1;
        let name = format!("picture-{}.png", self.files);
        (self.write_file)(&name, png).map(|p| p.to_string_lossy().into_owned())
    }

    /// `scale` brings the PNG to the pixels that `markdown2pdf` places at
    /// `IMAGE_DPI`, so the picture keeps its natural size (DXP-FR-RKJC).
    fn image_token(&mut self, png: &[u8], alt: &str, scale: f32) -> Token {
        let scaled = at_page_scale(png, scale);
        match self.picture_path(scaled.as_deref().unwrap_or(png)) {
            Some(url) => Token::Image { alt: vec![Token::Text(alt.to_string())], url, title: None },
            None => Token::Emphasis { level: 1, content: vec![Token::Text(format!("[{alt}]"))] },
        }
    }

    /// DXP-FR-ERZY: the lines of `inlines`, moved to `align` in `frame`.
    fn aligned(&self, inlines: &[Inline], frame: &Frame, align: Align) -> Vec<Token> {
        pdf_lines::placed_tokens(&self.fonts, frame, &pdf_lines::pieces(inlines), align)
    }

    fn top_block(&mut self, block: &Block, align: Align, out: &mut Vec<Token>) {
        if align == Align::Left {
            return self.block(block, out);
        }
        match block {
            Block::Paragraph(inlines) if !image_only(inlines) => {
                let frame = self.layout.paragraph;
                out.extend(self.aligned(inlines, &frame, align));
                paragraph_end(out);
            }
            Block::Heading { level, inlines } => {
                let index = usize::from(level.clamp(&1, &6) - 1);
                let frame = self.layout.headings[index];
                out.push(Token::Heading(self.aligned(inlines, &frame, align), *level as usize));
                paragraph_end(out);
            }
            Block::List { start, items } => {
                out.extend(self.list(*start, items, align, 0.0));
                paragraph_end(out);
            }
            Block::Quote(inner) => {
                let frame = self.layout.quote;
                let mut body = Vec::new();
                for b in inner {
                    match aligned_paragraph(b) {
                        Some(inlines) => {
                            body.extend(self.aligned(inlines, &frame, align));
                            paragraph_end(&mut body);
                        }
                        None => self.block(b, &mut body),
                    }
                }
                out.push(Token::BlockQuote(body));
                paragraph_end(out);
            }
            _ => self.block(block, out),
        }
    }

    fn block(&mut self, block: &Block, out: &mut Vec<Token>) {
        match block {
            Block::Heading { level, inlines } => {
                out.push(Token::Heading(self.inlines(inlines), *level as usize));
                paragraph_end(out);
            }
            Block::Paragraph(inlines) => {
                out.extend(self.inlines(inlines));
                paragraph_end(out);
            }
            Block::Code { language, text } => {
                out.push(Token::Code {
                    language: language.clone().unwrap_or_default(),
                    content: text.clone(),
                    block: true,
                });
                paragraph_end(out);
            }
            Block::Diagram { source, png, .. } => {
                match png {
                    Some(bytes) => {
                        let token = self.image_token(bytes, "diagram", DIAGRAM_SCALE);
                        out.push(token);
                    }
                    None => out.push(Token::Code {
                        language: "mermaid".to_string(),
                        content: source.clone(),
                        block: true,
                    }),
                }
                paragraph_end(out);
            }
            Block::List { start, items } => {
                out.extend(self.list(*start, items, Align::Left, 0.0));
                paragraph_end(out);
            }
            Block::Quote(inner) => {
                let mut body = Vec::new();
                for b in inner {
                    self.block(b, &mut body);
                }
                out.push(Token::BlockQuote(body));
                paragraph_end(out);
            }
            Block::Table { aligns, header, rows } => {
                let headers = header.iter().map(|c| TableCell::new(self.inlines(c))).collect();
                let rows = rows
                    .iter()
                    .map(|r| r.iter().map(|c| TableCell::new(self.inlines(c))).collect())
                    .collect();
                let aligns = aligns.iter().map(|a| table_align(*a)).collect();
                out.push(Token::Table { headers, aligns, rows });
                paragraph_end(out);
            }
            Block::Rule => {
                out.push(Token::HorizontalRule);
                paragraph_end(out);
            }
            Block::Spacer { lines } => {
                // DXP-FR-YVIV: an empty image of the height of the break.
                let height_pt = self.body_line_pt * *lines as f32;
                let height_px = (height_pt / 72.0 * IMAGE_DPI).round().max(1.0) as u32;
                if let Some(png) = empty_png(height_px) {
                    let token = self.image_token(&png, "", 1.0);
                    out.push(token);
                    paragraph_end(out);
                }
            }
            Block::Html(text) => {
                out.push(Token::Code {
                    language: String::new(),
                    content: text.clone(),
                    block: true,
                });
                paragraph_end(out);
            }
            Block::Metadata(rows) => {
                // DXP-FR-RPPW: no header row. A `markdown2pdf` table needs one,
                // so it is empty.
                let headers = vec![TableCell::new(Vec::new()), TableCell::new(Vec::new())];
                let mut table_rows = Vec::new();
                for (key, value) in rows {
                    // One row per line of the value. The key shows on the first.
                    for (n, line) in value.split('\n').enumerate() {
                        let key = match n {
                            0 => vec![Token::StrongEmphasis(vec![Token::Text(key.clone())])],
                            _ => Vec::new(),
                        };
                        let line = match line {
                            "" => Vec::new(),
                            text => vec![Token::Text(text.to_string())],
                        };
                        table_rows.push(vec![TableCell::new(key), TableCell::new(line)]);
                    }
                }
                out.push(Token::Table {
                    headers,
                    aligns: vec![TableAlignment::Left, TableAlignment::Left],
                    rows: table_rows,
                });
                paragraph_end(out);
            }
        }
    }

    /// DXP-FR-ERZY: the frame of the text of a list item, after its marker.
    /// `offset` is how far the list steps in from the left margin.
    fn item_frame(&self, task: Option<bool>, number: Option<usize>, offset: f32) -> (Frame, f32) {
        let (look, text) = self.layout.marker(task, number);
        let mut frame = look.frame;
        let marker_width = self.fonts.measure(Face::default(), &text, frame.size);
        frame.width = (frame.width - offset - marker_width - look.gap).max(MIN_TEXT_PT);
        (frame, look.indent)
    }

    fn list(
        &mut self,
        start: Option<u64>,
        items: &[ListItem],
        align: Align,
        offset: f32,
    ) -> Vec<Token> {
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let number = start.map(|s| (s + i as u64) as usize);
            let (frame, indent) = self.item_frame(item.task, number, offset);
            let mut content = Vec::new();
            for (n, block) in item.blocks.iter().enumerate() {
                match block {
                    Block::Paragraph(inlines) if n == 0 && align != Align::Left => {
                        content.extend(self.aligned(inlines, &frame, align));
                    }
                    Block::Paragraph(inlines) if n == 0 => content.extend(self.inlines(inlines)),
                    Block::List { start, items } => {
                        content.push(Token::Newline);
                        content.extend(self.list(*start, items, align, offset + indent));
                    }
                    other => {
                        let mut nested = Vec::new();
                        self.block(other, &mut nested);
                        content.push(Token::Newline);
                        content.extend(nested);
                    }
                }
            }
            out.push(Token::ListItem {
                content,
                ordered: start.is_some(),
                number,
                marker: if start.is_some() { '.' } else { '-' },
                checked: item.task,
                loose: false,
            });
            out.push(Token::Newline);
        }
        out
    }

    fn inlines(&mut self, inlines: &[Inline]) -> Vec<Token> {
        let mut out = Vec::new();
        for inline in inlines {
            match inline {
                Inline::Text(t) => out.push(Token::Text(t.clone())),
                Inline::Code(t) => out.push(Token::Code {
                    language: String::new(),
                    content: t.clone(),
                    block: false,
                }),
                Inline::Emphasis(c) => {
                    out.push(Token::Emphasis { level: 1, content: self.inlines(c) })
                }
                Inline::Strong(c) => out.push(Token::StrongEmphasis(self.inlines(c))),
                Inline::Strike(c) => out.push(Token::Strikethrough(self.inlines(c))),
                Inline::Link { url, children } => out.push(Token::Link {
                    content: self.inlines(children),
                    url: url.clone(),
                    title: None,
                }),
                Inline::Image { url, alt } => match self.assets.images.get(url) {
                    Some(picture) => {
                        let png = picture.png.clone();
                        let token = self.image_token(&png, alt, IMAGE_SCALE);
                        out.push(token);
                    }
                    None => out.push(Token::Emphasis {
                        level: 1,
                        content: vec![Token::Text(format!("[{alt}]"))],
                    }),
                },
                Inline::SoftBreak => out.push(Token::Text(" ".into())),
                Inline::HardBreak => out.push(Token::HardBreak),
            }
        }
        out
    }
}

/// A paragraph that holds one image and nothing else. `markdown2pdf` centres
/// such an image already, and a table cell would show its alt text instead.
fn image_only(inlines: &[Inline]) -> bool {
    let mut images = 0;
    for inline in inlines {
        match inline {
            Inline::Image { .. } => images += 1,
            Inline::Text(t) if t.trim().is_empty() => {}
            Inline::SoftBreak => {}
            _ => return false,
        }
    }
    images == 1
}

/// A transparent PNG one pixel wide and `height` pixels high.
fn empty_png(height: u32) -> Option<Vec<u8>> {
    let image = image::RgbaImage::new(1, height);
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
