//! CMS-FR-WIIE, CMS-FR-PTKT: the text and the images of a DOCX.
//!
//! The reader of `docx-rs` parses the package. The text is the body
//! paragraphs and table cells in document order, and each drawing gives one
//! image in its place, resolved through the package media by its relationship
//! id. Headers, footers, footnotes, and comments are not read.

use std::collections::HashMap;

use docx_rs::{
    DocumentChild, DrawingData, InsertChild, Paragraph, ParagraphChild, Run, RunChild,
    StructuredDataTag, StructuredDataTagChild, Table, TableCellContent, TableChild, TableRowChild,
};

use super::{
    ExtractError, RawImage, RawItem, MAX_DOCUMENT_IMAGES, MAX_DOCX_UNPACKED_BYTES, MAX_TEXT_BYTES,
    MIN_IMAGE_SIDE,
};

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<RawItem>, ExtractError> {
    check_unpacked_size(bytes)?;
    // The reader makes no PNG previews: a preview is made here only for an
    // image that is not sent as it is, so a large photo is not decoded twice.
    let options = docx_rs::ReadDocxOptions::default().with_image_previews(false);
    let docx = docx_rs::read_docx_with_options(bytes, options)
        .map_err(|e| ExtractError::Unreadable(format!("docx: {e}")))?;
    let media: HashMap<&str, &[u8]> = docx
        .images
        .iter()
        .map(|(id, _path, image, _preview)| (id.as_str(), image.0.as_slice()))
        .collect();
    let mut walker = Walker { media: &media, items: Vec::new(), text: String::new(), kept: 0 };
    for child in &docx.document.children {
        walker.document_child(child);
    }
    walker.flush();
    // CMS-FR-LZZV: text above the largest limit is above every limit.
    let text_bytes: usize = walker
        .items
        .iter()
        .map(|item| match item {
            RawItem::Text(text) => text.len(),
            RawItem::Image(_) => 0,
        })
        .sum();
    if text_bytes > MAX_TEXT_BYTES {
        return Err(ExtractError::TooLarge);
    }
    Ok(walker.items)
}

/// A package whose entries unpack to more than [`MAX_DOCX_UNPACKED_BYTES`] is
/// not read, so a compressed bomb does not fill the memory. The sizes come
/// from the central directory, and nothing is unpacked to read them.
fn check_unpacked_size(bytes: &[u8]) -> Result<(), ExtractError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ExtractError::Unreadable(format!("docx: {e}")))?;
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|e| ExtractError::Unreadable(format!("docx: {e}")))?;
        total = total.saturating_add(entry.size());
    }
    if total > MAX_DOCX_UNPACKED_BYTES {
        return Err(ExtractError::Unreadable("docx: too large when unpacked".to_string()));
    }
    Ok(())
}

struct Walker<'a> {
    media: &'a HashMap<&'a str, &'a [u8]>,
    items: Vec<RawItem>,
    text: String,
    /// The images so far that are not too small (CMS-FR-NHCE).
    kept: usize,
}

impl Walker<'_> {
    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.items.push(RawItem::Text(std::mem::take(&mut self.text)));
        }
    }

    fn document_child(&mut self, child: &DocumentChild) {
        match child {
            DocumentChild::Paragraph(paragraph) => self.paragraph(paragraph),
            DocumentChild::Table(table) => self.table(table),
            DocumentChild::StructuredDataTag(tag) => self.tag(tag),
            _ => {}
        }
    }

    fn paragraph(&mut self, paragraph: &Paragraph) {
        for child in &paragraph.children {
            self.paragraph_child(child);
        }
        self.text.push('\n');
    }

    fn paragraph_child(&mut self, child: &ParagraphChild) {
        match child {
            ParagraphChild::Run(run) => self.run(run),
            ParagraphChild::Insert(insert) => {
                for child in &insert.children {
                    if let InsertChild::Run(run) = child {
                        self.run(run);
                    }
                }
            }
            ParagraphChild::Hyperlink(link) => {
                for child in &link.children {
                    self.paragraph_child(child);
                }
            }
            ParagraphChild::StructuredDataTag(tag) => self.tag(tag),
            _ => {}
        }
    }

    fn run(&mut self, run: &Run) {
        for child in &run.children {
            match child {
                RunChild::Text(text) => self.text.push_str(&text.text),
                RunChild::Tab(_) | RunChild::PTab(_) => self.text.push('\t'),
                RunChild::Break(_) | RunChild::CarriageReturn(_) => self.text.push('\n'),
                RunChild::Drawing(drawing) => {
                    if let Some(DrawingData::Pic(pic)) = &drawing.data {
                        self.flush();
                        let image = self.image(&pic.id);
                        self.items.push(RawItem::Image(image));
                    }
                }
                _ => {}
            }
        }
    }

    fn table(&mut self, table: &Table) {
        for TableChild::TableRow(row) in &table.rows {
            for TableRowChild::TableCell(cell) in &row.cells {
                for content in &cell.children {
                    match content {
                        TableCellContent::Paragraph(paragraph) => self.paragraph(paragraph),
                        TableCellContent::Table(table) => self.table(table),
                        TableCellContent::StructuredDataTag(tag) => self.tag(tag),
                        TableCellContent::TableOfContents(_) => {}
                    }
                }
            }
        }
    }

    fn tag(&mut self, tag: &StructuredDataTag) {
        for child in &tag.children {
            match child {
                StructuredDataTagChild::Run(run) => self.run(run),
                StructuredDataTagChild::Paragraph(paragraph) => self.paragraph(paragraph),
                StructuredDataTagChild::Table(table) => self.table(table),
                StructuredDataTagChild::StructuredDataTag(inner) => self.tag(inner),
                _ => {}
            }
        }
    }

    /// CMS-FR-PTKT: a PNG, JPEG, or GIF is sent as it is. Another format that
    /// `image` decodes is sent as a PNG preview, as `docx-rs` makes one, and an
    /// image with neither is metadata.
    fn image(&mut self, id: &str) -> RawImage {
        let Some(original) = self.media.get(id).copied() else {
            self.kept += 1;
            return RawImage {
                size: None,
                media_type: "application/octet-stream".to_string(),
                data: None,
            };
        };
        let passed = match image::guess_format(original) {
            Ok(image::ImageFormat::Png) => Some("image/png"),
            Ok(image::ImageFormat::Jpeg) => Some("image/jpeg"),
            Ok(image::ImageFormat::Gif) => Some("image/gif"),
            _ => None,
        };
        // The size comes from the image header, so a too small image and an
        // image past the cap are not decoded (CMS-FR-NHCE).
        let size = dimensions(original);
        let fallback = match (passed, size) {
            (Some(media_type), _) => media_type,
            (None, Some(_)) => "image/png",
            (None, None) => "application/octet-stream",
        };
        let metadata = |size| RawImage { size, media_type: fallback.to_string(), data: None };
        if size.is_some_and(|(w, h)| w.min(h) < MIN_IMAGE_SIDE) {
            return metadata(size);
        }
        self.kept += 1;
        if self.kept > MAX_DOCUMENT_IMAGES {
            return metadata(size);
        }
        if let (Some(media_type), Some(size)) = (passed, size) {
            return RawImage {
                size: Some(size),
                media_type: media_type.to_string(),
                data: Some(original.to_vec()),
            };
        }
        if let Some((size, preview)) = png_preview(original) {
            return RawImage {
                size: Some(size),
                media_type: "image/png".to_string(),
                data: Some(preview),
            };
        }
        metadata(size)
    }
}

/// The PNG preview of an image in another raster format, made as `docx-rs`
/// makes its previews.
fn png_preview(bytes: &[u8]) -> Option<((u32, u32), Vec<u8>)> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(((image.width(), image.height()), out.into_inner()))
}

fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}
