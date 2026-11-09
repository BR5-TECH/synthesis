//! The block model of an export, read from the Markdown body with
//! `pulldown-cmark` (DXP-FR-YRVE).
//!
//! The model applies the marker rules of the rich editor at the top level
//! (DXP-FR-YVIV, DXP-FR-ERZY, DXP-FR-ANXU), so each writer receives blocks that
//! carry their alignment and their spacers, and no HTML comment at all.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// DXP-FR-ERZY: the alignment of a top-level block. Left has no marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A run of inline content.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { url: String, children: Vec<Inline> },
    Image { url: String, alt: String },
    SoftBreak,
    HardBreak,
}

/// One item of a list. `task` is `Some(checked)` for a task item.
#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

/// DXP-FR-INPH: the language of a diagram fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramKind {
    /// DXP-FR-RKJC: a `mermaid` fence, which the backend renders.
    Mermaid,
    /// DXP-FR-WGLT: a `calm` fence, which the frontend draws.
    Calm,
}

impl DiagramKind {
    /// The kind of a fence with the info string `language`, case ignored.
    pub fn of_language(language: &str) -> Option<DiagramKind> {
        if language.eq_ignore_ascii_case("mermaid") {
            Some(DiagramKind::Mermaid)
        } else if language.eq_ignore_ascii_case("calm") {
            Some(DiagramKind::Calm)
        } else {
            None
        }
    }
}

/// A block of the output.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    Code {
        language: Option<String>,
        text: String,
    },
    /// DXP-FR-RKJC, DXP-FR-WGLT: a diagram fence. `index` counts Mermaid and
    /// CALM fences together from 1 in document order (DXP-FR-INPH). `png`
    /// holds the rendered image once the diagram step has run.
    Diagram {
        index: usize,
        kind: DiagramKind,
        source: String,
        png: Option<Vec<u8>>,
    },
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Quote(Vec<Block>),
    Table {
        aligns: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Rule,
    /// DXP-FR-YVIV: empty space of this many body line heights.
    Spacer {
        lines: u32,
    },
    /// DXP-FR-YRVE: raw HTML other than a comment, shown as its text.
    Html(String),
    /// DXP-FR-RPPW: the metadata table, one `(key, value)` per row.
    Metadata(Vec<(String, String)>),
}

/// A block at the top level of the document, with its alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct TopBlock {
    pub block: Block,
    pub align: Align,
}

/// A parsed document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<TopBlock>,
}

impl Document {
    /// The number of diagram fences (DXP-FR-LSYA).
    pub fn diagram_count(&self) -> usize {
        let mut n = 0;
        for top in &self.blocks {
            count_diagrams(&top.block, &mut n);
        }
        n
    }
}

fn count_diagrams(block: &Block, n: &mut usize) {
    match block {
        Block::Diagram { .. } => *n += 1,
        Block::Quote(inner) => inner.iter().for_each(|b| count_diagrams(b, n)),
        Block::List { items, .. } => {
            items.iter().flat_map(|i| i.blocks.iter()).for_each(|b| count_diagrams(b, n))
        }
        _ => {}
    }
}

/// EDT-FR-CSKH: the spacer of each break level, in body line heights.
pub fn break_lines(level: usize) -> u32 {
    match level {
        1 => 12,
        2 => 8,
        3 => 5,
        _ => 3,
    }
}

/// The marker a single line holds (EDT-FR-QALB, EDT-FR-XPMC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Break(u32),
    Align(Align),
}

/// EDT-FR-QALB / EDT-FR-XPMC: the marker of one line, or `None`. The grammar
/// is the one of `src/markdown/markers.ts`: up to three spaces of indent,
/// spaces or tabs inside the comment, lowercase, nothing after it.
pub fn classify_marker_line(line: &str) -> Option<Marker> {
    let line = line.trim_end_matches(['\r', ' ', '\t']);
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let inner = line[indent..].strip_prefix("<!--")?.strip_suffix("-->")?;
    let inner = inner.trim_matches([' ', '\t']);
    if !inner.is_empty() && inner.chars().all(|c| c == '%') {
        return Some(Marker::Break(break_lines(inner.len())));
    }
    match inner {
        "center" => Some(Marker::Align(Align::Center)),
        "right" => Some(Marker::Align(Align::Right)),
        _ => None,
    }
}

/// DXP-FR-ANXU: `text` with every complete HTML comment removed. An `<!--`
/// with no `-->` after it opens no comment, so the text from it onward stays
/// (DXP-FR-BXIO).
pub fn strip_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(end) => rest = &rest[start + 4 + end + 3..],
            None => {
                rest = &rest[start..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// DXP-FR-DJZM: the blocks of an HTML block whose comments are stripped. Text
/// that is still HTML stays raw HTML (DXP-FR-YRVE). Other text, such as the
/// text after a comment on its line, is read again as Markdown.
fn html_remainder(text: &str) -> Vec<Block> {
    let kept = strip_comments(text);
    let trimmed = kept.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.starts_with('<') {
        return vec![Block::Html(kept.trim_end().to_string())];
    }
    // The text holds one comment less than `text`, and an HTML block always
    // starts with `<`, so this reads no HTML block again and ends.
    parse(trimmed).blocks.into_iter().map(|top| top.block).collect()
}

type Item<'a> = (Event<'a>, Range<usize>);

/// Parse the Markdown body (with no frontmatter) into a [`Document`].
pub fn parse(body: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<Item> = Parser::new_ext(body, options).into_offset_iter().collect();
    let mut reader = Reader { events, pos: 0, diagrams: 0, in_comment: None };
    let mut doc = Document::default();
    // DXP-FR-ERZY: an alignment waits for the block that starts on the very
    // next line, and is dropped otherwise.
    let mut pending: Option<(Align, usize)> = None;
    while reader.pos < reader.events.len() {
        let (event, range) = reader.events[reader.pos].clone();
        if let Event::Start(Tag::HtmlBlock) = event {
            let text = reader.html_block();
            let marker = single_line(&text).and_then(classify_marker_line);
            match marker {
                Some(Marker::Break(lines)) => {
                    pending = None;
                    doc.blocks
                        .push(TopBlock { block: Block::Spacer { lines }, align: Align::Left });
                }
                Some(Marker::Align(align)) => pending = Some((align, range.start)),
                None => {
                    pending = None;
                    for block in html_remainder(&text) {
                        doc.blocks.push(TopBlock { block, align: Align::Left });
                    }
                }
            }
            continue;
        }
        let align = match pending.take() {
            Some((align, at)) if newlines(&body[at..range.start.max(at)]) == 1 => align,
            _ => Align::Left,
        };
        for block in reader.block() {
            doc.blocks.push(TopBlock { block, align });
        }
    }
    doc
}

/// The text of a one-line HTML block, or `None` when it spans more lines.
fn single_line(text: &str) -> Option<&str> {
    let trimmed = text.trim_end_matches(['\n', '\r']);
    (!trimmed.contains('\n')).then_some(trimmed)
}

fn newlines(text: &str) -> usize {
    text.matches('\n').count()
}

struct Reader<'a> {
    events: Vec<Item<'a>>,
    pos: usize,
    diagrams: usize,
    /// DXP-FR-SRHN: the raw text of an inline comment that spans several
    /// events, while it is not closed yet.
    in_comment: Option<String>,
}

impl<'a> Reader<'a> {
    fn next(&mut self) -> Option<Event<'a>> {
        let event = self.events.get(self.pos).map(|(e, _)| e.clone());
        self.pos += 1;
        event
    }

    /// The text of the HTML block that starts at the current event.
    fn html_block(&mut self) -> String {
        self.pos += 1;
        let mut text = String::new();
        while let Some(event) = self.next() {
            match event {
                Event::End(TagEnd::HtmlBlock) => break,
                Event::Html(t) | Event::Text(t) => text.push_str(&t),
                _ => {}
            }
        }
        text
    }

    /// The blocks that start at the current event: none for an event that
    /// makes no block, and more than one for an HTML block whose text after a
    /// comment is Markdown (DXP-FR-DJZM).
    fn block(&mut self) -> Vec<Block> {
        let Some(event) = self.next() else {
            return Vec::new();
        };
        if let Event::Start(Tag::HtmlBlock) = event {
            self.pos -= 1;
            let text = self.html_block();
            return html_remainder(&text);
        }
        self.single_block(event).into_iter().collect()
    }

    fn single_block(&mut self, event: Event<'a>) -> Option<Block> {
        match event {
            Event::Start(Tag::Paragraph) => Some(Block::Paragraph(self.inlines(TagEnd::Paragraph))),
            Event::Start(Tag::Heading { level, .. }) => {
                let inlines = self.inlines(TagEnd::Heading(level));
                Some(Block::Heading { level: heading_level(level), inlines })
            }
            Event::Start(Tag::CodeBlock(kind)) => Some(self.code_block(kind)),
            Event::Start(Tag::BlockQuote(_)) => {
                Some(Block::Quote(self.blocks_until(|e| matches!(e, TagEnd::BlockQuote(_)))))
            }
            Event::Start(Tag::List(start)) => Some(self.list(start)),
            Event::Start(Tag::Table(aligns)) => Some(self.table(aligns)),
            Event::Rule => Some(Block::Rule),
            Event::Start(tag) => {
                // A construct the export does not model: keep its text.
                let end = tag.to_end();
                Some(Block::Paragraph(self.inlines(end)))
            }
            _ => None,
        }
    }

    fn blocks_until(&mut self, is_end: impl Fn(&TagEnd) -> bool) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some((event, _)) = self.events.get(self.pos) {
            if let Event::End(end) = event {
                if is_end(end) {
                    self.pos += 1;
                    break;
                }
            }
            blocks.extend(self.block());
        }
        blocks
    }

    fn code_block(&mut self, kind: CodeBlockKind<'a>) -> Block {
        let mut text = String::new();
        while let Some(event) = self.next() {
            match event {
                Event::End(TagEnd::CodeBlock) => break,
                Event::Text(t) => text.push_str(&t),
                _ => {}
            }
        }
        let language = match kind {
            CodeBlockKind::Fenced(info) => {
                info.split_whitespace().next().map(str::to_string).filter(|l| !l.is_empty())
            }
            CodeBlockKind::Indented => None,
        };
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        if let Some(kind) = language.as_deref().and_then(DiagramKind::of_language) {
            self.diagrams += 1;
            return Block::Diagram { index: self.diagrams, kind, source: text, png: None };
        }
        Block::Code { language, text }
    }

    fn list(&mut self, start: Option<u64>) -> Block {
        let mut items = Vec::new();
        while let Some(event) = self.next() {
            match event {
                Event::End(TagEnd::List(_)) => break,
                Event::Start(Tag::Item) => items.push(self.item()),
                _ => {}
            }
        }
        Block::List { start, items }
    }

    fn item(&mut self) -> ListItem {
        let mut task = None;
        let mut blocks = Vec::new();
        let mut loose: Vec<Inline> = Vec::new();
        while let Some((event, _)) = self.events.get(self.pos).cloned() {
            match event {
                Event::End(TagEnd::Item) => {
                    self.pos += 1;
                    self.end_open_comment(&mut loose);
                    break;
                }
                Event::TaskListMarker(checked) => {
                    task = Some(checked);
                    self.pos += 1;
                }
                Event::Start(Tag::Paragraph)
                | Event::Start(Tag::Heading { .. })
                | Event::Start(Tag::CodeBlock(_))
                | Event::Start(Tag::BlockQuote(_))
                | Event::Start(Tag::List(_))
                | Event::Start(Tag::Table(_))
                | Event::Start(Tag::HtmlBlock)
                | Event::Rule => {
                    self.end_open_comment(&mut loose);
                    if !loose.is_empty() {
                        blocks.push(Block::Paragraph(std::mem::take(&mut loose)));
                    }
                    blocks.extend(self.block());
                }
                _ => {
                    self.pos += 1;
                    self.inline_event(event, &mut loose);
                }
            }
        }
        if !loose.is_empty() {
            blocks.push(Block::Paragraph(loose));
        }
        ListItem { task, blocks }
    }

    fn table(&mut self, aligns: Vec<Alignment>) -> Block {
        let aligns = aligns
            .into_iter()
            .map(|a| match a {
                Alignment::Center => Align::Center,
                Alignment::Right => Align::Right,
                _ => Align::Left,
            })
            .collect();
        let mut header = Vec::new();
        let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
        let mut in_head = false;
        while let Some(event) = self.next() {
            match event {
                Event::End(TagEnd::Table) => break,
                Event::Start(Tag::TableHead) => in_head = true,
                Event::End(TagEnd::TableHead) => in_head = false,
                Event::Start(Tag::TableRow) => rows.push(Vec::new()),
                Event::Start(Tag::TableCell) => {
                    let cell = self.inlines(TagEnd::TableCell);
                    if in_head {
                        header.push(cell);
                    } else if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
                _ => {}
            }
        }
        Block::Table { aligns, header, rows }
    }

    /// The inline content up to the end tag `end`.
    fn inlines(&mut self, end: TagEnd) -> Vec<Inline> {
        let mut out = Vec::new();
        while let Some(event) = self.next() {
            if event == Event::End(end) {
                break;
            }
            self.inline_event(event, &mut out);
        }
        // DXP-FR-SRHN: a comment ends at the latest with the paragraph,
        // heading, or table cell that holds it.
        if matches!(end, TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell) {
            self.end_open_comment(&mut out);
        }
        out
    }

    /// DXP-FR-BXIO: a comment that the block ends before its `-->` was no
    /// comment, so its text goes back into the output.
    fn end_open_comment(&mut self, out: &mut Vec<Inline>) {
        if let Some(raw) = self.in_comment.take() {
            push_text(out, &raw);
        }
    }

    fn inline_event(&mut self, event: Event<'a>, out: &mut Vec<Inline>) {
        if let Some(raw) = self.in_comment.as_mut() {
            // DXP-FR-SRHN: the rest of a comment that spans several events.
            let piece = match &event {
                Event::InlineHtml(t) | Event::Text(t) | Event::Html(t) | Event::Code(t) => {
                    t.to_string()
                }
                Event::SoftBreak | Event::HardBreak => "\n".to_string(),
                _ => String::new(),
            };
            if let Some(end) = piece.find("-->") {
                self.in_comment = None;
                let rest = &piece[end + 3..];
                if !rest.is_empty() {
                    push_text(out, rest);
                }
            } else {
                raw.push_str(&piece);
            }
            return;
        }
        match event {
            Event::Text(t) => push_text(out, &t),
            Event::Code(t) => out.push(Inline::Code(t.to_string())),
            Event::SoftBreak => out.push(Inline::SoftBreak),
            Event::HardBreak => out.push(Inline::HardBreak),
            Event::InlineHtml(t) | Event::Html(t) => {
                if t.trim_start().starts_with("<!--") {
                    // DXP-FR-ANXU: a comment is dropped. One that does not
                    // close in this event continues in the next ones.
                    if t.contains("-->") {
                        let kept = strip_comments(&t);
                        if !kept.is_empty() {
                            push_text(out, &kept);
                        }
                    } else {
                        self.in_comment = Some(t.to_string());
                    }
                } else {
                    push_text(out, &t);
                }
            }
            Event::Start(Tag::Emphasis) => {
                out.push(Inline::Emphasis(self.inlines(TagEnd::Emphasis)))
            }
            Event::Start(Tag::Strong) => out.push(Inline::Strong(self.inlines(TagEnd::Strong))),
            Event::Start(Tag::Strikethrough) => {
                out.push(Inline::Strike(self.inlines(TagEnd::Strikethrough)))
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let children = self.inlines(TagEnd::Link);
                out.push(Inline::Link { url: dest_url.to_string(), children });
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let alt = plain_text(&self.inlines(TagEnd::Image));
                out.push(Inline::Image { url: dest_url.to_string(), alt });
            }
            Event::Start(tag) => {
                let children = self.inlines(tag.to_end());
                out.extend(children);
            }
            Event::FootnoteReference(label) => push_text(out, &format!("[{label}]")),
            Event::TaskListMarker(checked) => push_text(out, if checked { "☑ " } else { "☐ " }),
            _ => {}
        }
    }
}

fn push_text(out: &mut Vec<Inline>, text: &str) {
    if let Some(Inline::Text(last)) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Inline::Text(text.to_string()));
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The text of inline content with every style removed.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strike(c) => {
                out.push_str(&plain_text(c))
            }
            Inline::Link { children, .. } => out.push_str(&plain_text(children)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak => out.push(' '),
            Inline::HardBreak => out.push('\n'),
        }
    }
    out
}
