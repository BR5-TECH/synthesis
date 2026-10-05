//! The Custom LLM Gateway's address rules (AAP-FR-KRVT).
//!
//! The gateway root is entered without `/v1`: the model listing route and the
//! Rig adapter's base URL both add it, so a root that already ends in it would
//! build `/v1/v1/...`.

/// The path segment the gateway routes live under.
pub const API_PREFIX: &str = "/v1";

/// AAP-FR-KRVT: whether a normalised URL's path already ends in `/v1`.
pub fn has_v1_suffix(normalized: &str) -> bool {
    let rest = normalized.split_once("://").map_or(normalized, |(_, r)| r);
    let path = rest.find(['/', '?', '#']).map_or("", |i| &rest[i..]);
    let path = path.split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    path.to_ascii_lowercase().ends_with(API_PREFIX)
}

/// AAP-FR-ADPX: the base URL the Rig OpenAI-compatible client is given for a
/// Custom gateway — the normalised root with the `/v1` prefix the client's
/// route paths hang from.
pub fn adapter_base_url(root: &str) -> String {
    format!("{}{}", root.trim_end_matches('/'), API_PREFIX)
}
