//! The `artifact` section and the size of the document it holds (AGC-FR-07,
//! AGC-FR-XHHT, AGC-FR-NEQL, AGC-FR-BORR, AGC-FR-IJSE, AGC-FR-VAIY,
//! AGC-FR-JCPO).
//!
//! A document at or below the bound goes in whole. Above it, a discussion of a
//! fragment gets the passage around that fragment, and a discussion of the
//! whole target gets the document's size and no text. The agent then reads the
//! parts it needs.

use super::*;

/// The material section of a file or a draft, and the size of the whole
/// document it was built from. Both are `None` where the document could not be
/// read (AGC-FR-12).
#[derive(Default)]
pub(super) struct Material {
    pub section: Option<InputSection>,
    pub document: Option<DocumentSize>,
}

/// AGC-FR-NEQL, AGC-FR-BORR, AGC-FR-VAIY: how a document fills the section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fit {
    /// The whole text.
    Whole,
    /// The byte range `start..end` of the text, which holds the lines
    /// `first_line..=last_line` (0-based).
    Passage { start: usize, end: usize, first_line: usize, last_line: usize },
    /// No text at all.
    MetadataOnly,
}

/// The byte range of each line of `text`, with its terminator.
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        ranges.push((start, start + line.len()));
        start += line.len();
    }
    ranges
}

/// The byte offset of the character at `chars`, clamped to the end of `text`.
fn byte_of_char(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map(|(byte, _)| byte).unwrap_or(text.len())
}

/// AGC-FR-NEQL, AGC-FR-BORR, AGC-FR-IJSE, AGC-FR-VAIY: decide how `text` fills
/// a section bound to `bound` characters. `fragment` is the discussion's
/// fragment as character offsets, where it has one.
pub(super) fn fit(text: &str, fragment: Option<(usize, usize)>, bound: usize) -> Fit {
    if text.chars().count() <= bound {
        return Fit::Whole;
    }
    let Some((frag_start, frag_end)) = fragment else {
        return Fit::MetadataOnly;
    };
    let lines = line_ranges(text);
    if lines.is_empty() {
        return Fit::MetadataOnly;
    }
    let start_byte = byte_of_char(text, frag_start);
    // The last character the fragment covers. An empty fragment covers the
    // line its start is on.
    let end_byte = byte_of_char(text, frag_end.max(frag_start + 1) - 1);
    let line_of = |byte: usize| {
        lines.iter().position(|(s, e)| byte >= *s && byte < *e).unwrap_or(lines.len() - 1)
    };
    let mut first = line_of(start_byte);
    let mut last = line_of(end_byte).max(first);
    let chars_of = |a: usize, b: usize| text[lines[a].0..lines[b].1].chars().count();
    let mut size = chars_of(first, last);
    if size > bound {
        // AGC-FR-IJSE: the fragment's own lines exceed the bound, so the passage
        // starts at the fragment's first line and ends at the bound.
        let start = lines[first].0;
        let end = start + byte_of_char(&text[start..], bound);
        let last_line = if end > start { line_of(end - 1) } else { first };
        return Fit::Passage { start, end, first_line: first, last_line };
    }
    // AGC-FR-BORR: whole lines on both sides while the bound allows.
    loop {
        let mut grew = false;
        if first > 0 {
            let more = chars_of(first - 1, first - 1);
            if size + more <= bound {
                first -= 1;
                size += more;
                grew = true;
            }
        }
        if last + 1 < lines.len() {
            let more = chars_of(last + 1, last + 1);
            if size + more <= bound {
                last += 1;
                size += more;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    Fit::Passage { start: lines[first].0, end: lines[last].1, first_line: first, last_line: last }
}

/// AGC-FR-OYVH: the size attributes a passage and a metadata-only section
/// carry, together with `truncated` (AGC-FR-11).
fn size_attributes(size: &DocumentSize) -> Vec<(String, String)> {
    vec![
        ("truncated".to_string(), "true".to_string()),
        ("lines".to_string(), size.lines.to_string()),
        ("characters".to_string(), size.characters.to_string()),
        ("estimated_tokens".to_string(), size.estimated_tokens.to_string()),
    ]
}

/// AGC-FR-IJSE: the first line of a passage. Line numbers count from 1.
pub(super) fn passage_lead(size: &DocumentSize, first_line: usize, last_line: usize) -> String {
    format!(
        "[This is a passage of a larger document of {} lines and {} characters, about {} tokens \
         (an estimate). The passage holds lines {} to {}. The rest of the document is not \
         included here. Line 1 is at offset 0 when you read a range of lines.]\n\n",
        size.lines,
        size.characters,
        size.estimated_tokens,
        first_line + 1,
        last_line + 1,
    )
}

/// AGC-FR-VAIY: the one line of a metadata-only section.
pub(super) fn metadata_line(size: &DocumentSize) -> String {
    format!(
        "[The document has {} lines and {} characters, about {} tokens (an estimate). It is too \
         large to include here, so its text is not included.]",
        size.lines, size.characters, size.estimated_tokens,
    )
}

/// A section that is already within its bound, built without the generic cut
/// of AGC-FR-11.
fn fitted_section(attributes: Vec<(String, String)>, parts: Vec<InputPart>) -> InputSection {
    let mut section = section_of_parts_within(TAG_ARTIFACT, attributes, parts, usize::MAX);
    section.truncated = false;
    section
}

/// How a section of `text` is made into its ordered parts: plain text for a
/// file, the prompt with its images for a draft (AGC-FR-35, AGC-FR-JCPO).
type PartsOf<'a> = &'a dyn Fn(&str, &str, &str) -> Vec<InputPart>;

/// AGC-FR-NEQL, AGC-FR-BORR, AGC-FR-VAIY: the `artifact` section of `text`.
/// `prefix` and `suffix` frame the whole text where it fits.
fn fitted(
    text: &str,
    fragment: Option<(usize, usize)>,
    budget: &DocumentBudget,
    attributes: Vec<(String, String)>,
    parts_of: PartsOf<'_>,
    (prefix, suffix): (&str, &str),
) -> Material {
    let size = DocumentSize::of(text);
    let section = match fit(text, fragment, budget.bound_chars()) {
        Fit::Whole => fitted_section(attributes, parts_of(text, prefix, suffix)),
        Fit::Passage { start, end, first_line, last_line } => {
            let mut attributes = attributes;
            attributes.extend(size_attributes(&size));
            // AGC-FR-JCPO: only the images whose reference lies in the passage.
            let lead = passage_lead(&size, first_line, last_line);
            let mut section = fitted_section(attributes, parts_of(&text[start..end], &lead, ""));
            section.truncated = true;
            section
        }
        Fit::MetadataOnly => {
            let mut attributes = attributes;
            attributes.extend(size_attributes(&size));
            InputSection {
                tag: TAG_ARTIFACT.to_string(),
                attributes,
                body: metadata_line(&size),
                truncated: true,
                parts: Vec::new(),
            }
        }
    };
    Material { section: Some(section), document: Some(size) }
}

/// Plain text parts, for a project file.
fn text_parts(text: &str, prefix: &str, suffix: &str) -> Vec<InputPart> {
    vec![InputPart::Text(format!("{prefix}{text}{suffix}"))]
}

/// AGC-FR-07: the material of a file or a draft. A note has none: its
/// `note_context` is built by its own builder.
pub(super) fn material_of(
    roots: Roots<'_>,
    target: &DiscussionTarget,
    fragment: Option<&comments::FragmentTarget>,
    budget: &DocumentBudget,
) -> Material {
    let span = fragment.map(|f| (f.start, f.end));
    match target {
        DiscussionTarget::Artifact { artifact_id } => {
            // AGC-FR-12: a file that cannot be read is omitted.
            let Ok(path) = crate::fs::resolve_under(roots.worktree, artifact_id) else {
                return Material::default();
            };
            let Ok(text) = roots.worktree.read_text(&path) else {
                return Material::default();
            };
            let mut attributes = vec![("path".to_string(), artifact_id.to_string())];
            // AGC-FR-07: the type the Library shows for this same path.
            attributes.extend(type_attribute(crate::artifacts::resolved_type(
                roots.worktree,
                artifact_id,
            )));
            fitted(&text, span, budget, attributes, &text_parts, ("", ""))
        }
        DiscussionTarget::Draft { draft_id } => match fragment {
            Some(fragment) => {
                draft_fragment_material(roots, draft_id, &fragment.path, span, budget)
            }
            None => draft_material(roots, draft_id, budget),
        },
        DiscussionTarget::Note { .. } => Material::default(),
    }
}

/// AGC-FR-07: the draft file a fragment discussion is anchored in, carrying the
/// draft's name and the file's draft-relative path.
fn draft_fragment_material(
    roots: Roots<'_>,
    draft_id: &str,
    file_rel: &str,
    span: Option<(usize, usize)>,
    budget: &DocumentBudget,
) -> Material {
    // AGC-FR-12: a file that does not decode as UTF-8 is omitted.
    let Ok(contents) = crate::drafts::load_draft_file_impl(roots.worktree, draft_id, file_rel)
    else {
        return Material::default();
    };
    let mut attributes = Vec::new();
    if let Ok(record) = crate::drafts::read_draft_record(roots.worktree, draft_id) {
        attributes.push(("draft".to_string(), record.name.clone()));
    }
    attributes.push(("path".to_string(), file_rel.to_string()));
    // AGC-FR-07: the type this file will carry once the draft graduates.
    attributes.extend(type_attribute(draft_file_type(roots.worktree, draft_id, file_rel)));
    // AGC-FR-35: the prompt's text interleaved with the images it embeds.
    let parts = |text: &str, prefix: &str, suffix: &str| {
        prompt_parts(roots.worktree, draft_id, text, prefix, suffix)
    };
    fitted(&contents.body, span, budget, attributes, &parts, ("", ""))
}

/// AGC-FR-07: the draft's one prompt, for a discussion of the whole draft.
fn draft_material(roots: Roots<'_>, draft_id: &str, budget: &DocumentBudget) -> Material {
    let mut attributes = Vec::new();
    let mut prompt_path = None;
    if let Ok(record) = crate::drafts::read_draft_record(roots.worktree, draft_id) {
        attributes.push(("draft".to_string(), record.name.clone()));
        // AGC-FR-07: the type the draft's one prompt would classify as. A draft
        // is one prompt (DRS-FR-11), so one type describes it.
        if let Some(prompt) = record.prompt_path.as_deref() {
            attributes.extend(type_attribute(draft_file_type(roots.worktree, draft_id, prompt)));
            prompt_path = Some(prompt.to_string());
        }
    }
    // AGC-FR-12: a prompt that cannot be read is left out, and the turn proceeds
    // on its history alone.
    let Some(path) = prompt_path else {
        return Material::default();
    };
    let Ok(contents) = crate::drafts::load_draft_file_impl(roots.worktree, draft_id, &path) else {
        return Material::default();
    };
    let fits = fit(&contents.body, None, budget.bound_chars()) == Fit::Whole;
    if !fits {
        // AGC-FR-VAIY: a metadata-only section names its path.
        attributes.push(("path".to_string(), path.clone()));
    }
    let parts = |text: &str, prefix: &str, suffix: &str| {
        prompt_parts(roots.worktree, draft_id, text, prefix, suffix)
    };
    // Named by its own draft-relative path, so an agent asked to change it can
    // say which file it means in a `propose_draft_changes` call.
    let prefix = format!("--- {path} ---\n");
    fitted(&contents.body, None, budget, attributes, &parts, (&prefix, "\n\n"))
}
