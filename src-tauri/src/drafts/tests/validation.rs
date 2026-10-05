//! Activity stamps and the order the panel lists drafts in.

use super::*;

// -- validation --------------------------------------------------------

#[test]
fn saving_a_file_restamps_the_draft_so_the_panel_orders_by_real_activity() {
    // DRS-FR-24: DRP-FR-03 and DRP-FR-08 both rest on `updated_at` following
    // what the author actually did, which nothing else asserts.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    let before = read_record(root, &id).unwrap().updated_at;
    // The stamp has one-second resolution, so move the clock past it.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    save_draft_file_impl(root, &id, FIRST_FILE, "typed").unwrap();

    assert!(
        read_record(root, &id).unwrap().updated_at > before,
        "a write must restamp the draft"
    );
}

#[test]
fn drafts_are_listed_most_recent_activity_first_and_tie_broken_by_id() {
    // DRS-FR-08 / DRS-FR-22, DRS-FR-24: the panel renders this order as given and does
    // not sort it itself, so nothing else would notice the sort going away.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let old = draft(root, "old");
    let mid = draft(root, "mid");
    let new = draft(root, "new");
    for (id, stamp) in [
        (&old, "2026-07-01T00:00:00Z"),
        (&mid, "2026-07-15T00:00:00Z"),
        (&new, "2026-07-30T00:00:00Z"),
    ] {
        let mut record = read_record(root, id).unwrap();
        record.updated_at = stamp.to_string();
        write_record(root, &record).unwrap();
    }

    let order: Vec<String> = listed(root).into_iter().map(|d| d.id).collect();
    assert_eq!(order, vec![new.clone(), mid.clone(), old.clone()]);

    // A write moves its draft to the head, which is the whole point of the
    // ordering: the panel shows what the author was last working on.
    save_draft_file_impl(root, &old, "old.md", "typed").unwrap();
    assert_eq!(listed(root)[0].id, old);
}

#[test]
fn two_drafts_touched_at_the_same_instant_get_a_stable_order() {
    // DRS-FR-08's tie-break, so a reload never reshuffles the list.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let a = draft(root, "a");
    let b = draft(root, "b");
    for id in [&a, &b] {
        let mut record = read_record(root, id).unwrap();
        record.updated_at = "2026-07-30T00:00:00Z".to_string();
        write_record(root, &record).unwrap();
    }

    let first = listed(root);
    let second = listed(root);
    assert_eq!(
        first.iter().map(|d| &d.id).collect::<Vec<_>>(),
        second.iter().map(|d| &d.id).collect::<Vec<_>>(),
    );
    let mut ids = vec![a, b];
    ids.sort();
    assert_eq!(first.into_iter().map(|d| d.id).collect::<Vec<_>>(), ids);
}

#[test]
fn a_draft_carries_the_folder_its_spec_describes_and_an_idle_build_state() {
    // DRS-FR-01 / DRS-FR-23 / DRS-FR-03, DRS-FR-04, DRS-FR-06, DRS-FR-11, DRS-FR-25, DHS-FR-07, DRS-FR-23.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");

    let home = root.join(DRAFTS_REL).join(&id);
    // DRS-FR-23: a draft is created with no conversation log.
    assert!(!home.join(CONVERSATION_FILE).exists());
    assert_eq!(listed(root)[0].build, BuildState::Idle);

    // Both go with the draft, like everything else it holds.
    delete_draft_impl(root, root, &id).unwrap();
    assert!(!home.exists());
}
