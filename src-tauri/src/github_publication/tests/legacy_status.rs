//! Publication and the draft status are separate facts (GHP-FR-BSYH,
//! GHP-FR-VNCS, DRS-FR-VKQO, DRS-FR-OGZC, DRS-FR-RJYF, DRS-FR-PXHW).

use super::publication_commands::session;
use super::*;
use crate::drafts::DraftStatus;

fn stored_status(root: &crate::fs::RootFs, id: &str) -> DraftStatus {
    crate::drafts::draft_record(root, id).unwrap().status
}

/// GHP-FR-BSYH / NAW-FR-GKBP: a successful publication of an `active` draft
/// appends its record and leaves the draft `active`.
#[test]
fn a_publication_keeps_an_active_draft_active() {
    let s = session();
    let outcome = s.publish(PublicationChoiceInput::default()).unwrap();
    assert!(matches!(outcome, PublicationOutcome::Published { .. }));
    assert_eq!(s.store().publication.len(), 1);
    assert_eq!(stored_status(&s.root(), &s.id), DraftStatus::Active);
}

/// GHP-FR-BSYH / GHP-FR-GJEO / NAW-FR-GKBP: a `graduated` draft and a legacy
/// `published` draft publish, and the publication writes no status either.
#[test]
fn a_publication_keeps_a_graduated_or_legacy_published_draft_as_it_was() {
    let s = session();
    crate::drafts::set_draft_graduated(&s.root(), &s.id, "run-1").unwrap();
    s.publish(PublicationChoiceInput::default()).unwrap();
    assert_eq!(stored_status(&s.root(), &s.id), DraftStatus::Graduated);

    let s = session();
    store_legacy_published(&s.root(), &s.id);
    s.publish(PublicationChoiceInput::default()).unwrap();
    assert_eq!(s.store().publication.len(), 1);
    assert_eq!(stored_status(&s.root(), &s.id), DraftStatus::Published);
}

/// GHP-FR-VNCS: a failed publication writes no status and appends no record.
#[test]
fn a_failed_publication_writes_no_status() {
    let s = session();
    let refused = s.publish(PublicationChoiceInput {
        parent_issue_number: Some(999_999),
        ..Default::default()
    });
    assert!(refused.is_err());
    assert!(s.store().publication.is_empty());
    assert_eq!(stored_status(&s.root(), &s.id), DraftStatus::Active);
}

/// DRS-FR-VKQO / DRS-FR-RJYF / NAW-FR-16: a legacy `published` draft is not migrated on
/// load. Every read reports it as it stands, and a publication record alone
/// does not replace it.
#[test]
fn a_legacy_published_draft_reads_published_until_its_next_write() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    store_legacy_published(&root, &id);
    record_publication(&root, &id, "pub-1", 7);

    assert_eq!(stored_status(&root, &id), DraftStatus::Published);
    let listed = crate::drafts::list_drafts_impl(&root);
    let row = listed.drafts.iter().find(|d| d.id == id).expect("listed");
    assert_eq!(row.status, DraftStatus::Published);
    // The reads above wrote nothing back.
    assert_eq!(stored_status(&root, &id), DraftStatus::Published);
}

/// DRS-FR-VKQO / DRS-FR-OGZC: the next save stores `active`, and the draft
/// takes the write as an active one does.
#[test]
fn a_save_stores_active_over_legacy_published() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    store_legacy_published(&root, &id);
    save_prompt(&root, &id, "still editable\n");
    assert_eq!(stored_status(&root, &id), DraftStatus::Active);
}

/// DRS-FR-VKQO / DRS-FR-OGZC: the next rename stores `active`.
#[test]
fn a_rename_stores_active_over_legacy_published() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    store_legacy_published(&root, &id);
    let renamed = crate::drafts::rename_draft_impl(&root, &id, "widget 2").unwrap();
    assert_eq!(renamed.status, DraftStatus::Active);
    assert_eq!(stored_status(&root, &id), DraftStatus::Active);
}

/// DRS-FR-VKQO / DRS-FR-OGZC: the next move stores `active` and carries the
/// publication store with the draft.
#[test]
fn a_move_stores_active_over_legacy_published() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    record_publication(&root, &id, "pub-1", 7);
    store_legacy_published(&root, &id);
    crate::drafts::create_drafts_folder_impl(&root, "", "UI").unwrap();

    let moved = crate::drafts::move_draft_to_folder_impl(&root, &id, "UI").expect("moves");
    assert_eq!(moved.status, DraftStatus::Active);
    assert_eq!(stored_status(&root, &id), DraftStatus::Active);
    assert_eq!(store::read_store(&root, &id).unwrap().publication.len(), 1);
}

/// DRS-FR-VKQO: an archive stores `archived`, not `active`.
#[test]
fn an_archive_stores_archived_over_legacy_published() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    store_legacy_published(&root, &id);
    crate::drafts::set_draft_status_impl(&root, &id, DraftStatus::Archived).unwrap();
    assert_eq!(stored_status(&root, &id), DraftStatus::Archived);
}

/// DRS-FR-OGZC: a legacy `published` draft graduates and deletes like an
/// active one.
#[test]
fn a_legacy_published_draft_graduates_and_deletes_like_an_active_one() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    store_legacy_published(&root, &id);
    assert_eq!(
        crate::drafts::set_draft_graduated(&root, &id, "run-1").unwrap().status,
        DraftStatus::Graduated
    );

    let other = draft(&root, "other");
    store_legacy_published(&root, &other);
    crate::drafts::delete_draft_impl(&root, &root, &other).expect("deletes");
    assert!(crate::drafts::draft_record(&root, &other).is_err());
}

/// DRS-FR-PXHW: the listing carries the newest record of each draft's store,
/// on every status, and nothing where the history is empty.
#[test]
fn the_listing_carries_the_newest_publication_record_on_every_status() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let active = draft(&root, "active");
    record_publication(&root, &active, "pub-1", 7);
    record_publication(&root, &active, "pub-2", 9);
    let archived = draft(&root, "archived");
    record_publication(&root, &archived, "pub-3", 11);
    crate::drafts::set_draft_status_impl(&root, &archived, DraftStatus::Archived).unwrap();
    let graduated = draft(&root, "graduated");
    record_publication(&root, &graduated, "pub-4", 12);
    crate::drafts::set_draft_graduated(&root, &graduated, "run-1").unwrap();
    let plain = draft(&root, "plain");
    // A store holding only a standing attempt has no record yet.
    let pending = draft(&root, "pending");
    flow::open_attempt(&root, &pending, &origin(), "pub-5".into()).unwrap();

    let listed = crate::drafts::list_drafts_impl(&root);
    let row = |id: &str| listed.drafts.iter().find(|d| d.id == id).expect("listed").clone();

    let newest = row(&active).publication.expect("a record");
    assert_eq!(newest.issue_number, 9);
    assert_eq!(newest.issue_url, "https://github.com/acme/widgets/issues/9");
    assert_eq!(newest.repository_owner, "acme");
    assert_eq!(newest.repository_name, "widgets");
    assert_eq!(newest.repository_host, "github.com");
    let stored = store::read_store(&root, &active).unwrap();
    assert_eq!(newest.published_at, stored.publication[1].published_at);

    assert_eq!(row(&archived).publication.map(|p| p.issue_number), Some(11));
    assert_eq!(row(&graduated).publication.map(|p| p.issue_number), Some(12));
    assert!(row(&plain).publication.is_none());
    assert!(row(&pending).publication.is_none());
    // The wire shape: camelCase, and absent where there is no record.
    let wire = serde_json::to_value(row(&active)).unwrap();
    assert_eq!(wire["publication"]["issueNumber"], 9);
    assert_eq!(wire["publication"]["publishedAt"], newest.published_at.as_str());
    assert!(wire.get("publicationUnreadable").is_none());
    assert!(serde_json::to_value(row(&plain)).unwrap().get("publication").is_none());
}

/// DRS-FR-PXHW / DRS-FR-RJYF: a store that does not parse gives that row no
/// record, marks it for the warning, and makes no draft inconsistent.
#[test]
fn an_unreadable_publication_store_costs_its_row_the_record_alone() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let broken = draft(&root, "broken");
    let path = crate::drafts::draft_publication_path(&root, &broken).unwrap();
    root.write_text_atomic(&path, "<<<<<<< HEAD\nnot toml\n").unwrap();
    let fine = draft(&root, "fine");
    record_publication(&root, &fine, "pub-1", 7);

    let listed = crate::drafts::list_drafts_impl(&root);
    let row = |id: &str| listed.drafts.iter().find(|d| d.id == id).expect("listed").clone();
    let broken_row = row(&broken);
    assert!(broken_row.publication.is_none());
    assert!(broken_row.publication_unreadable);
    assert!(!broken_row.inconsistent);
    let fine_row = row(&fine);
    assert_eq!(fine_row.publication.map(|p| p.issue_number), Some(7));
    assert!(!fine_row.publication_unreadable);
}
