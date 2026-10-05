//! The mask hint and the redaction of a secret from text
//! (GTS-FR-02, GTS-FR-11).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- mask_hint (GTS-FR-02) -------------------------------------------

#[test]
fn mask_hint_is_the_last_four_characters() {
    assert_eq!(mask_hint("ghp_0123456789abcdefa3f9"), "a3f9");
    assert_eq!(mask_hint("abcd"), "abcd");
}

#[test]
fn mask_hint_never_panics_on_a_short_or_multibyte_secret() {
    // A slice by byte index would panic on the multi-byte case. Nothing
    // guarantees a pasted string is ASCII — the user can paste anything.
    assert_eq!(mask_hint(""), "");
    assert_eq!(mask_hint("ab"), "ab");
    assert_eq!(mask_hint("aé☃🔑"), "aé☃🔑");
    assert_eq!(mask_hint("ghp_é☃🔑x"), "é☃🔑x");
}

// -- redact (GTS-FR-11 / GTC-FR-11) ----------------------------------

#[test]
fn redact_removes_an_embedded_secret_from_output() {
    let secret = "ghp_0123456789abcdef";
    let line = format!("remote: https://x-access-token:{secret}@github.com/acme/app.git");
    let out = redact(&line, secret);
    assert!(!out.contains(secret), "{out}");
    assert!(out.contains("github.com/acme/app.git"));
}

#[test]
fn redact_leaves_text_alone_for_an_implausibly_short_secret() {
    // Replacing a short string would corrupt unrelated output far more
    // often than it would hide anything.
    assert_eq!(redact("pushing to main", "in"), "pushing to main");
}
