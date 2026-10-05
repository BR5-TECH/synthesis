//! The drafts and the conversations.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-GBWS, SAS-FR-XONP, SAS-FR-JZAC, SAS-FR-CQVE,
//! SAS-FR-PYSU, SAS-FR-KRMF, SAS-FR-WLIG.

use crate::application::access::AccessService;
use crate::application::commands::{
    AppendMessage, CreateConversation, CreateDraft, Page, UpdateConversation, UpdateDraft,
};
use crate::application::paginate;
use crate::application::ports::{Ports, Principal};
use crate::domain::content::{
    check_revision, Conversation, ConversationMessage, ConversationState, Draft, DraftState,
};
use crate::domain::error::DomainError;
use crate::domain::ids::{ConversationId, DraftId, MessageId, ProjectId, UserId};
use crate::domain::permissions::Permission;
use crate::domain::validation;

/// The inbound port of the draft and conversation operations.
///
/// SAS-FR-WLIG: every adapter that reaches these operations is authorized on
/// the same terms, so a relay-borne operation is checked as the REST route is.
pub trait ContentApi: Send + Sync {
    fn create_draft(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateDraft,
    ) -> Result<Draft, DomainError>;
    fn read_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError>;
    fn list_drafts(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Draft>, DomainError>;
    fn update_draft(
        &self,
        principal: &Principal,
        id: DraftId,
        command: UpdateDraft,
    ) -> Result<Draft, DomainError>;
    fn archive_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError>;
    fn graduate_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError>;
    fn delete_draft(&self, principal: &Principal, id: DraftId) -> Result<(), DomainError>;

    fn create_conversation(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateConversation,
    ) -> Result<Conversation, DomainError>;
    fn read_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError>;
    fn list_conversations(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Conversation>, DomainError>;
    fn update_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
        command: UpdateConversation,
    ) -> Result<Conversation, DomainError>;
    fn read_messages(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Vec<ConversationMessage>, DomainError>;
    fn append_message(
        &self,
        principal: &Principal,
        id: ConversationId,
        command: AppendMessage,
    ) -> Result<ConversationMessage, DomainError>;
    fn resolve_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError>;
    fn lock_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError>;
}

/// The draft and conversation operations over the ports.
#[derive(Clone)]
pub struct ContentService {
    ports: Ports,
    access: AccessService,
}

impl ContentService {
    /// The service over the given ports.
    pub fn new(ports: Ports) -> Self {
        ContentService {
            access: AccessService::new(ports.clone()),
            ports,
        }
    }

    /// The draft, when the principal holds the permission on its project
    /// (SAS-FR-GBWS).
    fn draft_for(
        &self,
        principal: &Principal,
        id: DraftId,
        permission: Permission,
    ) -> Result<Draft, DomainError> {
        let draft = self.ports.drafts.get(id)?;
        // The project check runs first, and a project the principal may not read
        // answers `not_found`, so a draft identifier discloses nothing.
        self.access
            .authorize(principal, draft.project_id, permission)?;
        Ok(draft)
    }

    /// The conversation, when the principal holds the permission on its project
    /// (SAS-FR-XONP).
    fn conversation_for(
        &self,
        principal: &Principal,
        id: ConversationId,
        permission: Permission,
    ) -> Result<Conversation, DomainError> {
        let conversation = self.ports.conversations.get(id)?;
        self.access
            .authorize(principal, conversation.project_id, permission)?;
        Ok(conversation)
    }

    /// The owner a create request names, which defaults to the caller.
    fn owner_of(
        &self,
        principal: &Principal,
        named: Option<&String>,
    ) -> Result<UserId, DomainError> {
        match named {
            None => Ok(principal.user_id),
            Some(text) => {
                let owner = UserId::parse(text)?;
                self.ports.users.get(owner)?;
                Ok(owner)
            }
        }
    }
}

impl ContentApi for ContentService {
    fn create_draft(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateDraft,
    ) -> Result<Draft, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::DraftCreate)?;
        let id: DraftId = self.ports.resolve_id("id", command.id.as_deref())?;
        let title = validation::name("title", &command.title)?;
        let content = validation::content("content", command.content.as_deref().unwrap_or(""))?;
        let owner_id = self.owner_of(principal, command.owner_id.as_ref())?;

        let now = self.ports.clock.now();
        self.ports.drafts.insert(
            id,
            Draft {
                id,
                project_id,
                owner_id,
                title,
                content,
                state: DraftState::Active,
                revision: 1,
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError> {
        self.draft_for(principal, id, Permission::DraftRead)
    }

    fn list_drafts(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Draft>, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::DraftRead)?;
        let drafts = self
            .ports
            .drafts
            .list()
            .into_iter()
            .filter(|draft| draft.project_id == project_id)
            .collect();
        paginate(drafts, page)
    }

    fn update_draft(
        &self,
        principal: &Principal,
        id: DraftId,
        command: UpdateDraft,
    ) -> Result<Draft, DomainError> {
        self.draft_for(principal, id, Permission::DraftUpdate)?;
        let now = self.ports.clock.now();

        // SAS-FR-JZAC: the revision is read under the store's lock, so two
        // writers that hold one revision cannot both succeed.
        self.ports.drafts.update(id, &mut |draft: &mut Draft| {
            check_revision(command.revision, draft.revision)?;
            if let Some(title) = command.title.as_deref() {
                draft.title = validation::name("title", title)?;
            }
            if let Some(content) = command.content.as_deref() {
                draft.content = validation::content("content", content)?;
            }
            draft.revision += 1;
            draft.updated_at = now.clone();
            Ok(())
        })
    }

    fn archive_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError> {
        self.draft_for(principal, id, Permission::DraftArchive)?;
        let now = self.ports.clock.now();
        self.ports.drafts.update(id, &mut |draft: &mut Draft| {
            draft.state = DraftState::Archived;
            draft.revision += 1;
            draft.updated_at = now.clone();
            Ok(())
        })
    }

    fn graduate_draft(&self, principal: &Principal, id: DraftId) -> Result<Draft, DomainError> {
        self.draft_for(principal, id, Permission::DraftGraduate)?;
        let now = self.ports.clock.now();
        self.ports.drafts.update(id, &mut |draft: &mut Draft| {
            draft.state = DraftState::Graduated;
            draft.revision += 1;
            draft.updated_at = now.clone();
            Ok(())
        })
    }

    fn delete_draft(&self, principal: &Principal, id: DraftId) -> Result<(), DomainError> {
        self.draft_for(principal, id, Permission::DraftDelete)?;

        // SAS-FR-SGXQ: a conversation that names the draft holds it back.
        let referenced = self
            .ports
            .conversations
            .list()
            .into_iter()
            .any(|conversation| conversation.draft_id == Some(id));
        if referenced {
            return Err(DomainError::Referenced {
                reason: "a conversation names the draft".to_string(),
            });
        }
        self.ports.drafts.remove(id).map(|_| ())
    }

    fn create_conversation(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        command: CreateConversation,
    ) -> Result<Conversation, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::ConversationCreate)?;
        let id: ConversationId = self.ports.resolve_id("id", command.id.as_deref())?;
        let title = validation::name("title", &command.title)?;
        let owner_id = self.owner_of(principal, command.owner_id.as_ref())?;

        // SAS-FR-KRMF: a conversation and the draft it names belong to one project.
        let draft_id = match command.draft_id.as_deref() {
            None => None,
            Some(text) => {
                let draft_id = DraftId::parse(text)?;
                let draft = self.ports.drafts.get(draft_id)?;
                if draft.project_id != project_id {
                    return Err(DomainError::invalid_field(
                        "draft_id",
                        "the draft belongs to another project",
                    ));
                }
                Some(draft_id)
            }
        };

        let now = self.ports.clock.now();
        self.ports.conversations.insert(
            id,
            Conversation {
                id,
                project_id,
                draft_id,
                owner_id,
                title,
                state: ConversationState::Open,
                revision: 1,
                messages: Vec::new(),
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }

    fn read_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError> {
        self.conversation_for(principal, id, Permission::ConversationRead)
    }

    fn list_conversations(
        &self,
        principal: &Principal,
        project_id: ProjectId,
        page: Page,
    ) -> Result<Vec<Conversation>, DomainError> {
        self.access
            .authorize(principal, project_id, Permission::ConversationRead)?;
        let conversations = self
            .ports
            .conversations
            .list()
            .into_iter()
            .filter(|conversation| conversation.project_id == project_id)
            .collect();
        paginate(conversations, page)
    }

    fn update_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
        command: UpdateConversation,
    ) -> Result<Conversation, DomainError> {
        self.conversation_for(principal, id, Permission::ConversationUpdate)?;
        let now = self.ports.clock.now();
        self.ports
            .conversations
            .update(id, &mut |conversation: &mut Conversation| {
                check_revision(command.revision, conversation.revision)?;
                if let Some(title) = command.title.as_deref() {
                    conversation.title = validation::name("title", title)?;
                }
                conversation.revision += 1;
                conversation.updated_at = now.clone();
                Ok(())
            })
    }

    fn read_messages(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Vec<ConversationMessage>, DomainError> {
        Ok(self
            .conversation_for(principal, id, Permission::ConversationRead)?
            .messages)
    }

    fn append_message(
        &self,
        principal: &Principal,
        id: ConversationId,
        command: AppendMessage,
    ) -> Result<ConversationMessage, DomainError> {
        self.conversation_for(principal, id, Permission::ConversationAppend)?;
        let message_id: MessageId = self.ports.resolve_id("id", command.id.as_deref())?;
        let body = validation::content("body", &command.body)?;
        let author = principal.user_id;
        let now = self.ports.clock.now();

        // The sequence number is taken under the store's lock, so two writers
        // never take one number (SAS-FR-HEIV).
        let mut appended: Option<ConversationMessage> = None;
        self.ports
            .conversations
            .update(id, &mut |conversation: &mut Conversation| {
                appended =
                    Some(conversation.append(message_id, author, body.clone(), now.clone())?);
                Ok(())
            })?;
        appended.ok_or(DomainError::Internal {
            reason: "the message was not appended".to_string(),
        })
    }

    fn resolve_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError> {
        self.conversation_for(principal, id, Permission::ConversationResolve)?;
        let now = self.ports.clock.now();
        self.ports
            .conversations
            .update(id, &mut |conversation: &mut Conversation| {
                conversation.state = ConversationState::Resolved;
                conversation.revision += 1;
                conversation.updated_at = now.clone();
                Ok(())
            })
    }

    fn lock_conversation(
        &self,
        principal: &Principal,
        id: ConversationId,
    ) -> Result<Conversation, DomainError> {
        self.conversation_for(principal, id, Permission::ConversationLock)?;
        let now = self.ports.clock.now();
        self.ports
            .conversations
            .update(id, &mut |conversation: &mut Conversation| {
                conversation.state = ConversationState::Locked;
                conversation.revision += 1;
                conversation.updated_at = now.clone();
                Ok(())
            })
    }
}
