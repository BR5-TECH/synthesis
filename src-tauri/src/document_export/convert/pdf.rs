//! The `pdf` writer of an export, built on `markdown2pdf` (DXP-FR-CPPT).
//!
//! The writer builds the token stream of `markdown2pdf` from the block model
//! and renders it with the crate's own renderer, so the marker rules the model
//! applied reach the PDF unchanged.
//!
//! `markdown2pdf` has no alignment of its own for one block. A centred or
//! right-aligned paragraph or heading is therefore drawn as the one cell of a
//! table with no border, whose column takes the alignment (DXP-FR-ERZY). A
//! spacer is an empty image of the height of the break (DXP-FR-YVIV).

use std::path::{Path, PathBuf};

use markdown2pdf::markdown::{TableAlignment, TableCell, Token};
use markdown2pdf::styling::ResolvedStyle;

use super::model::{Align, Block, Document, Inline, ListItem};
use super::Assets;

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

/// DXP-FR-CPPT: the bytes of the `.pdf` file of `doc`. `folder` is where
/// `write_file` puts the pictures, and the only folder the renderer reads.
pub fn write(
    doc: &Document,
    assets: &Assets,
    folder: &Path,
    write_file: FileWriter<'_>,
) -> Result<Vec<u8>, String> {
    let mut style = ResolvedStyle::default();
    style.security.image_root = Some(folder.to_path_buf());
    style.security.allow_absolute_image_paths = true;
    style.security.allow_remote_images = false;
    let body_line_pt = style.paragraph.font_size_pt * style.paragraph.line_height;
    let mut writer = Writer { assets, write_file, body_line_pt, files: 0, aligned_box: false };
    let mut tokens = Vec::new();
    for top in &doc.blocks {
        writer.top_block(&top.block, top.align, &mut tokens);
    }
    if writer.aligned_box {
        // An aligned block is a table cell, so a table draws no border here.
        style.table.border.top = None;
        style.table.border.right = None;
        style.table.border.bottom = None;
        style.table.border.left = None;
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        markdown2pdf::render::render_to_bytes(tokens, style, None)
    }));
    match outcome {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the PDF renderer stopped".to_string()),
    }
}

struct Writer<'a> {
    assets: &'a Assets,
    write_file: FileWriter<'a>,
    body_line_pt: f32,
    files: usize,
    aligned_box: bool,
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

    fn top_block(&mut self, block: &Block, align: Align, out: &mut Vec<Token>) {
        let inlines = match block {
            Block::Paragraph(inlines) if align != Align::Left && !image_only(inlines) => inlines,
            Block::Heading { inlines, .. } if align != Align::Left => inlines,
            _ => return self.block(block, out),
        };
        // DXP-FR-ERZY: the aligned text is the one cell of a table.
        self.aligned_box = true;
        let mut content = self.inlines(inlines);
        if matches!(block, Block::Heading { .. }) {
            content = vec![Token::StrongEmphasis(content)];
        }
        out.push(Token::Table {
            headers: vec![TableCell::new(Vec::new())],
            aligns: vec![table_align(align)],
            rows: vec![vec![TableCell::new(content)]],
        });
        paragraph_end(out);
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
                out.extend(self.list(*start, items));
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
                let headers = vec![
                    TableCell::new(vec![Token::Text("Key".into())]),
                    TableCell::new(vec![Token::Text("Value".into())]),
                ];
                let rows = rows
                    .iter()
                    .map(|(k, v)| {
                        vec![
                            TableCell::new(vec![Token::StrongEmphasis(vec![Token::Text(
                                k.clone(),
                            )])]),
                            TableCell::new(vec![Token::Text(v.replace('\n', " "))]),
                        ]
                    })
                    .collect();
                out.push(Token::Table {
                    headers,
                    aligns: vec![TableAlignment::Left, TableAlignment::Left],
                    rows,
                });
                paragraph_end(out);
            }
        }
    }

    fn list(&mut self, start: Option<u64>, items: &[ListItem]) -> Vec<Token> {
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let mut content = Vec::new();
            for (n, block) in item.blocks.iter().enumerate() {
                match block {
                    Block::Paragraph(inlines) if n == 0 => content.extend(self.inlines(inlines)),
                    Block::List { start, items } => {
                        content.push(Token::Newline);
                        content.extend(self.list(*start, items));
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
                number: start.map(|s| (s + i as u64) as usize),
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
