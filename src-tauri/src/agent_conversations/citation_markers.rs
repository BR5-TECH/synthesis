//! Provider citation markers, removed before anything reads a reply.
//!
//! OpenAI models mark the sources of a web search with private-use spans:
//! `U+E200 cite U+E202 turn0search1 U+E202 turn0search2 U+E201`. No surface
//! can render them, and a model copies them from the history it reads, so the
//! loop removes them from every reply (CVL-FR-VSSU) and every builder removes
//! them from every comment body it sends back (AGC-FR-INFL).

use std::borrow::Cow;

use super::*;

/// CVL-FR-VSSU: the character that opens a marker span.
const SPAN_OPEN: char = '\u{E200}';
/// CVL-FR-VSSU: the character that closes a marker span.
const SPAN_CLOSE: char = '\u{E201}';

/// CVL-FR-VSSU: a character of the block the markers are drawn from. A lone one
/// is a marker of its own.
fn is_marker_char(c: char) -> bool {
    ('\u{E200}'..='\u{E2FF}').contains(&c)
}

/// CVL-FR-VSSU: `text` with every provider citation marker removed.
///
/// A span from `U+E200` to the next `U+E201` is removed together with the
/// spaces and tabs just before it. A span that another `U+E200` interrupts, or
/// that never closes, is no span: its opening character is removed alone, as
/// any other lone character of the block is. Text that holds no character of
/// the block is returned borrowed.
pub(crate) fn strip_citation_markers(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_marker_char) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        if c == SPAN_OPEN {
            let stop = after.find(|ch| ch == SPAN_CLOSE || ch == SPAN_OPEN);
            if let Some(at) = stop.filter(|&at| after[at..].starts_with(SPAN_CLOSE)) {
                let kept = out.trim_end_matches([' ', '\t']).len();
                out.truncate(kept);
                rest = &after[at + SPAN_CLOSE.len_utf8()..];
                continue;
            }
        }
        if !is_marker_char(c) {
            out.push(c);
        }
        rest = after;
    }
    Cow::Owned(out)
}

/// CVL-FR-VSSU: removes the markers from every string inside `value`, in place,
/// and returns how many characters it removed. Object keys are left alone: a
/// model names an argument, and it is the text of a value that it cites in.
fn strip_value(value: &mut serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(text) => match strip_citation_markers(text) {
            Cow::Borrowed(_) => 0,
            Cow::Owned(clean) => {
                let removed = text.chars().count() - clean.chars().count();
                *text = clean;
                removed
            }
        },
        serde_json::Value::Array(items) => items.iter_mut().map(strip_value).sum(),
        serde_json::Value::Object(map) => map.values_mut().map(strip_value).sum(),
        _ => 0,
    }
}

/// CVL-FR-VSSU / CVL-FR-POAR: removes the markers from a reply's text and from
/// every string of every tool call's arguments, and returns how many characters
/// it removed. Nothing else of the reply changes.
pub(super) fn strip_reply_citation_markers(reply: &mut ModelReply) -> usize {
    let mut removed = 0;
    if let Cow::Owned(clean) = strip_citation_markers(&reply.text) {
        removed += reply.text.chars().count() - clean.chars().count();
        reply.text = clean;
    }
    for call in &mut reply.tool_calls {
        removed += strip_value(&mut call.function.arguments);
    }
    removed
}
