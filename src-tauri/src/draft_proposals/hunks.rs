//! The hunk document and the ledger derived from it
//! (`DCP-draft-change-proposals.md`).
//!
//! A proposal is two files (DCP-FR-01): `<id>.toml` holds the record **and the
//! per-hunk decision ledger**, and `<id>.hunks` holds the ordered hunk bodies
//! and their anchors. The bodies stay out of the record because listing reads
//! records and never a document (DCP-FR-09); the states stay **in** it because a
//! decision must be one atomic write, and a ledger row is tens of bytes.
//!
//! The hunk shapes, their planning, and their application are pure text and
//! live in `crate::text_hunks` (PCP-FR-29); they are re-exported here so the
//! rest of this module names them as its own. Everything here is pure over
//! `&str` and needs no runtime.

use serde::{Deserialize, Serialize};

use super::anchors::{self, HunkAnchor, HunkKind};

pub use crate::text_hunks::hunks::{
    apply_all, apply_hunk, plan, whole_document, HunkDocument, PlanError, ProposalHunk,
    ProposedHunk,
};

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

    /// DCP-FR-XDRV: a hunk its agent may still replace — an undecided one, or
    /// one the author rejected while the proposal stands.
    pub fn is_revisable(self) -> bool {
        self.is_undecided() || self == HunkState::Rejected
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_hunks::anchors::Resolution;

    fn anchor(lead: &str, trail: &str, hs: usize, he: usize) -> HunkAnchor {
        HunkAnchor {
            lead: lead.to_string(),
            trail: trail.to_string(),
            hint_start: hs,
            hint_end: he,
        }
    }

    fn hunk(
        id: &str,
        kind: HunkKind,
        before: &str,
        after: &str,
        anchor: HunkAnchor,
    ) -> ProposalHunk {
        ProposalHunk {
            id: id.to_string(),
            kind,
            revision: 0,
            note: None,
            before: (!before.is_empty()).then(|| before.to_string()),
            after: (!after.is_empty()).then(|| after.to_string()),
            anchor,
        }
    }

    // DCP-FR-01: the hunk document round-trips through TOML as `[[hunk]]` entries.
    #[test]
    fn the_hunk_document_round_trips_through_toml() {
        let doc = HunkDocument::new(vec![
            hunk("h1", HunkKind::Replace, "BETA", "DELTA", anchor("alpha ", " gamma", 6, 10)),
            hunk("h2", HunkKind::Add, "", "new\n", anchor("one\n", "two\n", 4, 4)),
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
                    lead: None,
                    trail: None,
                    note: None,
                },
                ProposedHunk {
                    kind: HunkKind::Del,
                    before: Some("three".into()),
                    // The same the other way round.
                    after: Some("ignored".into()),
                    after_text: None,
                    lead: None,
                    trail: None,
                    note: None,
                },
                // A replacement carries both, and keeps both.
                ProposedHunk {
                    kind: HunkKind::Replace,
                    before: Some("two".into()),
                    after: Some("TWO".into()),
                    after_text: None,
                    lead: None,
                    trail: None,
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
        let doc = HunkDocument::new(vec![hunk(
            "h1",
            HunkKind::Replace,
            "BETA",
            "DELTA",
            anchor("alpha ", " gamma", 6, 10),
        )]);
        let at = doc.resolve_all(prompt)[0];
        assert_eq!(apply_hunk(prompt, &doc.hunks[0], at).unwrap(), "alpha DELTA gamma");
    }

    // DCP-FR-VZTK: accepting one hunk moves the others, and their anchors still find them.
    #[test]
    fn hunks_stay_resolvable_after_an_earlier_hunk_is_accepted() {
        let prompt = "one ALPHA two\nthree BETA four\n";
        let a = hunk(
            "h1",
            HunkKind::Replace,
            "ALPHA",
            "ALPHA-LONGER-NOW",
            anchor("one ", " two", 4, 9),
        );
        let b = hunk("h2", HunkKind::Replace, "BETA", "GAMMA", anchor("three ", " four", 20, 24));
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
        let a = hunk("h1", HunkKind::Replace, "ALPHA", "AAA", anchor("one ", " two", 4, 9));
        let b = hunk("h2", HunkKind::Replace, "BETA", "BBB", anchor("three ", " four", 20, 24));
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
            hunk("h1", HunkKind::Replace, "A", "B", anchor("", "", 0, 1)),
            hunk("h2", HunkKind::Replace, "C", "D", anchor("", "", 2, 3)),
        ]);
        let next = hunk("ignored", HunkKind::Replace, "C", "REVISED", anchor("", "", 2, 3));
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
        let doc = HunkDocument::new(vec![hunk(
            "h1",
            HunkKind::Replace,
            "GONE",
            "X",
            anchor("no ", " ctx", 0, 4),
        )]);
        let at = doc.resolve_all("nothing like it here")[0];
        assert!(at.is_lost());
        assert!(apply_hunk("nothing like it here", &doc.hunks[0], at).is_none());
    }

    // DCP-FR-27: the checksum is the baseline an author's edit is checked against.
    #[test]
    fn the_checksum_changes_only_when_the_document_does() {
        let doc =
            HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "B", anchor("", "", 0, 1))]);
        let same =
            HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "B", anchor("", "", 0, 1))]);
        let other =
            HunkDocument::new(vec![hunk("h1", HunkKind::Replace, "A", "C", anchor("", "", 0, 1))]);
        assert_eq!(doc.checksum(), same.checksum());
        assert_ne!(doc.checksum(), other.checksum());
    }
}
