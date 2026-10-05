//! The membership, invitation, and device routes.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-MZQF, SAS-FR-YCWK, SAS-FR-ATLB, SAS-FR-FQVS.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::adapters::http::dto::ListBody;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::extract::{path_id, Params, Payload};
use crate::adapters::http::state::HttpState;
use crate::application::commands::{
    AcceptInvitation, CreateDevice, CreateInvitation, CreateMembership, MembershipFilter, Page,
    UpdateDevice, UpdateMembership,
};
use crate::application::ports::Principal;
use crate::domain::identity::{Device, Invitation, Membership};

/// The routes of the membership entities.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/v1/memberships",
            post(create_membership).get(list_memberships),
        )
        .route(
            "/v1/memberships/{membership_id}",
            get(read_membership)
                .patch(update_membership)
                .delete(delete_membership),
        )
        .route(
            "/v1/invitations",
            post(create_invitation).get(list_invitations),
        )
        .route("/v1/invitations/{invitation_id}", get(read_invitation))
        .route(
            "/v1/invitations/{invitation_id}/accept",
            post(accept_invitation),
        )
        .route(
            "/v1/invitations/{invitation_id}/revoke",
            post(revoke_invitation),
        )
        .route("/v1/devices", post(create_device).get(list_devices))
        .route(
            "/v1/devices/{device_id}",
            get(read_device).patch(update_device).delete(delete_device),
        )
        .route("/v1/devices/{device_id}/revoke", post(revoke_device))
}

async fn create_membership(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateMembership>,
) -> Result<(StatusCode, Json<Membership>), ApiError> {
    let membership = state
        .application
        .memberships
        .create_membership(&principal, command)?;
    Ok((StatusCode::CREATED, Json(membership)))
}

async fn list_memberships(
    State(state): State<HttpState>,
    principal: Principal,
    Params(filter): Params<MembershipFilter>,
) -> Result<Json<ListBody<Membership>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .memberships
            .list_memberships(&principal, filter)?,
    )))
}

async fn read_membership(
    State(state): State<HttpState>,
    principal: Principal,
    Path(membership_id): Path<String>,
) -> Result<Json<Membership>, ApiError> {
    Ok(Json(
        state
            .application
            .memberships
            .read_membership(&principal, path_id(&membership_id)?)?,
    ))
}

async fn update_membership(
    State(state): State<HttpState>,
    principal: Principal,
    Path(membership_id): Path<String>,
    Payload(command): Payload<UpdateMembership>,
) -> Result<Json<Membership>, ApiError> {
    Ok(Json(state.application.memberships.update_membership(
        &principal,
        path_id(&membership_id)?,
        command,
    )?))
}

async fn delete_membership(
    State(state): State<HttpState>,
    principal: Principal,
    Path(membership_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .memberships
        .delete_membership(&principal, path_id(&membership_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_invitation(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateInvitation>,
) -> Result<(StatusCode, Json<Invitation>), ApiError> {
    let invitation = state
        .application
        .memberships
        .create_invitation(&principal, command)?;
    Ok((StatusCode::CREATED, Json(invitation)))
}

async fn list_invitations(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Invitation>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .memberships
            .list_invitations(&principal, page)?,
    )))
}

async fn read_invitation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(invitation_id): Path<String>,
) -> Result<Json<Invitation>, ApiError> {
    Ok(Json(
        state
            .application
            .memberships
            .read_invitation(&principal, path_id(&invitation_id)?)?,
    ))
}

async fn accept_invitation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(invitation_id): Path<String>,
    Payload(command): Payload<AcceptInvitation>,
) -> Result<(StatusCode, Json<Membership>), ApiError> {
    let membership = state.application.memberships.accept_invitation(
        &principal,
        path_id(&invitation_id)?,
        command,
    )?;
    Ok((StatusCode::CREATED, Json(membership)))
}

async fn revoke_invitation(
    State(state): State<HttpState>,
    principal: Principal,
    Path(invitation_id): Path<String>,
) -> Result<Json<Invitation>, ApiError> {
    Ok(Json(
        state
            .application
            .memberships
            .revoke_invitation(&principal, path_id(&invitation_id)?)?,
    ))
}

async fn create_device(
    State(state): State<HttpState>,
    principal: Principal,
    Payload(command): Payload<CreateDevice>,
) -> Result<(StatusCode, Json<Device>), ApiError> {
    let device = state
        .application
        .memberships
        .create_device(&principal, command)?;
    Ok((StatusCode::CREATED, Json(device)))
}

async fn list_devices(
    State(state): State<HttpState>,
    principal: Principal,
    Params(page): Params<Page>,
) -> Result<Json<ListBody<Device>>, ApiError> {
    Ok(Json(ListBody::new(
        state
            .application
            .memberships
            .list_devices(&principal, page)?,
    )))
}

async fn read_device(
    State(state): State<HttpState>,
    principal: Principal,
    Path(device_id): Path<String>,
) -> Result<Json<Device>, ApiError> {
    Ok(Json(
        state
            .application
            .memberships
            .read_device(&principal, path_id(&device_id)?)?,
    ))
}

async fn update_device(
    State(state): State<HttpState>,
    principal: Principal,
    Path(device_id): Path<String>,
    Payload(command): Payload<UpdateDevice>,
) -> Result<Json<Device>, ApiError> {
    Ok(Json(state.application.memberships.update_device(
        &principal,
        path_id(&device_id)?,
        command,
    )?))
}

async fn revoke_device(
    State(state): State<HttpState>,
    principal: Principal,
    Path(device_id): Path<String>,
) -> Result<Json<Device>, ApiError> {
    Ok(Json(
        state
            .application
            .memberships
            .revoke_device(&principal, path_id(&device_id)?)?,
    ))
}

async fn delete_device(
    State(state): State<HttpState>,
    principal: Principal,
    Path(device_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .application
        .memberships
        .delete_device(&principal, path_id(&device_id)?)?;
    Ok(StatusCode::NO_CONTENT)
}
