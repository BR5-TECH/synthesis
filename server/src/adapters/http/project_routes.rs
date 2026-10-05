//! The project, grant, and role routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-OKUC, SAS-FR-LEQB, SAS-FR-DHLM, SAS-FR-RAKX,
//! SAS-FR-AVDJ, SAS-FR-EYUB.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::adapters::http::dto::{EffectivePermissionsBody, ListBody, RolesBody};
use crate::adapters::http::error::ApiError;
use crate::adapters::http::extract::{path_id, Params, Payload};
use crate::adapters::http::state::HttpState;
use crate::application::commands::{
    CreateGrant, CreateProject, Page, TransferOwnership, UpdateGrant, UpdateProject,
};
use crate::application::ports::Principal;
use crate::domain::ids::UserId;
use crate::domain::project::{Grant, Project};

/// The query of `DELETE /v1/projects/{project_id}` (SAS-FR-AVDJ).
#[derive(Debug, Default, Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    cascade: bool,
}

/// The query of the effective-permissions route.
#[derive(Debug, Default, Deserialize)]
struct SubjectQuery {
    user_id: Option<String>,
}

/// The routes of the projects and their grants.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route("/v1/roles", get(read_roles))
        .route("/v1/projects", post(create_project).get(list_projects))
        .route(
            "/v1/projects/{project_id}",
            get(read_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/v1/projects/{project_id}/transfer-ownership",
            post(transfer_ownership),
        )
        .route(
            "/v1/projects/{project_id}/permissions",
            get(effective_permissions),
        )
        .route(
            "/v1/projects/{project_id}/grants",
            post(create_grant).get(list_grants),
        )
        .route(
            "/v1/projects/{project_id}/grants/{grant_id}",
            get(read_grant).patch(update_grant).delete(delete_grant),
        )
        .route(
            "/v1/projects/{project_id}/grants/{grant_id}/revoke",
            post(revoke_grant),
        )
}

async fn read_roles(_principal: Principal) -> Json<RolesBody> {
    Json(RolesBody::current())
}

async fn create_project(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let project = state
        .application
        .projects
        .create_project(&principal, command)?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn list_projects(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Project>>, ApiError> {
    Ok(Json(ListBody::new(
        state.application.projects.list_projects(&principal, page)?,
    )))
}

async fn read_project(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(
        state
            .application
            .projects
            .read_project(&principal, path_id(&project_id)?)?,
    ))
}

async fn update_project(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Payload(command): Payload<UpdateProject>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(state.application.projects.update_project(
        &principal,
        path_id(&project_id)?,
        command,
    )?))
}

async fn delete_project(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Params(query): Params<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .projects
        .delete_project(&principal, path_id(&project_id)?, query.cascade)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn transfer_ownership(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Payload(command): Payload<TransferOwnership>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(state.application.projects.transfer_ownership(
        &principal,
        path_id(&project_id)?,
        command,
    )?))
}

async fn effective_permissions(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Params(query): Params<SubjectQuery>,
) -> Result<Json<EffectivePermissionsBody>, ApiError> {
    let subject = match query.user_id.as_deref() {
        None => None,
        Some(text) => Some(UserId::parse(text)?),
    };
    let (user_id, permissions) = state.application.projects.effective_permissions(
        &principal,
        path_id(&project_id)?,
        subject,
    )?;
    Ok(Json(EffectivePermissionsBody::new(user_id, permissions)))
}

async fn create_grant(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Payload(command): Payload<CreateGrant>,
) -> Result<(StatusCode, Json<Grant>), ApiError> {
    let grant =
        state
            .application
            .projects
            .create_grant(&principal, path_id(&project_id)?, command)?;
    Ok((StatusCode::CREATED, Json(grant)))
}

async fn list_grants(
    State(state): State<HttpState>,
    principal: Principal,
    Path(project_id): Path<String>,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Grant>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .projects
            .list_grants(&principal, path_id(&project_id)?, page)?,
    )))
}

async fn read_grant(
    State(state): State<HttpState>,
    principal: Principal,
    Path((project_id, grant_id)): Path<(String, String)>,
) -> Result<Json<Grant>, ApiError> {
    Ok(Json(state.application.projects.read_grant(
        &principal,
        path_id(&project_id)?,
        path_id(&grant_id)?,
    )?))
}

async fn update_grant(
    State(state): State<HttpState>,
    principal: Principal,
    Path((project_id, grant_id)): Path<(String, String)>,
    Payload(command): Payload<UpdateGrant>,
) -> Result<Json<Grant>, ApiError> {
    Ok(Json(state.application.projects.update_grant(
        &principal,
        path_id(&project_id)?,
        path_id(&grant_id)?,
        command,
    )?))
}

async fn revoke_grant(
    State(state): State<HttpState>,
    principal: Principal,
    Path((project_id, grant_id)): Path<(String, String)>,
) -> Result<Json<Grant>, ApiError> {
    Ok(Json(state.application.projects.revoke_grant(
        &principal,
        path_id(&project_id)?,
        path_id(&grant_id)?,
    )?))
}

async fn delete_grant(
    State(state): State<HttpState>,
    principal: Principal,
    Path((project_id, grant_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.application.projects.delete_grant(
        &principal,
        path_id(&project_id)?,
        path_id(&grant_id)?,
    )?;
    Ok(StatusCode::NO_CONTENT)
}
