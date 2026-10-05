//! Reference semantics: how a Markdown destination resolves, and what the saved prompt says about its images.

use super::*;

// ---------------------------------------------------------------------------
// Reference semantics (DAS-FR-06, DAS-FR-07)
// ---------------------------------------------------------------------------

/// What a Markdown destination resolves to, from the prompt's own location
/// inside `files/` (DAS-FR-07).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    /// A file directly inside this draft's own `assets/`, named here.
    Asset(String),
    /// Inside the draft but not an asset — anything under `files/`, `comments/`,
    /// `proposals/`, or `history/`, and an `assets/` path with a folder in it.
    InsideDraft,
    /// A data URI, an absolute path, a project-relative path, an external
    /// address, or any path that leaves the draft's own directory. Resolves to
    /// **no draft-owned asset**.
    Outside,
}

/// DAS-FR-07: resolve one Markdown destination by the standard relative-path
/// rule from the prompt's own location inside `files/`.
///
/// Nothing here touches the filesystem: it classifies a string. That is what
/// makes it usable for both of the two questions DAS-FR-07 keeps apart — a
/// direct read, which is *refused* for anything but an asset, and a resolution,
/// which refuses nothing and simply carries the classification forward. No
/// address is requested and no path outside the draft is ever opened, because no
/// path outside the draft ever leaves this function.
pub fn resolve_destination(dest: &str) -> Destination {
    resolve_from(dest, &["files"])
}

/// DAS-FR-08: resolve the `path` a caller named for a direct read or a discard.
///
/// A caller may name the asset either way and both mean the same file: the
/// draft-relative `assets/<name>` the [`DraftAsset`] record carries, and the
/// `../assets/<name>` destination the prompt carries, which a surface has read
/// straight out of the Markdown. A path that begins by going up is prompt-
/// relative and is resolved from `files/`; anything else is draft-relative.
/// Every other outcome is the same in both readings.
pub(super) fn resolve_named_path(path: &str) -> Destination {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with("../") || normalized == ".." {
        resolve_from(path, &["files"])
    } else {
        resolve_from(path, &[])
    }
}

/// The shared body of the two resolutions: walk `dest` from `base` (a stack of
/// path segments below the draft's own directory) and say where it lands.
pub(super) fn resolve_from(dest: &str, base: &[&str]) -> Destination {
    let dest = dest.trim();
    if dest.is_empty() {
        return Destination::Outside;
    }
    // A destination this module will not read as a path at all. Each of these is
    // "no draft-owned asset" for the same reason: it names something that is not
    // a file inside this draft, and nothing here resolves, requests, or reads
    // one (DAS-FR-07).
    if dest.starts_with('#') {
        return Destination::Outside;
    }
    let lower = dest.to_ascii_lowercase();
    if lower.starts_with("data:") || lower.contains("://") || lower.starts_with("mailto:") {
        return Destination::Outside;
    }
    let normalized = dest.replace('\\', "/");
    // An absolute path — POSIX, a Windows drive, or a UNC share.
    if normalized.starts_with('/')
        || normalized.starts_with("//")
        || (normalized.len() >= 3
            && normalized.as_bytes()[1] == b':'
            && normalized.as_bytes()[0].is_ascii_alphabetic())
    {
        return Destination::Outside;
    }
    // A destination may carry a fragment or a query; neither is part of the file
    // it names. Split before the walk so `../assets/a.png#fig` resolves to the
    // asset rather than to a name no file carries.
    let path = normalized
        .split(['#', '?'])
        .next()
        .unwrap_or("")
        .to_string();
    let path = percent_decode(&path);
    if path.is_empty() {
        return Destination::Outside;
    }
    let mut stack: Vec<String> = base.iter().map(|s| s.to_string()).collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                // Popping past the draft's own directory is what "leaves the
                // draft" means, and it is the whole of the escape test.
                if stack.pop().is_none() {
                    return Destination::Outside;
                }
            }
            other => stack.push(other.to_string()),
        }
    }
    match stack.as_slice() {
        [dir, name] if dir == crate::drafts::ASSETS_DIR && !name.is_empty() => {
            Destination::Asset(name.clone())
        }
        [] => Destination::Outside,
        _ => Destination::InsideDraft,
    }
}

/// Decode the `%XX` escapes a Markdown destination may carry.
///
/// A surface writes `../assets/<id>.png`, which needs none; an author who typed
/// a path by hand may well have written `%20` for a space. Decoding is what
/// makes the two spellings of one filename resolve to one file, and it is done
/// **after** the scheme and absolute-path tests above rather than before, so a
/// percent-encoded scheme cannot smuggle a destination past them.
pub(super) fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

/// One image use the saved prompt carries, as the reading of DAS-FR-06 finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageUse {
    /// The destination the Markdown carries, resolved through its reference
    /// definition where the image is reference-style.
    pub reference: String,
    /// The alt text the image was written with; empty where it has none.
    pub alt: String,
    /// Where the whole construct sits in the source, in bytes.
    pub start: usize,
    pub end: usize,
}

/// DAS-FR-06: everything the saved prompt says about its images and its links.
pub struct PromptReferences {
    /// Every image use, in the order it occurs in the document (DAS-FR-22).
    pub images: Vec<ImageUse>,
    /// Every destination that resolves to a draft-owned asset anywhere in the
    /// document, whichever construct carries it: an image use, a link
    /// destination, or a reference definition (DAS-FR-17).
    pub protected: BTreeSet<String>,
}

/// DAS-FR-06 / DAS-FR-17: read a prompt's image references with the same
/// Markdown semantics the prompt is rendered with.
///
/// A real CommonMark parse rather than a scan, because housekeeping deletes a
/// file on the strength of this reading (DAS-FR-12) and every spelling it missed
/// would be an image the author is still using. The parser reads the whole
/// document before anything is resolved, so a reference definition may stand
/// **before or after** the image that uses it, and it matches labels by the
/// renderer's own normalization, so two spellings of one label resolve to one
/// destination.
///
/// One reading is used for every purpose: what `read_prompt_images` returns and
/// what housekeeping counts as a reference come from this function and from
/// nowhere else.
pub fn read_references(markdown: &str) -> PromptReferences {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, options);
    // Taken before the events are consumed: the parse collects every reference
    // definition in the document up front, which is what makes a definition
    // standing after its use resolve exactly as one standing before it does.
    let definitions: Vec<(String, String)> = parser
        .reference_definitions()
        .iter()
        .map(|(label, def)| (label.to_string(), def.dest.to_string()))
        .collect();

    let mut images: Vec<ImageUse> = Vec::new();
    let mut protected: BTreeSet<String> = BTreeSet::new();
    let mut covered: Vec<(usize, usize)> = Vec::new();
    let mut literal: Vec<(usize, usize)> = Vec::new();

    // A stack, because an image may sit inside a link and a link inside nothing
    // else that matters here. The alt text of an image is the text between its
    // start and its end.
    let mut open_image: Option<(String, usize, usize, String)> = None;
    let parser = Parser::new_ext(markdown, options);
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                let dest = dest_url.to_string();
                note_protected(&dest, &mut protected);
                covered.push((range.start, range.end));
                open_image = Some((dest, range.start, range.end, String::new()));
            }
            Event::End(TagEnd::Image) => {
                if let Some((reference, start, end, alt)) = open_image.take() {
                    images.push(ImageUse {
                        reference,
                        alt,
                        start,
                        end,
                    });
                }
            }
            // DAS-FR-17: a valid link destination protects a draft-owned asset
            // exactly as an image use does. An author who links a file rather
            // than embedding it keeps that file.
            Event::Start(Tag::Link { dest_url, .. }) => {
                note_protected(&dest_url, &mut protected);
            }
            Event::Text(ref text) => {
                if let Some((_, _, _, alt)) = open_image.as_mut() {
                    alt.push_str(text);
                }
            }
            Event::Code(ref text) => {
                if let Some((_, _, _, alt)) = open_image.as_mut() {
                    alt.push_str(text);
                }
                literal.push((range.start, range.end));
            }
            Event::Start(Tag::CodeBlock(_)) => literal.push((range.start, range.end)),
            Event::Html(_) | Event::InlineHtml(_) => literal.push((range.start, range.end)),
            _ => {}
        }
    }

    // DAS-FR-17: a definition the document defines but no image uses protects
    // its destination all the same. An author who defines a label the document
    // has stopped using keeps that file.
    for (_, dest) in &definitions {
        note_protected(dest, &mut protected);
    }

    // DAS-FR-22: an image whose reference-style label matches no definition is
    // not an image the renderer draws, so the parse above emits it as ordinary
    // text — but it is still a place the author put a picture, and the entry has
    // to come back unresolved with its context intact rather than vanish. This
    // pass finds those and nothing else: every construct the parse already
    // accounted for is excluded, as is anything inside a code span, a fenced
    // block, or raw HTML, where a renderer would draw no image either.
    for (start, end, label, alt) in unresolved_reference_images(markdown, &covered, &literal) {
        images.push(ImageUse {
            reference: label,
            alt,
            start,
            end,
        });
    }
    images.sort_by_key(|i| (i.start, i.end));

    PromptReferences { images, protected }
}

/// Record a destination as protecting an asset, where it names one.
///
/// A destination that resolves to no draft-owned asset is **not evidence about
/// any file** (DAS-FR-07): it protects nothing and condemns nothing, so it is
/// simply not recorded.
pub(super) fn note_protected(dest: &str, protected: &mut BTreeSet<String>) {
    if let Destination::Asset(name) = resolve_destination(dest) {
        protected.insert(name);
    }
}

/// The `![…]` and `![…][…]` constructs no reference definition resolves, which
/// the CommonMark parse therefore emitted as ordinary text.
///
/// Deliberately narrow. It matches only where a `![` is followed by a balanced
/// `]` that is **not** followed by `(` — an inline image, which the parse would
/// have taken — and it skips any span the parse already accounted for and any
/// span inside a code construct. A candidate it gets wrong costs an extra
/// unresolved entry in a conversation's material and can never cost a file: an
/// unresolved reference protects nothing and condemns nothing (DAS-FR-16), so
/// housekeeping does not consult this pass at all.
pub(super) fn unresolved_reference_images(
    markdown: &str,
    covered: &[(usize, usize)],
    literal: &[(usize, usize)],
) -> Vec<(usize, usize, String, String)> {
    let bytes = markdown.as_bytes();
    let inside = |pos: usize, spans: &[(usize, usize)]| {
        spans.iter().any(|(s, e)| pos >= *s && pos < *e)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != b'!' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        if inside(i, covered) || inside(i, literal) {
            i += 1;
            continue;
        }
        let Some(alt_end) = matching_bracket(bytes, i + 1) else {
            i += 1;
            continue;
        };
        // An inline image; the parse took it, or it is malformed and is not an
        // image reference at all (DAS-FR-16).
        if bytes.get(alt_end + 1) == Some(&b'(') {
            i = alt_end + 1;
            continue;
        }
        let alt = markdown[i + 2..alt_end].to_string();
        // `![alt][label]` — the full form. `![alt][]` collapses onto the alt
        // text, and `![alt]` is the shortcut form; in both the label is the alt.
        let (end, label) = match bytes.get(alt_end + 1) {
            Some(&b'[') => match matching_bracket(bytes, alt_end + 1) {
                Some(label_end) => {
                    let inner = markdown[alt_end + 2..label_end].to_string();
                    let label = if inner.trim().is_empty() { alt.clone() } else { inner };
                    (label_end + 1, label)
                }
                None => (alt_end + 1, alt.clone()),
            },
            _ => (alt_end + 1, alt.clone()),
        };
        out.push((i, end, label, alt));
        i = end;
    }
    out
}

/// The index of the `]` closing the `[` at `open`, allowing for nesting and for
/// a backslash escape, or `None` where the bracket never closes on this line.
pub(super) fn matching_bracket(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            // A reference label cannot span a blank line, and a runaway scan
            // over a whole document is what a missing bracket would otherwise
            // cost every candidate after it.
            b'\n' if bytes.get(i + 1) == Some(&b'\n') => return None,
            _ => {}
        }
        i += 1;
    }
    None
}
