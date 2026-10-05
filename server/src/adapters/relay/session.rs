//! The handle lifecycle and the reason vocabulary of the relay session.
//!
//! Specification: `specifications/server/RSN-remote-session.md`.
//! Requirements: RSN-FR-JXBQ, RSN-FR-UDCM, RSN-FR-FKRE, RSN-FR-ATYV,
//! RSN-FR-DWLS, RSN-FR-KVBO, RSN-FR-SGQX, RSN-FR-ADXL.
//!
//! The reason codes are the one vocabulary the transport answers with
//! (`WSK-websocket.md` WSK-FR-BWZT). A reason names why a frame or a request
//! was refused; it names no payload, no key, no proof, no token, and no handle.

use serde::Serialize;

/// Every reason the relay refuses with, and every reason it closes with
/// (WSK-FR-BWZT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    UnsupportedVersion,
    ProtocolMismatch,
    InvalidFrameShape,
    MissingWorkerAuthentication,
    InvalidWorkerAuthentication,
    WorkerNotFound,
    RegistrationNotFound,
    HandleNotFound,
    HandleExpired,
    HandleRevoked,
    HandleReset,
    WorkerReplaced,
    DeliveryFailed,
    RelayRestarted,
    ProtocolReset,
}

impl ReasonCode {
    /// The stable identifier the wire carries.
    pub fn as_str(self) -> &'static str {
        match self {
            ReasonCode::UnsupportedVersion => "unsupported_version",
            ReasonCode::ProtocolMismatch => "protocol_mismatch",
            ReasonCode::InvalidFrameShape => "invalid_frame_shape",
            ReasonCode::MissingWorkerAuthentication => "missing_worker_authentication",
            ReasonCode::InvalidWorkerAuthentication => "invalid_worker_authentication",
            ReasonCode::WorkerNotFound => "worker_not_found",
            ReasonCode::RegistrationNotFound => "registration_not_found",
            ReasonCode::HandleNotFound => "handle_not_found",
            ReasonCode::HandleExpired => "handle_expired",
            ReasonCode::HandleRevoked => "handle_revoked",
            ReasonCode::HandleReset => "handle_reset",
            ReasonCode::WorkerReplaced => "worker_replaced",
            ReasonCode::DeliveryFailed => "delivery_failed",
            ReasonCode::RelayRestarted => "relay_restarted",
            ReasonCode::ProtocolReset => "protocol_reset",
        }
    }

    /// Whether the relay closes the connection after it answered this reason
    /// (WSK-FR-OGLM).
    ///
    /// A delivery failure is recoverable: the sender learns that one frame was
    /// not delivered and keeps its connection. Every other reason names a
    /// protocol error or a lost route, and the connection goes with it.
    pub fn closes_the_connection(self) -> bool {
        !matches!(self, ReasonCode::DeliveryFailed)
    }

    /// The reason of an invalidated handle, as a frame reads it back.
    pub fn parse(text: &str) -> Option<ReasonCode> {
        [
            ReasonCode::UnsupportedVersion,
            ReasonCode::ProtocolMismatch,
            ReasonCode::InvalidFrameShape,
            ReasonCode::MissingWorkerAuthentication,
            ReasonCode::InvalidWorkerAuthentication,
            ReasonCode::WorkerNotFound,
            ReasonCode::RegistrationNotFound,
            ReasonCode::HandleNotFound,
            ReasonCode::HandleExpired,
            ReasonCode::HandleRevoked,
            ReasonCode::HandleReset,
            ReasonCode::WorkerReplaced,
            ReasonCode::DeliveryFailed,
            ReasonCode::RelayRestarted,
            ReasonCode::ProtocolReset,
        ]
        .into_iter()
        .find(|reason| reason.as_str() == text)
    }
}

/// Why a handle is no longer usable (RSN-FR-FKRE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidReason {
    /// The IDE revoked the handle (RSN-FR-DWLS).
    Revoked,
    /// The IDE reported the handle as expired (RSN-FR-DWLS).
    Expired,
    /// A protocol failure reset the session (RSN-FR-SGQX).
    Reset,
    /// The registration the handle belongs to was rejected (RSN-FR-HSNA).
    Rejected,
}

impl InvalidReason {
    /// The reason a refused frame carries (RSN-FR-KVBO, RSN-FR-ADXL).
    ///
    /// A rejected registration leaves a handle that was never eligible, so a
    /// frame that presents it is answered as a handle the relay does not hold.
    pub fn reason_code(self) -> ReasonCode {
        match self {
            InvalidReason::Revoked => ReasonCode::HandleRevoked,
            InvalidReason::Expired => ReasonCode::HandleExpired,
            InvalidReason::Reset => ReasonCode::HandleReset,
            InvalidReason::Rejected => ReasonCode::HandleNotFound,
        }
    }
}

/// The lifecycle state of one handle (RSN-FR-FKRE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "reason")]
pub enum HandleState {
    /// The registration is routed and unresolved.
    Pending,
    /// The registration was accepted, so the handle may attach (RSN-FR-HSNA).
    Active,
    /// Revoked, expired, or reset. Every later frame is refused.
    Invalid(InvalidReason),
}

impl HandleState {
    /// The refusal a frame that presents this handle carries, or nothing when
    /// the handle may be used.
    pub fn refusal(self) -> Option<ReasonCode> {
        match self {
            HandleState::Active => None,
            // A pending handle has not been accepted yet, so it names no
            // session the client endpoint can route (RSN-FR-HSNA).
            HandleState::Pending => Some(ReasonCode::HandleNotFound),
            HandleState::Invalid(reason) => Some(reason.reason_code()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // WSK-FR-BWZT: the vocabulary is exactly the fifteen reasons, and each one
    // reads back from its own identifier.
    #[test]
    fn every_reason_round_trips_through_its_identifier() {
        for reason in [
            ReasonCode::UnsupportedVersion,
            ReasonCode::ProtocolMismatch,
            ReasonCode::InvalidFrameShape,
            ReasonCode::MissingWorkerAuthentication,
            ReasonCode::InvalidWorkerAuthentication,
            ReasonCode::WorkerNotFound,
            ReasonCode::RegistrationNotFound,
            ReasonCode::HandleNotFound,
            ReasonCode::HandleExpired,
            ReasonCode::HandleRevoked,
            ReasonCode::HandleReset,
            ReasonCode::WorkerReplaced,
            ReasonCode::DeliveryFailed,
            ReasonCode::RelayRestarted,
            ReasonCode::ProtocolReset,
        ] {
            assert_eq!(ReasonCode::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(ReasonCode::parse("something_else"), None);
    }

    // WSK-FR-OGLM, RSN-FR-HZTV: a delivery failure keeps the connection, and
    // every other reason closes it.
    #[test]
    fn a_delivery_failure_is_the_one_recoverable_reason() {
        assert!(!ReasonCode::DeliveryFailed.closes_the_connection());
        assert!(ReasonCode::ProtocolReset.closes_the_connection());
        assert!(ReasonCode::InvalidFrameShape.closes_the_connection());
    }

    // RSN-FR-FKRE, RSN-FR-KVBO, RSN-FR-ADXL: an invalid handle is refused with
    // the reason it was invalidated for, and a pending one is not yet a session.
    #[test]
    fn each_handle_state_carries_its_own_refusal() {
        assert_eq!(HandleState::Active.refusal(), None);
        assert_eq!(
            HandleState::Pending.refusal(),
            Some(ReasonCode::HandleNotFound)
        );
        assert_eq!(
            HandleState::Invalid(InvalidReason::Revoked).refusal(),
            Some(ReasonCode::HandleRevoked)
        );
        assert_eq!(
            HandleState::Invalid(InvalidReason::Expired).refusal(),
            Some(ReasonCode::HandleExpired)
        );
        assert_eq!(
            HandleState::Invalid(InvalidReason::Reset).refusal(),
            Some(ReasonCode::HandleReset)
        );
        assert_eq!(
            HandleState::Invalid(InvalidReason::Rejected).refusal(),
            Some(ReasonCode::HandleNotFound)
        );
    }
}
