//! The draft and conversation routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-GBWS, SAS-FR-XONP, SAS-FR-JZAC, SAS-FR-CQVE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::adapters::http::dto::ListBody;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::extract::{path_id, Params, Payload};
use crate::adapters::http::state::HttpState;
use crate::application::commands::{
    AppendMessage, CreateConversation, CreateDraft, Page, UpdateConversation, UpdateDraft,
};
use crate::application::ports::Principal;
use crate::domain::content::{Conversation, ConversationMessage, Draft};

/// The routes of the project-scoped application data.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/v1/projects/{project_id}/drafts",
            post(create_draft).get(list_drafts),
        )
        .route(
            "/v1/drafts/{draft_id}",
            get(read_draft).patch(update_draft).delete(delete_draft),
        )
        .route("/v1/drafts/{draft_id}/archive", post(archive_draft))
        .route("/v1/drafts/{draft_id}/graduate", post(graduate_draft))
        .route(
            "/v1/projects/{project_id}/conversations",
            post(create_conversation).get(list_conversations),
        )
        .route(
            "/v1/conversations/{conversation_id}",
            get(read_conversation).patch(update_conversation),
        )
        .route(
            "/v1/conversations/{conversation_id}/messages",
            get(read_messages).post(append_message),
        )
        .route(
            "/v1/conversations/{conversation_id}/resolve",
            post(resolve_conversation),
        )
        .route(
            "/v1/conversations/{conversation_id}/lock",
            post(lock_conversation),
        )
}

async fn create_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Payload(command): Payload<CreateDraft>,
) -> Result<(StatusCode, Json<Draft>), ApiError> {
    let draft =
        state
            .application
            .content
            .create_draft(&principal, path_id(&project_id)?, command)?;
    Ok((StatusCode::CREATED, Json(draft)))
}

async fn list_drafts(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Draft>>, ApiError> {
    Ok(Json(ListBody::new(state.application.content.list_drafts(
        &principal,
        path_id(&project_id)?,
        page,
    )?)))
}

async fn read_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(draft_id): Path<String>,
) -> Result<Json<Draft>, ApiError> {
    Ok(Json(
        state
            .application
            .content
            .read_draft(&principal, path_id(&draft_id)?)?,
    ))
}

async fn update_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(draft_id): Path<String>,
    Payload(command): Payload<UpdateDraft>,
) -> Result<Json<Draft>, ApiError> {
    Ok(Json(state.application.content.update_draft(
        &principal,
        path_id(&draft_id)?,
        command,
    )?))
}

async fn archive_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(draft_id): Path<String>,
) -> Result<Json<Draft>, ApiError> {
    Ok(Json(
        state
            .application
            .content
            .archive_draft(&principal, path_id(&draft_id)?)?,
    ))
}

async fn graduate_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(draft_id): Path<String>,
) -> Result<Json<Draft>, ApiError> {
    Ok(Json(
        state
            .application
            .content
            .graduate_draft(&principal, path_id(&draft_id)?)?,
    ))
}

async fn delete_draft(
    State(state): State<HttpState>,
    principal: Principal,
    Path(draft_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .content
        .delete_draft(&principal, path_id(&draft_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_conversation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Payload(command): Payload<CreateConversation>,
) -> Result<(StatusCode, Json<Conversation>), ApiError> {
    let conversation = state.application.content.create_conversation(
        &principal,
        path_id(&project_id)?,
        command,
    )?;
    Ok((StatusCode::CREATED, Json(conversation)))
}

async fn list_conversations(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Conversation>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .content
            .list_conversations(&principal, path_id(&project_id)?, page)?,
    )))
}

async fn read_conversation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
) -> Result<Json<Conversation>, ApiError> {
    Ok(Json(state.application.content.read_conversation(
        &principal,
        path_id(&conversation_id)?,
    )?))
}

async fn update_conversation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
    Payload(command): Payload<UpdateConversation>,
) -> Result<Json<Conversation>, ApiError> {
    Ok(Json(state.application.content.update_conversation(
        &principal,
        path_id(&conversation_id)?,
        command,
    )?))
}

async fn read_messages(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
) -> Result<Json<ListBody<ConversationMessage>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .content
            .read_messages(&principal, path_id(&conversation_id)?)?,
    )))
}

async fn append_message(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
    Payload(command): Payload<AppendMessage>,
) -> Result<(StatusCode, Json<ConversationMessage>), ApiError> {
    let message = state.application.content.append_message(
        &principal,
        path_id(&conversation_id)?,
        command,
    )?;
    Ok((StatusCode::CREATED, Json(message)))
}

async fn resolve_conversation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
) -> Result<Json<Conversation>, ApiError> {
    Ok(Json(state.application.content.resolve_conversation(
        &principal,
        path_id(&conversation_id)?,
    )?))
}

async fn lock_conversation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(conversation_id): Path<String>,
) -> Result<Json<Conversation>, ApiError> {
    Ok(Json(state.application.content.lock_conversation(
        &principal,
        path_id(&conversation_id)?,
    )?))
}
