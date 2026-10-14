//! What disables the action, decided from disk alone (GHP-FR-GJEO,
//! GHP-FR-AKUM, GHP-FR-DTVW, GHP-FR-KZAP, DRS-FR-10, DRS-FR-VKQO).

use super::*;
use crate::drafts::DraftStatus;

/// GHP-FR-GJEO: `active`, legacy `published`, and `graduated` may publish; `archived`
/// is refused with the reason that says to restore it first.
#[test]
fn only_an_unarchived_draft_is_publishable() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    for status in [DraftStatus::Active, DraftStatus::Published, DraftStatus::Graduated] {
        assert!(flow::local_eligibility(&root, &id, status, None).publishable, "{status:?}");
    }
    let refused = flow::local_eligibility(&root, &id, DraftStatus::Archived, None);
    assert!(!refused.publishable);
    assert_eq!(refused.reason_code.as_deref(), Some(ERR_DRAFT_ARCHIVED));
    assert!(refused.reason.unwrap().to_lowercase().contains("restore"));
}

/// GHP-FR-AKUM: a local asset reference disables publication and every
/// affected path is listed. GHP-FR-DTVW: an absolute remote URL does not.
#[test]
fn a_local_asset_reference_disables_publication_and_names_every_path() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    save_prompt(
        &root,
        &id,
        "![one](../assets/a1.png)\n![two](./local/b.png)\n![remote](https://cdn.example/c.png)\n",
    );
    let refused = flow::local_eligibility(&root, &id, DraftStatus::Active, None);
    assert!(!refused.publishable);
    assert_eq!(refused.reason_code.as_deref(), Some(ERR_LOCAL_ASSETS));
    assert_eq!(refused.local_assets, vec!["../assets/a1.png", "./local/b.png"]);
    assert!(refused.reason.unwrap().to_lowercase().contains("local media"));

    save_prompt(&root, &id, "![remote](https://cdn.example/c.png)\n");
    assert!(flow::local_eligibility(&root, &id, DraftStatus::Active, None).publishable);
}

/// GHP-FR-KZAP: a draft holding a standing attempt refuses a second one.
#[test]
fn a_standing_attempt_refuses_a_second_publication() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let attempt = flow::open_attempt(&root, &id, &origin(), "pub-1".into()).unwrap();
    let refused = flow::local_eligibility(&root, &id, DraftStatus::Active, Some(&attempt));
    assert!(!refused.publishable);
    assert_eq!(refused.reason_code.as_deref(), Some(ERR_ATTEMPT_IN_PROGRESS));
    assert_eq!(
        flow::open_attempt(&root, &id, &origin(), "pub-2".into()),
        Err(ERR_ATTEMPT_IN_PROGRESS.to_string())
    );
}

/// DRS-FR-10 / DRS-FR-VKQO: no caller sets `published` or `graduated`.
#[test]
fn no_caller_sets_published_or_graduated() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    assert_eq!(
        crate::drafts::set_draft_status_impl(&root, &id, DraftStatus::Published),
        Err(crate::drafts::ERR_STATUS_NOT_SETTABLE.to_string())
    );
    assert_eq!(
        crate::drafts::set_draft_status_impl(&root, &id, DraftStatus::Graduated),
        Err(crate::drafts::ERR_STATUS_NOT_SETTABLE.to_string())
    );
    assert_eq!(crate::drafts::draft_record(&root, &id).unwrap().status, DraftStatus::Active);
}

/// DRS-FR-10 / DRS-FR-VKQO: archiving a legacy `published` draft stores
/// `archived` and leaves the publication store as it was; restoring stores
/// `active` with that store intact.
#[test]
fn archive_and_restore_keep_the_publication_store() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    record_publication(&root, &id, "pub-1", 7);
    store_legacy_published(&root, &id);

    let archived = crate::drafts::set_draft_status_impl(&root, &id, DraftStatus::Archived).unwrap();
    assert_eq!(archived.status, DraftStatus::Archived);
    assert_eq!(store::read_store(&root, &id).unwrap().publication.len(), 1);

    let restored = crate::drafts::set_draft_status_impl(&root, &id, DraftStatus::Active).unwrap();
    assert_eq!(restored.status, DraftStatus::Active);
    let store = store::read_store(&root, &id).unwrap();
    assert_eq!(store.publication.len(), 1);
    assert_eq!(store.publication[0].issue_number, 7);
}

/// DRS-FR-KQTW: publication history releases nothing and restores nothing. A
/// draft whose graduation is released reads `active`, not `published`.
#[test]
fn a_release_reads_active_even_where_the_draft_has_publication_history() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    record_publication(&root, &id, "pub-1", 7);
    crate::drafts::set_draft_graduated(&root, &id, "run-1").unwrap();

    // The resolution against an empty queue is what a released draft reads.
    let queue = crate::graduation::GraduationQueue::default();
    let released = crate::drafts::resolved_status_for_test(&queue, &id, DraftStatus::Graduated);
    assert_eq!(released, DraftStatus::Active);
    // And the history is still there to read.
    assert_eq!(store::read_store(&root, &id).unwrap().publication.len(), 1);
}

/// DRS-FR-21: deleting the draft takes its publication store with the rest of
/// its folder.
#[test]
fn deleting_a_draft_removes_its_publication_store() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    flow::open_attempt(&root, &id, &origin(), "pub-1".into()).unwrap();
    let path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    assert!(path.exists());

    crate::drafts::delete_draft_impl(&root, &root, &id).expect("deletes");
    assert!(!path.exists());
}
