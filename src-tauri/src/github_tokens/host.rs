//! GitHub hosts — `specifications/core/GTS-github-token-storage.md`
//! (GTS-FR-BJCN, GTS-FR-VRYL, GTS-FR-PDWB).
//!
//! A token belongs to one host. The author gives only the host; every address
//! this application sends a request to is derived from it here, by fixed rules.
//! Everything in this file is pure, so the rules are testable without a network.

/// The host of a token that names none, and of every record stored before hosts
/// existed (GTS-FR-VRYL).
pub const DEFAULT_HOST: &str = "github.com";

/// The serde default of a stored `repository_host` field (GTS-FR-VRYL).
pub fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

/// The text a host must be written in, or `invalid_host` follows (GTS-FR-BJCN).
pub const ERR_INVALID_HOST: &str = "invalid_host";

/// The token of the project belongs to another host than the remote
/// (GTS-FR-OBAS). No request is made.
pub const ERR_HOST_MISMATCH: &str = "github_host_mismatch";

const GHE_SUFFIX: &str = ".ghe.com";

/// Longest host name DNS allows.
const MAX_HOST_LEN: usize = 253;

/// Longest label of a host name DNS allows.
const MAX_LABEL_LEN: usize = 63;

/// GTS-FR-BJCN: the normalized form of a host as the author typed it.
///
/// Trims, lowercases, and removes a scheme, a path, a query, a fragment, and a
/// trailing slash. An empty result is `github.com`. A user, a password, a port,
/// or any character a host name cannot hold is `invalid_host`.
pub fn normalize_host(input: &str) -> Result<String, String> {
    let lowered = input.trim().to_ascii_lowercase();
    let without_scheme = match lowered.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && scheme.chars().all(is_scheme_char) => rest,
        Some(_) => return Err(ERR_INVALID_HOST.to_string()),
        None => lowered.as_str(),
    };
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return Ok(DEFAULT_HOST.to_string());
    }
    if is_valid_host(authority) {
        Ok(authority.to_string())
    } else {
        Err(ERR_INVALID_HOST.to_string())
    }
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// Is `host` a dotted sequence of DNS labels? A port, a user, and an address in
/// brackets are not part of one.
fn is_valid_host(host: &str) -> bool {
    if host.len() > MAX_HOST_LEN {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= MAX_LABEL_LEN
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

/// GTS-FR-VRYL: the host a stored record reads as. A record stored without one
/// reads as `github.com`.
pub fn stored_host(stored: &str) -> String {
    match stored.trim() {
        "" => DEFAULT_HOST.to_string(),
        host => host.to_ascii_lowercase(),
    }
}

/// The part of a `<sub>.ghe.com` host before `.ghe.com`, or `None` for any other
/// host (GTS-FR-PDWB).
fn ghe_subdomain(host: &str) -> Option<&str> {
    host.strip_suffix(GHE_SUFFIX).filter(|sub| !sub.is_empty())
}

/// Hosts that are GitHub without any token naming them: `github.com` and every
/// `*.ghe.com` (GTC-FR-FSLC).
pub fn is_github_family_host(host: &str) -> bool {
    host == DEFAULT_HOST || ghe_subdomain(host).is_some()
}

/// GTS-FR-PDWB: the REST API base of `host`, without a trailing slash.
pub fn github_api_base(host: &str) -> String {
    if host == DEFAULT_HOST {
        "https://api.github.com".to_string()
    } else if let Some(sub) = ghe_subdomain(host) {
        format!("https://api.{sub}{GHE_SUFFIX}")
    } else {
        format!("https://{host}/api/v3")
    }
}

/// GTS-FR-PDWB: the GraphQL URL of `host`.
pub fn github_graphql_url(host: &str) -> String {
    if host == DEFAULT_HOST {
        "https://api.github.com/graphql".to_string()
    } else if let Some(sub) = ghe_subdomain(host) {
        format!("https://api.{sub}{GHE_SUFFIX}/graphql")
    } else {
        format!("https://{host}/api/graphql")
    }
}

/// The host name of the REST API of `host`, the name a certificate check is made
/// against (AAP-FR-LRTC).
pub fn github_api_host(host: &str) -> String {
    github_api_base(host)
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The address of the web pages of `host`, without a trailing slash.
pub fn github_web_base(host: &str) -> String {
    format!("https://{host}")
}

/// GTS-FR-12 / GTS-FR-PDWB: the token-creation page of `host` with the scopes
/// this application needs already selected.
///
/// `repo` covers pull-request work and HTTPS push and pull; `workflow` is needed
/// because a push that touches `.github/workflows/**` is refused without it.
pub fn token_creation_url(host: &str) -> String {
    format!(
        "{}/settings/tokens/new?scopes=repo,workflow&description=Synthesis",
        github_web_base(host)
    )
}

/// A record with no stored host reads as `github.com` (GTS-FR-VRYL), so the
/// default host is not the empty string.
impl Default for super::GithubTokenRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            host: DEFAULT_HOST.to_string(),
            account_login: None,
            account_display_name: None,
            account_email: None,
            scopes: Vec::new(),
            masked_hint: String::new(),
            added_at: String::new(),
            last_verified_at: None,
            state: super::TokenState::default(),
        }
    }
}

/// The lowercase host of a Git remote URL, in the HTTPS, `ssh://`, and scp-like
/// forms, with any user, password, and port removed. `None` for a local path or
/// anything else that names no host.
pub fn remote_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let authority = if let Some((_, rest)) = trimmed.split_once("://") {
        rest.split('/').next().unwrap_or_default()
    } else if let Some((user_host, _)) = trimmed.split_once(':') {
        // `git@github.com:owner/repo.git` — no scheme, a colon before the path.
        if user_host.contains('/') {
            return None;
        }
        user_host
    } else {
        return None;
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split_once(':').map_or(host, |(h, _)| h).to_ascii_lowercase();
    is_valid_host(&host).then_some(host)
}

/// The lowercase host of an `https://` remote URL, or `None` for any other form.
/// Only an HTTPS remote authenticates with a token (GTC-FR-09).
pub fn https_remote_host(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://")?;
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split_once(':').map_or(host, |(h, _)| h).to_ascii_lowercase();
    is_valid_host(&host).then_some(host)
}
