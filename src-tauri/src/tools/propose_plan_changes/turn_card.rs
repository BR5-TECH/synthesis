//! The plan card of one turn (`PPT-propose-plan-changes-tool.md` PPT-FR-ZMBA,
//! PPT-FR-IRFE; `CVL-conversation-loop.md` CVL-FR-KYBE).
//!
//! The tool and the loop of one turn share it. The first successful call of
//! the turn sets the card. A later call adds its counts. The loop reads it when
//! the turn ends, to know whether the turn posts a closing comment.

use std::sync::Mutex;

use crate::comments::ProposalUpdateCounts;

/// What a turn knows about its plan card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardState {
    /// The proposal of the card, once the first call of the turn succeeded.
    pub proposal_id: Option<String>,
    /// The rows that later calls of the turn changed, added, and withdrew.
    pub changed: usize,
    pub added: usize,
    pub withdrawn: usize,
}

impl CardState {
    /// PPT-FR-IRFE: whether a later call changed the card after its comment.
    pub fn changed_after_comment(&self) -> bool {
        self.changed + self.added + self.withdrawn > 0
    }

    /// CMS-FR-BSQB: the counts of the turn, as a comment attachment carries
    /// them.
    pub fn counts(&self) -> ProposalUpdateCounts {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        ProposalUpdateCounts {
            changed: count(self.changed),
            added: count(self.added),
            withdrawn: count(self.withdrawn),
        }
    }
}

/// PPT-FR-ZMBA: the plan card of one turn. A new turn gets a new one.
#[derive(Debug, Default)]
pub struct PlanTurnCard {
    state: Mutex<CardState>,
}

impl PlanTurnCard {
    fn lock(&self) -> std::sync::MutexGuard<'_, CardState> {
        // A panic in another holder leaves plain data that is still correct.
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The state as it stands now.
    pub fn snapshot(&self) -> CardState {
        self.lock().clone()
    }

    /// Whether the turn has a card.
    pub fn has_card(&self) -> bool {
        self.lock().proposal_id.is_some()
    }

    /// PPT-FR-ZMBA: the first successful call of the turn sets the card.
    pub fn set(&self, proposal_id: &str) {
        let mut state = self.lock();
        if state.proposal_id.is_none() {
            state.proposal_id = Some(proposal_id.to_string());
        }
    }

    /// PPT-FR-IRFE: a later call adds its counts to the counts of the turn.
    pub fn add(&self, changed: usize, added: usize, withdrawn: usize) {
        let mut state = self.lock();
        state.changed += changed;
        state.added += added;
        state.withdrawn += withdrawn;
    }
}
