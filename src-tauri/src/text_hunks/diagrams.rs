//! Turning a diagram hunk into fence text before it is placed
//! (`DCP-draft-change-proposals.md` DCP-FR-WSLE, DCP-FR-HXNP).
//!
//! The agent gives a diagram change as Mermaid sources. A hunk holds text and
//! an acceptance is one splice (DCP-FR-12), so this step finds the fence the
//! `before` source names, and builds the text that replaces it. The result goes
//! on through the same record and the same ledger as any text hunk.
//!
//! Pure over `&str`. It judges no source (PDM-FR-HOQI).

use super::anchors::{normalise_newlines, HunkKind};
use super::fences::{code_block_ranges, comparable, find_fences, Fence};
use super::hunks::{find_run, PlanError, ProposedHunk};

/// A diagram hunk in its text form, and the byte range of the prompt it covers.
pub(super) struct Placed {
    /// The hunk with fence text in `before` and `after`.
    pub hunk: ProposedHunk,
    /// The range the hunk covers. An insertion has an empty range.
    pub range: (usize, usize),
}

/// Place one diagram hunk against `prompt`, which is already normalised.
///
/// `at` is the one-based place of the hunk, which every refusal carries. The
/// caller has checked the bounds of `lead` and `trail`.
pub(super) fn place(prompt: &str, item: &ProposedHunk, at: usize) -> Result<Placed, PlanError> {
    let lead = normalise_newlines(item.lead.as_deref().unwrap_or(""));
    let trail = normalise_newlines(item.trail.as_deref().unwrap_or(""));
    match item.kind {
        HunkKind::Add => place_addition(prompt, item, &lead, &trail, at),
        HunkKind::Del | HunkKind::Replace => place_fence(prompt, item, &lead, &trail, at),
    }
}

/// An insertion of a new fence after the end of the line that holds the text
/// the new fence follows, or after the closing line of the fenced code block
/// that holds that line (DCP-FR-WSLE).
fn place_addition(
    prompt: &str,
    item: &ProposedHunk,
    lead: &str,
    trail: &str,
    at: usize,
) -> Result<Placed, PlanError> {
    let follows = normalise_newlines(item.after_text.as_deref().unwrap_or(""));
    let source = comparable(item.after.as_deref().unwrap_or(""));
    let fence = fence_text("```mermaid", &source, "", "```");
    let (point, after) = if follows.is_empty() && lead.is_empty() && trail.is_empty() {
        // As for a text insertion: only an empty prompt has one place for it.
        if !prompt.is_empty() {
            return Err(PlanError::AnchorLost(at));
        }
        (0, fence)
    } else {
        let found = find_run(prompt, lead, &follows, trail, at)?;
        let point = found + follows.len();
        let line_end = prompt[point..].find('\n').map_or(prompt.len(), |i| point + i);
        // A line inside a fenced code block of any language: the new fence goes
        // after the closing line of that block, so the block stays whole.
        let line_end = code_block_ranges(prompt)
            .into_iter()
            .find(|&(start, end)| start <= line_end && line_end <= end)
            .map_or(line_end, |(_, end)| end);
        (line_end, format!("\n\n{fence}"))
    };
    Ok(Placed { hunk: text_form(item, None, Some(after)), range: (point, point) })
}

/// A change or a removal of the one fence whose source the hunk names.
fn place_fence(
    prompt: &str,
    item: &ProposedHunk,
    lead: &str,
    trail: &str,
    at: usize,
) -> Result<Placed, PlanError> {
    let wanted = comparable(item.before.as_deref().unwrap_or(""));
    if wanted.is_empty() {
        return Err(PlanError::AnchorLost(at));
    }
    let mut candidates: Vec<Fence> = find_fences(prompt)
        .into_iter()
        .filter(|f| comparable(&f.source) == wanted)
        .filter(|f| prompt[..f.start].ends_with(lead) && prompt[f.end..].starts_with(trail))
        .collect();
    let fence = match candidates.len() {
        0 => return Err(PlanError::AnchorLost(at)),
        1 => candidates.remove(0),
        _ => return Err(PlanError::Ambiguous(at)),
    };
    let whole = prompt[fence.start..fence.end].to_string();
    if item.kind == HunkKind::Del {
        // The line break that ends the fence goes with it, and so does one
        // blank line after it, where the prompt holds one.
        let breaks = prompt[fence.end..].chars().take(2).take_while(|c| *c == '\n').count();
        let end = fence.end + breaks;
        let before = prompt[fence.start..end].to_string();
        return Ok(Placed { hunk: text_form(item, Some(before), None), range: (fence.start, end) });
    }
    let source = comparable(item.after.as_deref().unwrap_or(""));
    let after = fence_text(&fence.opening, &source, &fence.indent, &fence.closing);
    Ok(Placed { hunk: text_form(item, Some(whole), Some(after)), range: (fence.start, fence.end) })
}

/// The fence text of an opening line, source lines, and closing line. A source
/// line that is not empty takes the indent.
fn fence_text(opening: &str, source: &str, indent: &str, closing: &str) -> String {
    let mut parts = vec![opening.to_string()];
    if !source.is_empty() {
        parts.extend(source.split('\n').map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        }));
    }
    parts.push(closing.to_string());
    parts.join("\n")
}

/// The hunk as the record keeps it: fence text, and no context that the
/// placement has already used.
fn text_form(item: &ProposedHunk, before: Option<String>, after: Option<String>) -> ProposedHunk {
    ProposedHunk {
        kind: item.kind,
        before,
        after,
        after_text: None,
        lead: None,
        trail: None,
        note: item.note.clone(),
        diagram: true,
    }
}
