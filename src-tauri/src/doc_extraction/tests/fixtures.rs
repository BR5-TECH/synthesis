//! Documents built in the test, so no fixture file holds real content.
//!
//! A PDF is written with the `lopdf` that `pdf-extract` re-exports, and a DOCX
//! with the writer of `docx-rs`. Images are encoded with `image`.

use std::io::Cursor;

use pdf_extract::content::{Content, Operation};
use pdf_extract::{Dictionary, Document, Object, ObjectId, Stream};

/// A raster image of `width` × `height` in one format.
pub fn png(width: u32, height: u32) -> Vec<u8> {
    encode(width, height, image::ImageFormat::Png)
}

pub fn jpeg(width: u32, height: u32) -> Vec<u8> {
    encode(width, height, image::ImageFormat::Jpeg)
}

pub fn bmp(width: u32, height: u32) -> Vec<u8> {
    encode(width, height, image::ImageFormat::Bmp)
}

fn encode(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
    });
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut out, format).unwrap();
    out.into_inner()
}

/// One image a PDF page draws.
#[derive(Clone, Copy, Debug)]
pub enum PdfImage {
    /// Raw DeviceRGB samples, 8 bits per component, no filter.
    Rgb(u32, u32),
    /// Raw DeviceGray samples, compressed with FlateDecode.
    GrayFlate(u32, u32),
    /// A JPEG stream with DCTDecode.
    Jpeg(u32, u32),
    /// A JPXDecode stream, which no model accepts.
    Jpx(u32, u32),
    /// Raw DeviceCMYK samples, which no model accepts.
    Cmyk(u32, u32),
}

/// One page of a PDF: its text lines, then the images it draws. `twice` draws
/// the first image a second time.
#[derive(Clone, Debug, Default)]
pub struct PdfPage {
    pub lines: Vec<String>,
    pub images: Vec<PdfImage>,
    pub twice: bool,
}

pub fn page(lines: &[&str], images: &[PdfImage]) -> PdfPage {
    PdfPage {
        lines: lines.iter().map(|l| l.to_string()).collect(),
        images: images.to_vec(),
        twice: false,
    }
}

fn name(n: &str) -> Object {
    Object::Name(n.as_bytes().to_vec())
}

fn image_stream(image: PdfImage) -> Stream {
    let (w, h, space, filter, data): (u32, u32, &str, Option<&str>, Vec<u8>) = match image {
        PdfImage::Rgb(w, h) => (w, h, "DeviceRGB", None, vec![120; (w * h * 3) as usize]),
        PdfImage::GrayFlate(w, h) => (w, h, "DeviceGray", Some("FlateDecode"), Vec::new()),
        PdfImage::Jpeg(w, h) => (w, h, "DeviceRGB", Some("DCTDecode"), jpeg(w, h)),
        PdfImage::Jpx(w, h) => (w, h, "DeviceRGB", Some("JPXDecode"), vec![0, 0, 0, 12]),
        PdfImage::Cmyk(w, h) => (w, h, "DeviceCMYK", None, vec![40; (w * h * 4) as usize]),
    };
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Image"));
    dict.set("Width", Object::Integer(i64::from(w)));
    dict.set("Height", Object::Integer(i64::from(h)));
    dict.set("ColorSpace", name(space));
    dict.set("BitsPerComponent", Object::Integer(8));
    if let PdfImage::GrayFlate(w, h) = image {
        let mut stream = Stream::new(dict, vec![80; (w * h) as usize]);
        stream.compress().unwrap();
        assert!(stream.dict.has(b"Filter"), "the gray image is compressed");
        return stream;
    }
    if let Some(filter) = filter {
        dict.set("Filter", name(filter));
    }
    Stream::new(dict, data).with_compression(false)
}

/// A PDF of the given pages, with the Courier font of the standard 14.
pub fn pdf(pages: &[PdfPage]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id: ObjectId = doc.new_object_id();
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type1"));
    font.set("BaseFont", name("Courier"));
    font.set("Encoding", name("WinAnsiEncoding"));
    let font_id = doc.add_object(font);
    let mut kids = Vec::new();
    for page in pages {
        let mut ops = Vec::new();
        let mut y = 760;
        for line in &page.lines {
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new("Tf", vec![name("F1"), 12.into()]));
            ops.push(Operation::new("Td", vec![72.into(), y.into()]));
            ops.push(Operation::new("Tj", vec![Object::string_literal(line.as_str())]));
            ops.push(Operation::new("ET", vec![]));
            y -= 20;
        }
        let mut xobjects = Dictionary::new();
        let mut draws = Vec::new();
        for (i, image) in page.images.iter().enumerate() {
            let id = doc.add_object(image_stream(*image));
            let key = format!("Im{i}");
            xobjects.set(key.as_str(), Object::Reference(id));
            draws.push(key);
        }
        if page.twice {
            if let Some(first) = draws.first().cloned() {
                draws.push(first);
            }
        }
        for key in draws {
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "cm",
                vec![100.into(), 0.into(), 0.into(), 100.into(), 72.into(), 100.into()],
            ));
            ops.push(Operation::new("Do", vec![name(&key)]));
            ops.push(Operation::new("Q", vec![]));
        }
        let content = Content { operations: ops };
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        resources.set("XObject", Object::Dictionary(xobjects));
        let mut page_dict = Dictionary::new();
        page_dict.set("Type", name("Page"));
        page_dict.set("Parent", Object::Reference(pages_id));
        page_dict.set("Contents", Object::Reference(content_id));
        page_dict.set("Resources", Object::Dictionary(resources));
        page_dict.set("MediaBox", Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
        kids.push(Object::Reference(doc.add_object(page_dict)));
    }
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", name("Pages"));
    pages_dict.set("Count", Object::Integer(kids.len() as i64));
    pages_dict.set("Kids", Object::Array(kids));
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let mut catalog = Dictionary::new();
    catalog.set("Type", name("Catalog"));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// A PDF of one page that holds `text`.
pub fn text_pdf(text: &str) -> Vec<u8> {
    pdf(&[page(&[text], &[])])
}

/// One item of a DOCX body.
#[derive(Clone, Debug)]
pub enum DocxItem {
    Paragraph(String),
    /// A paragraph with one drawing of these image bytes.
    Image(Vec<u8>),
    /// A table of one row, one cell per text.
    Table(Vec<String>),
}

pub fn para(text: &str) -> DocxItem {
    DocxItem::Paragraph(text.to_string())
}

/// A DOCX of the given body items.
pub fn docx(items: &[DocxItem]) -> Vec<u8> {
    use docx_rs::{Docx, Paragraph, Pic, Run, Table, TableCell, TableRow};
    let mut doc = Docx::new();
    for item in items {
        doc = match item {
            DocxItem::Paragraph(text) => {
                doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)))
            }
            DocxItem::Image(bytes) => {
                // The bytes stay as they are: `Pic::new` would convert them to PNG.
                let (w, h) = image::ImageReader::new(Cursor::new(bytes))
                    .with_guessed_format()
                    .ok()
                    .and_then(|r| r.into_dimensions().ok())
                    .unwrap_or((100, 100));
                let pic = Pic::new_with_dimensions(bytes.clone(), w, h);
                doc.add_paragraph(Paragraph::new().add_run(Run::new().add_image(pic)))
            }
            DocxItem::Table(cells) => {
                let cells = cells
                    .iter()
                    .map(|text| {
                        TableCell::new()
                            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)))
                    })
                    .collect();
                doc.add_table(Table::new(vec![TableRow::new(cells)]))
            }
        };
    }
    let mut out = Cursor::new(Vec::new());
    doc.build().pack(&mut out).unwrap();
    out.into_inner()
}

/// A DOCX of one paragraph that holds `text`.
pub fn text_docx(text: &str) -> Vec<u8> {
    docx(&[para(text)])
}

/// A one-page PDF of `text`, encrypted with these passwords.
pub fn encrypted_pdf(text: &str, owner: &str, user: &str) -> Vec<u8> {
    use pdf_extract::{EncryptionState, EncryptionVersion, Permissions};
    let mut doc = Document::load_mem(&text_pdf(text)).unwrap();
    let id = Object::String(b"0123456789abcdef".to_vec(), pdf_extract::StringFormat::Literal);
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: owner,
        user_password: user,
        key_length: 128,
        permissions: Permissions::default(),
    };
    let state = EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

pub fn gif(width: u32, height: u32) -> Vec<u8> {
    encode(width, height, image::ImageFormat::Gif)
}

/// A DOCX whose body holds `body`, and whose header, footer, and comment hold
/// the other markers.
pub fn docx_with_parts(body: &str, header: &str, footer: &str, comment: &str) -> Vec<u8> {
    use docx_rs::{Comment, Docx, Footer, Header, Paragraph, Run};
    let text = |t: &str| Paragraph::new().add_run(Run::new().add_text(t));
    let doc = Docx::new()
        .header(Header::new().add_paragraph(text(header)))
        .footer(Footer::new().add_paragraph(text(footer)))
        .add_paragraph(
            text(body)
                .add_comment_start(Comment::new(1).add_paragraph(text(comment)))
                .add_comment_end(1),
        );
    let mut out = Cursor::new(Vec::new());
    doc.build().pack(&mut out).unwrap();
    out.into_inner()
}
