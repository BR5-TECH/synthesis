//! The conversion of a DOC artifact's Markdown to a PDF or a DOCX
//! (DXP-FR-RKJC to DXP-FR-CPPT).
//!
//! The conversion is pure over its inputs: the file text, a reader of the
//! local images, a diagram renderer, and the folder the PDF pictures go to.
//! The command (`super::commands`) wires those to the project and the window,
//! and the tests wire them to fakes.

use std::collections::HashMap;
use std::path::Path;

pub mod diagrams;
pub mod docx;
pub mod frontmatter;
pub mod model;
pub mod pdf;
pub mod pdf_lines;

#[cfg(test)]
pub(super) mod tests;
#[cfg(test)]
mod tests_comments;
#[cfg(test)]
mod tests_formats;
#[cfg(test)]
mod tests_pdf;

use model::{Block, Document, Inline, TopBlock};

/// DXP-FR-IEQA: the two formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Docx,
}

impl Format {
    pub fn parse(text: &str) -> Option<Format> {
        match text.to_ascii_lowercase().as_str() {
            "pdf" => Some(Format::Pdf),
            "docx" => Some(Format::Docx),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Docx => "docx",
        }
    }
}

/// DXP-FR-LSYA: the stages of an export, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Reading,
    Diagrams,
    Converting,
    Writing,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Reading => "reading",
            Stage::Diagrams => "diagrams",
            Stage::Converting => "converting",
            Stage::Writing => "writing",
        }
    }
}

/// A picture as PNG bytes and its size in pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// DXP-FR-JYHX: the local images of a document that decoded, by their URL.
#[derive(Clone, Debug, Default)]
pub struct Assets {
    pub images: HashMap<String, Picture>,
}

/// What a conversion needs.
pub struct Input<'a> {
    pub text: &'a str,
    pub format: Format,
    pub include_frontmatter: bool,
    /// DXP-FR-JYHX: the bytes of a local image by its URL, or `None` for one
    /// that is remote, outside the project, or missing.
    pub load_image: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    /// DXP-FR-RKJC: the PNG of one diagram, or why it fails.
    pub render_diagram: diagrams::DiagramRenderer<'a>,
    /// The folder the PDF pictures are written to, and the writer of one file.
    pub pictures_dir: &'a Path,
    pub write_file: pdf::FileWriter<'a>,
}

/// What a conversion gives back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Converted {
    pub bytes: Vec<u8>,
    /// DXP-FR-IOFM: the diagrams that stayed as code, counted from 1.
    pub failed_diagrams: Vec<diagrams::FailedDiagram>,
    pub diagram_count: usize,
}

/// Read `text` into the block model, with the frontmatter split off and
/// rendered when it is included (DXP-FR-RPPW, DXP-FR-WAQF).
pub fn document_of(text: &str, include_frontmatter: bool) -> Document {
    let split = frontmatter::split(text);
    let mut doc = model::parse(split.body);
    if let (true, Some(block)) = (include_frontmatter, split.frontmatter) {
        let first = match frontmatter::render(block) {
            frontmatter::Rendered::Table(rows) => Some(Block::Metadata(rows)),
            frontmatter::Rendered::Text(text) => Some(Block::Code { language: None, text }),
            frontmatter::Rendered::Nothing => None,
        };
        if let Some(block) = first {
            doc.blocks.insert(0, TopBlock { block, align: model::Align::Left });
        }
    }
    doc
}

/// DXP-FR-IEQA: convert the text. `progress(stage, done, total)` follows the
/// `diagrams` and `converting` stages (DXP-FR-LSYA).
pub fn convert(
    input: &Input<'_>,
    progress: &mut dyn FnMut(Stage, usize, usize),
) -> Result<Converted, String> {
    let mut doc = document_of(input.text, input.include_frontmatter);
    let total = doc.diagram_count();
    progress(Stage::Diagrams, 0, total);
    let failed = diagrams::render_all(&mut doc, input.render_diagram, &mut |done, total| {
        progress(Stage::Diagrams, done, total)
    });
    progress(Stage::Converting, 0, 0);
    let assets = load_assets(&doc, input.load_image);
    let bytes = match input.format {
        Format::Docx => docx::write(&doc, &assets)?,
        Format::Pdf => pdf::write(&doc, &assets, input.pictures_dir, input.write_file)?,
    };
    Ok(Converted { bytes, failed_diagrams: failed, diagram_count: total })
}

/// DXP-FR-JYHX: decode each image the document names once.
fn load_assets(doc: &Document, load: &dyn Fn(&str) -> Option<Vec<u8>>) -> Assets {
    let mut urls = Vec::new();
    for top in &doc.blocks {
        collect_block_images(&top.block, &mut urls);
    }
    let mut assets = Assets::default();
    for url in urls {
        if assets.images.contains_key(&url) {
            continue;
        }
        if let Some(picture) = load(&url).and_then(|bytes| decode(&bytes)) {
            assets.images.insert(url, picture);
        }
    }
    assets
}

/// PNG bytes of an image of any decodable format, with its size.
pub fn decode(bytes: &[u8]) -> Option<Picture> {
    let image = image::load_from_memory(bytes).ok()?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return None;
    }
    let png = if diagrams::png_size(bytes).is_some() {
        bytes.to_vec()
    } else {
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).ok()?;
        out.into_inner()
    };
    Some(Picture { png, width, height })
}

fn collect_block_images(block: &Block, urls: &mut Vec<String>) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            collect_inline_images(inlines, urls)
        }
        Block::Quote(inner) => inner.iter().for_each(|b| collect_block_images(b, urls)),
        Block::List { items, .. } => {
            items.iter().flat_map(|i| i.blocks.iter()).for_each(|b| collect_block_images(b, urls))
        }
        Block::Table { header, rows, .. } => {
            header.iter().for_each(|c| collect_inline_images(c, urls));
            rows.iter().flatten().for_each(|c| collect_inline_images(c, urls));
        }
        _ => {}
    }
}

fn collect_inline_images(inlines: &[Inline], urls: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Image { url, .. } => urls.push(url.clone()),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strike(c) => {
                collect_inline_images(c, urls)
            }
            Inline::Link { children, .. } => collect_inline_images(children, urls),
            _ => {}
        }
    }
}
