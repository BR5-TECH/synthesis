//! GitHub hosts (GTS-FR-BJCN, GTS-FR-VRYL, GTS-FR-PDWB, GTS-FR-OBAS).
//!
//! The host rules are pure, so most of these tests need no store at all.

use super::*;
use crate::github_tokens::host::{
    github_api_base, github_api_host, github_graphql_url, github_web_base, https_remote_host,
    is_github_family_host, remote_host, stored_host, token_creation_url,
};

// GTS-FR-BJCN
#[test]
fn an_empty_or_blank_host_means_github_com() {
    for input in ["", "   ", "\t\n", "https://", "/"] {
        assert_eq!(normalize_host(input).unwrap(), "github.com", "{input:?}");
    }
}

// GTS-FR-BJCN
#[test]
fn a_host_is_trimmed_lowercased_and_stripped_of_scheme_path_and_slash() {
    for (input, expected) in [
        ("GitHub.com", "github.com"),
        ("  Company.GHE.com  ", "company.ghe.com"),
        ("https://company.ghe.com", "company.ghe.com"),
        ("HTTP://company.ghe.com/", "company.ghe.com"),
        ("https://company.ghe.com/org/repo?tab=x#top", "company.ghe.com"),
        ("git.corp.example/", "git.corp.example"),
        ("localhost", "localhost"),
        ("10.0.0.7", "10.0.0.7"),
    ] {
        assert_eq!(normalize_host(input).unwrap(), expected, "{input:?}");
    }
}

// GTS-FR-BJCN
#[test]
fn a_value_that_is_not_a_host_is_refused_as_invalid_host() {
    for input in [
        "company ghe.com",
        "com pany.ghe.com",
        "company.ghe.com:8443",
        "user@company.ghe.com",
        "user:pw@company.ghe.com",
        "-bad.example.com",
        "bad-.example.com",
        "bad..example.com",
        ".example.com",
        "under_score.example.com",
        "[::1]",
        "ftp@://x",
        "://x",
    ] {
        assert_eq!(normalize_host(input).unwrap_err(), ERR_INVALID_HOST, "{input:?}");
    }
    let too_long = format!("{}.example.com", "a".repeat(64));
    assert_eq!(normalize_host(&too_long).unwrap_err(), ERR_INVALID_HOST);
}

// GTS-FR-PDWB
#[test]
fn the_api_base_follows_the_three_rules() {
    assert_eq!(github_api_base("github.com"), "https://api.github.com");
    assert_eq!(github_api_base("company.ghe.com"), "https://api.company.ghe.com");
    assert_eq!(github_api_base("a.b.ghe.com"), "https://api.a.b.ghe.com");
    assert_eq!(github_api_base("git.corp.example"), "https://git.corp.example/api/v3");
    // `ghe.com` alone is not a `<sub>.ghe.com` host.
    assert_eq!(github_api_base("ghe.com"), "https://ghe.com/api/v3");
}

// GTS-FR-PDWB
#[test]
fn the_graphql_url_follows_the_three_rules() {
    assert_eq!(github_graphql_url("github.com"), "https://api.github.com/graphql");
    assert_eq!(github_graphql_url("company.ghe.com"), "https://api.company.ghe.com/graphql");
    assert_eq!(github_graphql_url("git.corp.example"), "https://git.corp.example/api/graphql");
}

// GTS-FR-PDWB, GTS-FR-12
#[test]
fn the_token_creation_page_is_on_the_host_with_the_scopes_pre_selected() {
    for host in ["github.com", "company.ghe.com", "git.corp.example"] {
        let url = token_creation_url(host);
        assert!(url.starts_with(&format!("{}/settings/tokens/new?", github_web_base(host))), "{url}");
        assert!(url.contains("scopes=repo,workflow"), "{url}");
    }
    assert!(token_creation_url("github.com").starts_with("https://github.com/"));
}

// GTS-FR-PDWB
#[test]
fn the_certificate_check_names_the_host_of_the_api() {
    assert_eq!(github_api_host("github.com"), "api.github.com");
    assert_eq!(github_api_host("company.ghe.com"), "api.company.ghe.com");
    assert_eq!(github_api_host("git.corp.example"), "git.corp.example");
}

// GTS-FR-VRYL
#[test]
fn a_record_stored_without_a_host_reads_as_github_com() {
    let decoded: GithubTokenRecord = serde_json::from_str(r#"{"id":"a","label":"work"}"#).unwrap();
    assert_eq!(decoded.host, "github.com");
    assert_eq!(decoded.effective_host(), "github.com");
    assert_eq!(stored_host(""), "github.com");
    assert_eq!(GithubTokenRecord::default().host, "github.com");
    let with_host: GithubTokenRecord =
        serde_json::from_str(r#"{"id":"a","host":"Company.ghe.com"}"#).unwrap();
    assert_eq!(with_host.effective_host(), "company.ghe.com");
}

// GTS-FR-VRYL, GSS-FR-22
#[test]
fn a_registry_without_hosts_loads_through_the_store_as_github_com() {
    let store = GlobalSettingsStore::in_memory();
    store.save_github_token_registry(vec![record("a", "work")]).unwrap();
    let loaded = store.load_github_token_registry().unwrap();
    assert_eq!(loaded[0].effective_host(), "github.com");
}

// GTC-FR-FSLC
#[test]
fn only_github_com_and_ghe_com_subdomains_are_github_without_a_token_naming_them() {
    assert!(is_github_family_host("github.com"));
    assert!(is_github_family_host("company.ghe.com"));
    assert!(!is_github_family_host("ghe.com"));
    assert!(!is_github_family_host("gitlab.com"));
    assert!(!is_github_family_host("github.com.evil.example"));
    assert!(is_github_host("git.corp.example", &["git.corp.example".to_string()]));
    assert!(!is_github_host("git.corp.example", &["other.example".to_string()]));
}

// GTC-FR-FSLC
#[test]
fn a_remote_host_is_read_from_the_https_ssh_and_scp_forms() {
    for (url, expected) in [
        ("https://github.com/acme/x.git", "github.com"),
        ("https://user:pw@Company.GHE.com:8443/acme/x", "company.ghe.com"),
        ("ssh://git@company.ghe.com/acme/x.git", "company.ghe.com"),
        ("git@git.corp.example:acme/x.git", "git.corp.example"),
    ] {
        assert_eq!(remote_host(url).as_deref(), Some(expected), "{url}");
    }
    for url in ["", "/dev/acme", "file:///dev/acme"] {
        assert_eq!(remote_host(url), None, "{url}");
    }
    assert_eq!(https_remote_host("git@github.com:acme/x.git"), None);
    assert_eq!(https_remote_host("https://github.com@evil.example/x").as_deref(), Some("evil.example"));
}

fn token_on(host: &str, verifier: FakeVerifier) -> (Harness, GithubTokenRecord) {
    let h = harness(verifier);
    let added = add_token_impl(&h.store, &h.tokens, "work", "ghp_secret_1234", host).unwrap();
    (h, added)
}

// GTS-FR-BJCN, GTS-FR-04
#[test]
fn adding_a_token_stores_the_normalized_host_and_verifies_against_it() {
    use std::sync::Mutex as StdMutex;
    struct Recording(StdMutex<Vec<String>>);
    impl GithubVerifier for Recording {
        fn verify(&self, host: &str, _secret: &str) -> Result<VerifiedIdentity, VerifyError> {
            self.0.lock().unwrap().push(host.to_string());
            Ok(VerifiedIdentity { login: "raver119".into(), ..Default::default() })
        }
    }
    let seen = Arc::new(Recording(StdMutex::new(Vec::new())));
    struct Shared(Arc<Recording>);
    impl GithubVerifier for Shared {
        fn verify(&self, host: &str, secret: &str) -> Result<VerifiedIdentity, VerifyError> {
            self.0.verify(host, secret)
        }
    }
    let store = GlobalSettingsStore::in_memory();
    let tokens = GithubTokens::new(Box::new(FakeSecrets::default()), Box::new(Shared(seen.clone())));

    let record = add_token_impl(&store, &tokens, "work", "ghp_secret_1234", " HTTPS://Company.GHE.com/ ").unwrap();
    assert_eq!(record.host, "company.ghe.com");
    let default = add_token_impl(&store, &tokens, "personal", "ghp_secret_5678", "").unwrap();
    assert_eq!(default.host, "github.com");

    assert_eq!(*seen.0.lock().unwrap(), vec!["company.ghe.com", "github.com"]);
    let persisted = store.load_github_token_registry().unwrap();
    assert_eq!(persisted[0].host, "company.ghe.com");
}

// GTS-FR-BJCN
#[test]
fn an_invalid_host_stores_nothing_and_makes_no_request() {
    let Harness { store, tokens, secrets, .. } =
        harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let err = add_token_impl(&store, &tokens, "work", "ghp_secret_1234", "bad host").unwrap_err();
    assert_eq!(err, ERR_INVALID_HOST);
    assert_eq!(secrets.count(), 0);
    assert!(store.load_github_token_registry().unwrap().is_empty());
}

// GTS-FR-07, GTS-FR-VRYL
#[test]
fn validating_a_token_verifies_against_the_host_of_its_record() {
    use std::sync::Mutex as StdMutex;
    struct Recording(Arc<StdMutex<Vec<String>>>);
    impl GithubVerifier for Recording {
        fn verify(&self, host: &str, _secret: &str) -> Result<VerifiedIdentity, VerifyError> {
            self.0.lock().unwrap().push(host.to_string());
            Ok(VerifiedIdentity { login: "raver119".into(), ..Default::default() })
        }
    }
    let seen = Arc::new(StdMutex::new(Vec::new()));
    let store = GlobalSettingsStore::in_memory();
    let tokens = GithubTokens::new(Box::new(FakeSecrets::default()), Box::new(Recording(seen.clone())));
    let ghe = add_token_impl(&store, &tokens, "ghe", "ghp_secret_1234", "company.ghe.com").unwrap();
    // A record stored before hosts existed has none.
    let mut records = store.load_github_token_registry().unwrap();
    let mut legacy = records[0].clone();
    legacy.id = "legacy".into();
    legacy.label = "legacy".into();
    legacy.host = String::new();
    records.push(legacy);
    store.save_github_token_registry(records).unwrap();
    tokens.secrets.set("legacy", "ghp_secret_9999").unwrap();
    seen.lock().unwrap().clear();

    validate_token_impl(&store, &tokens, &ghe.id).unwrap();
    validate_token_impl(&store, &tokens, "legacy").unwrap();

    assert_eq!(*seen.lock().unwrap(), vec!["company.ghe.com", "github.com"]);
}

// GTS-FR-OBAS, GTS-FR-13
#[test]
fn a_token_is_resolved_only_for_a_remote_on_its_own_host() {
    let (h, _) = token_on("company.ghe.com", FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    assert_eq!(
        resolve_github_token_secret(&h.store, &h.tokens, "/dev/acme", "company.ghe.com").unwrap(),
        "ghp_secret_1234"
    );
    assert_eq!(
        resolve_github_token_secret(&h.store, &h.tokens, "/dev/acme", "COMPANY.ghe.com").unwrap(),
        "ghp_secret_1234"
    );
    assert_eq!(
        resolve_github_token_secret(&h.store, &h.tokens, "/dev/acme", "github.com").unwrap_err(),
        ERR_HOST_MISMATCH
    );
}

// GTS-FR-OBAS
#[test]
fn a_host_mismatch_is_reported_before_the_vault_is_asked_for_the_secret() {
    // A vault that refuses every read: had the secret been read first, the
    // answer would be `keychain_unavailable`.
    let store = GlobalSettingsStore::in_memory();
    let mut stored = record("a", "work");
    stored.host = "company.ghe.com".into();
    store.save_github_token_registry(vec![stored]).unwrap();
    let tokens = GithubTokens::new(Box::new(FakeSecrets::broken()), Box::new(FakeVerifier::rejecting()));
    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme", "github.com").unwrap_err(),
        ERR_HOST_MISMATCH
    );
    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme", "company.ghe.com").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
}

// GTS-FR-OBAS
#[test]
fn the_project_token_reports_its_host_with_its_secret() {
    let (h, _) = token_on("company.ghe.com", FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let token = resolve_project_token(&h.store, &h.tokens, "/dev/acme").unwrap();
    assert_eq!(token.host, "company.ghe.com");
    assert_eq!(token.secret, "ghp_secret_1234");
}

// GTC-FR-09, GTC-FR-FSLC
#[test]
fn a_remote_token_applies_to_github_hosts_only_and_must_match_the_host() {
    let (h, _) = token_on("company.ghe.com", FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let resolve = |url: &str| resolve_remote_token(&h.store, &h.tokens, "/dev/acme", url);

    assert_eq!(resolve("https://company.ghe.com/acme/x.git").unwrap().as_deref(), Some("ghp_secret_1234"));
    // The token is for another host than this remote.
    assert_eq!(resolve("https://github.com/acme/x.git").unwrap_err(), ERR_HOST_MISMATCH);
    assert_eq!(resolve("https://other.ghe.com/acme/x.git").unwrap_err(), ERR_HOST_MISMATCH);
    // Not an HTTPS remote, or not a GitHub host: unaffected.
    assert_eq!(resolve("git@company.ghe.com:acme/x.git").unwrap(), None);
    assert_eq!(resolve("https://gitlab.com/acme/x.git").unwrap(), None);
    assert_eq!(resolve("/dev/acme").unwrap(), None);
}

// GTC-FR-09
#[test]
fn a_host_that_only_a_stored_token_names_is_a_github_host() {
    let (h, _) = token_on("git.corp.example", FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    assert_eq!(
        resolve_remote_token(&h.store, &h.tokens, "/dev/acme", "https://git.corp.example/acme/x.git")
            .unwrap()
            .as_deref(),
        Some("ghp_secret_1234")
    );
    assert_eq!(
        resolve_remote_token(&h.store, &h.tokens, "/dev/acme", "https://git.other.example/acme/x.git").unwrap(),
        None
    );
}
