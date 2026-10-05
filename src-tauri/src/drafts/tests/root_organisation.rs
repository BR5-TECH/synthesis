//! The organisation of the drafts root (DRS-FR-01, DRS-FR-06, DRS-FR-08,
//! DRS-FR-22, DRS-FR-29 … DRS-FR-34, DRS-FR-35).

use super::*;

// -----------------------------------------------------------------------
// The organisation of the drafts root (DRS-FR-01, DRS-FR-06, DRS-FR-08, DRS-FR-22, DRS-FR-29 … DRS-FR-34, DRS-FR-35)
// -----------------------------------------------------------------------

#[test]
fn ts23_folders_are_directories_and_the_hierarchy_survives_a_restart() {
    // DRS-FR-29: the directory structure IS the organisation. No manifest is
    // written, so there is nothing to reload and nothing to fall out of step
    // — a second `list_drafts` over the same disk returns the same tree.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    let created =
        create_draft_impl(root, Some("spec"), Some("UI/Components")).unwrap();

    assert!(root
        .join(".synthesis/drafts/UI/Components")
        .join(&created.draft.id)
        .join(RECORD_FILE)
        .is_file());

    let hierarchy = list_drafts_impl(root);
    assert_eq!(
        hierarchy.folders,
        vec![
            DraftFolder { path: "UI".into(), parent: "".into() },
            DraftFolder { path: "UI/Components".into(), parent: "UI".into() },
        ]
    );
    assert_eq!(hierarchy.drafts[0].folder, "UI/Components");

    // "Restarting" is re-reading the same disk through a fresh handle: the
    // hierarchy is recovered from the directories alone.
    let reopened = &crate::fs::RootFs::for_root(dir.path());
    let again = list_drafts_impl(reopened);
    assert_eq!(again.folders, hierarchy.folders);
    assert_eq!(again.drafts[0].id, created.draft.id);
    assert_eq!(again.drafts[0].folder, "UI/Components");

    // DRS-FR-29: and there is no second copy of any of it anywhere.
    let manifests: Vec<_> = tree_bytes(&root.join(".synthesis/drafts"))
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name != RECORD_FILE
                && name != CONVERSATION_FILE
                // DRS-FR-DGMI / DRS-FR-JDRY: the drafts root's merge
                // attributes and ignore rules, which are application-owned
                // project metadata rather than a record of the organisation
                // (DRS-FR-BKFG).
                && name != ".gitattributes"
                && name != ".gitignore"
                && !p.to_string_lossy().contains("/files/")
                // The draft's own version history, which is inside the
                // draft's folder and travels with it (DHS-FR-01) rather
                // than being a second copy of the organisation.
                && !p.to_string_lossy().contains("/history/")
        })
        .collect();
    assert!(manifests.is_empty(), "no manifest may exist: {manifests:?}");
}

#[test]
fn ts23_an_empty_folder_is_returned_like_any_other() {
    // DRS-FR-29: a folder is the author's structure rather than a
    // consequence of what is in it, so an empty one is not swallowed.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "research").unwrap();

    let hierarchy = list_drafts_impl(root);
    assert_eq!(folder_paths(root), vec!["research".to_string()]);
    assert!(hierarchy.drafts.is_empty());
}

#[test]
fn ts24_folder_names_are_validated_and_siblings_may_not_collide() {
    // DRS-FR-30: empty, separator-bearing and dot names are refused, and a
    // sibling of that name is refused case-insensitively.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    draft(root, "in-ui");

    for bad in ["", "a/b", "a\\b", "..", ".", "trailing.", "colon:name"] {
        assert!(
            create_drafts_folder_impl(root, "UI", bad).is_err(),
            "{bad:?} should be refused"
        );
    }
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);

    // A sibling of the same name, differing only in case. The whole
    // message, because `FsError::AlreadyExists` also renders with "already"
    // in it — and on a folding filesystem `create_dir` would raise that by
    // itself even if `name_taken` stopped comparing case at all, so a
    // `contains` here checks the module's rule only on ext4.
    let err = create_drafts_folder_impl(root, "", "ui").unwrap_err();
    assert_eq!(err, "a folder called \"ui\" is already here");
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);

    // A parent that does not exist is the typed "not found".
    let err = create_drafts_folder_impl(root, "nowhere", "x").unwrap_err();
    assert!(err.contains("no such drafts folder"), "got {err}");
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);

    // DRS-FR-30: a draft, by contrast, never collides — its folder is named
    // for its opaque id rather than for its name.
    let a = create_draft_impl(root, Some("same"), Some("UI")).unwrap();
    let b = create_draft_impl(root, Some("same"), Some("UI")).unwrap();
    assert_ne!(a.draft.id, b.draft.id);
    assert_eq!(
        listed(root).iter().filter(|d| d.folder == "UI").count(),
        2
    );
}

#[test]
fn ts24_a_folder_path_may_not_escape_the_drafts_root() {
    // DRS-FR-34: the same refusals a draft-relative path meets inside a
    // draft, applied to the drafts root.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();

    for bad in ["../escape", "/abs", "UI/../../escape"] {
        assert!(create_drafts_folder_impl(root, bad, "x").is_err(), "{bad:?}");
        assert!(rename_drafts_folder_impl(root, bad, "x").is_err(), "{bad:?}");
        assert!(delete_drafts_folder_impl(root, bad).is_err(), "{bad:?}");
        assert!(move_drafts_folder_impl(root, "UI", bad).is_err(), "{bad:?}");
    }
    assert!(!dir.path().join("escape").exists());
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
}

#[test]
fn ts25_deleting_a_folder_reparents_its_children_and_deletes_no_draft() {
    // DRS-FR-31: the drafts and folders it holds move to its parent; the
    // descendant hierarchy beneath each is preserved whole.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    let nested = create_draft_impl(root, Some("nested"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;
    let one = create_draft_impl(root, Some("one"), Some("UI")).unwrap().draft.id;
    let two = create_draft_impl(root, Some("two"), Some("UI")).unwrap().draft.id;
    save_draft_file_impl(root, &nested, "nested.md", "held").unwrap();

    delete_drafts_folder_impl(root, "UI").unwrap();

    assert_eq!(folder_paths(root), vec!["Components".to_string()]);
    assert_eq!(folder_of(root, &one), "");
    assert_eq!(folder_of(root, &two), "");
    assert_eq!(folder_of(root, &nested), "Components");
    assert_eq!(listed(root).len(), 3, "no draft was deleted");
    assert_eq!(
        load_draft_file_impl(root, &nested, "nested.md").unwrap().body,
        "held"
    );
}

#[test]
fn ts25_a_nested_folder_reparents_to_its_own_parent_not_the_root() {
    // DRS-FR-31: reparenting targets the deleted folder's own parent.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "research").unwrap();
    let id = create_draft_impl(root, Some("r"), Some("UI/research"))
        .unwrap()
        .draft
        .id;

    delete_drafts_folder_impl(root, "UI/research").unwrap();

    assert_eq!(folder_of(root, &id), "UI");
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
}

#[test]
fn ts26_a_delete_that_would_collide_refuses_the_whole_call() {
    // DRS-FR-31: every child is checked against the destination BEFORE the
    // first one moves, so the operation is all-or-nothing in every case a
    // caller can provoke.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "docs").unwrap();
    create_drafts_folder_impl(root, "UI", "other").unwrap();
    create_drafts_folder_impl(root, "", "docs").unwrap();

    let err = delete_drafts_folder_impl(root, "UI").unwrap_err();
    assert!(err.contains("docs"), "the collision is named: {err}");

    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "docs".to_string(),
            "UI".to_string(),
            "UI/docs".to_string(),
            "UI/other".to_string(),
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>(),
        "nothing moved — `other` would have succeeded and must not have run"
    );
}

#[test]
fn ts26_the_root_is_never_renamed_moved_or_deleted() {
    // DRS-FR-29: the implicit root always exists and no operation touches it.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();

    assert!(rename_drafts_folder_impl(root, "", "x").is_err());
    assert!(delete_drafts_folder_impl(root, "").is_err());
    assert!(move_drafts_folder_impl(root, "", "UI").is_err());
    assert!(root.join(".synthesis/drafts").is_dir());
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
}

#[test]
fn ts26_renaming_a_folder_keeps_its_parent_and_its_subtree() {
    // DRS-FR-30: a rename moves nothing — the folder keeps its parent and
    // everything beneath it, and every draft keeps its id and its files.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let id = create_draft_impl(root, Some("b"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;

    let renamed = rename_drafts_folder_impl(root, "UI", "Interface").unwrap();
    assert_eq!(renamed.path, "Interface");
    assert_eq!(renamed.parent, "");
    assert_eq!(folder_of(root, &id), "Interface/Components");
    assert_eq!(listed(root).len(), 1);

    // A sibling collision, case-insensitively, refuses and leaves the name.
    //
    // Only ext4 can show that this is *the module's* rule. On a folding
    // filesystem the same refusal arrives whether `name_taken` compares
    // case or not — `rename_path` finds `backend` when asked about
    // `BACKEND` and raises `AlreadyExists`, which `rename_entry_named`
    // renders as the identical string. Tightening this to `assert_eq!`
    // would not separate them; only the filesystem does, so the folder
    // list is what carries the claim on the platform the message cannot.
    let err = rename_drafts_folder_impl(root, "Interface", "BACKEND").unwrap_err();
    assert_eq!(err, "a folder called \"BACKEND\" is already here");
    assert_eq!(folder_of(root, &id), "Interface/Components");
    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "Interface".to_string(),
            "Interface/Components".to_string(),
            "backend".to_string(),
        ],
        "nothing was renamed and no third folder appeared"
    );

    // The current name is refused rather than silently accepted.
    assert!(rename_drafts_folder_impl(root, "Interface", "Interface").is_err());

    // A case-only rename is an ordinary act and goes through.
    rename_drafts_folder_impl(root, "Interface", "INTERFACE").unwrap();
    assert_eq!(folder_of(root, &id), "INTERFACE/Components");
}


#[test]
fn ts27_moving_a_draft_changes_only_the_path_its_directory_sits_at() {
    // DRS-FR-32: everything the draft holds travels with the directory, no
    // draft-relative path changes, and `updated_at` is refreshed.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let id = create_draft_impl(root, Some("spec"), Some("UI"))
        .unwrap()
        .draft
        .id;
    save_draft_file_impl(root, &id, "spec.md", "kept").unwrap();
    let before = read_record(root, &id).unwrap();

    let summary = move_draft_to_folder_impl(root, &id, "backend").unwrap();

    assert_eq!(summary.folder, "backend");
    assert!(root
        .join(".synthesis/drafts/backend")
        .join(&id)
        .join(RECORD_FILE)
        .is_file());
    let after = read_record(root, &id).unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.name, before.name);
    assert_eq!(after.prompt_path, before.prompt_path);
    assert_eq!(after.status, before.status);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(
        load_draft_file_impl(root, &id, "spec.md").unwrap().body,
        "kept",
        "no draft-relative path changed",
    );

    // Asking for the folder it already sits in is a refusal, not a no-op.
    let err = move_draft_to_folder_impl(root, &id, "backend").unwrap_err();
    assert!(err.contains("already"), "got {err}");

    // A destination that does not exist is the typed "not found".
    let err = move_draft_to_folder_impl(root, &id, "nowhere").unwrap_err();
    assert!(err.contains("no such drafts folder"), "got {err}");
    assert_eq!(folder_of(root, &id), "backend");
}

#[test]
fn ts27_a_draft_folder_is_never_a_move_destination() {
    // DRS-FR-32: `folder` must name a drafts folder or the root. A draft's
    // own directory is neither, however valid its path reads.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let host = draft(root, "host");
    let moving = draft(root, "moving");

    let err = move_draft_to_folder_impl(root, &moving, &host).unwrap_err();
    assert!(err.contains("no such drafts folder"), "got {err}");
    assert_eq!(folder_of(root, &moving), "");
}

#[test]
fn ts28_a_folder_is_never_moved_inside_itself() {
    // DRS-FR-33: itself, a descendant, and its current parent are each
    // refused, and a refused move touches nothing.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let id = create_draft_impl(root, Some("c"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;

    for bad in ["UI/Components", "UI"] {
        let err = move_drafts_folder_impl(root, "UI", bad).unwrap_err();
        assert!(err.contains("inside itself"), "{bad:?} got {err}");
    }
    // Its current parent: already there.
    let err = move_drafts_folder_impl(root, "UI", "").unwrap_err();
    assert!(err.contains("already there"), "got {err}");
    assert_eq!(folder_of(root, &id), "UI/Components");

    // The move that is legal carries the subtree whole.
    let moved = move_drafts_folder_impl(root, "UI/Components", "backend").unwrap();
    assert_eq!(moved.path, "backend/Components");
    assert_eq!(folder_of(root, &id), "backend/Components");
    assert_eq!(files_in(root, &id).len(), 1, "the draft kept its prompt");

    // A name already occupying the destination refuses.
    create_drafts_folder_impl(root, "", "Components").unwrap();
    let err = move_drafts_folder_impl(root, "backend/Components", "").unwrap_err();
    assert!(err.contains("already there"), "got {err}");
    assert_eq!(folder_of(root, &id), "backend/Components");
}

#[test]
fn ts29_a_stale_path_is_a_typed_not_found() {
    // DRS-FR-35: the paths ARE the state, so a folder renamed underneath a
    // caller is simply a path that no longer resolves.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    rename_drafts_folder_impl(root, "UI", "Interface").unwrap();

    let err = rename_drafts_folder_impl(root, "UI", "x").unwrap_err();
    assert!(err.contains("no such drafts folder"), "got {err}");
    assert!(!root.join(".synthesis/drafts/UI").exists(), "and nothing was created in its place");
    assert_eq!(folder_paths(root), vec!["Interface".to_string()]);
}

#[test]
fn ts29_a_draft_with_an_unreadable_record_is_reported_rather_than_dropped() {
    // DRS-FR-35: a damaged record never costs the author the draft, nor the
    // rest of the hierarchy.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let damaged = create_draft_impl(root, Some("damaged"), Some("UI"))
        .unwrap()
        .draft
        .id;
    let healthy = create_draft_impl(root, Some("healthy"), Some("UI"))
        .unwrap()
        .draft
        .id;
    let record = root.join(".synthesis/drafts/UI").join(&damaged).join(RECORD_FILE);
    std::fs::write(&record, b"this is not toml = = =").unwrap();

    let hierarchy = list_drafts_impl(root);
    assert_eq!(hierarchy.folders, vec![DraftFolder { path: "UI".into(), parent: "".into() }]);
    assert!(
        hierarchy.drafts.iter().any(|d| d.id == damaged),
        "the damaged draft is still listed"
    );
    assert!(hierarchy.drafts.iter().any(|d| d.id == healthy));
    assert!(record.is_file(), "and its folder was neither moved nor deleted");
}

#[test]
fn a_deleted_draft_leaves_its_folder_standing() {
    // DRS-FR-21: the folder the draft was filed in is the author's structure
    // and outlives what was in it.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("UI")).unwrap().draft.id;

    delete_draft_impl(root, root, &id).unwrap();

    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
    assert!(listed(root).is_empty());
}


#[test]
fn a_draft_keeps_its_identity_and_its_proposals_across_a_move() {
    // DRS-FR-32: the comment logs, the proposals and the conversation log
    // travel with the directory, because they are inside it.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let id = draft(root, "d");
    std::fs::write(
        draft_proposals_dir(root, &id).unwrap().join("p.toml"),
        b"marker",
    )
    .unwrap();

    move_draft_to_folder_impl(root, &id, "UI").unwrap();

    assert_eq!(
        std::fs::read(draft_proposals_dir(root, &id).unwrap().join("p.toml")).unwrap(),
        b"marker"
    );
    assert!(root
        .join(".synthesis/drafts/UI")
        .join(&id)
        .join(RECORD_FILE)
        .is_file());
}

#[test]
fn a_folder_path_may_not_cross_into_a_draft() {
    // DRS-FR-29 / DRS-FR-34: `files`, `comments`, `proposals` and a draft's
    // own id are all syntactically legal folder names, so a path can read as
    // an ordinary drafts-folder path while pointing INSIDE a draft. The walk
    // stops at a draft folder, so anything filed under one would be
    // invisible to `list_drafts` for good — with no command here able to
    // reach it again.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let id = create_draft_impl(root, Some("host"), Some("UI")).unwrap().draft.id;

    for inside in [
        format!("UI/{id}"),
        format!("UI/{id}/files"),
        format!("UI/{id}/comments"),
        format!("UI/{id}/proposals"),
    ] {
        assert!(
            create_drafts_folder_impl(root, &inside, "x").is_err(),
            "creating under {inside:?} must be refused"
        );
        assert!(
            create_draft_impl(root, Some("d"), Some(&inside)).is_err(),
            "filing a draft under {inside:?} must be refused"
        );
        assert!(
            move_drafts_folder_impl(root, "UI", &inside).is_err(),
            "moving a folder into {inside:?} must be refused"
        );
        assert!(
            move_draft_to_folder_impl(root, &id, &inside).is_err(),
            "filing a draft into {inside:?} must be refused"
        );
        assert!(
            delete_drafts_folder_impl(root, &inside).is_err(),
            "deleting {inside:?} must be refused"
        );
        assert!(
            rename_drafts_folder_impl(root, &inside, "x").is_err(),
            "renaming {inside:?} must be refused"
        );
    }

    // Nothing was created inside the draft, and the draft is intact.
    assert_eq!(files_in(root, &id).len(), 1, "the draft still holds its one prompt");
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
    assert_eq!(folder_of(root, &id), "UI");
}

#[test]
fn a_folder_is_never_moved_inside_itself_whatever_the_capitalisation() {
    // DRS-FR-33: on APFS and NTFS `ui/Components` IS `UI/Components`, so a
    // byte-comparison would let the destination past the containment check
    // and hand the caller a raw kernel error instead of the typed one.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();

    for bad in ["ui", "ui/Components", "UI/components"] {
        let err = move_drafts_folder_impl(root, "UI", bad).unwrap_err();
        assert!(err.contains("inside itself"), "{bad:?} got {err}");
    }
    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(paths, vec!["UI".to_string(), "UI/Components".to_string()]);
}

#[test]
fn a_draft_already_in_the_destination_is_refused_whatever_the_capitalisation() {
    // DRS-FR-32: the same hazard on the "already there" check.
    //
    // And the same answer on every filesystem. `ui` names an existing
    // directory on APFS and nothing at all on ext4, so a check that runs
    // after the destination is resolved answers "already there" on a
    // developer's machine and "no such drafts folder" in CI — one state,
    // two contracts, and the divergence only ever shows up on the platform
    // the tests are not run on.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("UI")).unwrap().draft.id;

    let err = move_draft_to_folder_impl(root, &id, "ui").unwrap_err();
    assert!(err.contains("already in that folder"), "got {err}");
    assert_eq!(folder_of(root, &id), "UI");
}

#[test]
fn a_folder_already_in_the_destination_is_refused_whatever_the_capitalisation() {
    // DRS-FR-33: the folder move carries the identical hazard on its own
    // "already there" check, and answers the same on both filesystems.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;

    // The whole message, not a substring of it: "already there" also ends
    // the collision refusal a few lines below the one under test, so a
    // `contains` would pass just as happily if the two checks swapped and
    // the caller were told a name was taken rather than that it had asked
    // for nothing.
    let err = move_drafts_folder_impl(root, "UI/Components", "ui").unwrap_err();
    assert_eq!(err, "this folder is already there");

    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(paths, vec!["UI".to_string(), "UI/Components".to_string()]);
    assert_eq!(folder_of(root, &id), "UI/Components");
}

#[test]
fn a_delete_refuses_a_folder_holding_anything_that_is_not_a_folder() {
    // DRS-FR-31: refused before the first child moves. Left to the
    // non-recursive removal at the end, this would come back as a failure
    // *after* every child had already been reparented — the partial result
    // the pre-flight exists to make impossible.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "Components").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("UI")).unwrap().draft.id;
    std::fs::write(root.join(".synthesis/drafts/UI/stray.txt"), b"x").unwrap();

    let err = delete_drafts_folder_impl(root, "UI").unwrap_err();
    assert!(err.contains("stray.txt"), "the stray entry is named: {err}");
    assert!(err.contains("nothing was moved"), "got {err}");

    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(paths, vec!["UI".to_string(), "UI/Components".to_string()]);
    assert_eq!(folder_of(root, &id), "UI");
    assert!(root.join(".synthesis/drafts/UI/stray.txt").is_file());
}

#[test]
fn a_delete_refuses_a_child_that_would_collide_case_insensitively_in_the_parent() {
    // DRS-FR-31: the first pre-flight loop folds case, and unlike the
    // child-against-child one below this state is buildable through the
    // module's own API on either filesystem — `Docs` is free at the root
    // while only `UI` is there, and `docs` is free inside `UI`. So the fold
    // at that check is testable with no probe and no platform split at all,
    // which is the better way to cover it where it is available.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "", "Docs").unwrap();
    create_drafts_folder_impl(root, "UI", "docs").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("UI/docs")).unwrap().draft.id;

    let err = delete_drafts_folder_impl(root, "UI").unwrap_err();
    assert_eq!(
        err,
        "\"docs\" cannot move out of \"UI\": something of that name is already there"
    );

    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(
        paths,
        vec!["Docs".to_string(), "UI".to_string(), "UI/docs".to_string()],
        "refused before anything moved"
    );
    assert_eq!(folder_of(root, &id), "UI/docs");
}

#[test]
fn a_delete_refuses_two_children_that_would_collide_with_each_other() {
    // DRS-FR-31: the pre-flight checks each child against the destination.
    // Two children that differ only in case pass that and then collide with
    // ONE ANOTHER in the destination — after the first has already moved.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "UI", "docs").unwrap();
    // Created behind this module's back, because `create_drafts_folder`
    // refuses the case-insensitive sibling itself (DRS-FR-30) — which is
    // exactly why this state only arrives from outside the application.
    let made = std::fs::create_dir(root.join(".synthesis/drafts/UI/Docs"));
    if crate::fs::case_probe::folds_case(dir.path()) {
        // A folding filesystem cannot hold the pair at all: `Docs` IS
        // `docs`. That is the honest reason there is nothing to defend
        // against here — asserted rather than inferred from a failure that
        // could equally have been a permissions problem or a missing
        // parent, either of which would turn this test into a silent no-op
        // that still reports green.
        assert!(
            matches!(&made, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists),
            "the pair is one directory on a folding filesystem: {made:?}"
        );
        // And then the behaviour this filesystem *does* have, so the arm is
        // about DRS-FR-31 rather than about `create_dir`: with only the one
        // child there is nothing to collide with, and the delete goes
        // through and reparents it.
        delete_drafts_folder_impl(root, "UI").unwrap();
        assert_eq!(
            folder_paths(root),
            vec!["docs".to_string()],
            "the child was reparented to the root and the folder is gone"
        );
        return;
    }
    made.expect("a case-sensitive filesystem holds both names at once");

    let err = delete_drafts_folder_impl(root, "UI").unwrap_err();
    assert!(err.contains("nothing was moved"), "got {err}");
    assert!(root.join(".synthesis/drafts/UI/docs").is_dir());
    assert!(root.join(".synthesis/drafts/UI/Docs").is_dir());
}

#[test]
fn a_case_only_folder_rename_onto_a_real_sibling_is_put_back_under_its_own_name() {
    // The second leg of the staging two-step, and the rollback behind it.
    //
    // Only reachable on a case-sensitive filesystem, and for a reason worth
    // stating: the collision check skips a case-only change deliberately
    // (`rename_drafts_folder_impl`), so with a genuine `INTERFACE` sitting
    // beside `Interface` — a pair only ext4 can hold, and only from outside
    // this application — the rename gets past the check, stages, and then
    // fails on the second leg. If the rollback did not run, the author's
    // folder would be left parked under `.Interface.synthesis-rename`,
    // which DRS-FR-11's dot filter hides from the rail: the folder and
    // everything beneath it would look deleted.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "Interface").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("Interface"))
        .unwrap()
        .draft
        .id;

    let made = std::fs::create_dir(root.join(".synthesis/drafts/INTERFACE"));
    if crate::fs::case_probe::folds_case(dir.path()) {
        assert!(
            matches!(&made, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists),
            "on a folding filesystem the sibling IS the folder, so the \
             second leg has nothing to collide with: {made:?}"
        );
        // And there the same rename is the ordinary capitalisation it looks
        // like, which is the behaviour that filesystem does have.
        let renamed = rename_drafts_folder_impl(root, "Interface", "INTERFACE").unwrap();
        assert_eq!(renamed.path, "INTERFACE");
        assert_eq!(folder_of(root, &id), "INTERFACE");
        return;
    }
    made.expect("a case-sensitive filesystem holds both names at once");

    let err = rename_drafts_folder_impl(root, "Interface", "INTERFACE").unwrap_err();
    assert_eq!(err, "a folder called \"INTERFACE\" is already here");

    // Put back under its own name — not left under the staging one, and
    // the draft beneath it still reachable by the path it had.
    let mut paths = folder_paths(root);
    paths.sort();
    assert_eq!(paths, vec!["INTERFACE".to_string(), "Interface".to_string()]);
    assert_eq!(folder_of(root, &id), "Interface");
    assert_eq!(files_in(root, &id).len(), 1, "the draft kept its prompt");
}

#[cfg(unix)]
#[test]
fn a_folder_path_reaching_through_a_symlink_is_refused() {
    // DRS-FR-34: `resolve_under` is syntactic and cannot see a link, so a
    // link inside the drafts root resolves to a path that is textually
    // inside it and physically anywhere.
    use std::os::unix::fs::symlink;
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(outside.join("secret")).unwrap();
    symlink(&outside, root.join(".synthesis/drafts/UI/out")).unwrap();

    assert!(create_drafts_folder_impl(root, "UI/out", "x").is_err());
    assert!(delete_drafts_folder_impl(root, "UI/out").is_err());
    assert!(rename_drafts_folder_impl(root, "UI/out", "x").is_err());
    assert!(move_drafts_folder_impl(root, "UI", "UI/out").is_err());
    assert!(create_draft_impl(root, Some("d"), Some("UI/out")).is_err());
    assert!(!outside.join("x").exists(), "nothing was created outside the root");
    assert!(outside.join("secret").is_dir(), "and nothing outside was removed");

    // …and the link is not walked into, so its contents never reach the
    // panel as folders of the author's.
    assert_eq!(folder_paths(root), vec!["UI".to_string()]);
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_under_the_drafts_root_is_not_listed() {
    // A link to a directory elsewhere would otherwise put that directory's
    // contents in the Drafts panel.
    use std::os::unix::fs::symlink;
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(elsewhere.join("private")).unwrap();
    std::fs::create_dir_all(root.join(".synthesis/drafts")).unwrap();
    symlink(&elsewhere, root.join(".synthesis/drafts/linked")).unwrap();

    let hierarchy = list_drafts_impl(root);
    assert!(hierarchy.folders.is_empty(), "got {:?}", hierarchy.folders);
    assert!(hierarchy.drafts.is_empty());
}

#[test]
fn a_stale_path_is_a_typed_not_found_for_every_operation() {
    // DRS-FR-35 across the whole group, not just rename: the paths ARE the
    // state, so a folder removed underneath a caller is a path that no
    // longer resolves.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    create_drafts_folder_impl(root, "", "backend").unwrap();
    let id = create_draft_impl(root, Some("d"), Some("backend")).unwrap().draft.id;
    // Removed behind this module's back, as another process would.
    std::fs::remove_dir_all(root.join(".synthesis/drafts/UI")).unwrap();

    for err in [
        rename_drafts_folder_impl(root, "UI", "x").unwrap_err(),
        delete_drafts_folder_impl(root, "UI").unwrap_err(),
        move_drafts_folder_impl(root, "backend", "UI").unwrap_err(),
        move_draft_to_folder_impl(root, &id, "UI").unwrap_err(),
        create_drafts_folder_impl(root, "UI", "x").unwrap_err(),
        create_draft_impl(root, Some("d2"), Some("UI")).unwrap_err(),
    ] {
        assert!(err.contains("no such drafts folder"), "got {err}");
    }
    assert_eq!(folder_paths(root), vec!["backend".to_string()]);
    assert_eq!(folder_of(root, &id), "backend");
}

#[test]
fn a_draft_created_with_no_folder_lands_at_the_root_even_when_folders_exist() {
    // DRS-FR-07: `folder = None` is the drafts root, not "wherever".
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let id = create_draft_at_root(root, Some("loose")).unwrap().draft.id;
    assert_eq!(folder_of(root, &id), "");
}
