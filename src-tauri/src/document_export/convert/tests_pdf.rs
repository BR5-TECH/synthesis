//! Tests of the PDF lines of aligned blocks and of metadata values
//! (`DXP-document-export.md` DXP-FR-ERZY, DXP-FR-RPPW, DXP-FR-IJOH).

use markdown2pdf::markdown::Token;

use super::model::{parse, Align, Inline};
use super::pdf::{export_style, tokens};
use super::pdf_lines::{self, Face, Fonts, Frame};
use super::Assets;

const NBSP: char = '\u{00A0}';

fn frame(width: f32) -> Frame {
    Frame { width, size: 10.0, code_size: 9.0, code_pad: 1.0, base: Face::default() }
}

/// The plain text of a token, with its styles removed.
fn text_of(token: &Token) -> String {
    match token {
        Token::Text(t) => t.clone(),
        Token::Code { content, .. } => content.clone(),
        Token::Emphasis { content, .. }
        | Token::StrongEmphasis(content)
        | Token::Strikethrough(content)
        | Token::Link { content, .. }
        | Token::Heading(content, _)
        | Token::BlockQuote(content) => content.iter().map(text_of).collect(),
        Token::ListItem { content, .. } => content.iter().map(text_of).collect(),
        _ => String::new(),
    }
}

/// The lines of placed tokens: each line is its text, split at the breaking
/// spaces between lines.
fn lines_of(tokens: &[Token]) -> Vec<String> {
    let mut lines = vec![String::new()];
    for token in tokens {
        match token {
            Token::Text(t) if t == " " => lines.push(String::new()),
            other => lines.last_mut().unwrap().push_str(&text_of(other)),
        }
    }
    lines
}

fn leading_pad(line: &str) -> usize {
    line.chars().take_while(|c| *c == NBSP).count()
}

fn width(fonts: &Fonts, text: &str) -> f32 {
    fonts.measure(Face::default(), text, 10.0)
}

// DXP-FR-ERZY: a centred line has equal free space on each side, within one
// space, and a right-aligned line ends at the right edge.
#[test]
fn a_short_line_is_centred_or_moved_right() {
    let fonts = Fonts::load();
    let words = vec![Inline::Text("Centred words".into())];
    let pieces = pdf_lines::pieces(&words);
    let space = width(&fonts, " ");
    let text = width(&fonts, "Centred words");
    let f = frame(400.0);

    let centred = lines_of(&pdf_lines::placed_tokens(&fonts, &f, &pieces, Align::Center));
    assert_eq!(centred.len(), 1);
    let left = leading_pad(&centred[0]) as f32 * space;
    let right = 400.0 - left - text;
    assert!((left - right).abs() <= 2.0 * space + 0.5, "left {left} right {right}");
    assert!(centred[0].ends_with("Centred words"));

    let moved = lines_of(&pdf_lines::placed_tokens(&fonts, &f, &pieces, Align::Right));
    let free = 400.0 - leading_pad(&moved[0]) as f32 * space - text;
    assert!((0.0..=space + 0.5).contains(&free), "free {free}");

    let plain = lines_of(&pdf_lines::placed_tokens(&fonts, &f, &pieces, Align::Left));
    assert_eq!(plain, vec!["Centred words".to_string()]);
}

// DXP-FR-ERZY: a long aligned paragraph breaks into lines that each fit the
// frame. Each line but the last is filled, so `markdown2pdf` keeps the breaks.
#[test]
fn a_long_aligned_paragraph_breaks_into_lines_that_fit() {
    let fonts = Fonts::load();
    let text = "one two three four five six seven eight nine ten eleven twelve thirteen";
    let pieces = pdf_lines::pieces(&[Inline::Text(text.into())]);
    let f = frame(120.0);
    let lines = lines_of(&pdf_lines::placed_tokens(&fonts, &f, &pieces, Align::Center));
    assert!(lines.len() > 2, "{lines:?}");
    let space = width(&fonts, " ");
    for (n, line) in lines.iter().enumerate() {
        let w = width(&fonts, line);
        assert!(w <= 120.0, "line {n} is {w} wide: {line:?}");
        if n + 1 < lines.len() {
            assert!(w > 120.0 - 0.5 - space, "line {n} is filled: {w}");
        }
    }
    let words: Vec<String> = lines
        .iter()
        .flat_map(|l| l.split([' ', NBSP, '\u{200B}']).filter(|w| !w.is_empty()))
        .map(str::to_string)
        .collect();
    assert_eq!(words.join(" "), text, "no word is lost or moved");
}

// DXP-FR-ERZY: the styles of the runs stay on the aligned line.
#[test]
fn an_aligned_line_keeps_its_styles() {
    let fonts = Fonts::load();
    let inlines = vec![
        Inline::Strong(vec![Inline::Text("bold".into())]),
        Inline::Text(" and ".into()),
        Inline::Link { url: "https://x.test".into(), children: vec![Inline::Text("link".into())] },
    ];
    let tokens = pdf_lines::placed_tokens(
        &fonts,
        &frame(300.0),
        &pdf_lines::pieces(&inlines),
        Align::Center,
    );
    assert!(tokens.iter().any(|t| matches!(t, Token::StrongEmphasis(_))), "{tokens:?}");
    assert!(tokens.iter().any(|t| matches!(t, Token::Link { url, .. } if url == "https://x.test")));
}

/// The token stream of a PDF export of `text`.
fn pdf_tokens(text: &str) -> Vec<Token> {
    let dir = tempfile::tempdir().unwrap();
    let style = export_style(dir.path());
    let write_file = |_: &str, _: &[u8]| None;
    tokens(&parse(text), &Assets::default(), &style, &write_file)
}

fn tables(tokens: &[Token]) -> usize {
    tokens.iter().filter(|t| matches!(t, Token::Table { .. })).count()
}

// DXP-FR-ERZY, DXP-FR-IJOH: an aligned paragraph and heading are padded text,
// not a table, and the real table of the document stays a table.
#[test]
fn aligned_paragraphs_and_headings_are_not_tables() {
    let tokens = pdf_tokens(
        "<!-- center -->\n# Title\n\n<!-- right -->\nRight words\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
    );
    assert_eq!(tables(&tokens), 1, "only the real table: {tokens:?}");
    let heading = tokens.iter().find(|t| matches!(t, Token::Heading(..))).expect("the heading");
    assert!(leading_pad(&text_of(heading)) > 0, "the heading is centred: {heading:?}");
    let right =
        tokens.iter().position(|t| text_of(t).contains("Right words")).expect("the paragraph");
    assert!(leading_pad(&text_of(&tokens[right])) > 0, "the paragraph is moved right");
}

// DXP-FR-ERZY: an aligned list and an aligned quote have their text moved.
#[test]
fn an_aligned_list_and_quote_have_their_text_moved() {
    let tokens =
        pdf_tokens("<!-- center -->\n- centred item\n- second\n\n<!-- right -->\n> right quote\n");
    assert_eq!(tables(&tokens), 0, "{tokens:?}");
    let items: Vec<String> =
        tokens.iter().filter(|t| matches!(t, Token::ListItem { .. })).map(text_of).collect();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| leading_pad(i) > 0), "{items:?}");
    let quote = tokens.iter().find(|t| matches!(t, Token::BlockQuote(_))).expect("the quote");
    assert!(leading_pad(&text_of(quote)) > 0, "{quote:?}");
    // A list and a quote with no marker keep their text at the left.
    let plain = pdf_tokens("- item\n\n> quote\n");
    assert!(plain.iter().all(|t| leading_pad(&text_of(t)) == 0));
}

// DXP-FR-RPPW: a list value of the metadata table is one row per line, and the
// key shows in the first row only.
#[test]
fn a_metadata_list_value_is_one_row_per_line() {
    let dir = tempfile::tempdir().unwrap();
    let style = export_style(dir.path());
    let doc =
        super::document_of("---\ntags:\n  - alpha\n  - beta\ntitle: Guide\n---\nBody\n", true);
    let write_file = |_: &str, _: &[u8]| None;
    let tokens = tokens(&doc, &Assets::default(), &style, &write_file);
    let Some(Token::Table { headers, rows, .. }) = tokens.first() else {
        panic!("the metadata table: {tokens:?}");
    };
    assert!(headers.iter().all(|h| h.content.is_empty()), "no header text");
    let cells: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            (r[0].content.iter().map(text_of).collect(), r[1].content.iter().map(text_of).collect())
        })
        .collect();
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());
    assert_eq!(
        cells,
        vec![pair("tags", "- alpha"), pair("", "- beta"), pair("title", "Guide")],
        "{rows:?}"
    );
}

// DXP-FR-FNTP: the PDF is set in the bundled Liberation Sans and Liberation
// Mono files, written beside each other in the export folder so the renderer
// finds the bold and italic faces. It looks up no font of the host.
#[test]
fn the_pdf_is_pinned_to_the_bundled_fonts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let access = crate::fs::FsAccess::builder().allow_root(&root).build().unwrap();
    let write_file = |name: &str, bytes: &[u8]| {
        let path = root.join(name);
        access.write_bytes_atomic(&path, bytes).ok().map(|_| path)
    };
    let config = super::pdf::font_config(&write_file);
    let body = root.join(pdf_lines::SANS_REGULAR_FILE);
    let code = root.join(pdf_lines::MONO_REGULAR_FILE);
    assert!(
        matches!(&config.default_font_source, Some(markdown2pdf::fonts::FontSource::File(p)) if *p == body),
        "{config:?}"
    );
    assert!(
        matches!(&config.code_font_source, Some(markdown2pdf::fonts::FontSource::File(p)) if *p == code),
        "{config:?}"
    );
    for (name, bytes) in pdf_lines::FONT_FILES {
        assert_eq!(access.read_bytes(root.join(name)).unwrap(), bytes, "{name} is written");
    }
    let style = export_style(std::path::Path::new("."));
    assert!(style.fallback_fonts.is_empty(), "no fallback font of the host");
    // The body faces carry the padding characters of DXP-FR-ERZY, and every
    // face carries the non-ASCII text a document can hold.
    let sans = [
        pdf_lines::SANS_REGULAR,
        pdf_lines::SANS_BOLD,
        pdf_lines::SANS_ITALIC,
        pdf_lines::SANS_BOLD_ITALIC,
    ];
    for bytes in sans {
        let face = ttf_parser::Face::parse(bytes, 0).expect("the bundled font parses");
        for c in ['\u{00A0}', '\u{200B}', '\u{2022}', ' ', '→', 'ü'] {
            assert!(face.glyph_index(c).is_some(), "the body font has a glyph for {c:?}");
        }
    }
    for bytes in [pdf_lines::MONO_REGULAR, pdf_lines::MONO_BOLD] {
        let face = ttf_parser::Face::parse(bytes, 0).expect("the bundled font parses");
        for c in [' ', '→', 'ü', '{'] {
            assert!(face.glyph_index(c).is_some(), "the code font has a glyph for {c:?}");
        }
    }
}

// DXP-FR-FNTP: where a font file does not write, the PDF is still set in the
// bundled regular faces, from memory.
#[test]
fn a_font_file_that_does_not_write_leaves_the_bundled_regular_faces() {
    let write_file = |_: &str, _: &[u8]| None;
    let config = super::pdf::font_config(&write_file);
    assert!(matches!(
        config.default_font_source,
        Some(markdown2pdf::fonts::FontSource::Bytes(b)) if b == pdf_lines::SANS_REGULAR
    ));
    assert!(matches!(
        config.code_font_source,
        Some(markdown2pdf::fonts::FontSource::Bytes(b)) if b == pdf_lines::MONO_REGULAR
    ));
}

/// The text of a token stream, a break at each line.
fn flat(token: &Token, out: &mut String) {
    match token {
        Token::Text(t) if t == " " => out.push('\n'),
        Token::Text(t) => out.push_str(t),
        Token::Newline => out.push('\n'),
        Token::Code { content, .. } => out.push_str(content),
        Token::Emphasis { content, .. }
        | Token::StrongEmphasis(content)
        | Token::Strikethrough(content)
        | Token::Link { content, .. }
        | Token::Heading(content, _)
        | Token::BlockQuote(content)
        | Token::ListItem { content, .. } => content.iter().for_each(|t| flat(t, out)),
        _ => {}
    }
}

/// A line without its padding and with single spaces.
fn plain(line: &str) -> String {
    let kept: String = line.chars().filter(|c| *c != NBSP && *c != '\u{200B}').collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

const ALIGNED: &str = "<!-- center -->
This is a centred paragraph with `inline code` and a good number of words so that it wraps across several lines of the page body width, and **bold start** more words here to fill the third line as well so we see it all, wow.

<!-- right -->
**Bold start** of a right aligned paragraph that is long enough to wrap onto a second and a third line of the page for testing the pads here and there and also a fourth line please, thank you very much indeed.

<!-- center -->
# A centred heading that is long enough to wrap onto a second line of the page, which it does

<!-- center -->
- first item of the list with enough words to wrap across the line of the page for sure yes it is very very very long indeed, so long that it needs a second line to show
- [x] **bold** task
  - nested item that is also quite long to need a wrapped line over the width of the page for the test yes it is quite long and then some more words

<!-- right -->
9. numbered right item that is quite long to need a wrapped line over the width of the page for sure yes it is long and so on and so on and then more words
10. two
11. [ ] a task of a numbered list that is long enough to need a second line of the page body

<!-- center -->
> a centred quote that is long enough to wrap onto a second line of the quote block of the page for sure and more and more words are here so that it wraps at least twice in the page
";

/// A line of the PDF without the marker of a list item.
fn without_marker(line: &str) -> String {
    let line = plain(line);
    let line = line.strip_prefix('\u{2022}').unwrap_or(&line).trim_start().to_string();
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    match line[digits..].strip_prefix('.') {
        Some(rest) if digits > 0 => rest.trim_start().to_string(),
        _ => line,
    }
}

// DXP-FR-ERZY, DXP-FR-FNTP: every line of an aligned paragraph, heading, list,
// and quote breaks in the PDF where the export broke it. The renderer wraps no
// line that the export did not plan, and none of them loses its words.
#[test]
fn aligned_lines_break_in_the_pdf_where_the_export_broke_them() {
    let dir = tempfile::tempdir().unwrap();
    let style = export_style(dir.path());
    let write_file = |_: &str, _: &[u8]| None;
    let planned = tokens(&parse(ALIGNED), &Assets::default(), &style, &write_file);
    let mut text = String::new();
    planned.iter().for_each(|t| flat(t, &mut text));
    let expected: Vec<String> = text.lines().map(plain).filter(|l| !l.is_empty()).collect();
    assert!(expected.len() >= 12, "the document has many lines: {expected:?}");

    let (converted, _) =
        super::tests::run(ALIGNED, super::Format::Pdf, false, &super::tests::render_ok, dir.path());
    let extracted = pdf_extract::extract_text_from_mem(&converted.bytes).expect("a readable PDF");
    let actual: Vec<String> =
        extracted.lines().map(without_marker).filter(|l| !l.is_empty()).collect();
    assert_eq!(actual, expected);
}

// DXP-FR-ERZY: the padding in front of a line stays when the line starts with
// bold text. The renderer drops a padding that is a word of its own from the
// start of a line that it wrapped.
#[test]
fn the_padding_stays_in_front_of_a_line_that_starts_with_bold_text() {
    let words: Vec<String> = (1..=300).map(|n| format!("w{}{n}", "i".repeat(n * 7 % 5))).collect();
    let source = format!("<!-- right -->\n**{}**\n", words.join(" "));
    let dir = tempfile::tempdir().unwrap();
    let style = export_style(dir.path());
    let write_file = |_: &str, _: &[u8]| None;
    let planned = tokens(&parse(&source), &Assets::default(), &style, &write_file);
    let mut text = String::new();
    planned.iter().for_each(|t| flat(t, &mut text));
    let expected: Vec<usize> =
        text.lines().filter(|l| !plain(l).is_empty()).map(leading_pad).collect();
    assert!(expected.len() > 3 && expected.iter().filter(|p| **p > 0).count() > 2, "{expected:?}");

    let (converted, _) =
        super::tests::run(&source, super::Format::Pdf, false, &super::tests::render_ok, dir.path());
    let extracted = pdf_extract::extract_text_from_mem(&converted.bytes).expect("a readable PDF");
    let actual: Vec<usize> =
        extracted.lines().filter(|l| !plain(l).is_empty()).map(leading_pad).collect();
    assert_eq!(actual, expected);
}
