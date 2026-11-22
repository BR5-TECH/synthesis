//! Tests of document extraction (CMS-FR-DUGE through CMS-FR-AAMO).

mod cache_tests;
pub(crate) mod fixtures;

use super::*;
use fixtures::*;

fn blocks(kind: DocumentKind, bytes: Vec<u8>) -> Vec<DocBlock> {
    extract(kind, bytes).expect("the document extracts").blocks
}

/// A short form of the blocks: `T:<text>`, `I<n>:<type>`, `M<n>:<type>`.
fn shape(blocks: &[DocBlock]) -> Vec<String> {
    blocks
        .iter()
        .map(|b| match b {
            DocBlock::Text(t) => {
                format!("T:{}", t.split_whitespace().collect::<Vec<_>>().join(" "))
            }
            DocBlock::Image { media_type, ordinal, .. } => format!("I{ordinal}:{media_type}"),
            DocBlock::ImageMetadata { media_type, ordinal } => format!("M{ordinal}:{media_type}"),
        })
        .collect()
}

// CMS-FR-DUGE: the same PDF bytes give the same blocks every time.
#[test]
fn the_same_pdf_bytes_give_the_same_blocks() {
    let bytes = pdf(&[
        page(&["Alpha page"], &[PdfImage::Rgb(60, 60)]),
        page(&["Beta page"], &[PdfImage::Jpeg(80, 70)]),
    ]);
    let first = blocks(DocumentKind::Pdf, bytes.clone());
    let second = blocks(DocumentKind::Pdf, bytes);
    assert_eq!(first, second);
    assert_eq!(first.len(), 4);
}

// CMS-FR-DUGE: the same DOCX bytes give the same blocks every time.
#[test]
fn the_same_docx_bytes_give_the_same_blocks() {
    let bytes = docx(&[para("One"), DocxItem::Image(png(64, 64)), para("Two")]);
    assert_eq!(blocks(DocumentKind::Docx, bytes.clone()), blocks(DocumentKind::Docx, bytes));
}

// CMS-FR-LGXL: each page gives its text, then the images it draws, in draw
// order; an image drawn twice gives one block.
#[test]
fn a_pdf_page_gives_its_text_then_its_images_in_draw_order() {
    let mut second = page(&["Second page"], &[PdfImage::Jpeg(60, 60), PdfImage::Rgb(70, 50)]);
    second.twice = true;
    let bytes = pdf(&[page(&["First page"], &[PdfImage::Rgb(55, 55)]), second]);
    assert_eq!(
        shape(&blocks(DocumentKind::Pdf, bytes)),
        vec!["T:First page", "I1:image/png", "T:Second page", "I2:image/jpeg", "I3:image/png",]
    );
}

// CMS-FR-PTKT: DCTDecode passes the JPEG bytes as they are; raw RGB and
// Flate gray samples are encoded as PNG of the same size.
#[test]
fn pdf_images_are_jpeg_or_png_of_their_own_size() {
    let bytes = pdf(&[page(
        &["Images"],
        &[PdfImage::Jpeg(64, 52), PdfImage::Rgb(61, 53), PdfImage::GrayFlate(57, 51)],
    )]);
    let found = blocks(DocumentKind::Pdf, bytes);
    let images: Vec<(&str, &Vec<u8>)> = found
        .iter()
        .filter_map(|b| match b {
            DocBlock::Image { media_type, data, .. } => Some((media_type.as_str(), data)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0].0, "image/jpeg");
    assert_eq!(images[0].1, &jpeg(64, 52));
    for ((media_type, data), size) in images[1..].iter().zip([(61, 53), (57, 51)]) {
        assert_eq!(*media_type, "image/png");
        let decoded = image::load_from_memory(data).unwrap();
        assert_eq!((decoded.width(), decoded.height()), size);
    }
}

// CMS-FR-PTKT: a JPX image and a CMYK image have no encoding a model accepts,
// so each is a metadata block in its place.
#[test]
fn pdf_images_with_no_accepted_encoding_are_metadata() {
    let bytes = pdf(&[page(&["Odd images"], &[PdfImage::Jpx(60, 60), PdfImage::Cmyk(60, 60)])]);
    assert_eq!(
        shape(&blocks(DocumentKind::Pdf, bytes)),
        vec!["T:Odd images", "M1:image/jp2", "M2:application/octet-stream"]
    );
}

// CMS-FR-NHCE: a side of 49 pixels is skipped and leaves nothing; a side of
// 50 pixels is kept. The ordinal counts the kept images only.
#[test]
fn an_image_with_a_side_under_50_pixels_gives_no_block() {
    let bytes = pdf(&[page(
        &["Sizes"],
        &[PdfImage::Rgb(49, 300), PdfImage::Rgb(50, 50), PdfImage::Rgb(300, 49)],
    )]);
    assert_eq!(shape(&blocks(DocumentKind::Pdf, bytes)), vec!["T:Sizes", "I1:image/png"]);
    let bytes = docx(&[para("Sizes"), DocxItem::Image(png(49, 49)), DocxItem::Image(png(50, 80))]);
    assert_eq!(shape(&blocks(DocumentKind::Docx, bytes)), vec!["T:Sizes", "I1:image/png"]);
}

// CMS-FR-NHCE: of 21 images, the first 20 give image blocks and the 21st a
// metadata block in its place; text after it stays after it.
#[test]
fn the_21st_image_of_a_document_is_metadata() {
    let mut items = vec![para("Start")];
    items.extend((0..21).map(|_| DocxItem::Image(png(60, 60))));
    items.push(para("End"));
    let found = shape(&blocks(DocumentKind::Docx, docx(&items)));
    assert_eq!(found.len(), 23);
    assert_eq!(found[0], "T:Start");
    for (i, block) in found[1..21].iter().enumerate() {
        assert_eq!(block, &format!("I{}:image/png", i + 1));
    }
    assert_eq!(found[21], "M21:image/png");
    assert_eq!(found[22], "T:End");
}

// CMS-FR-WIIE: paragraphs and table cells in document order, and each drawing
// in its position.
#[test]
fn a_docx_gives_paragraphs_tables_and_drawings_in_order() {
    let bytes = docx(&[
        para("Intro"),
        DocxItem::Image(jpeg(90, 60)),
        DocxItem::Table(vec!["Cell A".to_string(), "Cell B".to_string()]),
        DocxItem::Image(png(60, 90)),
        para("Outro"),
    ]);
    assert_eq!(
        shape(&blocks(DocumentKind::Docx, bytes)),
        vec!["T:Intro", "I1:image/jpeg", "T:Cell A Cell B", "I2:image/png", "T:Outro"]
    );
}

// CMS-FR-PTKT: a DOCX image in another format uses the PNG preview of
// `docx-rs`.
#[test]
fn a_docx_bmp_image_is_sent_as_its_png_preview() {
    let bytes = docx(&[para("Bitmap"), DocxItem::Image(bmp(70, 60))]);
    let found = blocks(DocumentKind::Docx, bytes);
    let DocBlock::Image { media_type, data, .. } = &found[1] else {
        panic!("an image block: {found:?}");
    };
    assert_eq!(media_type, "image/png");
    assert_eq!(image::guess_format(data).unwrap(), image::ImageFormat::Png);
}

// CMS-FR-LZZV: a document whose text is empty after trimming is NoText, also
// when it holds images.
#[test]
fn a_document_with_no_text_is_no_text() {
    let pdf_bytes = pdf(&[page(&[], &[PdfImage::Rgb(200, 200)])]);
    assert_eq!(extract(DocumentKind::Pdf, pdf_bytes), Err(ExtractError::NoText));
    let docx_bytes = docx(&[para("   "), DocxItem::Image(png(80, 80))]);
    assert_eq!(extract(DocumentKind::Docx, docx_bytes), Err(ExtractError::NoText));
}

// CMS-FR-LZZV: bytes no parser reads are unreadable.
#[test]
fn corrupt_bytes_are_unreadable() {
    for kind in [DocumentKind::Pdf, DocumentKind::Docx] {
        let result = extract(kind, b"this is not a document".to_vec());
        assert!(matches!(result, Err(ExtractError::Unreadable(_))), "{kind:?}: {result:?}");
    }
    let mut cut = text_pdf("A cut file");
    cut.truncate(cut.len() / 3);
    assert!(matches!(extract(DocumentKind::Pdf, cut), Err(ExtractError::Unreadable(_))));
}

fn panicking_parser(_: DocumentKind, _: &[u8]) -> Result<Vec<RawItem>, ExtractError> {
    panic!("a parser defect");
}

/// A parser that never ends, so the deadline decides whatever the machine.
fn endless_parser(_: DocumentKind, _: &[u8]) -> Result<Vec<RawItem>, ExtractError> {
    loop {
        std::thread::park();
    }
}

// CMS-FR-DUGE, CMS-FR-LZZV, CMS-FR-RQTB: a parser panic and a parser past its
// deadline stop that extraction only. A panic is unreadable; a deadline is
// an interruption, which maps to `document_unreadable` and is not cached.
#[test]
fn a_panic_and_a_deadline_are_unreadable() {
    let result =
        extract_with(DocumentKind::Pdf, vec![1], Duration::from_secs(10), panicking_parser);
    assert!(matches!(result, Err(ExtractError::Unreadable(ref r)) if r.contains("panicked")));
    let result =
        extract_with(DocumentKind::Docx, vec![1], Duration::from_millis(50), endless_parser);
    assert!(matches!(result, Err(ExtractError::Interrupted(ref r)) if r.contains("deadline")));
    let mut runs = 0;
    for _ in 0..2 {
        let outcome = cache::get_or_extract("deadline-test-digest", || {
            runs += 1;
            Err(ExtractError::Interrupted("extraction ran past its deadline".to_string()))
        });
        assert!(outcome.is_err());
    }
    assert_eq!(runs, 2, "an interrupted extraction is tried again");
    // The next extraction in the same process still works.
    assert!(extract(DocumentKind::Pdf, text_pdf("ZQX-after-panic")).is_ok());
}

// CMS-FR-YSRD: the length the limit applies to is the UTF-8 length of all
// text blocks.
#[test]
fn text_bytes_count_utf8_bytes_of_the_text_blocks() {
    let document = ExtractedDocument {
        blocks: vec![
            DocBlock::Text("añb".to_string()),
            DocBlock::Image { media_type: "image/png".to_string(), data: vec![0; 99], ordinal: 1 },
            DocBlock::Text("€".to_string()),
        ],
    };
    assert_eq!(document.text_bytes(), 4 + 3);
}

// CMS-FR-EHOG: only PDF and DOCX are document attachments, whatever the case
// and the parameters of the media type.
#[test]
fn only_pdf_and_docx_are_documents() {
    assert_eq!(DocumentKind::of_media_type("Application/PDF"), Some(DocumentKind::Pdf));
    assert_eq!(
        DocumentKind::of_media_type(&format!("{DOCX_MEDIA_TYPE}; charset=x")),
        Some(DocumentKind::Docx)
    );
    for other in ["text/plain", "image/png", "application/msword", ""] {
        assert_eq!(DocumentKind::of_media_type(other), None, "{other}");
    }
}

// CMS-FR-LZZV: a PDF that a non-empty password encrypts is unreadable; one
// that the empty user password opens is read.
#[test]
fn a_pdf_with_a_user_password_is_unreadable_and_an_owner_only_pdf_is_read() {
    let locked = encrypted_pdf("ZQX-locked", "owner", "user");
    assert!(matches!(extract(DocumentKind::Pdf, locked), Err(ExtractError::Unreadable(_))));
    let open = encrypted_pdf("ZQX-owner-only", "owner", "");
    let found = blocks(DocumentKind::Pdf, open);
    assert!(matches!(&found[0], DocBlock::Text(t) if t.contains("ZQX-owner-only")), "{found:?}");
}

fn raw(size: Option<(u32, u32)>, data: bool) -> RawItem {
    RawItem::Image(RawImage {
        size,
        media_type: "image/png".to_string(),
        data: data.then(|| vec![1, 2, 3]),
    })
}

// CMS-FR-NHCE: an image of unknown size is never skipped; a skipped image
// takes no ordinal; an image with no accepted encoding takes an ordinal and
// one of the 20 places.
#[test]
fn the_image_rules_count_ordinals_over_the_kept_images() {
    let text = || RawItem::Text("words".to_string());
    let found = finish(vec![
        text(),
        raw(Some((10, 10)), true),
        raw(None, true),
        raw(Some((60, 60)), false),
        raw(Some((60, 60)), true),
    ])
    .unwrap();
    assert_eq!(
        shape(&found.blocks),
        vec!["T:words", "I1:image/png", "M2:image/png", "I3:image/png"]
    );
    assert_eq!(MIN_IMAGE_SIDE, 50);
    assert_eq!(MAX_DOCUMENT_IMAGES, 20);
    assert_eq!(EXTRACT_DEADLINE, Duration::from_secs(30));
}

// CMS-FR-LGXL: each page keeps its own text block.
#[test]
fn each_pdf_page_keeps_its_own_text_block() {
    let bytes = pdf(&[page(&["Page one"], &[]), page(&["Page two"], &[])]);
    assert_eq!(shape(&blocks(DocumentKind::Pdf, bytes)), vec!["T:Page one", "T:Page two"]);
}

// CMS-FR-LZZV: text above the largest limit stops the extraction as too
// large, which the append refuses as `extracted_text_too_large`.
#[test]
fn text_above_the_largest_limit_is_too_large() {
    let bytes = docx(&[para(&"x".repeat(MAX_TEXT_BYTES))]);
    assert_eq!(extract(DocumentKind::Docx, bytes), Err(ExtractError::TooLarge));
}

// CMS-FR-RQTB, CMS-FR-DUGE: at most four extractions run at one time; a
// worker abandoned at its deadline still counts, and a fifth extraction is
// interrupted without a worker.
#[test]
fn at_most_four_extractions_run_at_one_time() {
    for _ in 0..MAX_LIVE_EXTRACTIONS {
        let result =
            extract_with(DocumentKind::Pdf, vec![1], Duration::from_millis(20), endless_parser);
        assert!(matches!(result, Err(ExtractError::Interrupted(ref r)) if r.contains("deadline")));
    }
    let result =
        extract_with(DocumentKind::Pdf, vec![1], Duration::from_secs(10), panicking_parser);
    assert!(
        matches!(result, Err(ExtractError::Interrupted(ref r)) if r.contains("too many")),
        "{result:?}"
    );
}

// CMS-FR-WIIE: the header, the footer, and the comments of a DOCX are not
// read; the body is.
#[test]
fn a_docx_header_footer_and_comments_are_not_read() {
    let bytes = docx_with_parts("ZQX-body", "ZQX-header", "ZQX-footer", "ZQX-comment");
    let text: String = blocks(DocumentKind::Docx, bytes)
        .iter()
        .filter_map(|b| match b {
            DocBlock::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert!(text.contains("ZQX-body"));
    for absent in ["ZQX-header", "ZQX-footer", "ZQX-comment"] {
        assert!(!text.contains(absent), "{absent} was read: {text}");
    }
}

// CMS-FR-PTKT: a DOCX PNG, JPEG, or GIF is sent with its own bytes.
#[test]
fn docx_png_jpeg_and_gif_images_are_sent_as_they_are() {
    let images = [png(60, 61), jpeg(62, 63), gif(64, 65)];
    let mut items = vec![para("Three")];
    items.extend(images.iter().cloned().map(DocxItem::Image));
    let found = blocks(DocumentKind::Docx, docx(&items));
    let sent: Vec<(&str, &Vec<u8>)> = found
        .iter()
        .filter_map(|b| match b {
            DocBlock::Image { media_type, data, .. } => Some((media_type.as_str(), data)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        vec![("image/png", &images[0]), ("image/jpeg", &images[1]), ("image/gif", &images[2])]
    );
}
