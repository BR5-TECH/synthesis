//! Chunking (BMI-FR-05 to BMI-FR-07) and file language detection (BMI-FR-08).

use bm25::Language;

use super::MAX_CHUNK_CHARS;

// ---------------------------------------------------------------------------
// Chunking (BMI-FR-05, BMI-FR-06, BMI-FR-07)
// ---------------------------------------------------------------------------

/// Whether a line opens or closes a fenced code block.
fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Whether a line is an ATX Markdown heading.
///
/// Deliberately not applied inside a fenced code block by [`split_at_headings`]:
/// a shell comment (`# rebuild the index`) is not a section boundary, and
/// splitting on one would cut a code sample in half.
fn is_heading(line: &str) -> bool {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes)
        && matches!(t.chars().nth(hashes), Some(c) if c == ' ' || c == '\t')
}

/// Whether `text` carries heading structure at all — the choice BMI-FR-05 makes
/// between splitting at headings and falling back to the size-bounded blocks of
/// BMI-FR-06.
fn has_headings(text: &str) -> bool {
    let mut in_fence = false;
    for line in text.lines() {
        if is_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence && is_heading(line) {
            return true;
        }
    }
    false
}

/// BMI-FR-05: a chunk runs from one heading to the next heading of any level,
/// so headings never nest one chunk inside another. The content preceding the
/// first heading is a section of its own.
fn split_at_headings(text: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        if is_fence(line) {
            in_fence = !in_fence;
        } else if !in_fence && is_heading(line) && !current.is_empty() {
            sections.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        sections.push(current);
    }
    sections
}

/// Split `text` at blank lines into pieces no longer than [`MAX_CHUNK_CHARS`].
///
/// A single paragraph longer than the ceiling is hard-split on character
/// boundaries — the one case where a boundary does not fall at a blank line,
/// because there is no blank line to fall at.
pub(super) fn bound_to_blocks(text: &str) -> Vec<String> {
    if text.chars().count() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for paragraph in text.split("\n\n") {
        let para_len = paragraph.chars().count();
        if para_len > MAX_CHUNK_CHARS {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
                current_len = 0;
            }
            blocks.extend(hard_split(paragraph));
            continue;
        }
        if current_len + para_len > MAX_CHUNK_CHARS && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if !current.is_empty() {
            current.push_str("\n\n");
            current_len += 2;
        }
        current.push_str(paragraph);
        current_len += para_len;
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// Cut an over-long paragraph into ceiling-sized pieces, on character
/// boundaries so no piece splits a multi-byte character.
fn hard_split(paragraph: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut piece = String::new();
    for (i, ch) in paragraph.chars().enumerate() {
        if i > 0 && i % MAX_CHUNK_CHARS == 0 {
            out.push(std::mem::take(&mut piece));
        }
        piece.push(ch);
    }
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

/// Split a file's text into the documents that are indexed for it
/// (BMI-FR-05, BMI-FR-06). Chunks come back in file order, which is what makes
/// their positions the `chunk_ordinal` of BMI-FR-07.
///
/// A chunk holding nothing but whitespace is dropped rather than indexed: it
/// would match no query and would only shift every later ordinal.
pub fn chunk_text(text: &str) -> Vec<String> {
    // Line endings are normalised first. `split_at_headings` rebuilds its
    // sections from `str::lines`, which already drops a trailing `\r`, so
    // without this a CRLF file would take one path through a normalised text
    // and the other through a raw one — and `bound_to_blocks`, which finds a
    // blank line by looking for `\n\n`, would find none in a CRLF file and
    // hard-split the whole of it mid-word.
    let normalised;
    let text = if text.contains('\r') {
        normalised = text.replace("\r\n", "\n").replace('\r', "\n");
        normalised.as_str()
    } else {
        text
    };
    let sections = if has_headings(text) {
        split_at_headings(text)
    } else {
        vec![text.to_string()]
    };
    let mut out = Vec::new();
    for section in sections {
        for block in bound_to_blocks(&section) {
            if !block.trim().is_empty() {
                out.push(block);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Language (BMI-FR-08)
// ---------------------------------------------------------------------------

/// BMI-FR-08: detect a file's language once, over the whole file's text.
///
/// A file whose language cannot be determined, or whose language has no stemmer
/// in this tokenizer's set, falls back to English rather than being left
/// unstemmed or unindexed — a short heading-only chunk is therefore stemmed as
/// the document it came from rather than guessed at on its own few words.
pub fn detect_file_language(text: &str) -> Language {
    if text.trim().is_empty() {
        return Language::English;
    }
    Language::try_from(whichlang::detect_language(text)).unwrap_or(Language::English)
}

/// A stable key for a language, so shards order deterministically. `Language`
/// carries `Hash`/`Eq` but no `Ord`, and a `HashMap`'s iteration order would
/// make a merged result set's tie-breaking depend on hash seeding.
pub(super) fn language_key(language: &Language) -> &'static str {
    match language {
        Language::Arabic => "arabic",
        Language::Danish => "danish",
        Language::Dutch => "dutch",
        Language::English => "english",
        Language::French => "french",
        Language::German => "german",
        Language::Greek => "greek",
        Language::Hungarian => "hungarian",
        Language::Italian => "italian",
        Language::Norwegian => "norwegian",
        Language::Portuguese => "portuguese",
        Language::Romanian => "romanian",
        Language::Russian => "russian",
        Language::Spanish => "spanish",
        Language::Swedish => "swedish",
        Language::Tamil => "tamil",
        Language::Turkish => "turkish",
    }
}
