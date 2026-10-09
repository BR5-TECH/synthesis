//! The typed error of a failed probe (AAP-FR-05, AAP-FR-HZTB).

use super::*;

/// Map a probe failure onto this module's typed error vocabulary (AAP-FR-05).
///
/// AAP-FR-HZTB: a refused certificate gives `tls_untrusted:<cause>:<host>`. It
/// is never `unreachable`.
pub(super) fn api_probe_error(e: ProbeError) -> String {
    match e {
        ProbeError::Unreachable(_) => ERR_UNREACHABLE.to_string(),
        ProbeError::Rejected => ERR_REJECTED.to_string(),
        ProbeError::NotExpectedKind | ProbeError::Status(_) => ERR_NOT_AN_AI_ENDPOINT.to_string(),
        ProbeError::TimedOut => ERR_TIMED_OUT.to_string(),
        ProbeError::TlsUntrusted(failure) => failure.wire(),
    }
}
