//! The shapes the frontend types against, and the token creation URL.
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- Wire shapes -----------------------------------------------------

#[test]
fn the_record_wire_shape_is_camel_case_and_carries_no_secret_field() {
    // GTS-FR-02. A typo in a wire key is a silent no-op on the frontend
    // rather than a compile error, so the keys are pinned here.
    let json = serde_json::to_value(GithubTokenRecord {
        id: "a".into(),
        label: "work".into(),
        host: "company.ghe.com".into(),
        account_login: Some("raver119".into()),
        account_display_name: Some("Demo Author".into()),
        account_email: Some("author@example.com".into()),
        scopes: vec!["repo".into()],
        masked_hint: "a3f9".into(),
        added_at: "2026-07-29T00:00:00Z".into(),
        last_verified_at: Some("2026-07-29T00:00:00Z".into()),
        state: TokenState::Valid,
    })
    .unwrap();
    for key in [
        "id",
        "label",
        "host",
        "accountLogin",
        "accountDisplayName",
        "accountEmail",
        "scopes",
        "maskedHint",
        "addedAt",
        "lastVerifiedAt",
        "state",
    ] {
        assert!(json.get(key).is_some(), "missing {key} in {json}");
    }
    assert_eq!(json.get("state").unwrap(), "valid");
    assert_eq!(json.get("host").unwrap(), "company.ghe.com");
    // There is no field a secret could ride in. The two account fields added
    // for GTS-FR-16 describe a *person* — a display name and an email GitHub
    // publishes — so they widen what is known about the account without
    // widening what is known about the credential.
    assert!(json.get("secret").is_none());
    assert!(json.get("token").is_none());
    assert_eq!(
        json.as_object().unwrap().len(),
        11,
        "an added field must be a deliberate contract change: {json}"
    );
}

#[test]
fn the_binding_wire_shape_is_camel_case() {
    let json = serde_json::to_value(ProjectTokenBinding {
        token_id: Some("a".into()),
        resolution: BindingResolution::SelectionRequired,
    })
    .unwrap();
    assert_eq!(json.get("tokenId").unwrap(), "a");
    assert_eq!(json.get("resolution").unwrap(), "selection_required");
}

#[test]
fn a_registry_written_before_a_field_existed_still_loads() {
    // GSS-FR-13: `#[serde(default)]` is what keeps a forward/backward
    // `synthesis.toml` from failing the whole-store parse and taking
    // recents and both registries down with it.
    let decoded: GithubTokenRecord =
        serde_json::from_str(r#"{"id":"a","label":"work"}"#).unwrap();
    assert_eq!(decoded.id, "a");
    assert_eq!(decoded.state, TokenState::Unverified);
    assert!(decoded.scopes.is_empty());
    assert_eq!(decoded.account_login, None);
}

#[test]
fn the_token_creation_url_carries_the_scopes_the_operations_need() {
    // GTS-FR-11 / GTS-FR-12.
    let url = token_creation_url("github.com");
    assert!(url.starts_with("https://github.com/settings/tokens/new?"));
    let query = url.split_once('?').unwrap().1;
    let scopes: std::collections::HashSet<&str> = query
        .split('&')
        .find_map(|p| p.strip_prefix("scopes="))
        .expect("a scopes parameter")
        .split(',')
        .collect();
    // An equality check, not a `contains`: a silently appended scope (say
    // `admin:org`) would ask the author to mint a far broader credential.
    assert_eq!(
        scopes,
        std::collections::HashSet::from(["repo", "workflow"])
    );
}

#[test]
fn parse_scopes_handles_an_empty_or_padded_header() {
    assert_eq!(parse_scopes("repo, workflow"), vec!["repo", "workflow"]);
    assert!(parse_scopes("").is_empty());
    assert!(parse_scopes(",  ,").is_empty());
}
