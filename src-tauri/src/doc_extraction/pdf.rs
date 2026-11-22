//! CMS-FR-LGXL, CMS-FR-PTKT: the text and the images of a PDF.
//!
//! The text of each page comes from `pdf-extract`. The images come from the
//! `lopdf` parser that `pdf-extract` re-exports, in the order the page content
//! draws them, so no other crate reads the PDF.

use std::collections::{BTreeMap, HashSet};

use pdf_extract::content::Content;
use pdf_extract::{Dictionary, Document, Object, ObjectId, Stream};

use super::{
    ExtractError, RawImage, RawItem, MAX_DOCUMENT_IMAGES, MAX_RAW_IMAGE_BYTES, MAX_TEXT_BYTES,
    MIN_IMAGE_SIDE,
};

/// How deep a form inside a form is followed for the images it draws.
const MAX_FORM_DEPTH: usize = 8;

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<RawItem>, ExtractError> {
    let mut document =
        Document::load_mem(bytes).map_err(|e| ExtractError::Unreadable(format!("pdf: {e}")))?;
    // As `pdf-extract` does: a document that the empty password opens is
    // read; one that needs another password is unreadable.
    if document.is_encrypted() && document.decrypt("").is_err() {
        return Err(ExtractError::Unreadable("pdf: encrypted".to_string()));
    }
    let pages = document.get_pages();
    // CMS-FR-NHCE: only the first 20 images that are not too small are
    // decoded; the later ones are metadata whatever their bytes hold.
    let mut kept = 0usize;
    let images: Vec<Vec<RawImage>> =
        pages.values().map(|page_id| page_images(&document, *page_id, &mut kept)).collect();
    // `pdf-extract` reads an image XObject as page content, and its parser can
    // panic on image bytes. The text pass reads a copy in which every image is
    // an empty form, so the text is the same and the images cannot stop it.
    neutralise_images(&mut document);
    let mut items = Vec::new();
    let mut text_bytes = 0usize;
    for ((page_num, _), page_images) in pages.iter().zip(images) {
        let mut text = String::new();
        {
            let mut output = pdf_extract::PlainTextOutput::new(&mut text);
            pdf_extract::output_doc_page(&document, &mut output, *page_num)
                .map_err(|e| ExtractError::Unreadable(format!("pdf text: {e}")))?;
        }
        // CMS-FR-LZZV: text above the largest limit is refused here, so a
        // content-stream bomb does not grow without bound.
        text_bytes += text.len();
        if text_bytes > MAX_TEXT_BYTES {
            return Err(ExtractError::TooLarge);
        }
        items.push(RawItem::Text(text));
        items.extend(page_images.into_iter().map(RawItem::Image));
    }
    Ok(items)
}

/// Replaces each image XObject with an empty form XObject.
fn neutralise_images(document: &mut Document) {
    let ids: Vec<ObjectId> = document
        .objects
        .iter()
        .filter(|(_, object)| {
            object.as_stream().is_ok_and(|stream| {
                stream.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image")
            })
        })
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Form".to_vec()));
        dict.set("BBox", Object::Array(vec![0.into(), 0.into(), 1.into(), 1.into()]));
        document.objects.insert(id, Object::Stream(Stream::new(dict, Vec::new())));
    }
}

/// The images one page draws, in draw order, each once.
fn page_images(document: &Document, page_id: ObjectId, kept: &mut usize) -> Vec<RawImage> {
    let Ok(content) = document.get_page_content(page_id) else {
        return Vec::new();
    };
    let mut names = BTreeMap::new();
    if let Ok((own, inherited)) = document.get_page_resources(page_id) {
        if let Some(resources) = own {
            add_xobject_names(document, resources, &mut names);
        }
        for id in inherited {
            if let Ok(resources) = document.get_dictionary(id) {
                add_xobject_names(document, resources, &mut names);
            }
        }
    }
    let mut seen = HashSet::new();
    let mut images = Vec::new();
    walk(document, &content, &names, &mut seen, &mut images, kept, 0);
    images
}

fn add_xobject_names(
    document: &Document,
    resources: &Dictionary,
    names: &mut BTreeMap<Vec<u8>, ObjectId>,
) {
    if let Ok(xobjects) = document.get_dict_in_dict(resources, b"XObject") {
        for (name, value) in xobjects.iter() {
            if let Ok(id) = value.as_reference() {
                names.entry(name.clone()).or_insert(id);
            }
        }
    }
}

fn walk(
    document: &Document,
    content: &[u8],
    names: &BTreeMap<Vec<u8>, ObjectId>,
    seen: &mut HashSet<ObjectId>,
    images: &mut Vec<RawImage>,
    kept: &mut usize,
    depth: usize,
) {
    let Ok(content) = Content::decode(content) else {
        return;
    };
    for operation in content.operations {
        if operation.operator != "Do" {
            continue;
        }
        let Some(name) = operation.operands.first().and_then(|o| o.as_name().ok()) else {
            continue;
        };
        let Some(id) = names.get(name).copied() else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let Ok(stream) = document.get_object(id).and_then(Object::as_stream) else {
            continue;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => images.push(raw_image(stream, kept)),
            Ok(b"Form") if depth < MAX_FORM_DEPTH => {
                // The form's own names win over the names of the page.
                let mut inner = BTreeMap::new();
                if let Some(resources) = form_resources(document, &stream.dict) {
                    add_xobject_names(document, resources, &mut inner);
                }
                for (name, id) in names {
                    inner.entry(name.clone()).or_insert(*id);
                }
                let body = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                walk(document, &body, &inner, seen, images, kept, depth + 1);
            }
            _ => {}
        }
    }
}

fn form_resources<'a>(document: &'a Document, dict: &'a Dictionary) -> Option<&'a Dictionary> {
    match dict.get(b"Resources").ok()? {
        Object::Dictionary(resources) => Some(resources),
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        _ => None,
    }
}

fn filters(dict: &Dictionary) -> Vec<Vec<u8>> {
    match dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(array)) => {
            array.iter().filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec)).collect()
        }
        _ => Vec::new(),
    }
}

fn dimension(dict: &Dictionary, key: &[u8]) -> Option<u32> {
    dict.get(key).ok()?.as_i64().ok().and_then(|v| u32::try_from(v).ok())
}

/// CMS-FR-PTKT: the bytes a model accepts for one PDF image, or its metadata.
///
/// A too small image and an image past the cap are not decoded (CMS-FR-NHCE),
/// and raw samples above a fixed size are never decompressed.
fn raw_image(stream: &Stream, kept: &mut usize) -> RawImage {
    let dict = &stream.dict;
    let size = match (dimension(dict, b"Width"), dimension(dict, b"Height")) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };
    let filters = filters(dict);
    let tiny = size.is_some_and(|(w, h)| w.min(h) < MIN_IMAGE_SIDE);
    if !tiny {
        *kept += 1;
    }
    let decode = !tiny && *kept <= MAX_DOCUMENT_IMAGES;
    let names: Vec<&[u8]> = filters.iter().map(Vec::as_slice).collect();
    match names.as_slice() {
        [b"DCTDecode"] => RawImage {
            size,
            media_type: "image/jpeg".to_string(),
            data: decode.then(|| stream.content.clone()),
        },
        [b"JPXDecode"] => RawImage { size, media_type: "image/jp2".to_string(), data: None },
        [] | [b"FlateDecode"] => {
            let encodable = size.is_some_and(|(w, h)| raw_length(dict, w, h).is_some());
            if !encodable {
                // Raw samples in another color space have no media type.
                let media_type = "application/octet-stream".to_string();
                return RawImage { size, media_type, data: None };
            }
            if !decode {
                return RawImage { size, media_type: "image/png".to_string(), data: None };
            }
            let raw = if names.is_empty() {
                Some(stream.content.clone())
            } else {
                stream.decompressed_content().ok()
            };
            let data = match (raw, size) {
                (Some(raw), Some((w, h))) => encode_png(dict, &raw, w, h),
                _ => None,
            };
            RawImage { size, media_type: "image/png".to_string(), data }
        }
        _ => RawImage { size, media_type: "application/octet-stream".to_string(), data: None },
    }
}

/// The color type of a DeviceRGB or DeviceGray image with 8 bits per
/// component, and the length of its samples, where that length is within
/// [`MAX_RAW_IMAGE_BYTES`].
fn raw_length(
    dict: &Dictionary,
    width: u32,
    height: u32,
) -> Option<(image::ExtendedColorType, usize)> {
    use image::ExtendedColorType;
    if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 {
        return None;
    }
    let (channels, color) = match dict.get(b"ColorSpace").ok()?.as_name().ok()? {
        b"DeviceRGB" => (3usize, ExtendedColorType::Rgb8),
        b"DeviceGray" => (1usize, ExtendedColorType::L8),
        _ => return None,
    };
    let needed = (width as usize).checked_mul(height as usize)?.checked_mul(channels)?;
    (needed > 0 && needed <= MAX_RAW_IMAGE_BYTES).then_some((color, needed))
}

/// A DeviceRGB or DeviceGray image with 8 bits per component, as PNG.
fn encode_png(dict: &Dictionary, raw: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    use image::{codecs::png::PngEncoder, ImageEncoder};
    let (color, needed) = raw_length(dict, width, height)?;
    if raw.len() < needed {
        return None;
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(&raw[..needed], width, height, color).ok()?;
    Some(out)
}
