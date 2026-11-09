//! AGC-FR-KQNF: a Flow discussion's material carries, after the plan's own
//! document, the plan's delivery state (`FDL-flow-delivery.md`) and its
//! overlaps (`FOA-flow-overlap-analysis.md`), so the agent sees the picture the
//! architect sees on the canvas.
//!
//! The builders stay pure over their roots. The delivery state needs the run
//! queue, which only the running application holds, so the dispatch path adds
//! this text to the `artifact` section after the input is built.

use super::*;
use crate::flow_delivery::{FlowDeliveryState, ItemState};
use crate::flow_overlap::Overlap;
use crate::flow_validation::model::{parse_valid, FlowDocument};

fn state_word(state: ItemState) -> &'static str {
    match state {
        ItemState::NotStarted => "not started",
        ItemState::Drafting => "drafting",
        ItemState::InProgress => "in progress",
        ItemState::Done => "done",
        ItemState::Dropped => "dropped",
    }
}

/// AGC-FR-KQNF: the text that follows the plan's document. Each item is named
/// by its id and its name.
pub(super) fn flow_context_text(
    doc: &FlowDocument,
    state: &FlowDeliveryState,
    overlaps: &[Overlap],
) -> String {
    let name_of =
        |id: &str| doc.items.iter().find(|i| i.id == id).map(|i| i.name.as_str()).unwrap_or("");
    let mut out = String::from("\n\nDelivery state of this plan:\n");
    for s in &state.items {
        let kind =
            doc.items.iter().find(|i| i.id == s.item_id).map(|i| i.kind.as_str()).unwrap_or("");
        out.push_str(&format!(
            "- {} \"{}\" ({kind}): {}",
            s.item_id,
            name_of(&s.item_id),
            state_word(s.state)
        ));
        if let Some(d) = &s.draft {
            out.push_str(&format!("; draft \"{}\"", d.name));
        }
        if s.draft_missing {
            out.push_str("; its draft is missing");
        }
        if !s.blocked_by.is_empty() {
            let by: Vec<String> =
                s.blocked_by.iter().map(|id| format!("{id} \"{}\"", name_of(id))).collect();
            out.push_str(&format!("; blocked by {}", by.join(", ")));
        }
        out.push('\n');
    }
    if overlaps.is_empty() {
        out.push_str("\nOverlaps: none.\n");
    } else {
        out.push_str(
            "\nOverlaps (tasks that no blocks edge orders and that share footprint paths):\n",
        );
        for o in overlaps {
            out.push_str(&format!(
                "- {} \"{}\" and {} \"{}\" share {}\n",
                o.items[0],
                name_of(&o.items[0]),
                o.items[1],
                name_of(&o.items[1]),
                o.paths.join(", ")
            ));
        }
    }
    out
}

/// AGC-FR-KQNF, AGC-FR-XHHT: append `extra` to a whole `artifact` section when
/// the section stays within `bound` characters. Returns whether it was added.
pub(super) fn append_within(section: &mut InputSection, extra: &str, bound: usize) -> bool {
    if section.truncated || section.body.chars().count() + extra.chars().count() > bound {
        return false;
    }
    section.body.push_str(extra);
    section.parts.push(InputPart::Text(extra.to_string()));
    true
}

/// The `artifact` section of a valid Flow, with its parsed plan.
fn flow_section(sections: &mut [InputSection]) -> Option<(&mut InputSection, FlowDocument)> {
    let section = sections.iter_mut().find(|s| {
        s.tag == TAG_ARTIFACT && s.attributes.iter().any(|(k, v)| k == "type" && v == "flow")
    })?;
    if section.truncated {
        return None;
    }
    let doc = parse_valid(&section.body).ok()?;
    Some((section, doc))
}

/// AGC-FR-KQNF: add the delivery state and the overlaps to a Flow's material.
/// A Flow that fails validation adds neither.
pub(super) fn extend_flow_material<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    budget: &DocumentBudget,
    sections: &mut [InputSection],
) {
    let Some((section, doc)) = flow_section(sections) else { return };
    let mut drafts = crate::drafts::list_drafts_impl(roots.worktree).drafts;
    crate::drafts::attach_graduation(app, &mut drafts);
    let state = crate::flow_delivery::delivery_state(&doc, &drafts);
    let overlaps = crate::flow_overlap::overlaps(&doc);
    let extra = flow_context_text(&doc, &state, &overlaps);
    let added = append_within(section, &extra, budget.bound_chars());
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Ai],
        "flow delivery context for a turn",
        log_fields! { "items" => state.items.len(), "overlaps" => overlaps.len(), "added" => added },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_overlap::overlaps;

    const PLAN: &str = r#"{"version":2,"name":"P","items":[
      {"id":"d1","kind":"deliverable","name":"Pay","position":{"x":0,"y":0},"size":{"width":1,"height":1}},
      {"id":"m1","kind":"milestone","name":"Beta","parentId":"d1","position":{"x":0,"y":0},"size":{"width":1,"height":1}},
      {"id":"f1","kind":"feature","name":"Capture","parentId":"m1","position":{"x":0,"y":0},"size":{"width":1,"height":1}},
      {"id":"t1","kind":"task","name":"Tokenize","parentId":"f1","footprint":["src/pay/"],"position":{"x":0,"y":0}},
      {"id":"t2","kind":"task","name":"Form","parentId":"f1","footprint":["src/pay/form.rs"],"position":{"x":0,"y":0}},
      {"id":"t3","kind":"task","name":"Docs","parentId":"f1","draftId":"gone","position":{"x":0,"y":0}}
    ],"edges":[{"id":"e1","from":"t3","to":"t2","type":"blocks"}]}"#;

    fn section(body: &str, ty: &str) -> InputSection {
        InputSection {
            tag: TAG_ARTIFACT.to_string(),
            attributes: vec![("path".into(), "p.flow".into()), ("type".into(), ty.into())],
            body: body.to_string(),
            truncated: false,
            parts: vec![InputPart::Text(body.to_string())],
        }
    }

    /// AGC-FR-KQNF: the text names every item by id and name, its state, its
    /// blockers, and the overlaps with their shared paths.
    #[test]
    fn the_context_names_items_states_blockers_and_overlaps() {
        let doc = parse_valid(PLAN).unwrap();
        let state = crate::flow_delivery::delivery_state(&doc, &[]);
        let text = flow_context_text(&doc, &state, &overlaps(&doc));
        assert!(text.contains("- t1 \"Tokenize\" (task): not started"), "{text}");
        assert!(
            text.contains("- t2 \"Form\" (task): not started; blocked by t3 \"Docs\""),
            "{text}"
        );
        assert!(text.contains("- t3 \"Docs\" (task): not started; its draft is missing"), "{text}");
        assert!(text.contains("- t1 \"Tokenize\" and t2 \"Form\" share src/pay/form.rs"), "{text}");

        let quiet = parse_valid(r#"{"version":2}"#).unwrap();
        let state = crate::flow_delivery::delivery_state(&quiet, &[]);
        assert!(flow_context_text(&quiet, &state, &[]).contains("Overlaps: none."));
    }

    /// AGC-FR-KQNF, AGC-FR-XHHT: the text counts against the section's bound,
    /// and a cut section gets none.
    #[test]
    fn the_context_counts_against_the_bound() {
        let mut s = section("abc", "flow");
        assert!(!append_within(&mut s, "0123456789", 12));
        assert_eq!(s.body, "abc");
        assert!(append_within(&mut s, "0123456789", 13));
        assert_eq!(s.body, "abc0123456789");
        assert_eq!(s.parts.len(), 2);

        let mut cut = section("abc", "flow");
        cut.truncated = true;
        assert!(!append_within(&mut cut, "x", 100));
    }

    /// AGC-FR-KQNF: only the artifact section of a valid Flow qualifies.
    #[test]
    fn only_a_valid_flow_section_qualifies() {
        let mut sections = vec![section(PLAN, "flow")];
        assert!(flow_section(&mut sections).is_some());
        let mut invalid = vec![section(r#"{"version":1}"#, "flow")];
        assert!(flow_section(&mut invalid).is_none());
        let mut prompt = vec![section(PLAN, "prompt")];
        assert!(flow_section(&mut prompt).is_none());
    }
}
