//! The verifier's own decisions (GTS-FR-04).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- The verifier's own decisions (GTS-FR-04) ------------------------

#[test]
fn only_a_401_or_403_is_evidence_about_the_token() {
    // The whole add flow rests on this split. A 500 or a captive-portal
    // redirect classified as `Rejected` would tell an author their perfectly
    // good token is bad.
    for code in [401u16, 403] {
        assert!(matches!(
            verify_failure_for_status(code),
            VerifyError::Rejected
        ));
    }
    for code in [404u16, 429, 500, 502, 302] {
        assert!(
            matches!(verify_failure_for_status(code), VerifyError::Unreachable(_)),
            "{code} must not be read as a statement about the token"
        );
    }
}

#[test]
fn the_login_is_read_from_the_user_payload_and_nothing_else_is() {
    assert_eq!(
        login_from_user_payload(r#"{"login":"raver119","id":42}"#).as_deref(),
        Some("raver119")
    );
    for bad in ["", "not json", "{}", r#"{"login":42}"#, "[]"] {
        assert_eq!(login_from_user_payload(bad), None, "{bad:?}");
    }
}

#[test]
fn a_verifier_error_carrying_the_secret_never_reaches_the_caller() {
    // `VerifyError::Unreachable` carries an arbitrary message, and a real
    // HTTP client is entirely capable of echoing the Authorization header
    // into it. The typed error the command returns must be the bare code.
    struct Leaky;
    impl GithubVerifier for Leaky {
        fn verify(&self, secret: &str) -> Result<VerifiedIdentity, VerifyError> {
            Err(VerifyError::Unreachable(format!(
                "failed GET https://api.github.com/user with Bearer {secret}"
            )))
        }
    }
    let store = GlobalSettingsStore::in_memory();
    let tokens = GithubTokens::new(Box::new(FakeSecrets::default()), Box::new(Leaky));

    let err = add_token_impl(&store, &tokens, "work", "ghp_super_secret_1234").unwrap_err();
    assert_eq!(err, ERR_GITHUB_UNREACHABLE);
    assert!(
        !err.contains("ghp_super_secret"),
        "the secret must not ride out on an error string: {err}"
    );
}
