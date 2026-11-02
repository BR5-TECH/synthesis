//! The spoken part of a comment body (`CVL-conversation-loop.md` CVL-FR-AGNN).
//!
//! An agent comment carries one `<spoken>…</spoken>` tag, anywhere in its body.
//! Voice mode speaks that text, and a surface hides the tag. The stored body
//! keeps it. The rule matches `spokenPart` in `src/text/spokenPart.ts`.

use std::borrow::Cow;
use std::ops::Range;

const OPEN: &str = "<spoken>";
const CLOSE: &str = "</spoken>";

/// The one set of space that both parsers trim. The standard trims of the two
/// languages differ on U+0085 and U+FEFF.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{FEFF}'
}

fn trimmed(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// The marker of a fence line, as its character and its length. This is the rule
/// of the Markdown renderer (`src/diff/markdown.ts`), so a tag counts as code
/// where the author sees code: any indent and at most one word after the marker.
/// A backtick after the marker makes the line inline code.
fn fence_of(line: &str) -> Option<(char, usize)> {
    let start = line.trim_start_matches(is_space);
    let marker = start.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = start.chars().take_while(|c| *c == marker).count();
    let info = trimmed(&start[length..]);
    let inline = marker == '`' && info.contains('`');
    (length >= 3 && !inline && !info.contains(is_space)).then_some((marker, length))
}

/// The byte ranges of the inline code spans of one paragraph. A run of backticks
/// closes at the next run of the same length, and a run with no such partner is
/// plain text.
fn inline_code(paragraph: &str, offset: usize, ranges: &mut Vec<Range<usize>>) {
    let bytes = paragraph.as_bytes();
    let run_at = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let length = run_at(at);
        let mut probe = at + length;
        let mut close = None;
        while probe < bytes.len() {
            if bytes[probe] != b'`' {
                probe += 1;
                continue;
            }
            let run = run_at(probe);
            if run == length {
                close = Some(probe + run);
                break;
            }
            probe += run;
        }
        match close {
            Some(end) => {
                ranges.push(offset + at..offset + end);
                at = end;
            }
            None => at += length,
        }
    }
}

/// CVL-FR-AGNN: the byte ranges of the code of `text`: fenced blocks with their
/// fence lines, and inline spans. A fence that does not close runs to the end.
fn code_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut fence: Option<(char, usize, usize)> = None;
    let mut paragraph = 0;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let marker = fence_of(line);
        let next = offset + raw.len();
        if let Some((opened, length, start)) = fence {
            if marker.is_some_and(|(other, run)| other == opened && run >= length) {
                ranges.push(start..next);
                fence = None;
                paragraph = next;
            }
        } else if marker.is_some() || trimmed(line).is_empty() {
            inline_code(&text[paragraph..offset], paragraph, &mut ranges);
            paragraph = next;
            fence = marker.map(|(opened, length)| (opened, length, offset));
        }
        offset = next;
    }
    match fence {
        Some((_, _, start)) => ranges.push(start..text.len()),
        None => inline_code(&text[paragraph..], paragraph, &mut ranges),
    }
    ranges
}

/// The first place at or after `from` where `needle` stands outside code.
fn find_outside(text: &str, needle: &str, mut from: usize, code: &[Range<usize>]) -> Option<usize> {
    while let Some(found) = text[from..].find(needle).map(|at| from + at) {
        if !code.iter().any(|range| range.contains(&found)) {
            return Some(found);
        }
        from = found + needle.len();
    }
    None
}

/// CVL-FR-AGNN: every complete tag outside code, in order, with the whole tag
/// and then its text.
fn spoken_tags(body: &str) -> Vec<(Range<usize>, Range<usize>)> {
    if !body.contains(OPEN) {
        return Vec::new();
    }
    let code = code_ranges(body);
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(open) = find_outside(body, OPEN, from, &code) {
        let start = open + OPEN.len();
        let Some(close) = find_outside(body, CLOSE, start, &code) else {
            break;
        };
        from = close + CLOSE.len();
        tags.push((open..from, start..close));
    }
    tags
}

/// CVL-FR-AGNN: the trimmed text of the first complete `<spoken>` tag outside
/// code. A blank text, a tag that does not close, and a body with every tag in
/// code give `None`.
pub fn spoken_part(body: &str) -> Option<&str> {
    let (_, inner) = spoken_tags(body).into_iter().next()?;
    Some(trimmed(&body[inner])).filter(|text| !text.is_empty())
}

/// CVL-FR-JPHT: `body` less every complete tag outside code.
pub fn without_spoken_tags(body: &str) -> Cow<'_, str> {
    let tags = spoken_tags(body);
    if tags.is_empty() {
        return Cow::Borrowed(body);
    }
    let mut written = String::with_capacity(body.len());
    let mut at = 0;
    for (outer, _) in tags {
        written.push_str(&body[at..outer.start]);
        at = outer.end;
    }
    written.push_str(&body[at..]);
    Cow::Owned(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One list for both parsers: `src/text/spokenPart.test.ts` reads it too, so
    /// a case cannot pass in one language and be absent from the other.
    const CASES: &str = include_str!("../../../src/text/spokenPart.cases.json");

    #[derive(serde::Deserialize)]
    struct Case {
        body: String,
        spoken: Option<String>,
        written: String,
    }

    // CVL-FR-AGNN, CVL-FR-JPHT
    #[test]
    fn the_shared_cases_hold() {
        let cases: Vec<Case> = serde_json::from_str(CASES).expect("the cases parse");
        assert!(cases.len() >= 20, "the list is read");
        for case in cases {
            assert_eq!(spoken_part(&case.body), case.spoken.as_deref(), "{:?}", case.body);
            assert_eq!(without_spoken_tags(&case.body), case.written, "{:?}", case.body);
        }
    }
}
