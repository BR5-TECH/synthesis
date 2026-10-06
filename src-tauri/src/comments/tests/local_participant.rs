//! The local participant a project without a GitHub token writes as
//! (CMS-FR-11, CMS-FR-12, CMS-FR-13, CMS-FR-HTOA, CMS-FR-KTHN).

use super::*;
use crate::github_tokens::{GithubTokenRecord, ERR_SELECTION_REQUIRED};
use crate::global_settings::GlobalSettingsStore;

fn store_with(records: Vec<GithubTokenRecord>) -> GlobalSettingsStore {
    let store = GlobalSettingsStore::in_memory();
    store.save_github_token_registry(records).unwrap();
    store
}

fn token(id: &str, login: Option<&str>) -> GithubTokenRecord {
    GithubTokenRecord {
        id: id.into(),
        label: id.into(),
        account_login: login.map(str::to_string),
        ..Default::default()
    }
}

// CMS-FR-HTOA: the fixed participant is a human with an empty login, the
// display name `Me`, and no email.
#[test]
fn the_local_participant_is_one_fixed_human_without_a_github_account() {
    assert_eq!(
        Participant::local_human(),
        Participant::Human {
            login: String::new(),
            display_name: Some("Me".into()),
            email: None,
        }
    );
    let wire = serde_json::to_value(Participant::local_human()).unwrap();
    assert_eq!(wire["kind"], "human");
    assert_eq!(wire["login"], "");
    assert_eq!(wire["displayName"], "Me");
    assert!(wire.get("email").is_none());
}

// CMS-FR-11, CMS-FR-12, CMS-FR-13, CMS-FR-HTOA: an empty registry writes as
// the local participant, and no other state does.
#[test]
fn an_empty_token_registry_writes_as_the_local_participant() {
    let store = store_with(Vec::new());
    assert_eq!(
        participant_for_slot(&store, "/dev/acme").unwrap(),
        Participant::local_human()
    );
}

// CMS-FR-12: one token keeps GitHub attribution; several tokens keep the
// selection refusal; an unverified token keeps its own refusal.
#[test]
fn stored_tokens_keep_their_existing_attribution_and_refusals() {
    let one = store_with(vec![token("a", Some("raver119"))]);
    match participant_for_slot(&one, "/dev/acme").unwrap() {
        Participant::Human { login, .. } => assert_eq!(login, "raver119"),
        other => panic!("expected a human, got {other:?}"),
    }

    let two = store_with(vec![token("a", Some("raver119")), token("b", Some("octocat"))]);
    assert_eq!(
        participant_for_slot(&two, "/dev/acme").unwrap_err(),
        ERR_SELECTION_REQUIRED,
        "Me is not a fallback while a binding is required"
    );

    let unverified = store_with(vec![token("a", None)]);
    assert_eq!(
        participant_for_slot(&unverified, "/dev/acme").unwrap_err(),
        crate::github_tokens::ERR_IDENTITY_UNRESOLVED
    );
}

// CMS-FR-11, CMS-FR-HTOA, CMS-FR-KTHN: the local participant is stamped into
// the comment and every later event, and a saved GitHub participant is not
// touched by it.
#[test]
fn comments_and_events_stamp_the_local_participant_and_keep_saved_authors() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let me = Participant::local_human();
    let opened = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "first".into(),
        Vec::new(),
        &me,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let github = human("raver119");
    let replied = add_comment_in(
        root,
        "specs/a.md",
        &opened.id,
        "second".into(),
        Vec::new(),
        Vec::new(),
        &github,
        "2026-01-01T00:00:01Z",
    )
    .unwrap();
    let locked = set_lock_in(root, "specs/a.md", &opened.id, true, &me, "2026-01-01T00:00:02Z").unwrap();
    let resolved = set_resolution_in(root, "specs/a.md", &opened.id, true, &me, "2026-01-01T00:00:03Z").unwrap();

    assert_eq!(replied.comments[0].author, me);
    assert_eq!(replied.comments[1].author, github);
    assert!(locked.locked && resolved.resolved);

    let folded = list_discussions_in(root, &artifact_target("specs/a.md"));
    assert_eq!(folded[0].comments[0].author, me, "the saved snapshot is unchanged");
    assert_eq!(folded[0].comments[1].author, github);
}
