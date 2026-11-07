//! The lines of an aligned block in a PDF (DXP-FR-ERZY, DXP-FR-FNTP).
//!
//! `markdown2pdf` has no alignment for one block. This module breaks the text
//! into lines itself, with the widths of the one font file that the PDF is set
//! in (`BODY_FONT`). It puts no-break spaces in front of a line to move it to
//! the centre or to the right, and after a line to fill it, so the next word
//! starts a new line. `markdown2pdf` keeps a no-break space inside its word
//! and measures it as a space, so it breaks the lines at the same places.
//!
//! `markdown2pdf` drops a word of only white space from the start of a line
//! that it wrapped. The padding in front of a line therefore ends with a
//! zero-width space, which is not white space, so the padding stays.

use markdown2pdf::markdown::Token;

use super::model::{Align, Inline};

/// DXP-FR-FNTP: the font file of every PDF. Bold and italic text is drawn from
/// this one regular face, so it has the same widths.
pub const BODY_FONT: &[u8] = include_bytes!("../../../fonts/LiberationSans-Regular.ttf");

/// The no-break space that pads a line.
const NBSP: char = '\u{00A0}';

/// The zero-width space that ends the padding in front of a line.
const GUARD: char = '\u{200B}';

/// A little less than the width `markdown2pdf` gives, so that a rounding
/// difference never makes a padded line one word too long.
const SAFETY_PT: f32 = 0.5;

/// The width of a Courier character, in 1/1000 em.
const COURIER_WIDTH: f32 = 600.0;

/// The look of a run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Face {
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
}

/// The widths of the body font, and the rule for the built-in Courier.
pub struct Fonts {
    face: ttf_parser::Face<'static>,
    /// The width of a character the font has no glyph for. `markdown2pdf`
    /// uses the mean width of the glyphs it embeds.
    fallback: u16,
}

impl Fonts {
    pub fn load() -> Fonts {
        let face = ttf_parser::Face::parse(BODY_FONT, 0).expect("the bundled font parses");
        let widths: Vec<u16> = (0x21u8..0x7F)
            .filter_map(|c| face.glyph_index(char::from(c)))
            .filter_map(|g| face.glyph_hor_advance(g))
            .collect();
        let sum: u64 = widths.iter().map(|w| u64::from(*w)).sum();
        let fallback = sum.checked_div(widths.len() as u64).map_or(500, |v| v as u16);
        Fonts { face, fallback }
    }

    /// The advance width of `text` in `face` at `size`, in points.
    pub fn measure(&self, face: Face, text: &str, size: f32) -> f32 {
        if face.mono {
            let chars: usize = text.chars().map(courier_chars).sum();
            return chars as f32 * COURIER_WIDTH * size / 1000.0;
        }
        let units: u64 = text
            .chars()
            .map(|c| {
                self.face
                    .glyph_index(c)
                    .and_then(|g| self.face.glyph_hor_advance(g))
                    .map_or(self.fallback, |w| w)
            })
            .map(u64::from)
            .sum();
        units as f32 * size / f32::from(self.face.units_per_em())
    }
}

/// How many characters the built-in Courier writes for `c`: it writes a few
/// punctuation marks as several ASCII characters and any other character as `?`.
fn courier_chars(c: char) -> usize {
    match c as u32 {
        0x2014 | 0x00A9 | 0x00AE | 0x2026 => 3 - usize::from(c == '\u{2014}'),
        0x2122 => 4,
        _ => 1,
    }
}

/// Where the lines go: the width they have and the sizes of their text.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub width: f32,
    pub size: f32,
    pub code_size: f32,
    /// The space `markdown2pdf` adds on each side of an inline code span.
    pub code_pad: f32,
    /// The face of the block, which each run adds to.
    pub base: Face,
}

/// A run of text with its look.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub face: Face,
    pub strike: bool,
    pub link: Option<String>,
}

/// The runs of `inlines`. A hard break is a run of one `\n`.
pub fn pieces(inlines: &[Inline]) -> Vec<Piece> {
    let mut out = Vec::new();
    collect(inlines, Face::default(), false, None, &mut out);
    out
}

fn collect(inlines: &[Inline], face: Face, strike: bool, link: Option<&str>, out: &mut Vec<Piece>) {
    let piece = |text: &str, face: Face| Piece {
        text: text.to_string(),
        face,
        strike,
        link: link.map(str::to_string),
    };
    for inline in inlines {
        let mut push = |text: &str, face: Face| out.push(piece(text, face));
        match inline {
            Inline::Text(t) => push(t, face),
            Inline::Code(t) => push(t, Face { mono: true, ..face }),
            Inline::SoftBreak => push(" ", face),
            Inline::HardBreak => push("\n", face),
            // An inline image shows its alt text in a line of text.
            Inline::Image { alt, .. } => push(&format!("[{alt}]"), Face { italic: true, ..face }),
            Inline::Emphasis(c) => collect(c, Face { italic: true, ..face }, strike, link, out),
            Inline::Strong(c) => collect(c, Face { bold: true, ..face }, strike, link, out),
            Inline::Strike(c) => collect(c, face, true, link, out),
            Inline::Link { url, children } => collect(children, face, strike, Some(url), out),
        }
    }
}

/// One word or one space of a line, with its width.
#[derive(Clone, Debug)]
struct Atom {
    piece: Piece,
    width: f32,
    space: bool,
}

fn atoms(fonts: &Fonts, frame: &Frame, pieces: &[Piece]) -> Vec<Option<Atom>> {
    let mut out = Vec::new();
    for piece in pieces {
        let face = Face {
            bold: piece.face.bold || frame.base.bold,
            italic: piece.face.italic || frame.base.italic,
            mono: piece.face.mono,
        };
        let size = if face.mono { frame.code_size } else { frame.size };
        let mut first = true;
        let start = out.len();
        for (n, line) in piece.text.split('\n').enumerate() {
            if n > 0 {
                out.push(None);
            }
            let mut word = String::new();
            let mut flush = |word: &mut String, out: &mut Vec<Option<Atom>>, space: bool| {
                if word.is_empty() {
                    return;
                }
                let mut width = fonts.measure(face, word, size);
                if face.mono && first && !space {
                    width += frame.code_pad;
                    first = false;
                }
                let piece = Piece { text: std::mem::take(word), face: piece.face, ..piece.clone() };
                out.push(Some(Atom { piece, width, space }));
            };
            for c in line.chars() {
                if c == ' ' || c == '\t' {
                    flush(&mut word, &mut out, false);
                    word.push(' ');
                    flush(&mut word, &mut out, true);
                } else {
                    word.push(c);
                }
            }
            flush(&mut word, &mut out, false);
        }
        if piece.face.mono {
            // The right pad of a code span goes on its last word.
            let words = out[start..].iter_mut().rev().flatten();
            if let Some(last) = words.into_iter().find(|a| !a.space) {
                last.width += frame.code_pad;
            }
        }
    }
    out
}

/// One line: its atoms and its width.
struct Line {
    atoms: Vec<Atom>,
    width: f32,
}

/// Break `pieces` into lines of at most `frame.width`, at spaces and at each
/// `\n`. A word wider than the frame is a line of its own.
fn break_lines(fonts: &Fonts, frame: &Frame, pieces: &[Piece]) -> Vec<Line> {
    let limit = (frame.width - SAFETY_PT).max(1.0);
    let mut lines = vec![Line { atoms: Vec::new(), width: 0.0 }];
    let mut pending_space: Option<Atom> = None;
    for atom in atoms(fonts, frame, pieces) {
        let Some(atom) = atom else {
            lines.push(Line { atoms: Vec::new(), width: 0.0 });
            pending_space = None;
            continue;
        };
        if atom.space {
            if !lines.last().is_some_and(|l| l.atoms.is_empty()) {
                pending_space = Some(atom);
            }
            continue;
        }
        let line = lines.last_mut().expect("one line at least");
        let space = pending_space.take();
        let space_width = space.as_ref().map_or(0.0, |s| s.width);
        if !line.atoms.is_empty() && line.width + space_width + atom.width > limit {
            lines.push(Line { width: atom.width, atoms: vec![atom] });
            continue;
        }
        if let Some(space) = space.filter(|_| !line.atoms.is_empty()) {
            line.width += space.width;
            line.atoms.push(space);
        }
        line.width += atom.width;
        line.atoms.push(atom);
    }
    lines
}

/// DXP-FR-ERZY: the tokens of `pieces` broken into lines that
/// `markdown2pdf` keeps, each line moved to `align` within `frame.width`.
pub fn placed_tokens(fonts: &Fonts, frame: &Frame, pieces: &[Piece], align: Align) -> Vec<Token> {
    let lines = break_lines(fonts, frame, pieces);
    let limit = (frame.width - SAFETY_PT).max(1.0);
    let pad_face = Face { mono: false, ..frame.base };
    let nbsp = fonts.measure(pad_face, " ", frame.size).max(0.1);
    let count = |space: f32| (space.max(0.0) / nbsp).floor() as usize;
    let mut out = Vec::new();
    let last = lines.len().saturating_sub(1);
    for (n, line) in lines.iter().enumerate() {
        let free = limit - line.width;
        let before = match align {
            Align::Left => 0,
            Align::Center => count(free / 2.0),
            Align::Right => count(free),
        };
        if n > 0 {
            out.push(Token::Text(" ".to_string()));
        }
        let mut runs: Vec<Piece> = Vec::new();
        if before > 0 {
            runs.push(pad(before, true));
        }
        for atom in &line.atoms {
            match runs.last_mut() {
                Some(prev) if same_look(prev, &atom.piece) => prev.text.push_str(&atom.piece.text),
                _ => runs.push(atom.piece.clone()),
            }
        }
        // Fill the line, so the first word of the next one does not fit.
        let after = if n < last { count(free - before as f32 * nbsp) } else { 0 };
        if after > 0 {
            runs.push(pad(after, false));
        }
        out.extend(runs.into_iter().map(token_of));
    }
    out
}

/// `n` no-break spaces. The padding in front of a line ends with `GUARD`.
fn pad(n: usize, guard: bool) -> Piece {
    let mut text = NBSP.to_string().repeat(n);
    if guard {
        text.push(GUARD);
    }
    Piece { text, face: Face::default(), strike: false, link: None }
}

fn same_look(a: &Piece, b: &Piece) -> bool {
    a.face == b.face && a.strike == b.strike && a.link == b.link
}

fn token_of(piece: Piece) -> Token {
    let mut token = if piece.face.mono {
        Token::Code { language: String::new(), content: piece.text, block: false }
    } else {
        Token::Text(piece.text)
    };
    if piece.face.italic && !piece.face.mono {
        token = Token::Emphasis { level: 1, content: vec![token] };
    }
    if piece.face.bold {
        token = Token::StrongEmphasis(vec![token]);
    }
    if piece.strike {
        token = Token::Strikethrough(vec![token]);
    }
    match piece.link {
        Some(url) => Token::Link { content: vec![token], url, title: None },
        None => token,
    }
}
