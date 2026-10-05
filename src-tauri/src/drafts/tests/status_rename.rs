//! Status changes and renames.

use super::*;

// -- status / rename ---------------------------------------------------

#[test]
fn status_moves_between_its_two_positions_reversibly() {
    // DRS-FR-10 / NAW-FR-16.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");

    // DRS-FR-06: a draft is created active.
    assert_eq!(read_record(root, &id).unwrap().status, DraftStatus::Active);

    let archived = set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    assert_eq!(archived.status, DraftStatus::Archived);
    let back = set_draft_status_impl(root, &id, DraftStatus::Active).unwrap();
    assert_eq!(back.status, DraftStatus::Active);
}

#[test]
fn archiving_moves_the_record_and_not_one_byte_under_the_draft() {
    // DRS-FR-06, DRS-FR-22 (DRS-FR-10): archiving retires a draft from the panel's
    // default view and takes nothing off disk.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    save_draft_file_impl(root, &id, "d.md", "the body\n").unwrap();

    // Non-empty, so "not one comment log or conversation log is moved,
    // rewritten, or removed" is asserted rather than vacuously true of two
    // empty artefacts.
    let home = draft_dir(root, &id).unwrap();
    std::fs::write(home.join(PROPOSALS_DIR).join("p.toml"), "{\"a\":1}\n").unwrap();
    std::fs::write(home.join(CONVERSATION_FILE), "{\"turn\":1}\n").unwrap();

    let before = tree_bytes(&home);
    let record_before = read_record(root, &id).unwrap();
    set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    let after = tree_bytes(&home);
    // Everything under the draft except the record itself, byte-for-byte.
    let without_record = |t: Vec<(PathBuf, Vec<u8>)>| {
        t.into_iter()
            .filter(|(p, _)| p.file_name().unwrap() != RECORD_FILE)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        without_record(before),
        without_record(after),
        "archiving moved something under the draft"
    );

    // And the record moved in exactly one field. `updated_at` is the
    // draft's last-activity instant (DRS-FR-24), so it is expected to move;
    // nothing else is.
    let record_after = read_record(root, &id).unwrap();
    assert_eq!(record_after.status, DraftStatus::Archived);
    assert_eq!(
        DraftRecord {
            status: record_before.status,
            updated_at: record_before.updated_at.clone(),
            ..record_after.clone()
        },
        record_before,
        "archiving rewrote more of the record than the status"
    );
    // DRS-FR-03: `created_at` is never rewritten after creation.
    assert_eq!(record_after.created_at, record_before.created_at);

    // DRP-FR-07 / DRP-FR-08: the panel filters and groups by what the LIST
    // reports, which is a separate read from the record.
    assert_eq!(listed(root)[0].status, DraftStatus::Archived);

    // The wire strings the frontend's `DraftStatus` union is written
    // against (`src/types.ts`). A rename here — a `#[serde(rename)]`, a
    // different casing — reaches the UI as a status it cannot match, and
    // every Rust test comparing enum variants stays green through it.
    let toml = std::fs::read_to_string(home.join(RECORD_FILE)).unwrap();
    assert!(toml.contains("status = \"archived\""), "{toml}");
    set_draft_status_impl(root, &id, DraftStatus::Active).unwrap();
    let toml = std::fs::read_to_string(home.join(RECORD_FILE)).unwrap();
    assert!(toml.contains("status = \"active\""), "{toml}");
}

#[test]
fn a_record_carrying_a_status_this_enum_never_had_still_lists_and_opens() {
    // A `draft.toml` written before the two positions were named carries a
    // value serde cannot parse, and a parse failure fails the WHOLE record:
    // `read_record` reports "draft not found" and `list_drafts_impl` skips
    // what it cannot read, so the draft disappears from the panel with no
    // error anywhere while its files sit untouched on disk. Every draft on
    // an existing install is one of these.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    let home = draft_dir(root, &id).unwrap();

    for legacy in ["draft", "ready", "something-else-entirely"] {
        std::fs::write(
            home.join(RECORD_FILE),
            format!(
                "id = \"{id}\"\nname = \"spec\"\nprimaryPath = \"spec.md\"\nstatus = \"{legacy}\"\ncreatedAt = \"2026-07-31T08:00:00Z\"\nupdatedAt = \"2026-07-31T08:00:00Z\"\n"
            ),
        )
        .unwrap();

        let record = read_record(root, &id)
            .unwrap_or_else(|e| panic!("legacy status {legacy:?} was unreadable: {e}"));
        // Read as active, so it lands in the panel's default position
        // rather than in the one that hides it.
        assert_eq!(record.status, DraftStatus::Active, "status {legacy:?}");
        assert_eq!(record.name, "spec");
        assert_eq!(record.prompt_path.as_deref(), Some("spec.md"));
        assert_eq!(listed(root).len(), 1, "status {legacy:?}");
        assert_eq!(files_in(root, &id), vec!["spec.md"]);
    }
}


#[test]
fn every_command_answers_the_same_for_an_archived_draft() {
    // DRS-FR-06, DRS-FR-22 (DRS-FR-10): the status governs nothing this module does.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = archived_draft(root, "d");

    assert_eq!(files_in(root, &id), vec!["d.md"]);
    save_draft_file_impl(root, &id, "d.md", "invariant\n").unwrap();
    assert_eq!(load_draft_file_impl(root, &id, "d.md").unwrap().body, "invariant\n");
    assert_eq!(search_drafts_impl(root, "invariant").len(), 1);
    assert_eq!(listed(root).len(), 1);
    // A rename leaves the position where it was, rather than reviving it.
    let renamed = rename_draft_impl(root, &id, "other").unwrap();
    assert_eq!(renamed.status, DraftStatus::Archived);
    assert_eq!(files_in(root, &id), vec!["other.md"]);
}


#[test]
fn renaming_to_an_empty_name_is_refused_and_leaves_the_name_untouched() {
    // DRP-FR-11 / DRS-FR-09: a refusal, not a rename to nothing — and it
    // leaves the primary file alone as well as the stored name.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "keep");

    assert!(rename_draft_impl(root, &id, "   ").is_err());

    assert_eq!(read_record(root, &id).unwrap().name, "keep");
    assert_eq!(files_in(root, &id), vec!["keep.md"]);
}

#[test]
fn an_id_and_a_folder_outlive_every_rename_while_the_primary_file_follows() {
    // DRS-FR-02 (DRS-FR-02, DRS-FR-09, DRS-FR-10, DRS-FR-25): the draft is
    // identified by its id, so renaming it moves its name and its file and
    // nothing else.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "first");
    let home = draft_dir(root, &id).unwrap();

    rename_draft_impl(root, &id, "second").unwrap();
    set_draft_status_impl(root, &id, DraftStatus::Archived).unwrap();
    let record = rename_draft_impl(root, &id, "third").unwrap();
    set_draft_status_impl(root, &id, DraftStatus::Active).unwrap();

    assert_eq!(record.id, id);
    assert_eq!(draft_dir(root, &id).unwrap(), home);
    assert_eq!(files_in(root, &id), vec!["third.md"]);
}
