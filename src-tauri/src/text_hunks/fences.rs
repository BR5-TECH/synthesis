//! Mermaid fences in a prompt (`DCP-draft-change-proposals.md` DCP-FR-WSLE,
//! DCP-FR-HXNP).
//!
//! A diagram hunk changes, adds, or removes one Mermaid fence. This module finds
//! the fences of a text and nothing more. It judges no source: whether a source
//! is valid Mermaid is the tool's (PDM-FR-HOQI).
//!
//! Pure over `&str`. It reaches no store and needs no runtime.

use super::anchors::normalise_newlines;

/// One Mermaid fence of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence {
    /// Byte offset of the start of the opening line, indent included.
    pub start: usize,
    /// Byte offset of the end of the closing line. The line break is not part
    /// of the fence.
    pub end: usize,
    /// The whole opening line, as the text holds it.
    pub opening: String,
    /// The whole closing line, as the text holds it. For a fence that is not
    /// closed, this is a closing line the fence would need.
    pub closing: String,
    /// The spaces and tabs before the opening marker.
    pub indent: String,
    /// The lines between the two marker lines, with up to the indent of the
    /// opening line taken off each, joined with `\n`.
    pub source: String,
    /// False when the text ends before a closing line. Such a fence runs to
    /// the end of the text.
    pub closed: bool,
}

/// How an opening line starts a fence.
struct Opener {
    indent: String,
    marker: char,
    length: usize,
    mermaid: bool,
}

/// Read a line as the opening line of a fenced code block, if it is one.
fn read_opener(line: &str) -> Option<Opener> {
    let body = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - body.len()];
    let marker = body.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = body.chars().take_while(|c| *c == marker).count();
    if length < 3 {
        return None;
    }
    let info = &body[length..];
    // A backtick fence has no backtick in its info string. Without this rule,
    // a line of inline code is read as a fence.
    if marker == '`' && info.contains('`') {
        return None;
    }
    let mermaid = info.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("mermaid"));
    Some(Opener { indent: indent.to_string(), marker, length, mermaid })
}

/// True when the line closes a fence opened with `opener`.
fn closes(line: &str, opener: &Opener) -> bool {
    let body = line.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t']);
    let run = body.chars().take_while(|c| *c == opener.marker).count();
    run >= opener.length && run == body.chars().count()
}

/// Take up to `indent_chars` leading spaces and tabs off a line.
fn strip_indent(line: &str, indent_chars: usize) -> &str {
    let mut rest = line;
    for _ in 0..indent_chars {
        match rest.strip_prefix([' ', '\t']) {
            Some(next) => rest = next,
            None => break,
        }
    }
    rest
}

/// The lines of `text` with the byte offset where each starts and ends. The
/// end excludes the line break. A text that ends with a line break has no
/// empty last line.
fn lines_of(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        out.push((at, end));
        at = end + 1;
    }
    out
}

/// Every Mermaid fence of `text`, in order.
///
/// The text is normalised for line endings first, so the offsets are offsets in
/// `normalise_newlines(text)`. A fence of another language is skipped whole, so
/// a Mermaid fence quoted inside it is not found. A fence that is not closed
/// runs to the end of the text.
pub fn find_fences(text: &str) -> Vec<Fence> {
    let text = normalise_newlines(text);
    let lines = lines_of(&text);
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let (start, end) = lines[index];
        let Some(opener) = read_opener(&text[start..end]) else {
            index += 1;
            continue;
        };
        let close =
            (index + 1..lines.len()).find(|&i| closes(&text[lines[i].0..lines[i].1], &opener));
        let last = close.unwrap_or(lines.len() - 1);
        if opener.mermaid {
            let inner_end = close.unwrap_or(lines.len());
            let indent_chars = opener.indent.chars().count();
            let source: Vec<&str> = lines[index + 1..inner_end]
                .iter()
                .map(|&(s, e)| strip_indent(&text[s..e], indent_chars))
                .collect();
            let closing = match close {
                Some(i) => text[lines[i].0..lines[i].1].to_string(),
                None => {
                    format!("{}{}", opener.indent, opener.marker.to_string().repeat(opener.length))
                }
            };
            found.push(Fence {
                start,
                end: lines[last].1,
                opening: text[start..end].to_string(),
                closing,
                indent: opener.indent.clone(),
                source: source.join("\n"),
                closed: close.is_some(),
            });
        }
        index = last + 1;
    }
    found
}

/// A source made ready to compare: line endings normalised, blank lines at the
/// start cut, and white space at the end cut.
pub fn comparable(source: &str) -> String {
    let source = normalise_newlines(source);
    let mut rest = source.as_str();
    while let Some(i) = rest.find('\n') {
        if !rest[..i].trim().is_empty() {
            break;
        }
        rest = &rest[i + 1..];
    }
    rest.trim_end().to_string()
}

/// The source of the first fence in the text of a diagram hunk, without its
/// marker lines and its indent (DCP-FR-NDAQ). The text of a hunk that holds no
/// fence is given back as it is.
pub fn source_of(hunk_text: &str) -> String {
    match find_fences(hunk_text).into_iter().next() {
        Some(fence) => fence.source,
        None => normalise_newlines(hunk_text),
    }
}
