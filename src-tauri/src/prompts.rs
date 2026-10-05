//! What a prompt file contributes to an instruction
//! (`../../specifications/ai/GRL-graduation-loop.md` GRL-FR-YIJG,
//! `../../specifications/ai/CVL-conversation-loop.md` CVL-FR-29,
//! `../../specifications/tools/EAC-execute-agent-cli.md` EAC-FR-35).
//!
//! Every instruction this application sends is a Markdown file compiled into
//! the binary, and every one of those files may carry authoring comments —
//! notes naming the requirement the file satisfies and saying why it is worded
//! the way it is. Those notes are addressed to whoever maintains the file and
//! to nobody else. They stay in the file, which is the point: they are what
//! makes an instruction reviewable as prose. They never reach a model or an
//! execution agent, which would read them as part of the task.
//!
//! So a file's **instruction text** is its content with every comment removed
//! and the blank space the removal leaves closed up, and nothing else about it
//! altered: its words, its order, its Markdown, and the line breaks it holds
//! are what the file holds.

/// A file's instruction text: the file with its authoring comments taken out.
///
/// A file carrying no comment contributes itself **byte-for-byte**, which is
/// what keeps this from being a reformatter that happens to also drop comments.
///
/// Where a comment is removed, the blank space it leaves is closed up: a line
/// that held nothing but the comment goes with it rather than becoming an empty
/// line the file never had, and what is left begins at its first real word.
/// A comment sharing a line with prose leaves that prose where it was.
///
/// An **unterminated** `<!--` runs to the end of the file. It is a comment that
/// was opened and never closed, so what follows it was never instruction text
/// either — treating the delimiter as prose would send exactly the note this
/// exists to withhold.
///
/// It is **not** aware of fenced code blocks: a `<!-- … -->` written inside a
/// fence as example content is removed like any other. No prompt file does
/// that today, and the alternative — a Markdown fence parser in the path every
/// instruction goes through — buys less than it costs.
pub fn instruction_text(raw: &str) -> String {
    let spans = comment_spans(raw);
    if spans.is_empty() {
        return raw.to_string();
    }

    // Walked line by line against the spans themselves rather than by marking
    // the removals with a sentinel character. A sentinel would have to be a
    // character no instruction file can hold, and this repository already uses
    // `\u{0}` as a sentinel of its own elsewhere — a "cannot occur" that turns
    // out to occur is a function that silently edits text it was asked to leave
    // alone.
    let mut out = String::with_capacity(raw.len());
    let mut at = 0usize;
    for line in raw.split_inclusive('\n') {
        let start = at;
        let end = at + line.len();
        at = end;

        // The spans this line meets, in order. A comment may open on one line
        // and close on a later one, so a span can cover part of a line, the
        // whole of it, or straddle its edges.
        let touching: Vec<(usize, usize)> = spans
            .iter()
            .copied()
            .filter(|(from, to)| *from < end && *to > start)
            .collect();
        if touching.is_empty() {
            out.push_str(line);
            continue;
        }

        let mut kept = String::with_capacity(line.len());
        let mut cursor = start;
        for (from, to) in touching {
            if from > cursor {
                kept.push_str(&raw[cursor..from.min(end)]);
            }
            cursor = cursor.max(to);
        }
        if cursor < end {
            kept.push_str(&raw[cursor..end]);
        }

        // A line that held nothing but a comment goes with it — including its
        // terminator, which is the "closing up" — while a line that held prose
        // beside the comment keeps that prose exactly where it was.
        if kept.trim().is_empty() {
            continue;
        }
        out.push_str(&kept);
    }

    // A comment removed from the head of a file leaves whatever blank lines
    // stood between it and the prose. They were separating a note from the
    // instruction; with the note gone they are leading whitespace, and an
    // instruction begins at its first real word.
    out.trim_start().to_string()
}

/// The byte ranges every `<!-- … -->` occupies, in order.
///
/// A nested opener **extends** the span rather than closing it early. HTML
/// comments do not nest, so a renderer would end the outer comment at the first
/// `-->` and treat everything after it as prose — but the requirement here is
/// about what reaches an agent, not about what a browser draws. A maintainer
/// who writes `<!-- see the <!-- x --> convention -->` would otherwise ship the
/// tail of their own note, the trailing delimiter included, as instruction
/// text: exactly the thing this exists to withhold. Counting depth costs
/// nothing and fails safe, because the failure it can produce — removing
/// slightly more than a renderer would — removes a note, while the other
/// direction sends one.
fn comment_spans(raw: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0usize;
    while let Some(offset) = raw[from..].find("<!--") {
        let start = from + offset;
        let mut depth = 1usize;
        let mut at = start + "<!--".len();
        let end = loop {
            let opens = raw[at..].find("<!--").map(|i| at + i);
            let closes = raw[at..].find("-->").map(|i| at + i);
            match (opens, closes) {
                // A nested opener before the next close: one more to close.
                (Some(open), Some(close)) if open < close => {
                    depth += 1;
                    at = open + "<!--".len();
                }
                (_, Some(close)) => {
                    depth -= 1;
                    at = close + "-->".len();
                    if depth == 0 {
                        break at;
                    }
                }
                // Opened and never closed: what follows was never instruction
                // text either, so the span runs to the end of the file.
                (_, None) => break raw.len(),
            }
        };
        spans.push((start, end));
        from = end;
        if from >= raw.len() {
            break;
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file with no comment contributes itself, byte-for-byte.
    #[test]
    fn a_file_without_comments_is_unchanged() {
        let raw = "# Title\n\nDo the work.\n\n\nStill do it.\n";
        assert_eq!(instruction_text(raw), raw);
    }

    /// The head comment goes, and the instruction begins at its first real word.
    #[test]
    fn a_head_comment_is_removed_and_the_space_closed_up() {
        let raw = "<!--\n  GRL-FR-YIJG. A note for whoever edits this.\n-->\n\nYour task is the prompt.\n";
        assert_eq!(instruction_text(raw), "Your task is the prompt.\n");
    }

    /// A comment in the middle takes its own line and leaves the rest alone —
    /// including blank lines the file itself holds.
    #[test]
    fn a_middle_comment_leaves_the_files_own_blank_lines() {
        let raw = "One.\n\nTwo.\n<!-- aside -->\nThree.\n";
        assert_eq!(instruction_text(raw), "One.\n\nTwo.\nThree.\n");
    }

    /// Prose sharing a line with a comment stays where it was.
    #[test]
    fn prose_beside_a_comment_survives() {
        assert_eq!(
            instruction_text("Keep <!-- drop --> this.\n"),
            "Keep  this.\n"
        );
    }

    /// Several comments in one file all go.
    #[test]
    fn every_comment_goes() {
        let raw = "<!-- a -->\n## one\n\nbody\n\n<!-- b -->\n## two\n";
        let out = instruction_text(raw);
        assert!(!out.contains("<!--"), "{out:?}");
        assert!(!out.contains("-->"), "{out:?}");
        assert!(!out.contains(" a "), "{out:?}");
        assert_eq!(out, "## one\n\nbody\n\n## two\n");
    }

    /// A comment that was opened and never closed runs to the end: what follows
    /// it was never instruction text either.
    #[test]
    fn an_unterminated_comment_runs_to_the_end() {
        assert_eq!(instruction_text("Do it.\n<!-- oops\nnote\n"), "Do it.\n");
    }

    /// The delimiters themselves never survive — and the whole output is
    /// asserted, not merely searched for a delimiter. A function that returned
    /// the empty string for everything would satisfy a search.
    #[test]
    fn no_delimiter_survives() {
        for (raw, expected) in [
            ("<!-- x -->", ""),
            ("a<!-- x -->b", "ab"),
            ("<!--\nmulti\nline\n-->\nbody", "body"),
            ("<!-- one --><!-- two -->\nbody", "body"),
            // A close with no opener is prose: nothing opened it, so nothing
            // about it was ever a comment.
            ("a --> b\n", "a --> b\n"),
            // `<!-->` is not a comment at all — there is no `-->` after the
            // opener — so it is unterminated and runs to the end.
            ("keep\n<!-->", "keep\n"),
        ] {
            let out = instruction_text(raw);
            assert_eq!(out, expected, "for {raw:?}");
            // A `-->` with nothing that opened it is prose and stays prose, so
            // the delimiter check applies to the inputs that held a comment.
            if raw.contains("<!--") {
                assert!(!out.contains("<!--"), "{raw:?} -> {out:?}");
                assert!(!out.contains("-->"), "{raw:?} -> {out:?}");
            }
        }
    }

    /// A nested opener extends the comment rather than closing it early, so no
    /// part of the outer note — its trailing delimiter included — is sent.
    #[test]
    fn a_nested_comment_does_not_leak_its_tail() {
        assert_eq!(
            instruction_text("<!-- outer <!-- inner --> tail -->\nBody\n"),
            "Body\n",
        );
        assert_eq!(
            instruction_text("a <!-- x <!-- y --> z --> b\n"),
            "a  b\n",
        );
    }

    /// CRLF is the ending people break, and the one nothing else here covers.
    #[test]
    fn crlf_endings_are_closed_up_like_any_other() {
        assert_eq!(
            instruction_text("<!-- note -->\r\nBody.\r\n"),
            "Body.\r\n",
        );
        assert_eq!(
            instruction_text("Do it.\r\n<!--\r\n  a note\r\n-->\r\n\r\nBody.\r\n"),
            "Do it.\r\n\r\nBody.\r\n",
        );
        assert_eq!(
            instruction_text("Keep <!-- drop --> this.\r\n"),
            "Keep  this.\r\n",
        );
    }

    /// The shapes a file actually takes at its edges.
    #[test]
    fn the_edges_of_a_file() {
        // A comment at the end with no trailing newline.
        assert_eq!(instruction_text("Body.\n<!-- note -->"), "Body.\n");
        // A file that is nothing but a comment.
        assert_eq!(instruction_text("<!-- all of it -->\n"), "");
        // A comment between two paragraphs keeps the blank lines the file holds
        // around it rather than welding the paragraphs together.
        assert_eq!(
            instruction_text("One.\n\n<!-- aside -->\n\nTwo.\n"),
            "One.\n\n\nTwo.\n",
        );
    }

    /// A NUL in the source is left alone. This file's own repository uses
    /// `\u{0}` as a sentinel elsewhere, so "cannot occur" is not a safe thing
    /// to build on — the removal is decided from the comment byte-ranges, and
    /// no character outside them is touched.
    #[test]
    fn a_nul_in_the_source_survives() {
        assert_eq!(
            instruction_text("before\u{0}after\n<!-- note -->\n"),
            "before\u{0}after\n",
        );
    }

    /// A comment inside a fenced code block is removed like any other, which is
    /// the documented limitation of GRL-FR-YIJG's rule rather than an oversight:
    /// no prompt file writes one today, and a Markdown fence parser in the path
    /// every instruction goes through buys less than it costs. Pinned so the
    /// behaviour is a decision rather than an accident.
    #[test]
    fn a_comment_inside_a_fence_is_removed_too() {
        assert_eq!(
            instruction_text("Use this:\n\n```md\n<!-- a note -->\n```\n"),
            "Use this:\n\n```md\n```\n",
        );
    }
}
