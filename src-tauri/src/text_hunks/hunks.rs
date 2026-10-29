//! Anchored hunks over plain text: how a change is named, placed, and applied.
//!
//! A hunk names **the text it changes** rather than a position in the file, so
//! an agent can propose a change to a document far larger than its own context
//! (`PDC-propose-draft-changes-tool.md` PDC-FR-WCXD,
//! `PPC-propose-prompt-changes-tool.md` PPC-FR-RJCA). This module holds the hunk
//! shapes, the planning that places them, and the walk that applies them.
//!
//! It is neutral by contract (`PCP-prompt-change-proposals.md` PCP-FR-29): it
//! holds no proposal state and reaches no store, so both proposal modules use it
//! and neither reaches the other through it. Everything here is pure over
//! `&str` and needs no runtime.

use serde::{Deserialize, Serialize};

use super::anchors::{self, HunkAnchor, HunkKind, Resolution};

/// One change, as the agent composed it or as the author has since edited it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalHunk {
    pub id: String,
    pub kind: HunkKind,
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The exact prompt text this hunk replaces or deletes. Absent for an `add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The exact new text. Absent for a `del`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default)]
    pub anchor: HunkAnchor,
}

impl ProposalHunk {
    pub fn before_text(&self) -> &str {
        self.before.as_deref().unwrap_or("")
    }

    pub fn after_text(&self) -> &str {
        self.after.as_deref().unwrap_or("")
    }
}

/// The `<proposal-id>.hunks` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkDocument {
    #[serde(default = "one")]
    pub version: u32,
    /// Bumped by an agent revision, never by an author edit (DCP-FR-XDRV).
    #[serde(default)]
    pub revision: u32,
    #[serde(rename = "hunk", default)]
    pub hunks: Vec<ProposalHunk>,
}

fn one() -> u32 {
    1
}

impl HunkDocument {
    pub fn new(hunks: Vec<ProposalHunk>) -> Self {
        Self { version: 1, revision: 0, hunks }
    }

    pub fn find(&self, hunk_id: &str) -> Option<&ProposalHunk> {
        self.hunks.iter().find(|h| h.id == hunk_id)
    }

    /// Replace one hunk in place, keeping its id and its position (DCP-FR-XDRV).
    pub fn replace_in_place(&mut self, hunk_id: &str, next: ProposalHunk) -> bool {
        match self.hunks.iter_mut().find(|h| h.id == hunk_id) {
            Some(slot) => {
                let revision = slot.revision + 1;
                *slot = ProposalHunk { id: slot.id.clone(), revision, ..next };
                self.revision += 1;
                true
            }
            None => false,
        }
    }

    /// The baseline an author's edit is checked against (DCP-FR-27).
    pub fn checksum(&self) -> String {
        crate::fs::sha256_bytes(self.to_toml().as_bytes())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Resolve every hunk against the prompt as it stands (DCP-FR-VZTK).
    pub fn resolve_all(&self, prompt: &str) -> Vec<Resolution> {
        let prompt = anchors::normalise_newlines(prompt);
        self.hunks
            .iter()
            .map(|h| {
                let before = anchors::normalise_newlines(h.before_text());
                anchors::resolve(&prompt, h.kind, &before, &h.anchor)
            })
            .collect()
    }
}

/// Apply one hunk to the prompt. The caller resolved it first, so this is the
/// whole of what an acceptance does to the prompt's bytes (DCP-FR-12).
pub fn apply_hunk(prompt: &str, hunk: &ProposalHunk, at: Resolution) -> Option<String> {
    let (start, end) = at.range()?;
    let prompt = anchors::normalise_newlines(prompt);
    if start > prompt.len() || end > prompt.len() || start > end {
        return None;
    }
    let replacement = anchors::normalise_newlines(hunk.after_text());
    Some(anchors::splice(&prompt, start, end, &replacement))
}

// ---------------------------------------------------------------------------
// Planning a proposal (DCP-FR-JGCD)
// ---------------------------------------------------------------------------

/// One change as an agent composed it, before this module has placed it.
///
/// It carries text and never an offset, because a model cannot count characters
/// — the geometry is derived here from the prompt the tool has just read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedHunk {
    pub kind: HunkKind,
    /// The exact existing text. Required for every kind but `add`.
    pub before: Option<String>,
    /// The exact new text. Required for every kind but `del`.
    pub after: Option<String>,
    /// For an `add`: the exact existing text the new text follows.
    pub after_text: Option<String>,
    /// The exact text immediately before `before` (for an `add`, before
    /// `after_text`). It names the place and is never changed (PDC-FR-WCXD).
    pub lead: Option<String>,
    /// The exact text immediately after `before` (for an `add`, after
    /// `after_text`). It names the place and is never changed (PDC-FR-WCXD).
    pub trail: Option<String>,
    pub note: Option<String>,
}

/// The most characters `lead` or `trail` may hold (PDC-FR-DMTH, PPC-FR-RJCA).
pub const LEAD_TRAIL_BOUND: usize = 200;

impl ProposedHunk {
    /// One change from the fields a model sent, its kind read from the fields
    /// it carries (PDC-FR-VKMR).
    ///
    /// `before` with `after` is a replacement, `before` alone a deletion, and
    /// `after` alone an insertion. A change carrying neither text is read as a
    /// replacement, so placing it refuses on the `before` it does not hold
    /// rather than recording a change with no text in it.
    pub fn from_fields(
        before: Option<String>,
        after: Option<String>,
        after_text: Option<String>,
        lead: Option<String>,
        trail: Option<String>,
        note: Option<String>,
    ) -> Self {
        let has_before = before.as_deref().is_some_and(|s| !s.is_empty());
        let has_after = after.as_deref().is_some_and(|s| !s.is_empty());
        let kind = match (has_before, has_after) {
            (true, true) => HunkKind::Replace,
            (true, false) => HunkKind::Del,
            (false, true) => HunkKind::Add,
            (false, false) => HunkKind::Replace,
        };
        ProposedHunk { kind, before, after, after_text, lead, trail, note }
    }
}

/// Why a proposal was refused before anything was written (DCP-FR-JGCD).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// No changes at all — nothing for the author to decide.
    NoHunks,
    /// A `before` the prompt does not hold. The model invented the text it
    /// claims to be changing. Carries the **one-based place** of the change in
    /// the list the caller passed: a proposal of eight changes refused for one
    /// of them cannot be corrected by a caller that is not told which.
    AnchorLost(usize),
    /// A `before` occurring more than once, so it names no single place, and
    /// the place of the change that named it.
    Ambiguous(usize),
    /// Two changes covering the same text, which would make them undecidable
    /// in any order, and the place of the later of the two.
    Overlap(usize),
    /// A `lead` or a `trail` longer than `LEAD_TRAIL_BOUND` characters, and the
    /// place of the change that carried it (PDC-FR-DMTH).
    ContextTooLong(usize),
}

/// Place every proposed change against the prompt, or refuse the whole
/// proposal (DCP-FR-JGCD).
///
/// This is the highest-value check in the module: it catches a model that
/// invented its `before` text while the model can still correct it, and it is
/// what makes the resulting hunks decidable in any order.
pub fn plan(
    prompt: &str,
    proposed: &[ProposedHunk],
    mut mint_id: impl FnMut() -> String,
) -> Result<HunkDocument, PlanError> {
    if proposed.is_empty() {
        return Err(PlanError::NoHunks);
    }
    let prompt = anchors::normalise_newlines(prompt);
    let mut placed: Vec<(usize, usize, ProposalHunk)> = Vec::new();

    for (index, item) in proposed.iter().enumerate() {
        // One-based, as the model counts the list it sent.
        let at = index + 1;
        let before = anchors::normalise_newlines(item.before.as_deref().unwrap_or(""));
        let lead = anchors::normalise_newlines(item.lead.as_deref().unwrap_or(""));
        let trail = anchors::normalise_newlines(item.trail.as_deref().unwrap_or(""));
        // PDC-FR-DMTH: the context names a place in a few words. A longer one
        // is refused before anything is searched.
        if lead.chars().count() > LEAD_TRAIL_BOUND || trail.chars().count() > LEAD_TRAIL_BOUND {
            return Err(PlanError::ContextTooLong(at));
        }
        let (start, end) = match item.kind {
            HunkKind::Add => {
                let follows = anchors::normalise_newlines(item.after_text.as_deref().unwrap_or(""));
                if follows.is_empty() && lead.is_empty() && trail.is_empty() {
                    // DCP-FR-JGCD: an insertion that names no text to follow
                    // says nothing about where it goes. An empty prompt has
                    // exactly one place it could go, so that is allowed; any
                    // other prompt refuses, rather than putting the new text at
                    // the head of a file the agent never meant it for. The
                    // refusal reaches the agent while it can still fix it.
                    if !prompt.is_empty() {
                        return Err(PlanError::AnchorLost(at));
                    }
                    (0, 0)
                } else {
                    // PDC-FR-WCXD: `lead` + `after_text` + `trail` names the
                    // place, and the new text goes in after `after_text`.
                    let found = find_run(&prompt, &lead, &follows, &trail, at)?;
                    let point = found + follows.len();
                    (point, point)
                }
            }
            HunkKind::Del | HunkKind::Replace => {
                if before.is_empty() {
                    return Err(PlanError::AnchorLost(at));
                }
                // PDC-FR-WCXD: `lead` + `before` + `trail` names the place, and
                // only `before` changes.
                let found = find_run(&prompt, &lead, &before, &trail, at)?;
                (found, found + before.len())
            }
        };

        // DCP-FR-JGCD: non-overlap at record time plus content anchors is what
        // makes every hunk decidable whatever order the author takes them in.
        if placed.iter().any(|(s, e, _)| anchors::ranges_overlap((*s, *e), (start, end))) {
            return Err(PlanError::Overlap(at));
        }

        placed.push((
            start,
            end,
            ProposalHunk {
                id: mint_id(),
                kind: item.kind,
                revision: 0,
                note: item.note.clone(),
                // DCP-FR-HRQN: the record's kind and its text agree. An
                // insertion keeps no `before` and a deletion no `after`, so
                // nothing is stored that acceptance cannot use — and the review
                // cannot draw a side the decision would not take.
                before: (item.kind != HunkKind::Add && !before.is_empty()).then(|| before.clone()),
                after: (item.kind != HunkKind::Del)
                    .then(|| {
                        item.after
                            .as_deref()
                            .map(anchors::normalise_newlines)
                            .filter(|s| !s.is_empty())
                    })
                    .flatten(),
                // The contexts are filled in below, once every change's place is
                // known: each must stop short of its neighbours.
                anchor: HunkAnchor {
                    lead: String::new(),
                    trail: String::new(),
                    hint_start: start,
                    hint_end: end,
                },
            },
        ));
    }

    // DCP-FR-VZTK: give each change a context that quotes only text **no other
    // change of this proposal touches**.
    //
    // This is what makes the changes decidable in any order, and it is not
    // optional. Left unbounded, a change with a neighbour on each side would
    // quote both of them: accept those two and this one's lead and trail have
    // each been rewritten, so it can no longer be placed and the author is left
    // able only to reject the very change they were keeping for last.
    //
    // The bounds are the neighbours' own ranges, which is why this waits until
    // every change has been placed.
    let mut order: Vec<usize> = (0..placed.len()).collect();
    order.sort_by_key(|&i| placed[i].0);
    for (position, &i) in order.iter().enumerate() {
        let floor = position.checked_sub(1).map(|p| placed[order[p]].1).unwrap_or(0);
        let ceiling = order.get(position + 1).map(|&n| placed[n].0).unwrap_or(usize::MAX);
        let (start, end, hunk) = &mut placed[i];
        hunk.anchor.lead = anchors::lead_context_from(&prompt, *start, floor);
        hunk.anchor.trail = anchors::trail_context_to(&prompt, *end, ceiling);
    }

    if let Some(at) = first_unplaceable(&prompt, &placed) {
        return Err(PlanError::Overlap(at));
    }
    Ok(HunkDocument::new(placed.into_iter().map(|(_, _, h)| h).collect()))
}

/// The place of the first change whose stored context no longer finds it.
///
/// A context stops short of its neighbours, so a change that only its
/// neighbour's text placed can lose its place: an insertion between a
/// neighbour and the end of the text, or a `lead` that was the neighbour's own
/// text. Each change must resolve to the range it was placed at, and the
/// changes applied in order must give, after each step, the text the placed
/// ranges give. A change that fails is refused as touching another change, so
/// nothing that cannot be decided is recorded.
fn first_unplaceable(prompt: &str, placed: &[(usize, usize, ProposalHunk)]) -> Option<usize> {
    for (i, (start, end, hunk)) in placed.iter().enumerate() {
        let before = anchors::normalise_newlines(hunk.before_text());
        let at = anchors::resolve(prompt, hunk.kind, &before, &hunk.anchor);
        if at != (Resolution::Resolved { start: *start, end: *end }) {
            return Some(i + 1);
        }
    }
    let mut text = prompt.to_string();
    for (i, (_, _, hunk)) in placed.iter().enumerate() {
        let before = anchors::normalise_newlines(hunk.before_text());
        let at = anchors::resolve(&text, hunk.kind, &before, &hunk.anchor);
        text = match apply_hunk(&text, hunk, at) {
            Some(next) => next,
            None => return Some(i + 1),
        };
        if Some(&text) != spliced(prompt, &placed[..=i]).as_ref() {
            return Some(i + 1);
        }
    }
    None
}

/// `prompt` with `placed` applied at the ranges they were placed at. The last
/// range is applied first, so no range moves; at one point a range applies
/// before an insertion, which puts the inserted text ahead of the new text.
fn spliced(prompt: &str, placed: &[(usize, usize, ProposalHunk)]) -> Option<String> {
    let mut order: Vec<&(usize, usize, ProposalHunk)> = placed.iter().collect();
    order.sort_by_key(|(start, end, _)| std::cmp::Reverse((*start, *end)));
    let mut text = prompt.to_string();
    for (start, end, hunk) in order {
        text = apply_hunk(&text, hunk, Resolution::Resolved { start: *start, end: *end })?;
    }
    Some(text)
}

/// Find the one occurrence of `lead` + `text` + `trail` in `prompt`, and give
/// the byte offset where `text` starts (PDC-FR-WCXD, PDC-FR-YSKV).
///
/// `at` is the change's one-based place, which every refusal carries.
fn find_run(
    prompt: &str,
    lead: &str,
    text: &str,
    trail: &str,
    at: usize,
) -> Result<usize, PlanError> {
    let run = format!("{lead}{text}{trail}");
    match anchors::find_once(prompt, &run) {
        anchors::Occurrence::None => Err(PlanError::AnchorLost(at)),
        anchors::Occurrence::Once(found) => Ok(found + lead.len()),
        anchors::Occurrence::Many => Err(PlanError::Ambiguous(at)),
    }
}

/// A whole-document change expressed as the one change it is: replace
/// everything the prompt holds with the text given.
///
/// This is what a legacy proposal always meant, and what a caller that still
/// thinks in whole documents composes.
pub fn whole_document(current: &str, next: &str) -> Vec<ProposedHunk> {
    // A prompt holding nothing has no text to name, so the change that fills it
    // is an insertion rather than a replacement.
    if current.is_empty() {
        return vec![ProposedHunk {
            kind: HunkKind::Add,
            before: None,
            after: Some(next.to_string()),
            after_text: None,
            lead: None,
            trail: None,
            note: None,
        }];
    }
    vec![ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(current.to_string()),
        after: Some(next.to_string()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    }]
}

/// Every hunk of the document applied in order, each re-resolved against the
/// text the one before it produced.
///
/// This is what an accept-all walks (DCR-FR-CXZG), and what reads a
/// whole-document proposal back as the text it always meant.
pub fn apply_all(prompt: &str, doc: &HunkDocument) -> Option<String> {
    let mut text = anchors::normalise_newlines(prompt);
    for hunk in &doc.hunks {
        let before = anchors::normalise_newlines(hunk.before_text());
        let at = anchors::resolve(&text, hunk.kind, &before, &hunk.anchor);
        text = apply_hunk(&text, hunk, at)?;
    }
    Some(text)
}
