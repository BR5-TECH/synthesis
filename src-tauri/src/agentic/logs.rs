//! What verification, selection and clearing write to the log
//! (`AIC-agentic-integration-config.md`).

use super::*;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------
//
// Verification is the one operation here that runs another program or leaves the
// machine, and clearing is the one that destroys a credential, so both are worth
// a record in the Logs panel — a Claude Code verification that keeps failing is
// otherwise indistinguishable, from the outside, from one that was never
// attempted.
//
// **Nothing below carries credential material** (AIC-FR-20, LGC-FR-16). No token
// or key reaches a message or a field, not even as a masked hint: the facility
// redacts nothing, and a record travels to the panel, the clipboard, and any
// exported file verbatim. What is logged instead is the *shape* of the
// submission — which vendor, whether a new credential came with it — which is
// what actually distinguishes the interesting cases from each other.

pub(super) fn log_verify_attempt<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    vendor: &str,
    config: &VerifyConfig,
) {
    logging::log_info(
        sink,
        buffer,
        &[Domain::Ai],
        "verifying agentic integration",
        log_fields! {
            "vendor" => vendor,
            // Booleans, not values: which shape arrived is the whole of what a
            // reader needs to tell a new-credential submission from one reusing
            // what is stored, and a boolean cannot leak.
            "hasPath" => config.path.is_some(),
            "hasBaseUrl" => config.base_url.is_some(),
            "newApiKey" => config.api_key.is_some(),
            "newOauthToken" => config.oauth_token.is_some(),
            // AIC-FR-WNQR / AIC-FR-SXVA: the mode, whether a new gateway token
            // came with it, and how many author entries — never a value.
            "authMode" => match config.auth_mode.as_deref() {
                None => "subscription",
                Some(raw) => AuthMode::parse(raw).map_or("unrecognised", |_| raw),
            },
            "newGatewayToken" => config.gateway_token.is_some(),
            "envVars" => config.env_vars.as_ref().map_or(0, Vec::len),
        },
    );
}

pub(super) fn log_verify_outcome<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    vendor: &str,
    result: &Result<AgenticIntegration, String>,
    duration_ms: u64,
) {
    match result {
        Ok(integration) => logging::log_info(
            sink,
            buffer,
            &[Domain::Ai],
            "agentic integration verified",
            log_fields! {
                "vendor" => vendor,
                "state" => format!("{:?}", integration.state),
                "keyState" => format!("{:?}", integration.key_state),
                "version" => integration.version.clone().unwrap_or_else(|| "none".into()),
                "models" => integration.models.len(),
                "authMode" => format!("{:?}", integration.auth_mode),
                "durationMs" => duration_ms,
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai],
            "agentic verification failed",
            log_fields! {
                "vendor" => vendor,
                // One of this module's own typed errors, which are fixed strings
                // chosen here (AIC-FR-22 / AIC-FR-26 / AIC-FR-27). No CLI output
                // and no endpoint response body is carried through: either can
                // echo an argument or a header, and `token_malformed` in
                // particular must never be able to quote the value that provoked
                // it.
                "error" => error.as_str(),
                "durationMs" => duration_ms,
            },
        ),
    }
}

/// A selection this module refused, which the author sees inline beside the row
/// they were changing and a reader of the Logs panel would otherwise not see at
/// all (AIC-FR-10).
///
/// A refusal is the one interesting half of a selection: an accepted one is
/// already legible from the record, while a refused one leaves the record
/// exactly as it was and is invisible from the outside. The value refused is
/// **not** carried — a model identifier the record does not offer is still the
/// author's text, and the typed error names what was wrong with it.
pub(super) fn log_selection_refused<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    vendor: &str,
    selection: &'static str,
    turn_kind: Option<&str>,
    error: &str,
) {
    logging::log_warn(
        sink,
        buffer,
        &[Domain::Ai],
        "agentic selection refused",
        log_fields! {
            "vendor" => vendor,
            "selection" => selection,
            // The kind the caller named, or "default" where it named none.
            // It is an identifier the surface chooses from a closed set rather
            // than anything the author typed, so it is safe to carry even on
            // the `unknown_turn_kind` path — which is the one path where the
            // value is worth having, because it names what was wrong.
            "turnKind" => turn_kind.unwrap_or("default"),
            "error" => error,
        },
    );
}

pub(super) fn log_clear_outcome<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    vendor: &str,
    result: &Result<Vec<AgenticIntegration>, String>,
) {
    match result {
        Ok(_) => logging::log_info(
            sink,
            buffer,
            &[Domain::Ai],
            "agentic integration cleared",
            log_fields! {
                "vendor" => vendor,
                // Whether this clear had a keychain entry to destroy, which is
                // what separates a real revocation from an idempotent no-op.
                "hadCredential" => vendor_descriptor(vendor)
                    .is_some_and(AgenticVendor::holds_credential),
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai],
            "clearing agentic integration failed",
            log_fields! { "vendor" => vendor, "error" => error.as_str() },
        ),
    }
}

