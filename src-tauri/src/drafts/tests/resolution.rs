//! The resolution the agent tools use (DRS-FR-28, DRS-FR-36, DRS-FR-15, DRS-FR-38).

use super::*;

// -----------------------------------------------------------------------
// DRS-FR-28, DRS-FR-36, DRS-FR-15 — the resolution the agent tools use (DRS-FR-38)
// -----------------------------------------------------------------------

/// DRS-FR-28, DRS-FR-38, DRS-FR-15: both calls resolve a *filed* draft through the DRS-FR-36 walk,
/// report its current record, read only the live prompt, and change nothing.
///
/// The draft is deliberately filed two folders deep: a path composed from
/// the id alone would name the drafts root and find nothing, so this is the
/// clause that actually exercises the walk the whole design rests on.
#[test]
fn draft_record_and_read_draft_prompt_resolve_a_filed_draft() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(&root, "", "UI").unwrap();
    create_drafts_folder_impl(&root, "UI", "Components").unwrap();
    let created =
        create_draft_impl(&root, Some("filed"), Some("UI/Components")).unwrap();
    let id = created.draft.id.clone();
    save_draft_file_impl(&root, &id, "filed.md", "# Plan\n\nthe live prompt\n").unwrap();

    // Sibling storage, each carrying text the prompt does not.
    let draft_dir_path = dir.path().join(".synthesis/drafts/UI/Components").join(&id);
    std::fs::write(draft_dir_path.join("history/e1.snapshot"), "a snapshot\n").unwrap();
    std::fs::write(draft_dir_path.join("proposals/p1.md"), "a candidate\n").unwrap();
    std::fs::write(draft_dir_path.join("conversation.jsonl"), "a conversation\n").unwrap();

    let before = tree_bytes(dir.path());
    let updated_before = draft_record(&root, &id).unwrap().updated_at;

    let record = draft_record(&root, &id).unwrap();
    assert_eq!(record.id, id);
    assert_eq!(record.name, "filed");
    assert_eq!(record.prompt_path.as_deref(), Some("filed.md"));
    assert_eq!(record.status, DraftStatus::Active);

    let resolved = read_draft_prompt(&root, &id).unwrap();
    assert_eq!(resolved.draft.id, id);
    assert_eq!(resolved.prompt_path, "filed.md");
    assert_eq!(resolved.content, "# Plan\n\nthe live prompt\n");
    for leaked in ["snapshot", "candidate", "comment", "conversation"] {
        assert!(!resolved.content.contains(leaked), "{leaked} is not read");
    }

    // DRS-FR-38: read-only. No record rewritten, no `updated_at` refreshed,
    // nothing on disk changed at all.
    assert_eq!(before, tree_bytes(dir.path()));
    assert_eq!(draft_record(&root, &id).unwrap().updated_at, updated_before);

    // The draft still sits where it was filed; nothing composed a path from
    // the id at the drafts root.
    assert!(draft_dir_path.is_dir());
    assert!(!dir.path().join(".synthesis/drafts").join(&id).exists());
}

/// DRS-FR-28, DRS-FR-38, DRS-FR-36, DRS-FR-15: the current record after a rename and an archive, and the
/// prompt read on identical terms for either status.
#[test]
fn the_resolution_reports_the_record_as_it_now_stands() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let created = create_draft_at_root(&root, Some("before")).unwrap();
    let id = created.draft.id.clone();
    save_draft_file_impl(&root, &id, "before.md", "body\n").unwrap();

    rename_draft_impl(&root, &id, "after").unwrap();
    set_draft_status_impl(&root, &id, DraftStatus::Archived).unwrap();

    let record = draft_record(&root, &id).unwrap();
    assert_eq!(record.name, "after");
    assert_eq!(record.status, DraftStatus::Archived);
    assert_eq!(record.prompt_path.as_deref(), Some("after.md"));

    let resolved = read_draft_prompt(&root, &id).unwrap();
    assert_eq!(resolved.prompt_path, "after.md");
    assert_eq!(resolved.content, "body\n", "an archived draft reads identically");
}

/// DRS-FR-28, DRS-FR-38, DRS-FR-36, DRS-FR-15: the three typed failures, and no directory created by any of
/// them.
#[test]
fn the_resolution_reports_its_three_typed_failures() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let created = create_draft_at_root(&root, Some("ok")).unwrap();
    let id = created.draft.id.clone();
    save_draft_file_impl(&root, &id, "ok.md", "body\n").unwrap();

    // An id no draft carries.
    let missing = "01JQZ0000000000000000000AA";
    assert!(draft_record(&root, missing).is_err());
    assert!(read_draft_prompt(&root, missing).is_err());
    assert!(
        !dir.path().join(".synthesis/drafts").join(missing).exists(),
        "DRS-FR-36: nothing is created at a composed path",
    );

    // A draft that is not the single prompt DRS-FR-11 requires.
    let broken = create_draft_at_root(&root, Some("broken")).unwrap().draft.id;
    let broken_dir = draft_dir(&root, &broken).unwrap();
    std::fs::write(broken_dir.join("files/intruder.md"), "second\n").unwrap();
    assert_eq!(draft_record(&root, &broken).unwrap_err(), ERR_NOT_SINGLE_FILE);
    assert_eq!(
        read_draft_prompt(&root, &broken).unwrap_err(),
        ERR_NOT_SINGLE_FILE
    );

    // DRS-FR-38: a prompt that will not decode is its own outcome, told
    // apart from both of the above — the draft exists and holds exactly one
    // file, so neither of those sentences would be true of it.
    let unreadable = create_draft_at_root(&root, Some("binary")).unwrap().draft.id;
    let unreadable_dir = draft_dir(&root, &unreadable).unwrap();
    std::fs::write(unreadable_dir.join("files/binary.md"), [0xff, 0xfe, 0x00]).unwrap();
    assert!(
        draft_record(&root, &unreadable).is_ok(),
        "the record resolves; only the prompt will not read",
    );
    assert_eq!(
        read_draft_prompt(&root, &unreadable).unwrap_err(),
        ERR_PROMPT_UNREADABLE,
    );
}
