//! The verification of a token against the API of its host
//! (GTS-FR-04, GTS-FR-07, GTS-FR-16).
//!
//! Split out of the parent module so the registry logic and the network call
//! live apart. The only outbound request this module makes is
//! `GET <api base>/user`, sent to the API base of the host of the token.

use serde::{Deserialize, Serialize};

use super::host::github_api_base;

/// Who a token authenticates as, and what it may do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub login: String,
    /// The account's display name, when GitHub reports one (GTS-FR-16). Absent
    /// rather than substituted for an account that has set none.
    pub display_name: Option<String>,
    /// The account's email, when GitHub reports one (GTS-FR-16). A private email
    /// comes back `null` from `GET /user`, which is absence, not failure.
    pub email: Option<String>,
    pub scopes: Vec<String>,
}

/// The account a comment is attributed to (GTS-FR-16).
///
/// Distinct from `GithubTokenRecord`: this describes a *person*, carries nothing
/// token-derived (not even `masked_hint`), and is what
/// `CMS-comments-storage.md` stamps into an event's `by`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubIdentity {
    pub login: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// The two outcomes a caller must tell apart (GTS-FR-04): GitHub answered and
/// refused the token, versus GitHub never answered.
#[derive(Debug)]
pub enum VerifyError {
    Rejected,
    Unreachable(String),
    /// The TLS check refused GitHub's certificate (AAP-FR-HZTB).
    TlsUntrusted(crate::tls::TlsFailure),
}

pub trait GithubVerifier: Send + Sync {
    /// Ask the API of `host` (normalized) who `secret` is. The secret goes to
    /// that API base and nowhere else.
    fn verify(&self, host: &str, secret: &str) -> Result<VerifiedIdentity, VerifyError>;
}

/// Production verifier: one `GET <api base of the host>/user`.
///
/// This is the only outbound network call this module makes. The global timeout
/// is what keeps a hung network from wedging the add dialog indefinitely — the
/// UI has no way to cancel an in-flight `invoke`.
pub struct HttpGithubVerifier;

impl GithubVerifier for HttpGithubVerifier {
    fn verify(&self, host: &str, secret: &str) -> Result<VerifiedIdentity, VerifyError> {
        let user_api = format!("{}/user", github_api_base(host));
        let agent: ureq::Agent = crate::tls::ureq_config()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .into();

        let response = agent
            .get(&user_api)
            .header("Authorization", &format!("Bearer {secret}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            // GitHub rejects an API request with no User-Agent outright, which
            // would otherwise read as "the token is bad".
            .header("User-Agent", "synthesis")
            .call();

        let mut response = match response {
            Ok(r) => r,
            // A 4xx/5xx arrives here as a status error rather than an `Ok`.
            Err(ureq::Error::StatusCode(code)) => return Err(verify_failure_for_status(code)),
            Err(e) => {
                return Err(match crate::tls::ureq_failure(&e, &user_api) {
                    Some(failure) => VerifyError::TlsUntrusted(failure),
                    None => VerifyError::Unreachable(e.to_string()),
                })
            }
        };

        // The granted scopes ride on a response header. A fine-grained token
        // carries no such header at all, which is an empty scope list rather
        // than a failure.
        let scopes = response
            .headers()
            .get("x-oauth-scopes")
            .and_then(|v| v.to_str().ok())
            .map(parse_scopes)
            .unwrap_or_default();

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| VerifyError::Unreachable(e.to_string()))?;
        let account = account_from_user_payload(&body).ok_or_else(|| {
            VerifyError::Unreachable("GitHub returned an unrecognised response".into())
        })?;

        Ok(VerifiedIdentity {
            login: account.login,
            display_name: account.display_name,
            email: account.email,
            scopes,
        })
    }
}

/// GTS-FR-04: which HTTP statuses mean "GitHub refused this token" and which
/// mean "GitHub did not answer usefully".
///
/// Pure so the distinction the whole add flow rests on is testable without a
/// network: only 401 and 403 are evidence *about the token*. Anything else —
/// a 404, a 500, a captive-portal redirect — says something about the request
/// or the network, and calling it `Rejected` would tell the author their good
/// token is bad.
pub fn verify_failure_for_status(code: u16) -> VerifyError {
    match code {
        401 | 403 => VerifyError::Rejected,
        _ => VerifyError::Unreachable(format!("GitHub answered {code}")),
    }
}

/// Extract the account login from a `GET /user` payload. `None` for anything
/// that is not the JSON object this module expects.
pub fn login_from_user_payload(body: &str) -> Option<String> {
    account_from_user_payload(body).map(|a| a.login)
}

/// GTS-FR-16: extract the whole describable account — login, display name, email
/// — from a `GET /user` payload.
///
/// Only `login` is load-bearing: GitHub always returns it for an authenticated
/// user, and a payload without one is not a response this module understands.
/// `name` and `email` are both nullable and both commonly null (an account that
/// set no name; an account keeping its email private), so each maps to `None`
/// rather than making the parse fail. An empty string is normalised to `None`
/// too — a blank display name is absence wearing a different shape, and letting
/// it through would render a comment attributed to nobody.
pub fn account_from_user_payload(body: &str) -> Option<GithubIdentity> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let login = value.get("login")?.as_str()?.to_string();
    let field = |key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    };
    Some(GithubIdentity {
        login,
        display_name: field("name"),
        email: field("email"),
    })
}

/// Split the `x-oauth-scopes` header into its scope list, dropping the empty
/// entries a trailing comma or an empty header would otherwise produce.
pub fn parse_scopes(header: &str) -> Vec<String> {
    header
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
