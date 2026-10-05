//! The project-scoped application data: drafts and conversations.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-WBDI, SAS-FR-SVMH, SAS-FR-NUZG, SAS-FR-JZAC,
//! SAS-FR-CQVE, SAS-FR-PYSU.

use serde::{Deserialize, Serialize};

use crate::domain::clock::Timestamp;
use crate::domain::error::DomainError;
use crate::domain::ids::{ConversationId, DraftId, MessageId, ProjectId, UserId};

/// The state of a draft (SAS-FR-WBDI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    Active,
    Archived,
    Graduated,
}

/// Project-scoped application data the service stores (SAS-FR-WBDI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    /// Set when the record is created and never changed (SAS-FR-KRMF).
    pub project_id: ProjectId,
    pub owner_id: UserId,
    pub title: String,
    pub content: String,
    pub state: DraftState,
    /// Raised by 1 on each accepted update (SAS-FR-JZAC).
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The state of a conversation (SAS-FR-SVMH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationState {
    Open,
    Resolved,
    Locked,
}

/// One entry of the append-only sequence (SAS-FR-NUZG).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: MessageId,
    /// Starts at 1 and rises by 1. No request writes it.
    pub sequence: u64,
    pub author_id: UserId,
    pub body: String,
    pub created_at: Timestamp,
}

/// A conversation on a project, and optionally on a draft (SAS-FR-SVMH).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    /// Set when the record is created and never changed (SAS-FR-KRMF).
    pub project_id: ProjectId,
    pub draft_id: Option<DraftId>,
    pub owner_id: UserId,
    pub title: String,
    pub state: ConversationState,
    pub revision: u64,
    pub messages: Vec<ConversationMessage>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Conversation {
    /// The sequence number the next message takes (SAS-FR-NUZG).
    pub fn next_sequence(&self) -> u64 {
        self.messages
            .last()
            .map_or(1, |message| message.sequence + 1)
    }

    /// Appends one message (SAS-FR-CQVE, SAS-FR-PYSU).
    ///
    /// A locked conversation accepts none. A resolved conversation accepts one
    /// and returns to `open`. The message takes the next sequence number, the
    /// revision rises, and no earlier message changes.
    pub fn append(
        &mut self,
        id: MessageId,
        author_id: UserId,
        body: String,
        now: Timestamp,
    ) -> Result<ConversationMessage, DomainError> {
        if matches!(self.state, ConversationState::Locked) {
            return Err(DomainError::ConversationLocked);
        }

        let message = ConversationMessage {
            id,
            sequence: self.next_sequence(),
            author_id,
            body,
            created_at: now.clone(),
        };
        self.messages.push(message.clone());
        self.state = ConversationState::Open;
        self.revision += 1;
        self.updated_at = now;
        Ok(message)
    }
}

/// Refuses an update that carries a revision the record does not hold
/// (SAS-FR-JZAC, SAS-FR-CQVE).
pub fn check_revision(expected: u64, actual: u64) -> Result<(), DomainError> {
    if expected == actual {
        Ok(())
    } else {
        Err(DomainError::StaleRevision { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn stamp(text: &str) -> Timestamp {
        Timestamp(text.to_string())
    }

    fn conversation(state: ConversationState) -> Conversation {
        Conversation {
            id: ConversationId(Uuid::nil()),
            project_id: ProjectId(Uuid::nil()),
            draft_id: None,
            owner_id: UserId(Uuid::nil()),
            title: "A conversation".to_string(),
            state,
            revision: 1,
            messages: Vec::new(),
            created_at: stamp("2025-01-01T00:00:00Z"),
            updated_at: stamp("2025-01-01T00:00:00Z"),
        }
    }

    // SAS-FR-NUZG: the sequence starts at 1 and rises by 1, and no message changes.
    #[test]
    fn the_message_sequence_starts_at_one_and_rises_by_one() {
        let mut record = conversation(ConversationState::Open);
        for expected in 1..=5u64 {
            let message = record
                .append(
                    MessageId(Uuid::from_u128(expected as u128)),
                    UserId(Uuid::nil()),
                    format!("message {expected}"),
                    stamp("2025-01-02T00:00:00Z"),
                )
                .expect("an open conversation accepts a message");
            assert_eq!(message.sequence, expected);
        }
        assert_eq!(record.messages.len(), 5);
        assert_eq!(record.messages[0].body, "message 1");
        assert_eq!(record.revision, 6);
    }

    // SAS-FR-PYSU: a locked conversation refuses a message; a resolved one accepts
    // it and returns to open.
    #[test]
    fn a_locked_conversation_refuses_and_a_resolved_one_reopens() {
        let mut locked = conversation(ConversationState::Locked);
        let refused = locked
            .append(
                MessageId(Uuid::nil()),
                UserId(Uuid::nil()),
                "body".to_string(),
                stamp("2025-01-02T00:00:00Z"),
            )
            .expect_err("a locked conversation refuses a message");
        assert_eq!(refused.code(), "conversation_locked");
        assert!(locked.messages.is_empty());
        assert_eq!(locked.revision, 1);

        let mut resolved = conversation(ConversationState::Resolved);
        resolved
            .append(
                MessageId(Uuid::nil()),
                UserId(Uuid::nil()),
                "body".to_string(),
                stamp("2025-01-02T00:00:00Z"),
            )
            .expect("a resolved conversation accepts a message");
        assert_eq!(resolved.state, ConversationState::Open);
    }

    // SAS-FR-JZAC: an update that carries another revision is refused.
    #[test]
    fn a_stale_revision_is_refused() {
        assert!(check_revision(3, 3).is_ok());
        let error = check_revision(2, 3).expect_err("a stale revision is refused");
        assert_eq!(error.code(), "stale_revision");
    }
}
