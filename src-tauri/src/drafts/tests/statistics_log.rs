//! The statistics log goes with the draft (DRS-FR-ZLBK), and the assets
//! folder a draft is created holding (DRS-TS36, DRS-TS08).

use super::*;

// -----------------------------------------------------------------------
// The statistics log goes with the draft (DRS-FR-ZLBK)
// -----------------------------------------------------------------------

/// DRS-FR-ZLBK, DRS-FR-21: deleting a draft removes its statistics log and no other's,
/// and a log that is already missing causes no error.
#[test]
fn drs_ts_xkqm_deleting_a_draft_takes_its_statistics_log_and_leaves_every_other() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let doomed = draft(root, "doomed");
    let kept = draft(root, "kept");

    for id in [&doomed, &kept] {
        crate::statistics::append_events(
            root,
            id,
            &[crate::statistics::Pending::now(
                crate::statistics::EventBody::ProposalDecision {
                    proposal_id: format!("p-{id}"),
                    decision: crate::statistics::Decision::Accepted,
                },
            )],
        )
        .unwrap();
    }
    let kept_log = dir
        .path()
        .join(crate::statistics::STATISTICS_REL)
        .join(format!("{kept}.jsonl"));
    let kept_before = std::fs::read_to_string(&kept_log).unwrap();

    delete_draft_impl(root, root, &doomed).unwrap();

    assert!(!draft_dir(root, &doomed).is_ok());
    assert!(
        !dir.path()
            .join(crate::statistics::STATISTICS_REL)
            .join(format!("{doomed}.jsonl"))
            .exists(),
        "the draft's statistics log went with it",
    );
    // No statistics file for that id remains anywhere in the project.
    assert!(
        !dir.path()
            .join(DRAFTS_REL)
            .join(format!("{doomed}.jsonl"))
            .exists(),
    );
    assert_eq!(std::fs::read_to_string(&kept_log).unwrap(), kept_before);

    // A draft whose log is already missing deletes without an error.
    let bare = draft(root, "bare");
    delete_draft_impl(root, root, &bare).unwrap();
    assert!(!draft_dir(root, &bare).is_ok());
}

/// DRS-FR-ZLBK, DRS-FR-21: an event queued before a deletion never brings the log back.
///
/// Statistics recording is asynchronous (per
/// `DSS-draft-statistics-storage.md` DSS-FR-TUMX), so a graduation
/// operation or a conversational turn that settles while its draft is being
/// deleted reaches the writer after the log has gone. Nothing anywhere
/// cleans a resurrected log up — the graduation gates skip the folder
/// deliberately — so the append is what has to refuse.
#[test]
fn drs_ts_xkqm_a_late_event_never_resurrects_a_deleted_drafts_log() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let doomed = draft(root, "doomed");
    let event = crate::statistics::Pending::now(
        crate::statistics::EventBody::ProposalDecision {
            proposal_id: "p-late".to_string(),
            decision: crate::statistics::Decision::Accepted,
        },
    );
    crate::statistics::append_events(root, &doomed, std::slice::from_ref(&event)).unwrap();

    delete_draft_impl(root, root, &doomed).unwrap();
    let log = dir
        .path()
        .join(crate::statistics::STATISTICS_REL)
        .join(format!("{doomed}.jsonl"));
    assert!(!log.exists());

    // The event the writer was still holding, delivered through the same
    // path production delivers it on. The draft is gone, so it lands
    // nowhere and the log stays gone.
    crate::statistics::append_for_existing_draft(root, root, &doomed, std::slice::from_ref(&event));
    assert!(
        !log.exists(),
        "an append for a draft that no longer exists must create nothing",
    );

    // And a draft that does still exist is unaffected by the same guard.
    let kept = draft(root, "kept");
    crate::statistics::append_for_existing_draft(root, root, &kept, std::slice::from_ref(&event));
    assert!(dir
        .path()
        .join(crate::statistics::STATISTICS_REL)
        .join(format!("{kept}.jsonl"))
        .exists());
}

/// DRS-FR-ZLBK: the log is removed **before** the folder, which is what
/// makes the operation retryable — a failure at either step leaves the
/// draft's record on disk and the same call repeated completes what is left.
#[test]
fn drs_ts_pghu_the_deletion_order_is_the_log_and_then_the_folder() {
    let source = include_str!("../../drafts.rs");
    let body = source
        .split_once("pub fn delete_draft_impl")
        .expect("the deletion")
        .1;
    let statistics_at = body
        .find("delete_draft_statistics")
        .expect("the statistics log is removed");
    let folder_at = body.find("delete_under").expect("the folder is removed");
    assert!(
        statistics_at < folder_at,
        "the log is removed first, so a repeated call completes what is left",
    );
    // DRS-FR-WNTA: the draft's review threads go before the folder too, and
    // both removals are scoped to the one stable id.
    let comments_at = body
        .find("delete_draft_comments")
        .expect("the review threads are removed");
    assert!(comments_at < folder_at, "the threads are removed first");
    assert!(body[..folder_at].contains("delete_draft_statistics(store, id)"));
    assert!(body[..folder_at].contains("delete_draft_comments(store, id)"));
}

/// DRS-FR-04, DRS-FR-11, DRS-FR-15, DRS-FR-21, DRS-FR-40: a draft's directory holds exactly the seven entries DRS-FR-01
/// names, `assets/` among them and empty; storing an image leaves the single
/// prompt of DRS-FR-11 untouched; and the whole directory goes on a delete.
#[test]
fn drs_ts36_a_drafts_directory_holds_an_empty_assets_folder_and_nothing_else() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    let home = root.join(DRAFTS_REL).join(&id);

    let mut entries: Vec<String> = std::fs::read_dir(&home)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec![
            ASSETS_DIR.to_string(),
            RECORD_FILE.to_string(),
            FILES_DIR.to_string(),
            HISTORY_DIR.to_string(),
            PROPOSALS_DIR.to_string(),
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>(),
    );
    assert_eq!(
        std::fs::read_dir(home.join(ASSETS_DIR)).unwrap().count(),
        0,
        "scaffolded empty",
    );

    // DRS-FR-11 / DRS-FR-15: an image the prompt embeds is the draft's
    // material rather than a second prompt file, so `files/` still holds
    // exactly one file and the draft is still consistent.
    let asset = crate::draft_assets::store_image_impl(
        root,
        &id,
        "image/png",
        Some("a.png"),
        "iVBORw0KGgo=",
        &|_: &str| {},
    )
    .unwrap();
    save_draft_file_impl(root, &id, "d.md", &format!("![a]({})\n", asset.reference)).unwrap();
    assert_eq!(std::fs::read_dir(home.join(FILES_DIR)).unwrap().count(), 1);
    assert!(!listed(root)[0].inconsistent);
    assert!(open_draft_impl(root, &id).is_ok());

    // DRS-FR-40: a "not found" for an id no draft carries, and no directory
    // was created anywhere under the drafts root to answer it.
    let before = std::fs::read_dir(root.join(DRAFTS_REL)).unwrap().count();
    assert!(draft_assets_dir(root, "0000000000-0000-00000000").is_err());
    assert_eq!(std::fs::read_dir(root.join(DRAFTS_REL)).unwrap().count(), before);

    delete_draft_impl(root, root, &id).unwrap();
    assert!(!home.exists(), "the whole directory including assets/ is gone");
}

/// DRS-FR-11, DRS-FR-15, DRS-FR-21: a draft carrying images under `assets/` is **not**
/// inconsistent — the test of DRS-FR-15 is over `files/` alone.
#[test]
fn drs_ts08_images_under_assets_never_make_a_draft_inconsistent() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "pictures");
    for _ in 0..4 {
        crate::draft_assets::store_image_impl(
            root,
            &id,
            "image/png",
            Some("a.png"),
            "iVBORw0KGgo=",
            &|_: &str| {},
        )
        .unwrap();
    }
    let summary = listed(root).into_iter().find(|d| d.id == id).unwrap();
    assert!(!summary.inconsistent);
    assert!(open_draft_impl(root, &id).is_ok());
    assert!(load_draft_file_impl(root, &id, "pictures.md").is_ok());
}
