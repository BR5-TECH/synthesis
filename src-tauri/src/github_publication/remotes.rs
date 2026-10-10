//! Remote enumeration, canonicalization, and eligibility
//! (GHP-FR-WKDE, GHP-FR-BXTU, GHP-FR-MZPR, GHP-FR-LTAC, GHP-FR-HVQG,
//! GHP-FR-XAUP, GHP-FR-NDSB, GHP-FR-ZRFP).
//!
//! Nothing here mutates anything on GitHub. Classification is pure; the
//! eligibility check is one read of the repository record.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::client::{GithubIssues, ProbeOutcome};
use super::records::*;
use crate::project_settings::PublicationRemoteSelection;

/// How long one repository's eligibility answer stands before it is asked for
/// again.
///
/// Eligibility is one HTTP read per GitHub remote, and it is asked for by the
/// New Artifact tab, by the Draft Information modal, and again on every
/// `"draft publication changed"` — which one publication emits several times.
/// Without this, one publication costs a burst of identical requests, and a
/// project with several GitHub remotes pays it per remote. Short enough that a
/// token the author has just fixed is picked up while they are still looking at
/// the tab.
const PROBE_TTL: Duration = Duration::from_secs(60);

type ProbeCache = Mutex<HashMap<String, (Instant, ProbeOutcome)>>;

fn probe_cache() -> &'static ProbeCache {
    static CACHE: OnceLock<ProbeCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The repository's eligibility, from the cache where it is still fresh.
///
/// Keyed by the repository **and** the token, so a project bound to a different
/// token is never answered from another one's result. `cache` false asks the
/// client every time, which is what a test that counts requests needs — the
/// cache is process-wide, and two cases about the same repository would
/// otherwise answer for one another.
fn probe_cached(
    secret: &str,
    host: &str,
    owner: &str,
    repo: &str,
    client: &dyn GithubIssues,
    cache: bool,
) -> ProbeOutcome {
    if !cache {
        return client.probe(secret, owner, repo);
    }
    // The key carries the token's own hash rather than the token: a cache key
    // is an in-memory value, but it is one more place a secret would otherwise
    // sit (GHP-FR-DHXK).
    use std::hash::{BuildHasher, Hasher};
    static SEED: OnceLock<std::collections::hash_map::RandomState> = OnceLock::new();
    let mut hasher = SEED.get_or_init(std::collections::hash_map::RandomState::new).build_hasher();
    hasher.write(secret.as_bytes());
    let key = format!("{:016x}/{host}/{owner}/{repo}", hasher.finish());

    if let Ok(cache) = probe_cache().lock() {
        if let Some((at, outcome)) = cache.get(&key) {
            if at.elapsed() < PROBE_TTL {
                return *outcome;
            }
        }
    }
    let outcome = client.probe(secret, owner, repo);
    if let Ok(mut cache) = probe_cache().lock() {
        cache.insert(key, (Instant::now(), outcome));
    }
    outcome
}

/// Drop every cached eligibility answer. Called when a publication settles, so
/// the next read reflects a repository the author has just changed.
pub fn forget_probes() {
    if let Ok(mut cache) = probe_cache().lock() {
        cache.clear();
    }
}

/// One configured Git remote as the repository reports it, before any check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredRemote {
    pub name: String,
    pub url: String,
}

/// The host, owner, and repository a Git remote URL names (GHP-FR-BXTU).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteRepository {
    /// Lowercase, without a user, a password, or a port.
    pub host: String,
    pub owner: String,
    pub repo: String,
}

/// GHP-FR-BXTU: the owner and repository a `github.com` URL names, in either
/// the HTTPS or the SSH form, or `None` for anything else.
pub fn parse_github_remote(url: &str) -> Option<(String, String)> {
    parse_remote_repository(url)
        .filter(|parsed| parsed.host == crate::github_tokens::DEFAULT_HOST)
        .map(|parsed| (parsed.owner, parsed.repo))
}

/// GHP-FR-BXTU: the host, owner, and repository a remote URL names, in either
/// the HTTPS or the SSH form, on any host, or `None` for anything else. Whether
/// the host is a GitHub host is decided by the caller
/// (`github_tokens::is_github_host`).
///
/// Canonicalization drops the scheme, the credentials, the port, a trailing
/// `.git`, and a trailing slash, and lowercases the host, so one repository
/// reached by two URL forms canonicalizes to one value. Dropping the
/// credentials is also what keeps an embedded token out of every record, log,
/// and payload this module produces (GHP-FR-DHXK).
pub fn parse_remote_repository(url: &str) -> Option<RemoteRepository> {
    let trimmed = url.trim();
    // `git@github.com:owner/repo.git` — the scp-like SSH form, which carries no
    // scheme and separates the path with a colon.
    let rest = if let Some(rest) = trimmed.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        let after_scheme = trimmed
            .split_once("://")
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_else(|| trimmed.to_string());
        // Userinfo is everything before the last `@` of the authority.
        match after_scheme.split_once('/') {
            Some((authority, path)) => {
                let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
                format!("{host}/{path}")
            }
            None => after_scheme,
        }
    };
    let (authority, path) = rest.split_once('/')?;
    let host = authority.split_once(':').map_or(authority, |(h, _)| h);
    let host = crate::github_tokens::host::remote_host(&format!("https://{host}/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let owner = segments.next()?.to_string();
    let repo = segments.next()?.to_string();
    // A URL with more path than `owner/repo` is not a repository address.
    if segments.next().is_some() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(RemoteRepository { host, owner, repo })
}

/// GHP-FR-BXTU: the canonical form two URLs are compared by.
pub fn canonical_url(url: &str) -> String {
    canonical_url_with(url, |host| crate::github_tokens::host::is_github_family_host(host))
}

/// [`canonical_url`], where `is_github` decides which hosts have the
/// `host/owner/repo` form. The URL of a remote on any other host keeps its own
/// spelling, minus the credentials.
pub fn canonical_url_with(url: &str, is_github: impl Fn(&str) -> bool) -> String {
    match parse_remote_repository(url).filter(|parsed| is_github(&parsed.host)) {
        Some(RemoteRepository { host, owner, repo }) => format!("{host}/{owner}/{repo}"),
        // A non-GitHub remote still needs a stable spelling for the persisted
        // choice to be compared against; the credentials come off it just the
        // same, because a persisted value is written to disk.
        None => strip_credentials(url.trim()).trim_end_matches('/').to_string(),
    }
}

/// Remove any `user:password@` userinfo from a URL's authority.
fn strip_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    match rest.split_once('/') {
        Some((authority, path)) => {
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            format!("{scheme}://{host}/{path}")
        }
        None => {
            let host = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
            format!("{scheme}://{host}")
        }
    }
}

/// GHP-FR-WKDE / GHP-FR-MZPR / GHP-FR-LTAC: classify every configured remote.
///
/// `secret` is the project-resolved token, or `None` where the project resolves
/// none — in which case every GitHub remote is `token_unavailable` and **no
/// request is made at all**. The token is taken to belong to `github.com`; see
/// [`classify_hosted`] for a token of another host.
pub fn classify(
    configured: &[ConfiguredRemote],
    secret: Option<&str>,
    client: &dyn GithubIssues,
) -> Vec<PublicationRemote> {
    classify_with(configured, secret, client, true)
}

/// [`classify`], with the eligibility cache under the caller's control.
pub fn classify_with(
    configured: &[ConfiguredRemote],
    secret: Option<&str>,
    client: &dyn GithubIssues,
    cache: bool,
) -> Vec<PublicationRemote> {
    let token = secret.map(|secret| (secret, crate::github_tokens::DEFAULT_HOST));
    classify_hosted(configured, token, &[], client, cache)
}

/// GHP-FR-BXTU / GHP-FR-MZPR: classify every configured remote against a token
/// that belongs to a host.
///
/// `token` is the secret and the host of the project token. `known_hosts` are
/// the hosts of every stored token; with `github.com`, every `*.ghe.com` host,
/// and the host of the project token, they decide which remotes are GitHub
/// remotes. A GitHub remote on another host than the token is `host_mismatch`,
/// and no request is made for it (GTS-FR-OBAS).
pub fn classify_hosted(
    configured: &[ConfiguredRemote],
    token: Option<(&str, &str)>,
    known_hosts: &[String],
    client: &dyn GithubIssues,
    cache: bool,
) -> Vec<PublicationRemote> {
    let is_github = |host: &str| {
        crate::github_tokens::is_github_host(host, known_hosts)
            || token.is_some_and(|(_, token_host)| token_host == host)
    };
    configured
        .iter()
        .map(|remote| match parse_remote_repository(&remote.url).filter(|p| is_github(&p.host)) {
            None => PublicationRemote {
                name: remote.name.clone(),
                url: canonical_url_with(&remote.url, is_github),
                kind: RemoteKind::Other,
                repository_host: None,
                repository_owner: None,
                repository_name: None,
                eligibility: RemoteEligibility::NotGithub,
                reason: RemoteEligibility::NotGithub.reason().map(str::to_string),
                tls_failure: None,
            },
            Some(RemoteRepository { host, owner, repo }) => {
                let mut tls_failure: Option<crate::tls::TlsFailure> = None;
                let eligibility = match token {
                    None => RemoteEligibility::TokenUnavailable,
                    Some((_, token_host)) if token_host != host => RemoteEligibility::HostMismatch,
                    Some((secret, _)) => {
                        // The client of the host of the remote (GHP-FR-HSTA).
                        let scoped = client.for_host(&host);
                        let client = scoped.as_deref().unwrap_or(client);
                        match probe_cached(secret, &host, &owner, &repo, client, cache) {
                            ProbeOutcome::TlsUntrusted(cause) => {
                                tls_failure = Some(crate::tls::TlsFailure::new(
                                    &crate::github_tokens::github_api_host(&host),
                                    cause,
                                ));
                                RemoteEligibility::TlsUntrusted
                            }
                            ProbeOutcome::Publishable => RemoteEligibility::Eligible,
                            ProbeOutcome::IssuesUnreadable => {
                                RemoteEligibility::IssuesInaccessible
                            }
                            ProbeOutcome::IssuesDisabled => RemoteEligibility::IssuesDisabled,
                            ProbeOutcome::CreateForbidden => {
                                RemoteEligibility::IssuesCreateForbidden
                            }
                        }
                    }
                };
                PublicationRemote {
                    name: remote.name.clone(),
                    url: canonical_url_with(&remote.url, is_github),
                    kind: RemoteKind::Github,
                    repository_host: Some(host),
                    repository_owner: Some(owner),
                    repository_name: Some(repo),
                    eligibility,
                    reason: tls_failure
                        .as_ref()
                        .map(crate::tls::TlsFailure::describe)
                        .or_else(|| eligibility.reason().map(str::to_string)),
                    tls_failure,
                }
            }
        })
        .collect()
}

/// GHP-FR-HVQG / GHP-FR-XAUP / GHP-FR-NDSB: which remote the next attempt uses,
/// and how that choice was reached.
pub fn resolve(
    remotes: Vec<PublicationRemote>,
    persisted: Option<PublicationRemoteSelection>,
) -> PublicationRemoteResolution {
    // GHP-FR-HVQG: a persisted choice applies only where the remote is still
    // enumerated under the same name, its canonicalized URL still matches, and
    // it is still eligible. A rename or a re-point invalidates it for this
    // attempt, and it is not rewritten either way.
    let persisted_selection = persisted.as_ref().and_then(|choice| {
        remotes
            .iter()
            .find(|remote| {
                remote.name == choice.name
                    && remote.url == choice.url
                    && remote.eligibility == RemoteEligibility::Eligible
            })
            .map(|remote| remote.name.clone())
    });

    let (selection, origin) = match persisted_selection {
        Some(name) => (Some(name), SelectionOrigin::Persisted),
        // GHP-FR-XAUP: exactly one configured remote, and it is eligible.
        None if remotes.len() == 1 && remotes[0].eligibility == RemoteEligibility::Eligible => {
            (Some(remotes[0].name.clone()), SelectionOrigin::Automatic)
        }
        // Two or more: the caller chooses, so the picker's initial choice is the
        // first eligible remote and the origin says the choice is this
        // attempt's alone until the author says otherwise.
        None => match remotes.iter().find(|r| r.eligibility == RemoteEligibility::Eligible) {
            Some(remote) if remotes.len() > 1 => {
                (Some(remote.name.clone()), SelectionOrigin::AttemptOnly)
            }
            _ => (None, SelectionOrigin::None),
        },
    };

    PublicationRemoteResolution {
        remotes,
        selection,
        origin,
        persisted_choice: persisted
            .map(|choice| PersistedChoice { name: choice.name, url: choice.url }),
    }
}

/// GHP-FR-ZRFP: which of the refusals a resolution with no selection stands
/// on.
///
/// AAP-FR-LRTC: a refused certificate answers with the full wire text
/// `tls_untrusted:<cause>:<host>`, so the host and the cause reach every
/// surface. Every other refusal is its bare code.
pub fn refusal_for(remotes: &[PublicationRemote]) -> String {
    if remotes.is_empty() {
        return ERR_NO_REMOTE.to_string();
    }
    if remotes.iter().all(|r| r.kind == RemoteKind::Other) {
        return ERR_NO_GITHUB_REMOTE.to_string();
    }
    // GHP-FR-WNJC: name the condition that stops the most remotes. The first
    // three are conditions of the token and hold for every remote at once;
    // `issues_disabled` holds for one repository, so it comes last — an author
    // sent to a repository's settings while their token is the problem fixes
    // nothing.
    let github = remotes.iter().filter(|r| r.kind == RemoteKind::Github);
    let mut inaccessible = false;
    let mut forbidden = false;
    let mut disabled = false;
    let mut token_missing = false;
    let mut host_mismatch = false;
    let mut untrusted: Option<String> = None;
    for remote in github {
        match remote.eligibility {
            RemoteEligibility::TokenUnavailable => token_missing = true,
            RemoteEligibility::HostMismatch => host_mismatch = true,
            RemoteEligibility::TlsUntrusted => {
                untrusted.get_or_insert_with(|| remote.refusal_code());
            }
            RemoteEligibility::IssuesInaccessible => inaccessible = true,
            RemoteEligibility::IssuesCreateForbidden => forbidden = true,
            RemoteEligibility::IssuesDisabled => disabled = true,
            _ => {}
        }
    }
    if host_mismatch {
        ERR_HOST_MISMATCH.to_string()
    } else if token_missing {
        ERR_TOKEN_UNAVAILABLE.to_string()
    } else if let Some(wire) = untrusted {
        wire
    } else if inaccessible {
        ERR_ISSUES_INACCESSIBLE.to_string()
    } else if forbidden {
        ERR_ISSUES_CREATE_FORBIDDEN.to_string()
    } else if disabled {
        ERR_ISSUES_DISABLED.to_string()
    } else {
        ERR_NO_GITHUB_REMOTE.to_string()
    }
}

/// The displayable text a refusal carries.
///
/// Every code an eligibility value stands for is answered by that value's own
/// `reason()`, so the sentence has one source. A code with two spellings could
/// drift, and a missed arm falls through to the text below without a compiler
/// error — which is exactly how a refusal stops naming its own cause.
pub fn refusal_reason(code: &str) -> String {
    // AAP-FR-LRTC: a refused certificate names its host and its cause.
    if let Some(failure) = crate::tls::TlsFailure::parse_wire(code) {
        return failure.describe();
    }
    let eligibility = match code {
        ERR_ISSUES_INACCESSIBLE => Some(RemoteEligibility::IssuesInaccessible),
        ERR_ISSUES_DISABLED => Some(RemoteEligibility::IssuesDisabled),
        ERR_ISSUES_CREATE_FORBIDDEN => Some(RemoteEligibility::IssuesCreateForbidden),
        ERR_TOKEN_UNAVAILABLE => Some(RemoteEligibility::TokenUnavailable),
        ERR_HOST_MISMATCH => Some(RemoteEligibility::HostMismatch),
        crate::tls::ERR_TLS_UNTRUSTED => Some(RemoteEligibility::TlsUntrusted),
        _ => None,
    };
    if let Some(reason) = eligibility.and_then(RemoteEligibility::reason) {
        return reason.to_string();
    }
    // These two are statements about the project rather than about one remote,
    // so no eligibility value spells them. `no_github_remote` in particular is
    // not `RemoteEligibility::NotGithub`: that names one remote which is not a
    // GitHub repository, and this names a project holding no GitHub remote.
    match code {
        ERR_NO_REMOTE => "This project has no configured Git remote.",
        ERR_NO_GITHUB_REMOTE => "This project has no GitHub remote.",
        _ => "Publication is not available.",
    }
    .to_string()
}
