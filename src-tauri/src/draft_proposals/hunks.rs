//! The hunk document and the ledger derived from it
//! (`DCP-draft-change-proposals.md`).
//!
//! A proposal is two files (DCP-FR-01): `<id>.toml` holds the record **and the
//! per-hunk decision ledger**, and `<id>.hunks` holds the ordered hunk bodies
//! and their anchors. The bodies stay out of the record because listing reads
//! records and never a document (DCP-FR-09); the states stay **in** it because a
//! decision must be one atomic write, and a ledger row is tens of bytes.
//!
//! Everything here is pure over `&str` and needs no runtime.

use serde::{Deserialize, Serialize};

use super::anchors::{self, HunkAnchor, HunkKind, Resolution};

/// What a proposal's own state is derived from (DCP-FR-PWSF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkState {
    Pending,
    Accepted,
    Rejected,
    /// Held for discussion. Undecided, so it still holds the draft's one slot.
    Discussing,
}

impl HunkState {
    /// A hunk the author has not settled either way.
    pub fn is_undecided(self) -> bool {
        matches!(self, HunkState::Pending | HunkState::Discussing)
    }
}

/// One row of the decision ledger the record carries (DCP-FR-01).
///
/// It holds the hunk's identity and what the author has done with it, and none
/// of its text — the text is a document and lives in the `.hunks` file, so a
/// list never reads one (DCP-FR-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkLedgerRow {
    pub id: String,
    pub kind: HunkKind,
    #[serde(default = "pending")]
    pub state: HunkState,
    /// True while this hunk's text differs from the agent's own (DCP-FR-25).
    #[serde(default)]
    pub edited: bool,
    /// Bumped by an agent revision, never by an author edit (DCP-FR-XDRV).
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

fn pending() -> HunkState {
    HunkState::Pending
}

impl HunkLedgerRow {
    pub fn new(hunk: &ProposalHunk) -> Self {
        Self {
            id: hunk.id.clone(),
            kind: hunk.kind,
            state: HunkState::Pending,
            edited: false,
            revision: hunk.revision,
            decided_at: None,
        }
    }
}

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

/// A proposal held as a whole document, read as one change covering the whole
/// prompt (DCP-FR-QLMH).
///
/// `proposals/` travels through Git, so such a proposal can arrive on a pull long
/// after this module stopped writing them. Read-compatibility is permanent and
/// nothing is ever rewritten on disk.
pub fn legacy_document(prompt: &str, candidate: &str, hunk_id: &str) -> HunkDocument {
    let prompt = anchors::normalise_newlines(prompt);
    HunkDocument::new(vec![ProposalHunk {
        id: hunk_id.to_string(),
        kind: HunkKind::Replace,
        revision: 0,
        note: None,
        before: Some(prompt.clone()),
        after: Some(anchors::normalise_newlines(candidate)),
        anchor: HunkAnchor {
            lead: String::new(),
            trail: String::new(),
            hint_start: 0,
            hint_end: prompt.len(),
        },
    }])
}

/// How many hunks stand in each state (DCP-FR-PWSF).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkCounts {
    #[serde(default)]
    pub pending: u32,
    #[serde(default)]
    pub accepted: u32,
    #[serde(default)]
    pub rejected: u32,
    #[serde(default)]
    pub discussing: u32,
}

impl HunkCounts {
    pub fn of(states: impl IntoIterator<Item = HunkState>) -> Self {
        let mut c = Self::default();
        for s in states {
            match s {
                HunkState::Pending => c.pending += 1,
                HunkState::Accepted => c.accepted += 1,
                HunkState::Rejected => c.rejected += 1,
                HunkState::Discussing => c.discussing += 1,
            }
        }
        c
    }

    pub fn undecided(&self) -> u32 {
        self.pending + self.discussing
    }
}

/// The three spellings every surface keys off. A proposal is `pending` while any
/// hunk is undecided, `accepted` once all are decided and at least one was
/// accepted, and `rejected` otherwise (DCP-FR-PWSF).
pub fn derive_state(counts: &HunkCounts) -> &'static str {
    if counts.undecided() > 0 {
        "pending"
    } else if counts.accepted > 0 {
        "accepted"
    } else {
        "rejected"
    }
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
    pub note: Option<String>,
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
        let (start, end) = match item.kind {
            HunkKind::Add => {
                let follows = anchors::normalise_newlines(item.after_text.as_deref().unwrap_or(""));
                if follows.is_empty() {
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
                    match anchors::occurrences(&prompt, &follows) {
                        0 => return Err(PlanError::AnchorLost(at)),
                        1 => {
                            let found = prompt.find(&follows).unwrap() + follows.len();
                            (found, found)
                        }
                        _ => return Err(PlanError::Ambiguous(at)),
                    }
                }
            }
            HunkKind::Del | HunkKind::Replace => {
                if before.is_empty() {
                    return Err(PlanError::AnchorLost(at));
                }
                match anchors::occurrences(&prompt, &before) {
                    0 => return Err(PlanError::AnchorLost(at)),
                    1 => {
                        let found = prompt.find(&before).unwrap();
                        (found, found + before.len())
                    }
                    _ => return Err(PlanError::Ambiguous(at)),
                }
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
                before: (item.kind != HunkKind::Add && !before.is_empty())
                    .then(|| before.clone()),
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

    Ok(HunkDocument::new(placed.into_iter().map(|(_, _, h)| h).collect()))
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
            note: None,
        }];
    }
    vec![ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(current.to_string()),
        after: Some(next.to_string()),
        after_text: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(id: &str, kind: HunkKind, before: &str, after: &str, lead: &str, trail: &str, hs: usize, he: usize) -> ProposalHunk {
        ProposalHunk {
            id: id.to_string(),
            kind,
            revision: 0,
            note: None,
            before: (!before.is_empty()).then(|| before.to_string()),
            after: (!after.is_empty()).then(|| after.to_string()),
            anchor: HunkAnchor {
                lead: lead.to_string(),
                trail: trail.to_string(),
                hint_start: hs,
                hint_end: he,
            },
        }
    }

    // DCP-FR-01: the hunk document round-trips through TOML as `[[hunk]]` entries.
    #[test]
    fn the_hunk_document_round_trips_through_toml() {
        let doc = HunkDocument::new(vec![
            hunk("h1", HunkKind::Replace, "BETA", "DELTA", "alpha ", " gamma", 6, 10),
            hunk("h2", HunkKind::Add, "", "new\n", "one\n", "two\n", 4, 4),
        ]);
        let text = doc.to_toml();
        assert!(text.contains("[[hunk]]"));
        assert_eq!(HunkDocument::from_toml(&text).unwrap(), doc);
    }

    // DCP-FR-12: applying one hunk changes that range and no other byte.
    // DCP-FR-HRQN: a recorded change's kind and its text agree. An insertion
    // keeps no `before` and a deletion no `after`, so the review cannot draw a
    // side the decision would not take.
    #[test]
    fn a_recorded_change_holds_only_the_text_its_kind_uses() {
        let prompt = "one\ntwo\nthree\n";
        let doc = plan(
            prompt,
            &[
                ProposedHunk {
                    kind: HunkKind::Add,
                    // A model that sent `before` beside an insertion: the text
                    // is not what an insertion applies, so it is not kept.
                    before: Some("two".into()),
                    after: Some("inserted\n".into()),
                    after_text: Some("one\n".into()),
                    note: None,
                },
                ProposedHunk {
                    kind: HunkKind::Del,
                    before: Some("three".into()),
                    // The same the other way round.
                    after: Some("ignored".into()),
                    after_text: None,
                    note: None,
                },
                // A replacement carries both, and keeps both.
                ProposedHunk {
                    kind: HunkKind::Replace,
                    before: Some("two".into()),
                    after: Some("TWO".into()),
                    after_text: None,
                    note: None,
                },
            ],
            || "id".to_string(),
        )
        .unwrap();
        assert_eq!(doc.hunks[0].kind, HunkKind::Add);
        assert_eq!(doc.hunks[0].before, None);
        assert_eq!(doc.hunks[0].after.as_deref(), Some("inserted\n"));
        assert_eq!(doc.hunks[1].kind, HunkKind::Del);
        assert_eq!(doc.hunks[1].before.as_deref(), Some("three"));
        assert_eq!(doc.hunks[1].after, None);
        assert_eq!(doc.hunks[2].kind, HunkKind::Replace);
        assert_eq!(doc.hunks[2].before.as_deref(), Some("two"));
        assert_eq!(doc.hunks[2].after.as_deref(), Some("TWO"));
    }

    #[test]
    fn applying_one_hunk_leaves_every_other_byte_alone() {
        let prompt = "alpha BETA gamma";
        let doc = HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "BETA", "DELTA", "alpha ", " gamma", 6, 10)]);
        let at = doc.resolve_all(prompt)[0];
        assert_eq!(apply_hunk(prompt, &doc.hunks[0], at).unwrap(), "alpha DELTA gamma");
    }

    // DCP-FR-VZTK: accepting one hunk moves the others, and their anchors still find them.
    #[test]
    fn hunks_stay_resolvable_after_an_earlier_hunk_is_accepted() {
        let prompt = "one ALPHA two\nthree BETA four\n";
        let a = hunk("h1", HunkKind::Replace, "ALPHA", "ALPHA-LONGER-NOW", "one ", " two", 4, 9);
        let b = hunk("h2", HunkKind::Replace, "BETA", "GAMMA", "three ", " four", 20, 24);
        let doc = HunkDocument::new(vec![a, b]);

        let after_first = apply_hunk(prompt, &doc.hunks[0], doc.resolve_all(prompt)[0]).unwrap();
        let second = doc.resolve_all(&after_first)[1];
        let (s, e) = second.range().expect("the second hunk is still placeable");
        assert_eq!(&after_first[s..e], "BETA");
        assert_eq!(
            apply_hunk(&after_first, &doc.hunks[1], second).unwrap(),
            "one ALPHA-LONGER-NOW two\nthree GAMMA four\n"
        );
    }

    // DCP-FR-VZTK: the order the author decides in does not change what lands.
    #[test]
    fn deciding_in_either_order_reaches_the_same_prompt() {
        let prompt = "one ALPHA two\nthree BETA four\n";
        let a = hunk("h1", HunkKind::Replace, "ALPHA", "AAA", "one ", " two", 4, 9);
        let b = hunk("h2", HunkKind::Replace, "BETA", "BBB", "three ", " four", 20, 24);
        let doc = HunkDocument::new(vec![a, b]);

        let forward = {
            let t = apply_hunk(prompt, &doc.hunks[0], doc.resolve_all(prompt)[0]).unwrap();
            apply_hunk(&t, &doc.hunks[1], doc.resolve_all(&t)[1]).unwrap()
        };
        let backward = {
            let t = apply_hunk(prompt, &doc.hunks[1], doc.resolve_all(prompt)[1]).unwrap();
            apply_hunk(&t, &doc.hunks[0], doc.resolve_all(&t)[0]).unwrap()
        };
        assert_eq!(forward, backward);
    }

    // DCP-FR-XDRV: a revision keeps the hunk's id and its place, and bumps the revision.
    #[test]
    fn a_revision_keeps_the_hunk_id_and_its_position() {
        let mut doc = HunkDocument::new(vec![
            hunk("h1", HunkKind::Replace, "A", "B", "", "", 0, 1),
            hunk("h2", HunkKind::Replace, "C", "D", "", "", 2, 3),
        ]);
        let next = hunk("ignored", HunkKind::Replace, "C", "REVISED", "", "", 2, 3);
        assert!(doc.replace_in_place("h2", next));
        assert_eq!(doc.hunks[1].id, "h2", "the id a reply names is kept");
        assert_eq!(doc.hunks[1].after_text(), "REVISED");
        assert_eq!(doc.hunks[1].revision, 1);
        assert_eq!(doc.hunks.len(), 2);
        assert!(!doc.replace_in_place("nope", doc.hunks[0].clone()));
    }

    // DCP-FR-QLMH: a whole-document proposal reads as one change over the whole prompt.
    #[test]
    fn a_legacy_proposal_reads_as_one_change_covering_the_prompt() {
        let prompt = "the prompt as it stands";
        let doc = legacy_document(prompt, "the candidate text", "legacy");
        assert_eq!(doc.hunks.len(), 1);
        let at = doc.resolve_all(prompt)[0];
        assert_eq!(at, Resolution::Resolved { start: 0, end: prompt.len() });
        assert_eq!(apply_hunk(prompt, &doc.hunks[0], at).unwrap(), "the candidate text");
    }

    // DCP-FR-PWSF: the proposal's state is derived from the ledger and nothing else.
    #[test]
    fn the_proposal_state_is_derived_from_its_ledger() {
        use HunkState::*;
        assert_eq!(derive_state(&HunkCounts::of([Pending, Accepted])), "pending");
        assert_eq!(derive_state(&HunkCounts::of([Discussing, Accepted])), "pending");
        assert_eq!(derive_state(&HunkCounts::of([Accepted, Rejected])), "accepted");
        assert_eq!(derive_state(&HunkCounts::of([Rejected, Rejected])), "rejected");
        assert_eq!(HunkCounts::of([Pending, Discussing, Accepted]).undecided(), 2);
    }

    // DCP-FR-PWSF: a hunk held for discussion is undecided, so it holds the draft's slot.
    #[test]
    fn a_hunk_held_for_discussion_is_undecided() {
        assert!(HunkState::Discussing.is_undecided());
        assert!(HunkState::Pending.is_undecided());
        assert!(!HunkState::Accepted.is_undecided());
        assert!(!HunkState::Rejected.is_undecided());
    }

    // DCP-FR-BMLX: a lost hunk cannot be applied.
    #[test]
    fn a_lost_hunk_applies_to_nothing() {
        let doc = HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "GONE", "X", "no ", " ctx", 0, 4)]);
        let at = doc.resolve_all("nothing like it here")[0];
        assert!(at.is_lost());
        assert!(apply_hunk("nothing like it here", &doc.hunks[0], at).is_none());
    }

    // DCP-FR-27: the checksum is the baseline an author's edit is checked against.
    #[test]
    fn the_checksum_changes_only_when_the_document_does() {
        let doc = HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "B", "", "", 0, 1)]);
        let same = HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "B", "", "", 0, 1)]);
        let other = HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "C", "", "", 0, 1)]);
        assert_eq!(doc.checksum(), same.checksum());
        assert_ne!(doc.checksum(), other.checksum());
    }
}
