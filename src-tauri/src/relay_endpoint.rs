//! The user-global **relay endpoint** and its validation state.
//!
//! Specification: `specifications/core/GSS-global-settings-storage.md`
//! (GSS-FR-ZKQT, GSS-FR-VMRB, GSS-FR-HPWE, GSS-FR-NLDC, GSS-FR-TXAO), surfaced
//! by `specifications/ui/GLS-global-settings.md` GLS-FR-KVNP through
//! GLS-FR-XDUJ.
//!
//! The application reaches one configured relay, which is where a remote client
//! meets this IDE. This module persists the URL of that relay and the validation
//! state it last earned, and it reaches the relay in one operation alone:
//! `verify_relay_endpoint`, which reads the unauthenticated health response of
//! `specifications/server/BMS-backend-microservice.md` BMS-FR-11.
//!
//! GSS-FR-VMRB: no `SYNTHESIS_SERVER_TOKEN`, no relay credential, and no private
//! key is stored, logged, or sent here. Verification is unauthenticated, so
//! `synthesis.toml` gains no secret by gaining a relay endpoint.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The capabilities a relay must advertise before this application uses it
/// (GSS-FR-HPWE). Each names one specification: `RSN-remote-session.md` and
/// `WSK-websocket.md`.
pub const REQUIRED_CAPABILITIES: [&str; 2] = ["remote_session", "websocket"];

/// How long the one probe may take (the non-functional bound of GSS).
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// The typed failures `verify_relay_endpoint` reports (GSS-FR-HPWE).
pub const ERR_ENDPOINT_EMPTY: &str = "endpoint_empty";
pub const ERR_ENDPOINT_INVALID: &str = "endpoint_invalid";
pub const ERR_SCHEME_UNSUPPORTED: &str = "scheme_unsupported";
pub const ERR_UNREACHABLE: &str = "unreachable";
pub const ERR_TIMED_OUT: &str = "timed_out";
pub const ERR_NOT_A_RELAY: &str = "not_a_relay";
pub const ERR_CAPABILITY_MISSING: &str = "capability_missing";

// ---------------------------------------------------------------------------
// The outbound shapes (GSS contract surface)
// ---------------------------------------------------------------------------

/// Whether the URL standing in the store has verified (GSS-FR-NLDC).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayEndpointState {
    /// No URL is stored.
    #[default]
    Unset,
    /// A URL is stored, and no success is bound to it.
    Unverified,
    /// The stored URL equals the one the last success used.
    Verified,
}

/// What `load_relay_endpoint`, `save_relay_endpoint`, and a successful
/// `verify_relay_endpoint` all return.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RelayEndpoint {
    pub url: Option<String>,
    pub state: RelayEndpointState,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub verified_at: Option<String>,
}

// ---------------------------------------------------------------------------
// The persisted record (GSS-FR-ZKQT, GSS-FR-NLDC)
// ---------------------------------------------------------------------------

/// The success a verification earned, together with **the URL it was earned
/// against** (GSS-FR-NLDC).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayVerification {
    pub url: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub verified_at: String,
}

/// The relay endpoint as it sits in `synthesis.toml` (GSS-FR-ZKQT).
///
/// The scalar is declared before the one sub-table, because TOML requires a
/// table's scalar values to be emitted before its sub-tables and serde
/// serialises in declaration order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayEndpointRecord {
    /// The URL the author gave, or `""` while none has been given.
    pub url: String,
    /// `None` until something has verified. Declared last: it is the record's
    /// only sub-table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<RelayVerification>,
}

impl RelayEndpointRecord {
    /// Writes a URL and **drops the stored verification** (GSS-FR-NLDC).
    ///
    /// The success is dropped rather than merely stopped from matching, so a URL
    /// changed away and back reads unverified and asks for Verify again: what a
    /// success describes is a relay that answered at the moment it was asked.
    pub fn apply(&mut self, url: &str) {
        let url = url.trim().to_string();
        if url != self.url {
            self.verified = None;
        }
        self.url = url;
    }

    /// Records a success against the URL that earned it (GSS-FR-NLDC).
    pub fn record_success(&mut self, url: &str, version: &str, capabilities: &[String], at: &str) {
        let url = url.trim().to_string();
        self.url = url.clone();
        self.verified = Some(RelayVerification {
            url,
            version: version.to_string(),
            capabilities: capabilities.to_vec(),
            verified_at: at.to_string(),
        });
    }

    /// The record as the frontend reads it (GSS-FR-NLDC).
    ///
    /// `verified` only while the stored URL still equals the bound one, so a
    /// record whose URL was edited by another writer reads unverified without
    /// anything having to clear a flag.
    pub fn outbound(&self) -> RelayEndpoint {
        let url = self.url.trim();
        if url.is_empty() {
            return RelayEndpoint::default();
        }
        match &self.verified {
            Some(success) if success.url == url => RelayEndpoint {
                url: Some(url.to_string()),
                state: RelayEndpointState::Verified,
                version: Some(success.version.clone()),
                capabilities: success.capabilities.clone(),
                verified_at: Some(success.verified_at.clone()),
            },
            _ => RelayEndpoint {
                url: Some(url.to_string()),
                state: RelayEndpointState::Unverified,
                version: None,
                capabilities: Vec::new(),
                verified_at: None,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Validation and the probe (GSS-FR-HPWE)
// ---------------------------------------------------------------------------

/// The health answer a relay gives (`BMS-backend-microservice.md` BMS-FR-11).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct RelayHealth {
    pub version: String,
    pub capabilities: Vec<String>,
}

/// Reads the health route of one relay. A seam, so verification is tested
/// without a network.
pub trait RelayProbe: Send + Sync {
    fn health(&self, health_url: &str) -> Result<RelayHealth, String>;
}

/// The URL of the health route, or the refusal the URL itself earns
/// (GSS-FR-HPWE).
///
/// Pure: it parses the URL the author typed and reaches nothing.
pub fn health_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err(ERR_ENDPOINT_EMPTY.to_string());
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(ERR_ENDPOINT_INVALID.to_string());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(ERR_SCHEME_UNSUPPORTED.to_string());
    }
    // The authority is everything up to the first path separator, and a URL
    // that names none is not one a relay is reached at.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains(' ') {
        return Err(ERR_ENDPOINT_INVALID.to_string());
    }
    // A URL that carries credentials is refused rather than sent: the transport
    // token is runtime configuration and never reaches this store (GSS-FR-VMRB).
    if authority.contains('@') {
        return Err(ERR_ENDPOINT_INVALID.to_string());
    }
    // The base path is kept, so a relay behind a path prefix is verified where
    // it actually answers rather than at the origin above it. A query and a
    // fragment name no part of a route, so both are dropped.
    let base = rest[authority.len()..]
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    Ok(format!("{scheme}://{authority}{base}/v1/health"))
}

/// Whether an answer advertises everything this application needs
/// (GSS-FR-HPWE).
pub fn missing_capability(capabilities: &[String]) -> Option<&'static str> {
    REQUIRED_CAPABILITIES
        .into_iter()
        .find(|required| !capabilities.iter().any(|held| held == required))
}

/// The production probe: one unauthenticated `GET <url>/v1/health` under a
/// bounded timeout, carrying no credential of any kind.
pub struct HttpRelayProbe;

impl RelayProbe for HttpRelayProbe {
    fn health(&self, health_url: &str) -> Result<RelayHealth, String> {
        let agent: ureq::Agent = crate::tls::ureq_config()
            .timeout_global(Some(VERIFY_TIMEOUT))
            .build()
            .into();

        let mut response = match agent
            .get(health_url)
            .header("User-Agent", "synthesis")
            .call()
        {
            Ok(response) => response,
            // A relay that answers with a status is reachable but is not
            // answering the health contract.
            Err(ureq::Error::StatusCode(_)) => return Err(ERR_NOT_A_RELAY.to_string()),
            Err(ureq::Error::Timeout(_)) => return Err(ERR_TIMED_OUT.to_string()),
            // AAP-FR-LRTC: a refused certificate is its own typed error.
            Err(e) => {
                return Err(crate::tls::ureq_wire(&e, health_url)
                    .unwrap_or_else(|| ERR_UNREACHABLE.to_string()))
            }
        };

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|_| ERR_UNREACHABLE.to_string())?;
        parse_health(&body)
    }
}

/// Reads the health body, or reports that the answer is not a relay's.
pub fn parse_health(body: &str) -> Result<RelayHealth, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| ERR_NOT_A_RELAY.to_string())?;
    let version = value
        .get("version")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ERR_NOT_A_RELAY.to_string())?;
    let capabilities = value
        .get("capabilities")
        .and_then(|value| value.as_array())
        .ok_or_else(|| ERR_NOT_A_RELAY.to_string())?
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    Ok(RelayHealth {
        version: version.to_string(),
        capabilities,
    })
}

/// The seam the verification runs through, held as Tauri state.
pub struct RelayEndpointSeams {
    pub probe: Arc<dyn RelayProbe>,
}

impl Default for RelayEndpointSeams {
    fn default() -> Self {
        RelayEndpointSeams {
            probe: Arc::new(HttpRelayProbe),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (GSS contract surface)
// ---------------------------------------------------------------------------

use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use tauri::State;

const DOMAINS: [Domain; 2] = [Domain::Backend, Domain::Remote];

/// GSS-FR-ZKQT: the stored record. Reads the store and reaches no network, so
/// the Remote connectivity section renders offline and instantly.
#[tauri::command]
pub fn load_relay_endpoint(store: State<'_, GlobalSettingsStore>) -> Result<RelayEndpoint, String> {
    Ok(store.load_relay_endpoint()?.outbound())
}

/// GSS-FR-NLDC: persist the URL, and return the record with the validation
/// state that URL carries.
#[tauri::command]
pub fn save_relay_endpoint(
    url: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<RelayEndpoint, String> {
    save_relay_endpoint_with(&app, &BUFFER, &store, url)
}

pub fn save_relay_endpoint_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &GlobalSettingsStore,
    url: String,
) -> Result<RelayEndpoint, String> {
    let record = store.save_relay_endpoint(&url)?;
    let outbound = record.outbound();
    // The URL is not logged: it names a host the author would not want read
    // back out of an exported log, and its shape is what a reader needs.
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "relay endpoint saved",
        log_fields! {
            "url_set" => !record.url.trim().is_empty(),
            "state" => format!("{:?}", outbound.state)
        },
    );
    Ok(outbound)
}

/// GSS-FR-HPWE: read the relay's unauthenticated health answer, and commit the
/// success only where the relay answered and advertised what is needed.
#[tauri::command]
pub fn verify_relay_endpoint(
    url: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    seams: State<'_, RelayEndpointSeams>,
) -> Result<RelayEndpoint, String> {
    verify_relay_endpoint_with(&app, &BUFFER, &store, seams.probe.as_ref(), url)
}

pub fn verify_relay_endpoint_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &GlobalSettingsStore,
    probe: &dyn RelayProbe,
    url: String,
) -> Result<RelayEndpoint, String> {
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "relay endpoint verification started",
        log_fields! { "url_set" => !url.trim().is_empty() },
    );

    let refuse = |reason: String| -> Result<RelayEndpoint, String> {
        logging::log_warn(
            sink,
            buffer,
            &DOMAINS,
            "relay endpoint verification refused",
            log_fields! { "reason" => reason.as_str() },
        );
        Err(reason)
    };

    let health_route = match health_url(&url) {
        Ok(route) => route,
        Err(reason) => return refuse(reason),
    };

    let answer = match probe.health(&health_route) {
        Ok(answer) => answer,
        Err(reason) => return refuse(reason),
    };

    if let Some(missing) = missing_capability(&answer.capabilities) {
        logging::log_warn(
            sink,
            buffer,
            &DOMAINS,
            "relay endpoint lacks a capability",
            log_fields! { "capability" => missing },
        );
        return Err(ERR_CAPABILITY_MISSING.to_string());
    }

    let at = crate::notes::now_rfc3339();
    let record =
        store.record_relay_verification(url.trim(), &answer.version, &answer.capabilities, &at)?;
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "relay endpoint verified",
        log_fields! {
            "relay_version" => answer.version.as_str(),
            "capabilities" => answer.capabilities.join(",")
        },
    );
    Ok(record.outbound())
}

#[cfg(test)]
mod tests;
