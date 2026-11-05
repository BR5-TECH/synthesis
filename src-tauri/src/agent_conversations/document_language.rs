//! The document language of a discussion target (AGC-FR-LDTX).
//!
//! A detector in the application reads the text of the target. No model call
//! is made. A text that the detector cannot name with confidence is English.

use super::*;

/// AGC-FR-LDTX: the language of a text that has no detected language.
pub const FALLBACK_LANGUAGE: &str = "English";

/// AGC-FR-LDTX: the part of a text that the detector reads, in characters.
const SAMPLE_CHARS: usize = 20_000;

/// AGC-FR-LDTX: the fewest letters that a detection takes.
const MIN_LETTERS: usize = 40;

/// AGC-FR-LDTX: the text without fenced code, inline code, and URLs.
fn prose_of(text: &str) -> String {
    let sample: String = text.chars().take(SAMPLE_CHARS).collect();
    let mut prose = String::with_capacity(sample.len());
    let mut fenced = false;
    for line in sample.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut inline = false;
        let mut kept = String::with_capacity(line.len());
        for c in line.chars() {
            if c == '`' {
                inline = !inline;
            } else if !inline {
                kept.push(c);
            }
        }
        for word in kept.split_whitespace() {
            if !(word.contains("://") || word.starts_with("www.")) {
                prose.push_str(word);
                prose.push(' ');
            }
        }
        prose.push('\n');
    }
    prose
}

/// AGC-FR-LDTX: the English name of a detected language.
fn english_name(lang: whatlang::Lang) -> &'static str {
    match lang {
        // The detector names the written form of Chinese by its spoken form.
        whatlang::Lang::Cmn => "Chinese",
        other => other.eng_name(),
    }
}

/// AGC-FR-LDTX: why a text has its document language. The basis goes to the
/// log, and it holds no text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    Detected,
    /// The text could not be read, or the target holds none.
    Unreadable,
    /// The text has fewer than [`MIN_LETTERS`] letters.
    TooShort,
    /// The detector names no language with confidence.
    Unreliable,
}

impl Basis {
    pub fn label(self) -> &'static str {
        match self {
            Basis::Detected => "detected",
            Basis::Unreadable => "unreadable",
            Basis::TooShort => "too_short",
            Basis::Unreliable => "unreliable",
        }
    }
}

/// AGC-FR-LDTX: a document language and why it is that language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentLanguage {
    /// The English name of the language.
    pub name: &'static str,
    pub basis: Basis,
}

/// AGC-FR-LDTX: the detection of one text.
fn detect(text: &str) -> DocumentLanguage {
    let prose = prose_of(text);
    if prose.chars().filter(|c| c.is_alphabetic()).count() < MIN_LETTERS {
        return DocumentLanguage { name: FALLBACK_LANGUAGE, basis: Basis::TooShort };
    }
    match whatlang::detect(&prose) {
        Some(info) if info.is_reliable() => {
            DocumentLanguage { name: english_name(info.lang()), basis: Basis::Detected }
        }
        _ => DocumentLanguage { name: FALLBACK_LANGUAGE, basis: Basis::Unreliable },
    }
}

/// AGC-FR-LDTX, AGC-FR-WSKP: the English name of the language that the
/// detector names reliably, or `None`.
pub fn detect_language(text: &str) -> Option<&'static str> {
    let found = detect(text);
    (found.basis == Basis::Detected).then_some(found.name)
}

/// AGC-FR-LDTX: the document language of a text that may not exist.
pub fn language_of(text: Option<&str>) -> DocumentLanguage {
    match text {
        Some(text) => detect(text),
        None => DocumentLanguage { name: FALLBACK_LANGUAGE, basis: Basis::Unreadable },
    }
}

/// AGC-FR-LDTX: the first bytes of a file, as many as [`SAMPLE_CHARS`]
/// characters can take. A large artifact is not read in full for its
/// language. A character that the bound cuts is left out.
fn read_prefix(root: &crate::fs::RootFs, path: &std::path::Path) -> Option<String> {
    let mut handle = root.open_file(path, crate::fs::OpenMode::ReadOnly).ok()?;
    let limit = SAMPLE_CHARS * 4;
    let mut bytes = vec![0; limit];
    let mut filled = 0;
    while filled < limit {
        match handle.read(&mut bytes[filled..]).ok()? {
            0 => break,
            n => filled += n,
        }
    }
    bytes.truncate(filled);
    match String::from_utf8(bytes) {
        Ok(text) => Some(text),
        // Only a character that the bound cuts is accepted: other bytes that
        // are not UTF-8 make the file a text that cannot be read.
        Err(e) if filled == limit && e.utf8_error().error_len().is_none() => {
            let valid = e.utf8_error().valid_up_to();
            let mut bytes = e.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).ok()
        }
        Err(_) => None,
    }
}

/// AGC-FR-LDTX: the text of a target. A draft is one prompt (DRS-FR-11), so
/// the file of a fragment of a draft is that prompt.
pub(crate) fn target_text(
    roots: Roots<'_>,
    target: &DiscussionTarget,
    fragment: Option<&comments::FragmentTarget>,
) -> Option<String> {
    match target {
        DiscussionTarget::Artifact { artifact_id } => {
            let path = crate::fs::resolve_under(roots.worktree, artifact_id).ok()?;
            read_prefix(roots.worktree, &path)
        }
        DiscussionTarget::Draft { draft_id } => {
            let file = match fragment {
                Some(fragment) => fragment.path.clone(),
                None => {
                    crate::drafts::read_draft_record(roots.worktree, draft_id).ok()?.prompt_path?
                }
            };
            crate::drafts::load_draft_file_impl(roots.worktree, draft_id, &file)
                .ok()
                .map(|contents| contents.body)
        }
        DiscussionTarget::Note { note_id } => {
            crate::notes::load_note(roots.worktree, note_id).map(|note| note.body)
        }
    }
}

/// AGC-FR-LDTX: the document language of a target.
pub fn document_language(
    roots: Roots<'_>,
    target: &DiscussionTarget,
    fragment: Option<&comments::FragmentTarget>,
) -> DocumentLanguage {
    language_of(target_text(roots, target, fragment).as_deref())
}

/// AGC-FR-LDTX: the record of a document language. It holds the kind of the
/// target, the language, and the basis, and no text and no path.
pub(crate) fn log_document_language<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    use_: &str,
    target: &DiscussionTarget,
    found: DocumentLanguage,
) {
    let kind = match target {
        DiscussionTarget::Artifact { .. } => "artifact",
        DiscussionTarget::Draft { .. } => "draft",
        DiscussionTarget::Note { .. } => "note",
    };
    logging::log_debug(
        app,
        buffer,
        &[Domain::Ai],
        "document language chosen",
        log_fields! {
            "use" => use_,
            "targetKind" => kind,
            "documentLanguage" => found.name,
            "basis" => found.basis.label(),
        },
    );
}
