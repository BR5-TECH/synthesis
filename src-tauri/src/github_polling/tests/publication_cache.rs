//! A change of the selected GitHub Project refreshes the publication
//! metadata cache (`GHP-github-publication.md` GHP-FR-JDMU, GHP-FR-HGVL).

use tauri::Manager;

use super::*;
use crate::github_publication::metadata_cache::RefreshTrigger;
use crate::github_publication::metadata_refresh::{self as mr, PublicationMetadataCache};

fn harness() -> Harness {
    let h = Harness::new();
    h.app.manage(PublicationMetadataCache::default());
    // The refresh resolves no token here, so it fails before it lists anything
    // and contacts no fake that refuses publication reads.
    h.app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    h.app.manage(crate::github_tokens::GithubTokens::default());
    mr::open_for_test(&h.handle());
    h
}

// GHP-FR-JDMU: a newly selected GitHub Project starts one refresh and re-arms
// the timer. Saving the same GitHub Project again starts none.
#[test]
fn a_changed_github_project_starts_one_refresh_and_rearms_the_timer() {
    let h = harness();
    let (n0, _, e0) = mr::starts_for_test(&h.handle());
    h.select(Some(PROJECT));
    let (n1, trigger, e1) = mr::starts_for_test(&h.handle());
    assert_eq!(n1, n0 + 1);
    assert_eq!(trigger, Some(RefreshTrigger::Invalidation));
    assert_eq!(e1, e0 + 1);

    h.select(Some(PROJECT));
    assert_eq!(mr::starts_for_test(&h.handle()), (n1, trigger, e1));
}

// GHP-FR-JDMU / GHP-FR-HGVL: a change that leaves no GitHub Project selected
// starts no refresh and re-arms the idle timer.
#[test]
fn deselecting_the_github_project_starts_no_refresh() {
    let h = harness();
    h.select(Some(PROJECT));
    let (n1, _, e1) = mr::starts_for_test(&h.handle());
    h.select(None);
    let (n2, _, e2) = mr::starts_for_test(&h.handle());
    assert_eq!(n2, n1);
    // One emptied cache and one armed idle timer.
    assert_eq!(e2, e1 + 2);
}
