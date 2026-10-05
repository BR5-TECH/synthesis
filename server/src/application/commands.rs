//! The inputs of the inbound ports.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-VBQJ, SAS-FR-TQIM, SAS-FR-OZET.
//!
//! Each command is plain data. It names no HTTP type: an adapter fills one in
//! from whatever it reads, and the application service is what checks it.

use serde::Deserialize;

/// The identifier a create request may supply (SAS-FR-OZET).
type SuppliedId = Option<String>;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateUser {
    pub id: SuppliedId,
    pub display_name: String,
    pub email: Option<String>,
    pub role: Option<String>,
    /// The `(issuer, subject)` pair a later OIDC mode maps (SAS-FR-TWEL).
    pub issuer: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateGroup {
    pub id: SuppliedId,
    pub name: String,
    /// The owner, when the request names one other than the caller.
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateGroup {
    pub name: Option<String>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateMembership {
    pub id: SuppliedId,
    pub member_id: String,
    pub target_type: String,
    pub target_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateMembership {
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateInvitation {
    pub id: SuppliedId,
    pub target_type: String,
    pub target_id: String,
    pub invitee_user_id: Option<String>,
    pub invitee_email: Option<String>,
    pub role: String,
    /// An RFC 3339 time. An invitation that names one in the past is created
    /// and is never acceptable (SAS-FR-ATLB).
    pub expires_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AcceptInvitation {
    /// The user the membership is written for. It defaults to the invitee the
    /// invitation names.
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateDevice {
    pub id: SuppliedId,
    pub owner_id: Option<String>,
    pub device_type: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateDevice {
    pub display_name: Option<String>,
    /// Moves the last-seen time to now.
    pub seen: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateProject {
    pub id: SuppliedId,
    pub display_name: String,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateProject {
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TransferOwnership {
    pub new_owner_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateGrant {
    pub id: SuppliedId,
    pub target_type: String,
    pub target_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateGrant {
    pub role: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateDraft {
    pub id: SuppliedId,
    pub title: String,
    pub content: Option<String>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateDraft {
    /// The revision the client holds (SAS-FR-JZAC).
    pub revision: u64,
    pub title: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateConversation {
    pub id: SuppliedId,
    pub title: String,
    pub draft_id: Option<String>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateConversation {
    /// The revision the client holds (SAS-FR-CQVE).
    pub revision: u64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppendMessage {
    pub id: SuppliedId,
    pub body: String,
}

/// The window a list route serves (SAS-FR-TQIM).
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Page {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// The filters `GET /v1/memberships` accepts.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MembershipFilter {
    pub member_id: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    #[serde(flatten)]
    pub page: Page,
}
