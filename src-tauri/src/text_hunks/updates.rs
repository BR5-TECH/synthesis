//! Planning an update of an open proposal (`DCP-draft-change-proposals.md`
//! DCP-FR-XDRV, DCP-FR-TQBO, DCP-FR-IAFV, DCP-FR-BFDR, DCP-FR-FKMH).
//!
//! An update is an ordered list of entries: `change` replaces a hunk in place,
//! `add` appends a new hunk, and `withdraw` takes a hunk back. The plan is all
//! or nothing: one refused entry refuses the whole list, and each refusal names
//! the one-based place of the entry it is about.
//!
//! Neutral by the contract of PCP-FR-29: it holds no proposal state and reaches
//! no store. The caller says where each hunk stands, and writes the result.

use super::anchors::{self, HunkKind};
use super::hunks::{plan_among, HunkDocument, PlanError, ProposedHunk};

/// One entry of an update, as the tool read it from the model's arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEntry {
    /// DCP-FR-TQBO: replace the hunk `id` in place.
    Change { id: String, hunk: ProposedHunk },
    /// DCP-FR-IAFV: a new hunk after the existing hunks.
    Add(ProposedHunk),
    /// DCP-FR-IAFV: take the hunk `id` back.
    Withdraw { id: String },
}

impl UpdateEntry {
    /// The hunk id the entry names, if it names one.
    pub fn id(&self) -> Option<&str> {
        match self {
            UpdateEntry::Change { id, .. } | UpdateEntry::Withdraw { id } => Some(id),
            UpdateEntry::Add(_) => None,
        }
    }

    /// The hunk the entry places, if it places one.
    fn proposed(&self) -> Option<&ProposedHunk> {
        match self {
            UpdateEntry::Change { hunk, .. } | UpdateEntry::Add(hunk) => Some(hunk),
            UpdateEntry::Withdraw { .. } => None,
        }
    }
}

/// Where a hunk of the open proposal stands, as the caller's ledger says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// `pending`, `discussing`, or `rejected`: an update may change or withdraw
    /// it, and a new hunk must not overlap it.
    Live,
    /// `accepted` or `withdrawn`: no update reaches it (DCP-FR-BFDR).
    Settled,
}

/// Why an update was refused. Every variant about one entry carries the entry's
/// one-based place in the list the model sent (DCP-FR-FKMH, PDC-FR-SNUK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// The list is empty.
    NoUpdates,
    /// The id names no hunk of the open proposal.
    UnknownHunk(usize),
    /// The id names an accepted or a withdrawn hunk.
    NotUpdatable(usize),
    /// An earlier entry of the call already named this id.
    NamedTwice(usize),
    /// A `change` whose `after` equals its `before` (PDC-FR-VSMM).
    NoChange(usize),
    /// The entry could not be placed; the place in the error is the entry's.
    Plan(PlanError),
}

/// What an update produces: the new hunk document, and the ids each action
/// reached, in the order of the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    pub doc: HunkDocument,
    pub changed: Vec<String>,
    pub added: Vec<String>,
    pub withdrawn: Vec<String>,
}

/// Plan an update of `doc` against the prompt as it stands, or refuse it whole.
///
/// `standing` answers for each hunk id of the proposal, and `None` for an id
/// the proposal does not hold. `mint_id` gives the id of each added hunk.
pub fn plan_update(
    prompt: &str,
    doc: &HunkDocument,
    standing: impl Fn(&str) -> Option<Standing>,
    entries: &[UpdateEntry],
    mut mint_id: impl FnMut() -> String,
) -> Result<UpdatePlan, UpdateError> {
    if entries.is_empty() {
        return Err(UpdateError::NoUpdates);
    }

    // DCP-FR-BFDR: every named id first, in the order of the call.
    let mut named: Vec<&str> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let at = index + 1;
        let Some(id) = entry.id() else {
            continue;
        };
        match standing(id) {
            None => return Err(UpdateError::UnknownHunk(at)),
            Some(_) if doc.find(id).is_none() => return Err(UpdateError::UnknownHunk(at)),
            Some(Standing::Settled) => return Err(UpdateError::NotUpdatable(at)),
            Some(Standing::Live) => {}
        }
        if named.contains(&id) {
            return Err(UpdateError::NamedTwice(at));
        }
        named.push(id);
    }

    // PDC-FR-VSMM: a change that leaves its text as it is decides nothing.
    for (index, entry) in entries.iter().enumerate() {
        if let Some(hunk) = entry.proposed() {
            let before = anchors::normalise_newlines(hunk.before.as_deref().unwrap_or(""));
            let after = anchors::normalise_newlines(hunk.after.as_deref().unwrap_or(""));
            if hunk.kind == HunkKind::Replace && !before.is_empty() && before == after {
                return Err(UpdateError::NoChange(index + 1));
            }
        }
    }

    // DCP-FR-BFDR: a live hunk this call leaves alone keeps its place, and no
    // new or changed hunk may overlap it. A lost hunk has no place to keep.
    let resolutions = doc.resolve_all(prompt);
    let kept_hunks: Vec<(String, (usize, usize))> = doc
        .hunks
        .iter()
        .zip(resolutions)
        .filter(|(hunk, _)| {
            standing(&hunk.id) == Some(Standing::Live) && !named.contains(&hunk.id.as_str())
        })
        .filter_map(|(hunk, at)| at.range().map(|range| (hunk.id.clone(), range)))
        .collect();
    let kept: Vec<(usize, usize)> = kept_hunks.iter().map(|(_, range)| *range).collect();

    // The entries that place a hunk, each with its place in the call.
    let placing: Vec<(usize, &ProposedHunk)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.proposed().map(|hunk| (index + 1, hunk)))
        .collect();
    let mut planned = Vec::new();
    if !placing.is_empty() {
        let proposed: Vec<ProposedHunk> = placing.iter().map(|(_, h)| (*h).clone()).collect();
        let place = |at: usize| placing[at - 1].0;
        let placed = plan_among(prompt, &proposed, &kept, &mut mint_id).map_err(|error| {
            UpdateError::Plan(match error {
                PlanError::AnchorLost(at) => PlanError::AnchorLost(place(at)),
                PlanError::Ambiguous(at) => PlanError::Ambiguous(place(at)),
                PlanError::Overlap(at) => PlanError::Overlap(place(at)),
                PlanError::ContextTooLong(at, side) => PlanError::ContextTooLong(place(at), side),
                PlanError::NoHunks => PlanError::NoHunks,
            })
        })?;
        planned = placed.hunks;
    }

    // DCP-FR-TQBO / DCP-FR-IAFV: apply the plan in the order of the call.
    let mut next = doc.clone();
    let mut planned = planned.into_iter();
    let mut out = UpdatePlan {
        doc: HunkDocument::new(Vec::new()),
        changed: Vec::new(),
        added: Vec::new(),
        withdrawn: Vec::new(),
    };
    for entry in entries {
        match entry {
            UpdateEntry::Change { id, .. } => {
                if let Some(hunk) = planned.next() {
                    next.replace_in_place(id, hunk);
                }
                out.changed.push(id.clone());
            }
            UpdateEntry::Add(_) => {
                if let Some(hunk) = planned.next() {
                    out.added.push(hunk.id.clone());
                    next.hunks.push(hunk);
                }
            }
            UpdateEntry::Withdraw { id } => out.withdrawn.push(id.clone()),
        }
    }
    // DCP-FR-BFDR: a kept hunk's context stops short of every new or changed
    // range, so the kept hunk is still found after the author accepts them.
    let placed: Vec<(usize, usize)> = next
        .hunks
        .iter()
        .filter(|h| out.changed.contains(&h.id) || out.added.contains(&h.id))
        .map(|h| (h.anchor.hint_start, h.anchor.hint_end))
        .collect();
    let text = anchors::normalise_newlines(prompt);
    for (id, (start, end)) in &kept_hunks {
        let floor = placed.iter().filter(|(_, e)| e <= start).map(|(_, e)| *e).max();
        let ceiling = placed.iter().filter(|(s, _)| s >= end).map(|(s, _)| *s).min();
        let Some(hunk) = next.hunks.iter_mut().find(|h| &h.id == id) else {
            continue;
        };
        if let Some(floor) = floor.filter(|f| start - f < hunk.anchor.lead.len()) {
            hunk.anchor.lead = anchors::lead_context_from(&text, *start, floor);
        }
        if let Some(ceiling) = ceiling.filter(|c| c - end < hunk.anchor.trail.len()) {
            hunk.anchor.trail = anchors::trail_context_to(&text, *end, ceiling);
        }
    }
    out.doc = next;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_hunks::hunks::plan;

    const PROMPT: &str = "alpha beta gamma delta epsilon";

    fn replace(before: &str, after: &str) -> ProposedHunk {
        ProposedHunk::from_fields(Some(before.into()), Some(after.into()), None, None, None, None)
    }

    fn recorded() -> HunkDocument {
        let mut n = 0;
        plan(PROMPT, &[replace("beta", "BETA"), replace("delta", "DELTA")], || {
            n += 1;
            format!("h{n}")
        })
        .unwrap()
    }

    fn live(_: &str) -> Option<Standing> {
        Some(Standing::Live)
    }

    fn ids() -> impl FnMut() -> String {
        let mut n = 10;
        move || {
            n += 1;
            format!("h{n}")
        }
    }

    // DCP-FR-TQBO: a change keeps the hunk's id and its position and bumps its
    // revision. DCP-FR-IAFV: an add goes after the existing hunks.
    #[test]
    fn a_change_keeps_its_place_and_an_add_goes_last() {
        let doc = recorded();
        let plan = plan_update(
            PROMPT,
            &doc,
            live,
            &[
                UpdateEntry::Add(replace("epsilon", "EPSILON")),
                UpdateEntry::Change { id: "h1".into(), hunk: replace("beta", "Beta") },
                UpdateEntry::Withdraw { id: "h2".into() },
            ],
            ids(),
        )
        .unwrap();
        let order: Vec<&str> = plan.doc.hunks.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(order, ["h1", "h2", "h11"]);
        assert_eq!(plan.doc.hunks[0].after.as_deref(), Some("Beta"));
        assert_eq!(plan.doc.hunks[0].revision, 1);
        assert_eq!(plan.changed, ["h1"]);
        assert_eq!(plan.added, ["h11"]);
        assert_eq!(plan.withdrawn, ["h2"]);
    }

    // DCP-FR-BFDR / DCP-FR-FKMH: an unknown id, a settled hunk, and an id named
    // twice are refused with the place of the entry.
    #[test]
    fn named_ids_are_checked_with_their_place() {
        let doc = recorded();
        let settled = |id: &str| match id {
            "h1" => Some(Standing::Settled),
            "h2" => Some(Standing::Live),
            _ => None,
        };
        let refused = |entries: &[UpdateEntry]| {
            plan_update(PROMPT, &doc, settled, entries, ids()).unwrap_err()
        };
        let withdraw = |id: &str| UpdateEntry::Withdraw { id: id.into() };
        assert_eq!(refused(&[withdraw("h2"), withdraw("nope")]), UpdateError::UnknownHunk(2));
        assert_eq!(refused(&[withdraw("h1")]), UpdateError::NotUpdatable(1));
        assert_eq!(refused(&[withdraw("h2"), withdraw("h2")]), UpdateError::NamedTwice(2));
        assert_eq!(refused(&[]), UpdateError::NoUpdates);
    }

    // DCP-FR-BFDR: a new hunk must not overlap a live hunk the call keeps, but
    // it may cover the text of a hunk the call withdraws or changes.
    #[test]
    fn a_new_hunk_must_not_overlap_a_kept_live_hunk() {
        let doc = recorded();
        let add = UpdateEntry::Add(replace("beta gamma", "x"));
        assert_eq!(
            plan_update(PROMPT, &doc, live, std::slice::from_ref(&add), ids()).unwrap_err(),
            UpdateError::Plan(PlanError::Overlap(1))
        );
        let entries = [UpdateEntry::Withdraw { id: "h1".into() }, add];
        assert!(plan_update(PROMPT, &doc, live, &entries, ids()).is_ok());
        // An accepted hunk is in the text already, so nothing can overlap it.
        let accepted = |id: &str| Some(if id == "h1" { Standing::Settled } else { Standing::Live });
        let add = UpdateEntry::Add(replace("beta gamma", "x"));
        assert!(plan_update(PROMPT, &doc, accepted, &[add], ids()).is_ok());
    }

    // PDC-FR-SNUK: a placement refusal names the place of the entry, not the
    // place among the placing entries alone.
    #[test]
    fn a_placement_refusal_names_the_entry_place() {
        let doc = recorded();
        let entries = [
            UpdateEntry::Withdraw { id: "h2".into() },
            UpdateEntry::Add(replace("not there", "x")),
        ];
        assert_eq!(
            plan_update(PROMPT, &doc, live, &entries, ids()).unwrap_err(),
            UpdateError::Plan(PlanError::AnchorLost(2))
        );
    }

    // DCP-FR-BFDR / DCP-FR-JGCD: a kept hunk stays decidable after the author
    // accepts new hunks placed right beside it.
    #[test]
    fn a_kept_hunk_is_still_found_after_its_new_neighbours_land() {
        let doc = recorded();
        let entries =
            [UpdateEntry::Add(replace("alpha", "A")), UpdateEntry::Add(replace("gamma", "G"))];
        let plan = plan_update(PROMPT, &doc, live, &entries, ids()).unwrap();
        let mut text = PROMPT.to_string();
        for id in &plan.added {
            let hunk = plan.doc.find(id).unwrap();
            let before = anchors::normalise_newlines(hunk.before_text());
            let at = anchors::resolve(&text, hunk.kind, &before, &hunk.anchor);
            text = crate::text_hunks::hunks::apply_hunk(&text, hunk, at).unwrap();
        }
        assert_eq!(text, "A beta G delta epsilon");
        let kept = plan.doc.find("h1").unwrap();
        let at = anchors::resolve(&text, kept.kind, "beta", &kept.anchor);
        assert!(!at.is_lost(), "h1 is still placed: {:?}", kept.anchor);
    }

    // DCP-FR-BFDR: two entries of one call must not overlap each other, a
    // change must not overlap another kept hunk, and a rejected or held hunk
    // is live for overlap while an accepted or withdrawn one is not.
    #[test]
    fn the_overlap_rules_of_an_update() {
        let doc = recorded();
        // h2 is withdrawn, so the second add overlaps the first alone.
        let two_adds = [
            UpdateEntry::Withdraw { id: "h2".into() },
            UpdateEntry::Add(replace("gamma", "G")),
            UpdateEntry::Add(replace("gamma delta", "GD")),
        ];
        assert_eq!(
            plan_update(PROMPT, &doc, live, &two_adds, ids()).unwrap_err(),
            UpdateError::Plan(PlanError::Overlap(3))
        );
        let change_over_kept =
            [UpdateEntry::Change { id: "h1".into(), hunk: replace("beta gamma delta", "x") }];
        assert_eq!(
            plan_update(PROMPT, &doc, live, &change_over_kept, ids()).unwrap_err(),
            UpdateError::Plan(PlanError::Overlap(1))
        );
        let add = [UpdateEntry::Add(replace("delta epsilon", "x"))];
        // `Live` stands for a pending, a held, and a rejected hunk alike.
        assert_eq!(
            plan_update(PROMPT, &doc, live, &add, ids()).unwrap_err(),
            UpdateError::Plan(PlanError::Overlap(1))
        );
        let settled = |_: &str| Some(Standing::Settled);
        assert!(plan_update(PROMPT, &doc, settled, &add, ids()).is_ok());
    }

    // PDC-FR-VSMM: a change that leaves its text as it is is refused.
    #[test]
    fn a_change_to_the_same_text_is_refused() {
        let doc = recorded();
        let entries = [UpdateEntry::Change { id: "h1".into(), hunk: replace("beta", "beta") }];
        assert_eq!(
            plan_update(PROMPT, &doc, live, &entries, ids()).unwrap_err(),
            UpdateError::NoChange(1)
        );
    }
}
