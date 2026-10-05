//! What `resolve_binding` decides from the records it is given (GTS-FR-10).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- resolve_binding (GTS-FR-10) -------------------------------------

#[test]
fn no_stored_token_resolves_none_stored() {
    let b = resolve_binding(&[], None);
    assert_eq!(b.resolution, BindingResolution::NoneStored);
    assert_eq!(b.token_id, None);
}

#[test]
fn a_single_token_is_used_implicitly_so_the_author_is_never_asked() {
    // GTS-FR-09 first half: the single-token author never sees the picker.
    let records = vec![record("a", "work")];
    let b = resolve_binding(&records, None);
    assert_eq!(b.resolution, BindingResolution::Implicit);
    assert_eq!(b.token_id.as_deref(), Some("a"));
}

#[test]
fn two_tokens_and_no_binding_require_a_selection() {
    // GTS-FR-09 second half.
    let records = vec![record("a", "work"), record("b", "personal")];
    let b = resolve_binding(&records, None);
    assert_eq!(b.resolution, BindingResolution::SelectionRequired);
    assert_eq!(b.token_id, None);
}

#[test]
fn a_live_binding_resolves_bound() {
    let records = vec![record("a", "work"), record("b", "personal")];
    let b = resolve_binding(&records, Some("b"));
    assert_eq!(b.resolution, BindingResolution::Bound);
    assert_eq!(b.token_id.as_deref(), Some("b"));
}

#[test]
fn a_dangling_binding_falls_through_to_the_count_rule() {
    // GTS-FR-10: the bound token was removed. With two others left the
    // author is asked; with one left it is used implicitly rather than
    // presenting a choice between a set of one.
    let two = vec![record("a", "work"), record("b", "personal")];
    let b = resolve_binding(&two, Some("gone"));
    assert_eq!(b.resolution, BindingResolution::SelectionRequired);
    assert_eq!(b.token_id, None);

    let one = vec![record("a", "work")];
    let b = resolve_binding(&one, Some("gone"));
    assert_eq!(b.resolution, BindingResolution::Implicit);
    assert_eq!(b.token_id.as_deref(), Some("a"));
}
