//! How an `agent_surface` attachment reads to the model
//! (`AGC-agent-conversations.md` AGC-FR-JYBM, AGC-FR-TJDG).

use std::collections::HashMap;

use serde_json::Value;

use super::*;
use crate::agent_surfaces::{AgentSurface, SurfaceState};

/// The surfaces that a thread's attachments name, by id.
pub(in crate::agent_conversations) type SurfaceReads = HashMap<String, AgentSurface>;

/// The most characters of one literal text in the rendering.
const TEXT_MAX_CHARS: usize = 200;
/// The most characters of one JSON value in the rendering.
const JSON_MAX_CHARS: usize = 2_000;

/// AGC-FR-JYBM: one read of each surface that the thread's comments name.
pub(in crate::agent_conversations) fn surface_reads(
    roots: Roots<'_>,
    thread: &comments::Discussion,
) -> SurfaceReads {
    let mut reads = SurfaceReads::new();
    for attachment in thread.comments.iter().flat_map(|c| &c.attachments) {
        if let comments::Attachment::AgentSurface { surface_id } = attachment {
            if reads.contains_key(surface_id) {
                continue;
            }
            if let Ok(surface) = crate::agent_surfaces::store::get_impl(roots.store, surface_id) {
                reads.insert(surface_id.clone(), surface);
            }
        }
    }
    reads
}

/// AGC-FR-JYBM / AGC-FR-TJDG: the lines of one surface, or one bare line
/// when its record did not read.
pub(in crate::agent_conversations) fn surface_line(
    surface_id: &str,
    reads: &SurfaceReads,
) -> String {
    let Some(surface) = reads.get(surface_id) else {
        return format!("[agent surface {surface_id}]");
    };
    let state = match surface.state {
        SurfaceState::Open => "open",
        SurfaceState::Submitted => "submitted",
    };
    let mut out = format!("[agent surface {surface_id}: {state}]");
    let current = surface.current();
    for component in &current.components {
        out.push('\n');
        out.push_str(&component_line(component));
    }
    out.push_str(&format!("\ndata model: {}", compact(&current.data_model)));
    if let Some(submission) = &surface.submission {
        out.push_str(&format!(
            "\npressed: {} on {}\ncontext: {}\nsubmitted data model: {}",
            submission.name,
            submission.source_component_id,
            compact(&submission.context),
            compact(&submission.data_model),
        ));
    }
    out.push_str("\n[end of agent surface]");
    out
}

/// One component as its id, its type, and its literal texts.
fn component_line(component: &Value) -> String {
    let id = component.get("id").and_then(Value::as_str).unwrap_or("?");
    let kind = component.get("component").and_then(Value::as_str).unwrap_or("?");
    let mut texts = Vec::new();
    collect_texts(component, &mut texts);
    // The literal cells of a table are what the agent showed, so they read too.
    if let Some(rows) = component.get("rows").filter(|rows| rows.is_array()) {
        texts.push(format!("rows {}", compact(rows)));
    }
    if texts.is_empty() {
        format!("- {id} {kind}")
    } else {
        format!("- {id} {kind}: {}", texts.join(", "))
    }
}

/// The literal texts of a component: its text, labels, titles, and captions.
fn collect_texts(value: &Value, texts: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        if let Some(items) = value.as_array() {
            items.iter().for_each(|item| collect_texts(item, texts));
        }
        return;
    };
    for (key, item) in object {
        match (key.as_str(), item) {
            (
                "text" | "label" | "placeholder" | "caption" | "title" | "question" | "body"
                | "description",
                Value::String(text),
            ) => {
                texts.push(format!("\"{}\"", bounded(text, TEXT_MAX_CHARS)));
            }
            // The options, tabs, columns, items, and buttons of a component and
            // of a card.
            ("options" | "tabs" | "columns" | "items" | "criteria", Value::Array(_))
            | ("confirm" | "decline" | "pick", Value::Object(_)) => collect_texts(item, texts),
            _ => {}
        }
    }
}

fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

fn compact(value: &Value) -> String {
    bounded(&value.to_string(), JSON_MAX_CHARS)
}
