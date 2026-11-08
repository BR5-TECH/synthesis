//! The leading YAML frontmatter of a DOC artifact (DXP-FR-RPPW, DXP-FR-WAQF).
//!
//! The export reads the block and never rewrites the file. It reads the top
//! level of the mapping only: a key and its value, where a list or a mapping
//! value stays the YAML text it is written as.

/// The frontmatter split off a file: its raw text and the body after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split<'a> {
    pub frontmatter: Option<&'a str>,
    pub body: &'a str,
}

/// EDT-FR-18: a leading block between `---` fences. A file whose first line
/// is not `---`, or whose block has no closing fence, has no frontmatter.
pub fn split(text: &str) -> Split<'_> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let none = Split { frontmatter: None, body: text };
    let Some(first_end) = text.find('\n') else { return none };
    if text[..first_end].trim_end() != "---" {
        return none;
    }
    let inner_start = first_end + 1;
    let mut at = inner_start;
    while at <= text.len() {
        let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        let line = text[at..line_end].trim_end();
        if line == "---" || line == "..." {
            let body_start = (line_end + 1).min(text.len());
            return Split { frontmatter: Some(&text[inner_start..at]), body: &text[body_start..] };
        }
        if line_end == text.len() {
            break;
        }
        at = line_end + 1;
    }
    none
}

/// What the frontmatter renders as when it is included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rendered {
    /// DXP-FR-RPPW: one `(key, value)` per top-level key, in file order.
    Table(Vec<(String, String)>),
    /// DXP-FR-WAQF: a block that is not a YAML mapping, as its text.
    Text(String),
    /// An empty block renders nothing.
    Nothing,
}

/// DXP-FR-RPPW / DXP-FR-WAQF: read the top level of the block.
pub fn render(block: &str) -> Rendered {
    let lines: Vec<&str> = block.lines().collect();
    let meaningful = |l: &&str| !l.trim().is_empty() && !l.trim_start().starts_with('#');
    if !lines.iter().any(meaningful) {
        return Rendered::Nothing;
    }
    let mut rows: Vec<(String, Vec<String>, String)> = Vec::new();
    for line in &lines {
        let top_level = !line.starts_with([' ', '\t']) && !line.starts_with('-');
        if top_level && meaningful(line) {
            let Some((key, value)) = split_key(line) else {
                return Rendered::Text(block.trim_end().to_string());
            };
            rows.push((key, Vec::new(), value));
        } else if let Some(row) = rows.last_mut() {
            row.1.push(line.to_string());
        } else if meaningful(line) {
            return Rendered::Text(block.trim_end().to_string());
        }
    }
    let table =
        rows.into_iter().map(|(key, nested, value)| (key, value_text(&value, &nested))).collect();
    Rendered::Table(table)
}

/// `key: value` at the top level, with the key unquoted.
fn split_key(line: &str) -> Option<(String, String)> {
    let colon = if line.starts_with(['"', '\'']) {
        let quote = line.chars().next()?;
        let close = line[1..].find(quote)? + 1;
        close + line[close..].find(':')?
    } else {
        line.find(": ")
            .or_else(|| line.trim_end().ends_with(':').then(|| line.trim_end().len() - 1))?
    };
    let key = unquote(line[..colon].trim());
    if key.is_empty() {
        return None;
    }
    Some((key, line[colon + 1..].trim().to_string()))
}

/// The text a value shows: a scalar unquoted, a block scalar as its lines,
/// and a nested list or mapping as its YAML text.
fn value_text(value: &str, nested: &[String]) -> String {
    let nested: Vec<&str> = {
        let mut lines: Vec<&str> = nested.iter().map(String::as_str).collect();
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        lines
    };
    if value.starts_with('|') || value.starts_with('>') {
        let indent = nested
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        let body: Vec<&str> = nested.iter().map(|l| l.get(indent..).unwrap_or("")).collect();
        return if value.starts_with('>') { body.join(" ") } else { body.join("\n") };
    }
    if nested.is_empty() {
        return unquote(value);
    }
    let mut text = String::new();
    if !value.is_empty() {
        text.push_str(value);
        text.push('\n');
    }
    let indent = nested
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let body: Vec<&str> = nested.iter().map(|l| l.get(indent..).unwrap_or("")).collect();
    text.push_str(&body.join("\n"));
    text
}

fn unquote(text: &str) -> String {
    let t = text.trim();
    for q in ['"', '\''] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}
