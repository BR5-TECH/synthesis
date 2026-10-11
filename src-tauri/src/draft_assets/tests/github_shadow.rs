//! The asset writes of a GitHub-shadow draft (`DRS-draft-storage.md`
//! DRS-FR-QPSC, per DAS-FR-25).

use super::*;

fn link() -> drafts::GithubIssueLink {
    drafts::GithubIssueLink {
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: 4,
        issue_url: "https://github.com/acme/widgets/issues/4".into(),
        project_node_id: "PVT_1".into(),
        claim_state: drafts::GithubClaimState::Claimed,
    }
}

/// DRS-FR-QPSC: storing and discarding an image both refuse a shadow draft
/// with `draft_github_shadow`, and the assets folder is left as it was.
#[test]
fn a_shadow_draft_takes_no_asset_write() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let asset = store_png(&root, &id, "one.png", "one");
    drafts::mark_github_shadow_for_test(&root, &id, link());
    let before = everything_in_assets(&root, &id);

    let stored = store_image_impl(&root, &id, "image/png", None, &b64(&png("two")), &no_hold());
    assert_eq!(stored.err(), Some(drafts::ERR_GITHUB_SHADOW.to_string()));
    let discarded = discard_image_impl(&root, &id, &asset.path, &DraftAssetSweeper::default());
    assert_eq!(discarded.err(), Some(drafts::ERR_GITHUB_SHADOW.to_string()));
    assert_eq!(everything_in_assets(&root, &id), before);
}
