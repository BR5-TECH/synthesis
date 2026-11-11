//! The material of a `change` target (AGC-FR-NBZN, AGC-FR-HETC, AGC-FR-OYJX).
//!
//! The section `openspec_change` holds the change name, the stream name, and
//! every file of the change folder in the stream's working copy, in path order.
//! The files are read through FSA, and no CLI process runs.

use super::*;

/// AGC-FR-NBZN: the tag of the material section of a change.
pub(crate) const TAG_OPENSPEC_CHANGE: &str = "openspec_change";

/// AGC-FR-NBZN / AGC-FR-HETC: the section of one change, or `None` where the
/// change cannot be found (AGC-FR-OYJX).
pub(super) fn change_material(
    stream_id: &str,
    change: &str,
    budget: &DocumentBudget,
) -> Option<Material> {
    let (stream_name, site) = crate::change_proposals::change_site_now(stream_id, change).ok()?;
    let bound = budget.bound_chars();
    let mut parts: Vec<InputPart> = Vec::new();
    let mut all = String::new();
    for path in site.files() {
        let Ok(at) = site.file_path(&path) else {
            continue;
        };
        let bytes = site.working_copy.read_bytes(&at).unwrap_or_default();
        parts.push(InputPart::Text(format!("--- {path} ---\n")));
        match String::from_utf8(bytes) {
            // AGC-FR-HETC: a file that is not text is named, with no content.
            Err(_) => parts.push(InputPart::Text(
                "[This file is not text. Its content is not included.]\n\n".to_string(),
            )),
            Ok(text) => {
                let size = DocumentSize::of(&text);
                // AGC-FR-NBZN: each file is whole where it fits the bound.
                if size.characters as usize <= bound {
                    // AGC-FR-WMIY: the file's text with its images in place.
                    parts.extend(file_parts(&site, &path, &text));
                    let close = if text.ends_with('\n') { "\n" } else { "\n\n" };
                    parts.push(InputPart::Text(close.to_string()));
                } else {
                    // AGC-FR-DLZE: a file left out contributes no image.
                    parts.push(InputPart::Text(format!("{}\n\n", metadata_line(&size))));
                }
                all.push_str(&text);
            }
        }
    }
    let attributes =
        vec![("change".to_string(), change.to_string()), ("stream".to_string(), stream_name)];
    let mut section = section_of_parts_within(TAG_OPENSPEC_CHANGE, attributes, parts, usize::MAX);
    section.truncated = false;
    Some(Material { section: Some(section), document: Some(DocumentSize::of(&all)) })
}

/// AGC-FR-TKXH: the text the document language of a change is detected from:
/// `proposal.md`, or for a fragment discussion the file of the fragment.
pub(crate) fn change_language_text(
    stream_id: &str,
    change: &str,
    fragment: Option<&comments::FragmentTarget>,
) -> Option<String> {
    let (_, site) = crate::change_proposals::change_site_now(stream_id, change).ok()?;
    let path = fragment.map(|f| f.path.as_str()).unwrap_or("proposal.md");
    site.read_file(path)
}

/// AGC-FR-WMIY / AGC-FR-DLZE: one file's text, with each image it references
/// after the reference. A reference that resolves inside the change folder to
/// an image file contributes the image; any other reference contributes a line
/// of metadata and no bytes.
fn file_parts(
    site: &crate::change_proposals::ChangeSite,
    path: &str,
    text: &str,
) -> Vec<InputPart> {
    let mut parts = Vec::new();
    let mut cursor = 0usize;
    for image in crate::draft_assets::read_references(text).images {
        let fits = image.start >= cursor
            && image.end <= text.len()
            && text.is_char_boundary(image.start)
            && text.is_char_boundary(image.end);
        if !fits {
            continue;
        }
        parts.push(InputPart::Text(text[cursor..image.end].to_string()));
        cursor = image.end;
        let context = format!("{path}: ![{}]({})", image.alt, image.reference);
        match change_image(site, path, &image.reference) {
            Some((data, media_type)) => {
                let metadata = format!("[image: {context} ({media_type}), not shown]");
                parts.push(InputPart::Image(InputImage { data, media_type, context, metadata }));
            }
            None => parts.push(InputPart::Text(format!("[image: {context}, not resolved]"))),
        }
    }
    parts.push(InputPart::Text(text[cursor..].to_string()));
    parts
}

/// AGC-FR-WMIY: the bytes and the media type of an image reference that stays
/// inside the change folder, relative to the file that holds it.
fn change_image(
    site: &crate::change_proposals::ChangeSite,
    from: &str,
    reference: &str,
) -> Option<(String, String)> {
    use base64::Engine as _;
    let dest = reference.split(['#', '?']).next().unwrap_or("").trim();
    if dest.is_empty() || dest.contains(':') || dest.starts_with('/') || dest.starts_with('\\') {
        return None;
    }
    // Resolved against the folder of the file, and refused where a `..`
    // segment would leave the change folder.
    let mut segments: Vec<&str> = from.split('/').collect();
    segments.pop();
    for segment in dest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    let rel = segments.join("/");
    let extension = rel.rsplit_once('.').map(|(_, e)| e)?;
    let media_type = crate::draft_assets::image_media_type_for_extension(extension)?;
    let at = site.file_path(&rel).ok()?;
    let bytes = site.working_copy.read_bytes(&at).ok()?;
    Some((base64::engine::general_purpose::STANDARD.encode(bytes), media_type.to_string()))
}
