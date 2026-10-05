//! The user, team, and organization routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-LFCA, SAS-FR-KDSM, SAS-FR-GVAB, SAS-FR-RHYT.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::adapters::http::dto::ListBody;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::extract::{path_id, Params, Payload};
use crate::adapters::http::state::HttpState;
use crate::application::commands::{CreateGroup, CreateUser, Page, UpdateGroup, UpdateUser};
use crate::application::ports::Principal;
use crate::domain::identity::{Organization, Team, User};

/// The routes of the identity entities.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route("/v1/users", post(create_user).get(list_users))
        .route(
            "/v1/users/{user_id}",
            get(read_user).patch(update_user).delete(delete_user),
        )
        .route("/v1/teams", post(create_team).get(list_teams))
        .route(
            "/v1/teams/{team_id}",
            get(read_team).patch(update_team).delete(delete_team),
        )
        .route(
            "/v1/organizations",
            post(create_organization).get(list_organizations),
        )
        .route(
            "/v1/organizations/{organization_id}",
            get(read_organization)
                .patch(update_organization)
                .delete(delete_organization),
        )
}

async fn create_user(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateUser>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let user = state
        .application
        .identity
        .create_user(&principal, command)?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn list_users(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<User>>, ApiError> {
    Ok(Json(ListBody::new(
        state.application.identity.list_users(&principal, page)?,
    )))
}

async fn read_user(
    State(state): State<HttpState>,
    principal: Principal,
    Path(user_id): Path<String>,
) -> Result<Json<User>, ApiError> {
    Ok(Json(
        state
            .application
            .identity
            .read_user(&principal, path_id(&user_id)?)?,
    ))
}

async fn update_user(
    State(state): State<HttpState>,
    principal: Principal,
    Path(user_id): Path<String>,
    Payload(command): Payload<UpdateUser>,
) -> Result<Json<User>, ApiError> {
    Ok(Json(state.application.identity.update_user(
        &principal,
        path_id(&user_id)?,
        command,
    )?))
}

async fn delete_user(
    State(state): State<HttpState>,
    principal: Principal,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .identity
        .delete_user(&principal, path_id(&user_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_team(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateGroup>,
) -> Result<(StatusCode, Json<Team>), ApiError> {
    let team = state
        .application
        .identity
        .create_team(&principal, command)?;
    Ok((StatusCode::CREATED, Json(team)))
}

async fn list_teams(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Team>>, ApiError> {
    Ok(Json(ListBody::new(
        state.application.identity.list_teams(&principal, page)?,
    )))
}

async fn read_team(
    State(state): State<HttpState>,
    principal: Principal,
    Path(team_id): Path<String>,
) -> Result<Json<Team>, ApiError> {
    Ok(Json(
        state
            .application
            .identity
            .read_team(&principal, path_id(&team_id)?)?,
    ))
}

async fn update_team(
    State(state): State<HttpState>,
    principal: Principal,
    Path(team_id): Path<String>,
    Payload(command): Payload<UpdateGroup>,
) -> Result<Json<Team>, ApiError> {
    Ok(Json(state.application.identity.update_team(
        &principal,
        path_id(&team_id)?,
        command,
    )?))
}

async fn delete_team(
    State(state): State<HttpState>,
    principal: Principal,
    Path(team_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .identity
        .delete_team(&principal, path_id(&team_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_organization(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateGroup>,
) -> Result<(StatusCode, Json<Organization>), ApiError> {
    let organization = state
        .application
        .identity
        .create_organization(&principal, command)?;
    Ok((StatusCode::CREATED, Json(organization)))
}

async fn list_organizations(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Organization>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .identity
            .list_organizations(&principal, page)?,
    )))
}

async fn read_organization(
    State(state): State<HttpState>,
    principal: Principal,
    Path(organization_id): Path<String>,
) -> Result<Json<Organization>, ApiError> {
    Ok(Json(state.application.identity.read_organization(
        &principal,
        path_id(&organization_id)?,
    )?))
}

async fn update_organization(
    State(state): State<HttpState>,
    principal: Principal,
    Path(organization_id): Path<String>,
    Payload(command): Payload<UpdateGroup>,
) -> Result<Json<Organization>, ApiError> {
    Ok(Json(state.application.identity.update_organization(
        &principal,
        path_id(&organization_id)?,
        command,
    )?))
}

async fn delete_organization(
    State(state): State<HttpState>,
    principal: Principal,
    Path(organization_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .identity
        .delete_organization(&principal, path_id(&organization_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}
