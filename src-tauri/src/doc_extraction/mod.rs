//! The text and the embedded images of a PDF or DOCX comment attachment
//! (`CMS-comments-storage.md` CMS-FR-EHOG through CMS-FR-RQTB).
//!
//! Extraction is deterministic: the same bytes always give the same blocks
//! (CMS-FR-DUGE). It makes no network request, calls no model, and does no
//! OCR. The parsers are `pdf-extract` with the `lopdf` it re-exports
//! (CMS-FR-LGXL) and the reader of `docx-rs` (CMS-FR-WIIE).

pub mod bounded;
pub mod cache;
mod docx;
mod pdf;
#[cfg(test)]
pub(crate) mod tests;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bounded::{run_bounded, Bounded};

/// The media type of a PDF document attachment.
pub const PDF_MEDIA_TYPE: &str = "application/pdf";
/// The media type of a DOCX document attachment (CMS-FR-47).
pub const DOCX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// CMS-FR-NHCE: an image with a side under this many pixels gives no block.
pub const MIN_IMAGE_SIDE: u32 = 50;
/// CMS-FR-NHCE: the most image blocks that one document gives.
pub const MAX_DOCUMENT_IMAGES: usize = 20;
/// CMS-FR-DUGE: how long a caller waits for one extraction.
pub const EXTRACT_DEADLINE: Duration = Duration::from_secs(30);
/// CMS-FR-LZZV: the most text an extraction collects, the largest extracted
/// text limit (GSS-FR-IRYZ). A document with more is refused as too large
/// before all its text is in memory.
pub const MAX_TEXT_BYTES: usize = 10240 * 1024;
/// The largest raw samples of one PDF image that are decompressed.
pub const MAX_RAW_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// The largest unpacked size of a DOCX package that is read.
pub const MAX_DOCX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
/// The most extractions that run at one time. A worker abandoned at its
/// deadline still counts until it ends.
pub const MAX_LIVE_EXTRACTIONS: usize = 4;

/// The two document formats whose text reaches an agent (CMS-FR-EHOG).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Pdf,
    Docx,
}

impl DocumentKind {
    /// The document kind of a media type, or `None` for any other type. The
    /// parameters after `;` and the case are ignored, as `media_type_accepted`
    /// ignores them.
    pub fn of_media_type(media_type: &str) -> Option<DocumentKind> {
        let base = media_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        match base.as_str() {
            PDF_MEDIA_TYPE => Some(DocumentKind::Pdf),
            DOCX_MEDIA_TYPE => Some(DocumentKind::Docx),
            _ => None,
        }
    }

    /// The name a log record uses for the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Pdf => "pdf",
            DocumentKind::Docx => "docx",
        }
    }
}

/// One block of an extracted document, in document order (CMS-FR-DUGE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocBlock {
    /// Text of the document.
    Text(String),
    /// An image with bytes in a media type that a model accepts (CMS-FR-PTKT).
    /// `ordinal` counts from 1 over the images that are not too small.
    Image { media_type: String, data: Vec<u8>, ordinal: usize },
    /// An image that is sent as metadata alone: one past the cap, or one with
    /// no encoding a model accepts (CMS-FR-PTKT, CMS-FR-NHCE).
    ImageMetadata { media_type: String, ordinal: usize },
}

/// The blocks of one document (CMS-FR-DUGE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub blocks: Vec<DocBlock>,
}

impl ExtractedDocument {
    /// The UTF-8 length of all text blocks, which the limit applies to
    /// (CMS-FR-YSRD).
    pub fn text_bytes(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| match block {
                DocBlock::Text(text) => text.len(),
                _ => 0,
            })
            .sum()
    }

    /// The number of blocks that carry image bytes.
    pub fn image_count(&self) -> usize {
        self.blocks.iter().filter(|block| matches!(block, DocBlock::Image { .. })).count()
    }

    /// The approximate memory that the document holds, for the cache bound
    /// (CMS-FR-AAMO).
    pub fn footprint(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| match block {
                DocBlock::Text(text) => text.len(),
                DocBlock::Image { media_type, data, .. } => media_type.len() + data.len(),
                DocBlock::ImageMetadata { media_type, .. } => media_type.len(),
            } + 32)
            .sum()
    }
}

/// Why a document gave no blocks (CMS-FR-LZZV).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// The extraction did not end in time, or its worker did not run. This
    /// depends on the load of the machine, so the cache does not keep it.
    Interrupted(String),
    /// The text is empty after trimming: a scanned file, or a file with no
    /// text layer.
    NoText,
    /// A parser failed, the document is encrypted, or the extraction panicked
    /// or passed its deadline. The reason is for the log alone.
    Unreadable(String),
    /// The text is above [`MAX_TEXT_BYTES`], so it is above every limit.
    TooLarge,
}

impl ExtractError {
    /// The short reason a log record carries. It holds no document content.
    pub fn reason(&self) -> &str {
        match self {
            ExtractError::NoText => "no text",
            ExtractError::TooLarge => "text above the largest limit",
            ExtractError::Unreadable(reason) | ExtractError::Interrupted(reason) => reason,
        }
    }
}

/// One item that a parser gives, before the image rules apply.
#[derive(Debug)]
pub(crate) enum RawItem {
    Text(String),
    Image(RawImage),
}

/// One image as a parser found it.
#[derive(Debug)]
pub(crate) struct RawImage {
    /// The pixel size, where the parser can read it. An image of unknown size
    /// is never skipped as too small.
    pub size: Option<(u32, u32)>,
    /// The media type that names the image in its metadata.
    pub media_type: String,
    /// The bytes to send, in `media_type`, where an encoding that a model
    /// accepts exists (CMS-FR-PTKT).
    pub data: Option<Vec<u8>>,
}

/// The parser that an extraction runs. A function pointer, so a test can
/// substitute one that panics or that does not end.
pub(crate) type Parser = fn(DocumentKind, &[u8]) -> Result<Vec<RawItem>, ExtractError>;

static EXTRACTIONS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// One running extraction, counted in [`LIVE`] until its worker ends.
struct LiveSlot;

impl LiveSlot {
    fn take() -> Option<LiveSlot> {
        let taken = LIVE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
            (live < MAX_LIVE_EXTRACTIONS).then_some(live + 1)
        });
        taken.ok().map(|_| LiveSlot)
    }
}

impl Drop for LiveSlot {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

/// How many extractions this process ran. A test reads it to prove that a
/// cached document is not extracted again (CMS-FR-AAMO).
pub fn extraction_count() -> usize {
    EXTRACTIONS.load(Ordering::Relaxed)
}

/// CMS-FR-DUGE: extracts a document on a worker thread, under the deadline.
pub fn extract(kind: DocumentKind, bytes: Vec<u8>) -> Result<ExtractedDocument, ExtractError> {
    extract_with(kind, bytes, EXTRACT_DEADLINE, parse)
}

pub(crate) fn extract_with(
    kind: DocumentKind,
    bytes: Vec<u8>,
    deadline: Duration,
    parser: Parser,
) -> Result<ExtractedDocument, ExtractError> {
    EXTRACTIONS.fetch_add(1, Ordering::Relaxed);
    let Some(slot) = LiveSlot::take() else {
        return Err(ExtractError::Interrupted("too many extractions are running".to_string()));
    };
    let name = format!("attachment-{}-extract", kind.as_str());
    // The slot moves into the worker, so a worker abandoned at the deadline
    // holds it until it ends.
    match run_bounded(&name, deadline, move || {
        let _slot = slot;
        parser(kind, &bytes)
    }) {
        Bounded::Done(Ok(items)) => finish(items),
        Bounded::Done(Err(error)) => Err(error),
        Bounded::Panicked => Err(ExtractError::Unreadable("parser panicked".to_string())),
        Bounded::PastDeadline => {
            Err(ExtractError::Interrupted("extraction ran past its deadline".to_string()))
        }
        Bounded::NotStarted => {
            Err(ExtractError::Interrupted("extractor could not start".to_string()))
        }
        Bounded::Stopped => Err(ExtractError::Interrupted("extractor stopped".to_string())),
    }
}

fn parse(kind: DocumentKind, bytes: &[u8]) -> Result<Vec<RawItem>, ExtractError> {
    match kind {
        DocumentKind::Pdf => pdf::parse(bytes),
        DocumentKind::Docx => docx::parse(bytes),
    }
}

/// CMS-FR-NHCE, CMS-FR-LZZV: applies the image rules to what a parser found,
/// drops text of whitespace alone, and refuses a document whose text is empty.
pub(crate) fn finish(items: Vec<RawItem>) -> Result<ExtractedDocument, ExtractError> {
    let mut blocks: Vec<DocBlock> = Vec::new();
    let mut ordinal = 0usize;
    for item in items {
        match item {
            // CMS-FR-LGXL: each page of a PDF keeps its own text block.
            RawItem::Text(text) => blocks.push(DocBlock::Text(text)),
            RawItem::Image(image) => {
                if let Some((width, height)) = image.size {
                    if width.min(height) < MIN_IMAGE_SIDE {
                        continue;
                    }
                }
                ordinal += 1;
                let block = match image.data {
                    Some(data) if ordinal <= MAX_DOCUMENT_IMAGES => {
                        DocBlock::Image { media_type: image.media_type, data, ordinal }
                    }
                    _ => DocBlock::ImageMetadata { media_type: image.media_type, ordinal },
                };
                blocks.push(block);
            }
        }
    }
    // A text block of whitespace alone, such as the paragraph end around a
    // drawing, carries nothing.
    blocks.retain(|block| !matches!(block, DocBlock::Text(text) if text.trim().is_empty()));
    let has_text = blocks.iter().any(|block| matches!(block, DocBlock::Text(_)));
    if !has_text {
        return Err(ExtractError::NoText);
    }
    Ok(ExtractedDocument { blocks })
}
