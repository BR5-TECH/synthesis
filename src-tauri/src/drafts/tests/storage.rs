//! A draft's own storage is resolved, never composed (DRS-FR-36, DRS-FR-32,
//! DRS-FR-01, DRS-FR-37, DRS-FR-29, CMS-FR-06).

use super::*;

// -----------------------------------------------------------------------
// A draft's own storage is resolved, never composed (DRS-FR-36, DRS-FR-32, DRS-FR-01, DRS-FR-37, DRS-FR-29, CMS-FR-06)
// -----------------------------------------------------------------------

#[test]
fn ts30_a_drafts_storage_is_resolved_wherever_it_is_filed_and_travels_with_it() {
    // DRS-FR-36: the id says nothing about where the draft sits, so the
    // storage folders are found by the walk and follow the draft through a
    // move (DRS-FR-32) without anything composing a path from the id.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let id = create_draft_impl(root, Some("spec"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;

    let filed = root.join(".synthesis/drafts/UI/Components").join(&id);
    assert_eq!(draft_dir(root, &id).unwrap(), filed);
    assert_eq!(draft_proposals_dir(root, &id).unwrap(), filed.join(PROPOSALS_DIR));
    // CMS-FR-37 / DRS-FR-01: the draft's review is not one of its folders — it
    // is addressed by the draft's stable id in the repository machine store.
    assert!(!filed.join("comments").exists());
    // DRS-FR-40: the images the prompt embeds resolve on exactly the same
    // terms as their three siblings — by the walk, never composed from the
    // id (per `DAS-draft-assets.md` DAS-FR-01).
    assert_eq!(draft_assets_dir(root, &id).unwrap(), filed.join(ASSETS_DIR));
    // The path an id alone would have named is not where the draft is, and
    // nothing created it.
    assert!(!root.join(DRAFTS_REL).join(&id).exists());

    // Something in each storage folder, so the move has cargo to carry.
    std::fs::write(filed.join(PROPOSALS_DIR).join("log.jsonl"), "{\"a\":1}\n").unwrap();
    std::fs::write(filed.join(PROPOSALS_DIR).join("p.toml"), "id = \"p\"\n").unwrap();
    std::fs::write(filed.join(ASSETS_DIR).join("a.png"), b"\x89PNG\r\n\x1a\n").unwrap();

    move_draft_to_folder_impl(root, &id, "backend").unwrap();

    let moved = root.join(".synthesis/drafts/backend").join(&id);
    assert_eq!(draft_dir(root, &id).unwrap(), moved);
    assert_eq!(
        std::fs::read_to_string(draft_proposals_dir(root, &id).unwrap().join("log.jsonl"))
            .unwrap(),
        "{\"a\":1}\n",
        "the review travelled with the draft byte-for-byte",
    );
    assert!(draft_proposals_dir(root, &id).unwrap().join("p.toml").is_file());
    assert_eq!(
        std::fs::read(draft_assets_dir(root, &id).unwrap().join("a.png")).unwrap(),
        b"\x89PNG\r\n\x1a\n",
        "the image travelled with the draft byte-for-byte (DAS-FR-01, DRS-FR-21, DRS-FR-32)",
    );
    assert!(!filed.exists(), "and nothing was left behind where it was");

    // DRS-FR-36: an id naming no draft is the typed "not found", and
    // resolving it creates nothing.
    let before = folder_paths(root);
    assert!(draft_dir(root, "19f0000000-0000-deadbeef").is_err());
    assert!(draft_proposals_dir(root, "19f0000000-0000-deadbeef").is_err());
    assert!(draft_assets_dir(root, "19f0000000-0000-deadbeef").is_err());
    assert_eq!(folder_paths(root), before);
}

#[test]
fn ts31_misplaced_draft_storage_is_reunited_with_its_draft_by_the_walk() {
    // DRS-FR-37: a directory named for a draft filed elsewhere, holding
    // nothing but that draft's storage folders, is the draft's storage in
    // the wrong place — not a folder the author cut.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let before = read_record(root, &id).unwrap();
    let husk = husk_with_log(
        root,
        &id,
        "p.toml",
        "{\"e\":1}\n",
    );

    // Before the repair, the husk and its `comments/` read as the author's —
    // and the plain list is a read that leaves them exactly there.
    assert!(folder_paths(root).contains(&id));
    assert!(husk.exists(), "listing alone repairs nothing");

    let (hierarchy, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert_eq!(
        hierarchy.folders,
        vec![DraftFolder { path: "UI".into(), parent: "".into() }],
        "the husk and its comments folder are gone from the tree",
    );
    assert!(!husk.exists(), "and gone from disk");
    assert_eq!(
        std::fs::read_to_string(
            draft_proposals_dir(root, &id).unwrap().join("p.toml")
        )
        .unwrap(),
        "{\"e\":1}\n",
        "the discussion rejoined the draft it belongs to",
    );
    // DRS-FR-37: no draft is created, deleted, moved, or changed in id,
    // record, or files by any of it.
    assert_eq!(hierarchy.drafts.len(), 1);
    assert_eq!(hierarchy.drafts[0].id, id);
    assert_eq!(hierarchy.drafts[0].folder, "UI");
    assert_eq!(read_record(root, &id).unwrap(), before, "the record is untouched");
    assert!(draft_dir(root, &id).unwrap().join(FILES_DIR).join("spec.md").is_file());

    // Repeating it is a no-op: nothing left to repair, nothing reported.
    let (again, repaired_again) = list_drafts_reporting(root);
    assert!(repaired_again.is_empty());
    assert_eq!(again.folders, hierarchy.folders);
}

#[test]
fn ts31_a_log_in_both_places_concatenates_and_a_duplicated_line_folds_once() {
    // DRS-FR-37: an append-only log present in both places concatenates, and
    // the merge needs no conflict rule because the fold applies each event
    // identity once however many times a line appears (CMS-FR-06).
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let shared = "{\"eventId\":\"shared\"}";

    // The `comments/` folder an earlier build kept a draft's review in, which
    // is the storage folder a husk written by that build holds.
    let own = draft_dir(root, &id).unwrap().join("comments");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("discussion.jsonl"),
        format!("{{\"eventId\":\"a\"}}\n{shared}\n"),
    )
    .unwrap();
    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join("comments")).unwrap();
    std::fs::write(
        husk.join("comments").join("discussion.jsonl"),
        format!("{shared}\n{{\"eventId\":\"b\"}}\n"),
    )
    .unwrap();

    list_drafts_reporting(root);

    let merged = std::fs::read_to_string(own.join("discussion.jsonl")).unwrap();
    assert_eq!(
        merged,
        format!("{{\"eventId\":\"a\"}}\n{shared}\n{shared}\n{{\"eventId\":\"b\"}}\n"),
        "every line survives; neither side replaced the other",
    );
    // The duplicate is on disk and costs the reader nothing: the fold takes
    // the first event of a given id and ignores every later one.
    let ids: Vec<&str> = merged.lines().collect();
    assert_eq!(ids.iter().filter(|l| **l == shared).count(), 2);
    assert!(!husk.exists());
}

/// DRS-FR-37: a husk holding only the `comments/` folder an earlier build kept
/// a draft's review in is reunited like any other, so the import pass of
/// `RMS-repository-machine-storage.md` (RMS-FR-JVEC) finds those conversations
/// where it reads a draft's review from.
#[test]
fn ts31_a_legacy_review_husk_is_reunited_so_the_import_pass_can_find_it() {
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join("comments")).unwrap();
    std::fs::write(
        husk.join("comments").join("discussion.jsonl"),
        "{\"eventId\":\"legacy\"}\n",
    )
    .unwrap();

    let (_, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists(), "the husk is gone from disk");
    // Inside the draft, which is where `draft_locations` takes the import pass.
    let reunited = draft_dir(root, &id)
        .unwrap()
        .join("comments")
        .join("discussion.jsonl");
    assert_eq!(
        std::fs::read_to_string(&reunited).unwrap(),
        "{\"eventId\":\"legacy\"}\n",
    );
}

#[test]
fn ts31_an_attachment_present_in_both_places_is_one_file_and_the_husks_copy_goes() {
    // DRS-FR-37: an attachment is named by the digest of its own bytes, so a
    // file present in both places is the same file (CMS-FR-44) — verified
    // rather than assumed.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let own = draft_proposals_dir(root, &id).unwrap().join("attachments");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("deadbeef"), b"same-bytes").unwrap();

    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR).join("attachments")).unwrap();
    std::fs::write(
        husk.join(PROPOSALS_DIR).join("attachments").join("deadbeef"),
        b"same-bytes",
    )
    .unwrap();

    let (_, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists(), "the husk emptied and went");
    assert_eq!(
        std::fs::read(own.join("deadbeef")).unwrap(),
        b"same-bytes",
        "the draft's own copy was never rewritten",
    );
}

#[test]
fn ts31_a_file_whose_bytes_differ_is_never_dropped_and_the_tree_stays_honest() {
    // DRS-FR-37: nothing is deleted that was not accounted for. A name in
    // both places whose bytes differ is neither a log nor content-addressed,
    // so it is left where it is — and the husk, still holding it, stands.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let own = draft_proposals_dir(root, &id).unwrap().join("attachments");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("deadbeef"), b"the-draft-copy").unwrap();

    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR).join("attachments")).unwrap();
    let stray = husk.join(PROPOSALS_DIR).join("attachments").join("deadbeef");
    std::fs::write(&stray, b"a-different-copy").unwrap();

    let (hierarchy, _) = list_drafts_reporting(root);

    assert_eq!(
        std::fs::read(own.join("deadbeef")).unwrap(),
        b"the-draft-copy",
        "the draft's file was not clobbered",
    );
    assert_eq!(
        std::fs::read(&stray).unwrap(),
        b"a-different-copy",
        "and the husk's file was not dropped",
    );
    assert!(husk.exists(), "so the husk is left standing");
    // DRP-FR-32: whatever the repair did or declined to do, the tree
    // reported is the one on disk.
    for folder in &hierarchy.folders {
        assert!(
            root.join(DRAFTS_REL).join(&folder.path).is_dir(),
            "reported folder {:?} does not exist",
            folder.path,
        );
    }
    assert!(hierarchy.folders.iter().any(|f| f.path == id));
}

#[test]
fn ts31_a_husk_holding_only_an_empty_storage_folder_is_removed() {
    // Deliberately unlike `ts31_an_empty_directory_named_for_a_draft_is_left
    // _alone`: an empty `<id>/` is a folder the author may simply not have
    // filled, where an empty `<id>/comments/` is the shape a composed path
    // leaves and nothing else makes. Removing it is what clears the phantom
    // row; the draft's own storage is untouched either way.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR)).unwrap();

    let (hierarchy, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists());
    assert!(!hierarchy.folders.iter().any(|f| f.path == id));
    assert!(draft_proposals_dir(root, &id).unwrap().is_dir());
}

#[test]
fn ts31_a_proposals_husk_is_reunited_like_a_comments_one() {
    // DRS-FR-37 names both storage folders of DRS-FR-01. A candidate is
    // neither a log nor content-addressed, so it merges by arriving where
    // nothing of its name stands.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR)).unwrap();
    std::fs::write(husk.join(PROPOSALS_DIR).join("p1.toml"), "id = \"p1\"\n").unwrap();
    std::fs::write(husk.join(PROPOSALS_DIR).join("p1.content"), "proposed\n").unwrap();

    let (_, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists());
    let landed = draft_proposals_dir(root, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(landed.join("p1.content")).unwrap(),
        "proposed\n",
    );
    assert!(landed.join("p1.toml").is_file());
}

#[test]
fn ts31_a_husk_is_reunited_from_wherever_under_the_root_it_sits() {
    // DRS-FR-37 says "beneath the drafts root", not "at it": the husk is
    // wherever the composing caller happened to be pointed.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let nested = husk_with_log(
        root,
        &format!("backend/{id}"),
        "p.toml",
        "{\"e\":\"nested\"}\n",
    );

    let (hierarchy, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!nested.exists());
    assert_eq!(
        std::fs::read_to_string(
            draft_proposals_dir(root, &id).unwrap().join("p.toml")
        )
        .unwrap(),
        "{\"e\":\"nested\"}\n",
    );
    assert!(
        hierarchy.folders.iter().any(|f| f.path == "backend"),
        "the folder the husk sat in is the author's and stays",
    );
}

#[test]
fn ts31_two_husks_for_one_draft_both_rejoin_it_and_it_is_named_once() {
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let a = husk_with_log(root, &id, "one.jsonl", "{\"e\":\"a\"}\n");
    let b = husk_with_log(root, &format!("backend/{id}"), "two.jsonl", "{\"e\":\"b\"}\n");

    let (_, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()], "one draft, named once");
    assert!(!a.exists() && !b.exists());
    let own = draft_proposals_dir(root, &id).unwrap();
    assert!(own.join("one.jsonl").is_file() && own.join("two.jsonl").is_file());
}

#[test]
fn ts31_a_nested_subtree_in_a_husk_arrives_whole() {
    // Both branches: a destination that does not exist takes the subtree in
    // one move, and one that does is descended into.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let own = draft_proposals_dir(root, &id).unwrap();
    std::fs::create_dir_all(own.join("attachments")).unwrap();
    std::fs::write(own.join("attachments").join("kept"), b"mine").unwrap();

    let husk = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR).join("attachments")).unwrap();
    std::fs::write(
        husk.join(PROPOSALS_DIR).join("attachments").join("incoming"),
        b"theirs",
    )
    .unwrap();
    std::fs::create_dir_all(husk.join(PROPOSALS_DIR).join("fresh").join("deep")).unwrap();
    std::fs::write(
        husk.join(PROPOSALS_DIR).join("fresh").join("deep").join("f"),
        b"x",
    )
    .unwrap();

    list_drafts_reporting(root);

    assert!(!husk.exists());
    assert_eq!(std::fs::read(own.join("attachments").join("kept")).unwrap(), b"mine");
    assert_eq!(
        std::fs::read(own.join("attachments").join("incoming")).unwrap(),
        b"theirs",
        "merged into the directory that already existed",
    );
    assert_eq!(
        std::fs::read(own.join("fresh").join("deep").join("f")).unwrap(),
        b"x",
        "and a directory that did not arrived whole",
    );
}

#[test]
fn ts31_a_husk_rejoins_an_archived_draft_without_touching_its_status() {
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    let husk = husk_with_log(
        root,
        &id,
        "p.toml",
        "{\"e\":1}\n",
    );

    let (hierarchy, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists());
    assert_eq!(hierarchy.drafts[0].status, DraftStatus::Archived);
}

#[test]
fn ts31_a_husk_rejoins_a_draft_whose_record_is_unreadable() {
    // DRS-FR-35: a damaged record is reported rather than dropped, and the
    // repair does not depend on reading it — the walk finds the draft by its
    // `draft.toml` existing, not by its contents.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(draft_dir(root, &id).unwrap().join(RECORD_FILE), "not toml {{").unwrap();
    let husk = husk_with_log(
        root,
        &id,
        "p.toml",
        "{\"e\":1}\n",
    );

    let (hierarchy, repaired) = list_drafts_reporting(root);

    assert_eq!(repaired, vec![id.clone()]);
    assert!(!husk.exists());
    assert_eq!(hierarchy.drafts.len(), 1, "the damaged draft is still reported");
    assert!(draft_proposals_dir(root, &id)
        .unwrap()
        .join("p.toml")
        .is_file());
}

#[test]
fn ts31_a_folder_the_author_made_is_never_adopted_however_it_is_named() {
    // DRS-FR-37: all three conditions together. The load-bearing one is that
    // a draft carrying that id exists elsewhere — a resemblance is not
    // enough, because an author may name a folder anything DRS-FR-30 takes.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());

    // (1) Named like a draft id, but no draft carries it.
    let orphan = "19f0000000-0000-deadbeef";
    create_drafts_folder_impl(root, "", orphan).unwrap();
    std::fs::create_dir_all(root.join(DRAFTS_REL).join(orphan).join(PROPOSALS_DIR)).unwrap();

    // (2) Named for a real draft filed elsewhere, but holding something of
    // the author's besides the storage folders.
    let mixed = root.join(DRAFTS_REL).join(&id);
    std::fs::create_dir_all(mixed.join(PROPOSALS_DIR)).unwrap();
    std::fs::write(mixed.join("notes.md"), "mine\n").unwrap();

    let before = folder_paths(root);
    assert!(before.contains(&orphan.to_string()) && before.contains(&id));

    let (_, repaired) = list_drafts_reporting(root);

    assert!(repaired.is_empty(), "neither directory was adopted");
    assert_eq!(folder_paths(root), before, "and the author's tree is as it was");
    assert!(mixed.join("notes.md").is_file(), "nothing of theirs moved");
    assert!(root.join(DRAFTS_REL).join(orphan).join(PROPOSALS_DIR).is_dir());
}
