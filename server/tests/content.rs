//! The draft and conversation tests.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.

mod common;

use std::sync::Arc;

use common::{make_project, make_user, Api};
use synthesis_server::application::commands::{
    AppendMessage, CreateConversation, CreateDraft, Page, UpdateConversation, UpdateDraft,
};
use synthesis_server::domain::content::{ConversationState, DraftState};

// SAS-FR-WBDI, SAS-FR-KRMF: a draft belongs to one project, and its project
// never changes.
#[test]
fn a_draft_is_written_read_and_scoped_to_its_project() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let first = make_project(&application, &admin, owner, "The first project");
    let second = make_project(&application, &admin, owner, "The second project");

    let draft = application
        .content
        .create_draft(
            &admin,
            first.id,
            CreateDraft {
                title: "A draft".to_string(),
                content: Some("The content".to_string()),
                ..Default::default()
            },
        )
        .expect("the draft is written");
    assert_eq!(draft.project_id, first.id);
    assert_eq!(draft.revision, 1);
    assert_eq!(draft.state, DraftState::Active);

    assert_eq!(
        application
            .content
            .list_drafts(&admin, first.id, Page::default())
            .expect("the drafts are read")
            .len(),
        1
    );
    assert!(application
        .content
        .list_drafts(&admin, second.id, Page::default())
        .expect("the drafts are read")
        .is_empty());
}

// SAS-FR-JZAC: an update carries the revision, a stale revision is refused, and
// an accepted update raises the revision by 1.
#[test]
fn a_stale_draft_update_is_refused_and_changes_nothing() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let draft = application
        .content
        .create_draft(
            &admin,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                content: Some("First".to_string()),
                ..Default::default()
            },
        )
        .expect("the draft is written");

    let updated = application
        .content
        .update_draft(
            &admin,
            draft.id,
            UpdateDraft {
                revision: 1,
                content: Some("Second".to_string()),
                ..Default::default()
            },
        )
        .expect("the update is accepted");
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.content, "Second");

    let refused = application
        .content
        .update_draft(
            &admin,
            draft.id,
            UpdateDraft {
                revision: 1,
                content: Some("Third".to_string()),
                ..Default::default()
            },
        )
        .expect_err("the second writer holds a stale revision");
    assert_eq!(refused.code(), "stale_revision");
    assert_eq!(
        application
            .content
            .read_draft(&admin, draft.id)
            .expect("the draft is read")
            .content,
        "Second"
    );
}

// SAS-FR-WBDI: a draft is archived, graduated, and deleted.
#[test]
fn a_draft_moves_through_its_states() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let draft = application
        .content
        .create_draft(
            &admin,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("the draft is written");

    assert_eq!(
        application
            .content
            .archive_draft(&admin, draft.id)
            .expect("the draft is archived")
            .state,
        DraftState::Archived
    );
    assert_eq!(
        application
            .content
            .graduate_draft(&admin, draft.id)
            .expect("the draft is graduated")
            .state,
        DraftState::Graduated
    );
    application
        .content
        .delete_draft(&admin, draft.id)
        .expect("the draft is deleted");
    assert_eq!(
        application
            .content
            .read_draft(&admin, draft.id)
            .expect_err("the draft is gone")
            .code(),
        "not_found"
    );
}

// SAS-FR-SGXQ: a draft a conversation names is not deleted.
#[test]
fn a_draft_a_conversation_names_is_not_deleted() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let draft = application
        .content
        .create_draft(
            &admin,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("the draft is written");
    application
        .content
        .create_conversation(
            &admin,
            project.id,
            CreateConversation {
                title: "A conversation".to_string(),
                draft_id: Some(draft.id.to_string()),
                ..Default::default()
            },
        )
        .expect("the conversation is written");

    assert_eq!(
        application
            .content
            .delete_draft(&admin, draft.id)
            .expect_err("the draft is referenced")
            .code(),
        "referenced"
    );
}

// SAS-FR-NUZG, SAS-FR-CQVE: the message sequence rises by 1, keeps its order,
// and no message is overwritten.
#[test]
fn the_message_sequence_keeps_its_order() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let conversation = application
        .content
        .create_conversation(
            &admin,
            project.id,
            CreateConversation {
                title: "A conversation".to_string(),
                ..Default::default()
            },
        )
        .expect("the conversation is written");

    for index in 1..=5u64 {
        let message = application
            .content
            .append_message(
                &admin,
                conversation.id,
                AppendMessage {
                    body: format!("message {index}"),
                    ..Default::default()
                },
            )
            .expect("the message is appended");
        assert_eq!(message.sequence, index);
    }

    let messages = application
        .content
        .read_messages(&admin, conversation.id)
        .expect("the messages are read");
    assert_eq!(messages.len(), 5);
    for (index, message) in messages.iter().enumerate() {
        assert_eq!(message.sequence, index as u64 + 1);
        assert_eq!(message.body, format!("message {}", index + 1));
    }
}

// SAS-FR-HEIV: concurrent appends take one sequence number each.
#[test]
fn concurrent_appends_never_repeat_a_sequence_number() {
    let api = Api::new();
    let application = Arc::new(api.application());
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let conversation = application
        .content
        .create_conversation(
            &admin,
            project.id,
            CreateConversation {
                title: "A conversation".to_string(),
                ..Default::default()
            },
        )
        .expect("the conversation is written");

    let threads: Vec<_> = (0..8)
        .map(|writer| {
            let application = Arc::clone(&application);
            let conversation_id = conversation.id;
            std::thread::spawn(move || {
                for index in 0..20 {
                    application
                        .content
                        .append_message(
                            &admin,
                            conversation_id,
                            AppendMessage {
                                body: format!("{writer}-{index}"),
                                ..Default::default()
                            },
                        )
                        .expect("the message is appended");
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("the thread completes");
    }

    let messages = application
        .content
        .read_messages(&admin, conversation.id)
        .expect("the messages are read");
    assert_eq!(messages.len(), 160);
    let mut sequences: Vec<u64> = messages.iter().map(|message| message.sequence).collect();
    sequences.sort_unstable();
    sequences.dedup();
    assert_eq!(sequences.len(), 160, "no sequence number repeated");
    assert_eq!(sequences.first(), Some(&1));
    assert_eq!(sequences.last(), Some(&160));
}

// SAS-FR-PYSU: a locked conversation refuses a message, and a resolved one
// accepts it and returns to open.
#[test]
fn a_locked_conversation_refuses_a_message() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let conversation = application
        .content
        .create_conversation(
            &admin,
            project.id,
            CreateConversation {
                title: "A conversation".to_string(),
                ..Default::default()
            },
        )
        .expect("the conversation is written");

    application
        .content
        .resolve_conversation(&admin, conversation.id)
        .expect("the conversation is resolved");
    application
        .content
        .append_message(
            &admin,
            conversation.id,
            AppendMessage {
                body: "One more".to_string(),
                ..Default::default()
            },
        )
        .expect("a resolved conversation accepts a message");
    assert_eq!(
        application
            .content
            .read_conversation(&admin, conversation.id)
            .expect("the conversation is read")
            .state,
        ConversationState::Open
    );

    application
        .content
        .lock_conversation(&admin, conversation.id)
        .expect("the conversation is locked");
    let refused = application
        .content
        .append_message(
            &admin,
            conversation.id,
            AppendMessage {
                body: "Refused".to_string(),
                ..Default::default()
            },
        )
        .expect_err("a locked conversation refuses a message");
    assert_eq!(refused.code(), "conversation_locked");
    assert_eq!(
        application
            .content
            .read_messages(&admin, conversation.id)
            .expect("the messages are read")
            .len(),
        1
    );
}

// SAS-FR-CQVE: a stale metadata update is refused.
#[test]
fn a_stale_conversation_update_is_refused() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let conversation = application
        .content
        .create_conversation(
            &admin,
            project.id,
            CreateConversation {
                title: "A conversation".to_string(),
                ..Default::default()
            },
        )
        .expect("the conversation is written");

    application
        .content
        .update_conversation(
            &admin,
            conversation.id,
            UpdateConversation {
                revision: 1,
                title: Some("A better title".to_string()),
            },
        )
        .expect("the update is accepted");
    assert_eq!(
        application
            .content
            .update_conversation(
                &admin,
                conversation.id,
                UpdateConversation {
                    revision: 1,
                    title: Some("A third title".to_string()),
                },
            )
            .expect_err("the revision is stale")
            .code(),
        "stale_revision"
    );
}

// SAS-FR-KRMF: a conversation names a draft of its own project alone.
#[test]
fn a_conversation_names_a_draft_of_its_own_project() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let first = make_project(&application, &admin, owner, "The first project");
    let second = make_project(&application, &admin, owner, "The second project");

    let draft = application
        .content
        .create_draft(
            &admin,
            first.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("the draft is written");

    let refused = application
        .content
        .create_conversation(
            &admin,
            second.id,
            CreateConversation {
                title: "A conversation".to_string(),
                draft_id: Some(draft.id.to_string()),
                ..Default::default()
            },
        )
        .expect_err("the draft belongs to another project");
    assert_eq!(refused.code(), "invalid_field");
}

// SAS-FR-AVDJ: a project that holds a draft is deleted with a cascade alone.
#[test]
fn a_project_that_holds_content_needs_a_cascade() {
    let api = Api::new();
    let application = api.application();
    let admin = api.administrator();
    let owner = make_user(&application, &admin, "The owner");
    let project = make_project(&application, &admin, owner, "A project");

    let draft = application
        .content
        .create_draft(
            &admin,
            project.id,
            CreateDraft {
                title: "A draft".to_string(),
                ..Default::default()
            },
        )
        .expect("the draft is written");

    assert_eq!(
        application
            .projects
            .delete_project(&admin, project.id, false)
            .expect_err("the project holds a draft")
            .code(),
        "referenced"
    );
    application
        .projects
        .delete_project(&admin, project.id, true)
        .expect("the cascade deletes the project");
    assert_eq!(
        application
            .content
            .read_draft(&admin, draft.id)
            .expect_err("the draft left with the project")
            .code(),
        "not_found"
    );
    assert!(
        api.ports.grants.list().is_empty(),
        "the grants left as well"
    );
}
