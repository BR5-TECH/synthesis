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
    let mut body = String::new();
    let mut all = String::new();
    for path in site.files() {
        let Ok(at) = site.file_path(&path) else {
            continue;
        };
        let bytes = site.working_copy.read_bytes(&at).unwrap_or_default();
        body.push_str(&format!("--- {path} ---\n"));
        match String::from_utf8(bytes) {
            // AGC-FR-HETC: a file that is not text is named, with no content.
            Err(_) => body.push_str("[This file is not text. Its content is not included.]\n\n"),
            Ok(text) => {
                let size = DocumentSize::of(&text);
                // AGC-FR-NBZN: each file is whole where it fits the bound.
                if size.characters as usize <= bound {
                    body.push_str(&text);
                    if !text.ends_with('\n') {
                        body.push('\n');
                    }
                    body.push('\n');
                } else {
                    body.push_str(&metadata_line(&size));
                    body.push_str("\n\n");
                }
                all.push_str(&text);
            }
        }
    }
    let attributes =
        vec![("change".to_string(), change.to_string()), ("stream".to_string(), stream_name)];
    let mut section = section_of_parts_within(
        TAG_OPENSPEC_CHANGE,
        attributes,
        vec![InputPart::Text(body)],
        usize::MAX,
    );
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
