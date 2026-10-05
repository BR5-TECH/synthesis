//! Prompt activity (DRS-FR-24, DRS-FR-41).

use super::*;

// -----------------------------------------------------------------------
// DRS-FR-24 — prompt activity (DRS-FR-41)
// -----------------------------------------------------------------------

/// Give `path` an exact modification time, so the assertions below do not
/// rest on how fast the test ran.
fn set_mtime(path: &Path, unix_secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs),
    ))
    .unwrap();
}

/// DRS-FR-41, DRS-FR-24: `prompt_activity_at` is the prompt file's modification time and
/// **nothing else**. Everything that moves `updated_at` — a rename, an
/// archive, a move between folders — and every write under the draft's
/// sibling folders leaves it exactly where it was; saving the prompt itself
/// moves it to the instant of that write.
#[test]
fn ts37_prompt_activity_follows_the_prompt_alone() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let created = create_draft_impl(root, Some("spec"), None).unwrap();
    let id = created.draft.id.clone();
    let prompt_path = dir
        .path()
        .join(DRAFTS_REL)
        .join(&id)
        .join(FILES_DIR)
        .join(&created.file);
    set_mtime(&prompt_path, 1_000);

    let activity = |root: &crate::fs::RootFs| -> String {
        let listed = listed(root);
        let row = listed.iter().find(|d| d.id == id).expect("the draft");
        let from_row = row.prompt_activity_at.clone().expect("an activity instant");
        assert_eq!(
            Some(from_row.clone()),
            draft_prompt_activity(root, &id).unwrap(),
            "the row and the id-only call report the same instant"
        );
        from_row
    };

    let at_creation = activity(root);
    assert_eq!(at_creation, "1970-01-01T00:16:40.000Z");

    // The record moves, the prompt does not.
    rename_draft_impl(root, &id, "renamed").unwrap();
    assert_eq!(activity(root), at_creation, "a rename moves no prompt");
    set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    assert_eq!(activity(root), at_creation, "nor does a status change");
    set_draft_status_impl(root, &id, DraftStatus::Active).unwrap();
    move_draft_to_folder_impl(root, &id, "UI").unwrap();
    assert_eq!(activity(root), at_creation, "nor a move between folders");

    // Nor does anything written beside the prompt.
    let draft_dir_path = dir.path().join(DRAFTS_REL).join("UI").join(&id);
    std::fs::write(draft_dir_path.join("assets/img.png"), b"bytes").unwrap();
    std::fs::write(draft_dir_path.join("proposals/p1.md"), "candidate\n").unwrap();
    std::fs::write(draft_dir_path.join("history/h1.snapshot"), "version\n").unwrap();
    std::fs::write(draft_dir_path.join("conversation.jsonl"), "line\n").unwrap();
    assert_eq!(
        activity(root),
        at_creation,
        "a write under assets/, proposals/, history/, comments/ or the \
         conversation log is not prompt activity"
    );
    assert!(
        listed(root).iter().find(|d| d.id == id).unwrap().updated_at >= created.draft.updated_at,
        "while `updated_at` has moved with the record all along (DRS-FR-24)"
    );

    // Saving the prompt is the one thing that moves it.
    save_draft_file_impl(root, &id, "renamed.md", "# rewritten\n").unwrap();
    assert!(
        activity(root) > at_creation,
        "the prompt's own write moves the instant"
    );
}

/// DRS-FR-41: the walk-free reading agrees with the id-only one, which is
/// what lets the drafts-root watch (DRS-FR-42) answer from the path an event
/// names rather than by walking the drafts root on every batch.
#[test]
fn prompt_activity_in_a_known_directory_agrees_with_the_id_only_call() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let created = create_draft_impl(root, Some("spec"), Some("UI")).unwrap();
    let id = created.draft.id.clone();
    let draft_dir_path = dir.path().join(DRAFTS_REL).join("UI").join(&id);
    set_mtime(&draft_dir_path.join(FILES_DIR).join(&created.file), 4_000);

    assert_eq!(
        prompt_activity_in(root, &draft_dir_path, &id),
        draft_prompt_activity(root, &id).unwrap(),
    );
    assert_eq!(
        prompt_activity_in(root, &draft_dir_path, &id).as_deref(),
        Some("1970-01-01T01:06:40.000Z"),
    );

    // A directory holding no record answers `None` rather than erroring:
    // to the watch there is simply no prompt activity to report there.
    assert_eq!(
        prompt_activity_in(root, &dir.path().join(DRAFTS_REL).join("UI"), &id),
        None,
    );
}

/// DRS-FR-41 / DRS-FR-15: an inconsistent draft has no single prompt to
/// stat, so it carries no activity instant and the id-only call refuses.
#[test]
fn an_inconsistent_draft_carries_no_prompt_activity() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = create_draft_impl(root, Some("spec"), None).unwrap();
    let id = created.draft.id.clone();
    std::fs::write(
        dir.path().join(DRAFTS_REL).join(&id).join(FILES_DIR).join("second.md"),
        "by hand\n",
    )
    .unwrap();

    let row = listed(root).into_iter().find(|d| d.id == id).expect("still listed");
    assert!(row.inconsistent, "precondition: the draft is inconsistent");
    assert_eq!(row.prompt_activity_at, None);
    assert_eq!(
        draft_prompt_activity(root, &id).unwrap_err(),
        ERR_NOT_SINGLE_FILE,
    );
}

#[test]
fn ts31_an_empty_directory_named_for_a_draft_is_left_alone() {
    // DRS-FR-37: misplaced *storage* is what is reunited. An empty directory
    // holds none, and is far likelier a folder the author has not filled
    // than the residue of a write that never happened.
    let (dir, id) = project_with_filed_draft();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", &id).unwrap();

    let (_, repaired) = list_drafts_reporting(root);

    assert!(repaired.is_empty());
    assert!(root.join(DRAFTS_REL).join(&id).is_dir());
    assert!(folder_paths(root).contains(&id));
}
