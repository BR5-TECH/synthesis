//! Creating a draft, its record, and the name that binds its one prompt file.

use super::*;

// -- creation / record ------------------------------------------------

#[test]
fn create_draft_writes_a_record_and_one_markdown_file() {
    // DRS-FR-06: a draft is never created empty — the workspace opens on
    // something to type into rather than on an empty workspace.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let created = create_draft_at_root(root, Some("artifact-window")).unwrap();

    let home = root.join(DRAFTS_REL).join(&created.draft.id);
    assert!(home.join(RECORD_FILE).is_file());
    assert!(home.join(FILES_DIR).is_dir());
    assert_eq!(created.draft.name, "artifact-window");
    assert_eq!(created.draft.status, DraftStatus::Active);
    // DRS-FR-25: the one file, named from the draft's own name, reported
    // back so the tab can open on it and recorded as the primary file the
    // name stays bound to.
    assert_eq!(created.file, "artifact-window.md");
    assert_eq!(created.draft.prompt_path.as_deref(), Some("artifact-window.md"));
    assert_eq!(
        require_prompt(root, &created.draft.id).unwrap(),
        "artifact-window.md"
    );
    assert_eq!(
        load_draft_file_impl(root, &created.draft.id, &created.file)
            .unwrap()
            .body,
        ""
    );
    assert!(!listed(root)[0].inconsistent);
    // DRS-FR-01: the three storage folders, each scaffolded and each empty.
    // DRS-FR-01: the draft's review is not among them — it stands in the
    // repository machine store under the draft's stable id (per
    // `CMS-comments-storage.md` CMS-FR-37).
    assert!(!home.join("comments").exists(), "no review folder in the draft");
    for folder in [PROPOSALS_DIR, HISTORY_DIR] {
        assert!(home.join(folder).is_dir(), "{folder} was not scaffolded");
    }
    // DRS-FR-23: a draft is created with no conversation log.
    assert!(!home.join(CONVERSATION_FILE).exists());
    // DRS-FR-06 / DHS-FR-07: creating a draft settles no version. The live
    // prompt is the `Original` until a change is accepted against it, so
    // `history/` is created empty and the rail has one thing to show — the
    // prompt itself — rather than an empty snapshot beside it.
    assert_eq!(
        std::fs::read_dir(home.join(HISTORY_DIR)).unwrap().count(),
        0,
        "creating a draft wrote something into its history"
    );
}

/// Put a second file into a draft's `files/` by hand — which is the only way
/// one can get there, no operation anywhere creating a file inside a draft
/// (DRS-FR-13). What DRS-FR-15 is about.
fn smuggle(root: &crate::fs::RootFs, id: &str, name: &str) {
    let files = draft_dir(root, id).unwrap().join(FILES_DIR);
    std::fs::write(files.join(name), "put here by hand").unwrap();
}


#[test]
fn a_draft_holding_anything_but_its_one_prompt_is_reported_and_never_repaired() {
    // DRS-FR-11 (DRS-FR-11, DRS-FR-15, DRS-FR-21).
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    smuggle(root, &id, "notes.md");

    let before = tree_bytes(&draft_dir(root, &id).unwrap());
    let rows = listed(root);
    assert_eq!(rows.len(), 1, "the draft is reported rather than dropped");
    assert!(rows[0].inconsistent);
    assert_eq!(rows[0].name, "d", "every field that could be recovered");

    for refusal in [
        open_draft_impl(root, &id).err(),
        load_draft_file_impl(root, &id, FIRST_FILE).err(),
        save_draft_file_impl(root, &id, FIRST_FILE, "x").err(),
        require_prompt(root, &id).err(),
    ] {
        assert_eq!(refusal.as_deref(), Some(ERR_NOT_SINGLE_FILE));
    }
    assert_eq!(
        tree_bytes(&draft_dir(root, &id).unwrap()),
        before,
        "nothing was deleted, moved, or rewritten, and no file was chosen as \
         the prompt",
    );

    // DRS-FR-21: deleting it is the one thing left to do, and it works on the
    // ordinary terms.
    delete_draft_impl(root, root, &id).unwrap();
    assert!(listed(root).is_empty());
}

#[test]
fn a_folder_inside_a_draft_makes_it_inconsistent_too() {
    // DRS-FR-11: no folder, no second file, and no nested path anywhere.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    std::fs::create_dir(draft_dir(root, &id).unwrap().join(FILES_DIR).join("ui")).unwrap();
    assert!(listed(root)[0].inconsistent);
    assert_eq!(require_prompt(root, &id).unwrap_err(), ERR_NOT_SINGLE_FILE);
}

#[test]
fn an_os_dropping_beside_the_prompt_leaves_the_draft_consistent() {
    // DRS-FR-11: a `.DS_Store` is not a second document, and reading one as
    // one would make a draft unopenable for a reason the author neither
    // caused nor can see.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    std::fs::write(
        draft_dir(root, &id).unwrap().join(FILES_DIR).join(".DS_Store"),
        [0u8, 1, 2],
    )
    .unwrap();
    assert!(!listed(root)[0].inconsistent);
    assert_eq!(require_prompt(root, &id).unwrap(), FIRST_FILE);
}

#[test]
fn creating_a_draft_settles_no_version_and_announces_none() {
    // DHS-FR-08, DHS-FR-01, DHS-FR-02, DHS-FR-03, DHS-FR-04 / DHS-FR-16 (DHS-FR-07, DHS-FR-22): creating a draft records
    // no version, so no surface is told one appeared. Asserted against the
    // **command**, because that is where the handle to emit through exists —
    // the impl has none, and a test over the impl alone would leave the
    // silence untested by testing nothing that could have spoken.
    use tauri::{Listener, Manager};
    let dir = project();
    let app = crate::tools::tests::mock_app();
    app.manage(ProjectState::default());
    app.state::<ProjectState>().set_root(dir.path().to_path_buf());

    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&seen);
    app.listen(crate::draft_history::HISTORY_CHANGED, move |event| {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
        }
    });

    let created = create_draft(
        Some("spec".into()),
        None,
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect("created");

    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        events,
        Vec::<serde_json::Value>::new(),
        "creating a draft announced a version it did not create"
    );
    // …and the draft it did create holds none, its live prompt being the
    // `Original` (DHS-FR-07).
    let root = crate::fs::RootFs::for_root(dir.path());
    assert!(crate::draft_history::list_impl(&root, &created.draft.id)
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn a_draft_lives_only_under_the_drafts_folder() {
    // NAW non-functional requirements / ASC-FR-09: nothing a draft holds is
    // written anywhere the scan would surface it.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let id = draft(root, "d");
    save_draft_file_impl(root, &id, FIRST_FILE, "# hello").unwrap();

    let stray: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".synthesis")
        .collect();
    assert!(stray.is_empty(), "draft content escaped .synthesis: {stray:?}");
}

#[test]
fn an_unnamed_draft_is_untitled_and_then_untitled_n() {
    // DRS-FR-06 (DRS-FR-06, DRS-FR-25, DRS-FR-27).
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let first = create_draft_at_root(root, None).unwrap();
    assert_eq!(first.draft.name, "Untitled");
    assert_eq!(first.file, "Untitled.md");
    assert_eq!(first.draft.prompt_path.as_deref(), Some("Untitled.md"));
    assert_eq!(
        load_draft_file_impl(root, &first.draft.id, "Untitled.md").unwrap().body,
        ""
    );

    // A worktree accumulating unnamed drafts lists them distinguishably
    // rather than as a column of identical rows. Whitespace is no name.
    let second = create_draft_at_root(root, None).unwrap();
    let third = create_draft_at_root(root, Some("   ")).unwrap();
    assert_eq!(second.draft.name, "Untitled 2");
    assert_eq!(second.file, "Untitled 2.md");
    assert_eq!(third.draft.name, "Untitled 3");
    assert_eq!(third.file, "Untitled 3.md");

    // DRS-FR-27: only the auto-assigned default is disambiguated. A name the
    // caller supplies is stored verbatim, whoever else already carries it.
    let supplied = create_draft_at_root(root, Some("Untitled")).unwrap();
    assert_eq!(supplied.draft.name, "Untitled");
    assert_ne!(supplied.draft.id, first.draft.id);
}

#[test]
fn a_gap_in_the_untitled_run_is_filled_rather_than_skipped_past() {
    // DRS-FR-27: the *first* free `Untitled N`, so deleting one and creating
    // another reuses the name rather than counting ever upwards.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    create_draft_at_root(root, None).unwrap();
    let second = create_draft_at_root(root, None).unwrap();
    create_draft_at_root(root, None).unwrap();
    delete_draft_impl(root, root, &second.draft.id).unwrap();

    assert_eq!(create_draft_at_root(root, None).unwrap().draft.name, "Untitled 2");
}

#[test]
fn a_name_derives_a_filename_one_way_and_a_filename_derives_a_name_back() {
    // DRS-FR-25: separators and characters illegal in a filename become `-`,
    // and the stored name keeps the free text it was given.
    assert_eq!(prompt_file_name("editor tweaks"), "editor tweaks.md");
    assert_eq!(prompt_file_name("UI: pass 2/final"), "UI- pass 2-final.md");
    assert_eq!(prompt_file_name(r#"a\b*c?d"e<f>g|h"#), "a-b-c-d-e-f-g-h.md");
    assert_eq!(prompt_file_name("  spaced  "), "spaced.md");

    // The reverse direction takes the file's name without its extension,
    // verbatim — including the sanitising the forward direction did, which
    // is what a rename in the rail means the author typed.
    assert_eq!(name_from_file_name("overview.md"), "overview");
    assert_eq!(name_from_file_name("UI- pass 2-final.md"), "UI- pass 2-final");
    assert_eq!(name_from_file_name("notes"), "notes");
    assert_eq!(name_from_file_name(".gitignore"), ".gitignore");
}

#[test]
fn renaming_a_draft_renames_the_file_bound_to_its_name() {
    // DRS-FR-14, CMS-FR-38 (DRS-FR-09, DRS-FR-25).
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = create_draft_at_root(root, None).unwrap();
    let id = created.draft.id.clone();
    save_draft_file_impl(root, &id, "Untitled.md", "# working").unwrap();

    let renamed = rename_draft_impl(root, &id, "editor tweaks").unwrap();

    assert_eq!(renamed.name, "editor tweaks");
    assert_eq!(renamed.prompt_path.as_deref(), Some("editor tweaks.md"));
    assert_eq!(
        files_in(root, &id),
        vec!["editor tweaks.md"],
        "the old name is gone rather than left beside the new one"
    );
    // The content moved with the file rather than being recreated empty.
    assert_eq!(
        load_draft_file_impl(root, &id, "editor tweaks.md").unwrap().body,
        "# working"
    );

    // DRS-FR-25: the record keeps the free text; the file carries what a
    // filesystem will take. The two legitimately read differently.
    let sanitised = rename_draft_impl(root, &id, "UI: pass 2/final").unwrap();
    assert_eq!(sanitised.name, "UI: pass 2/final");
    assert_eq!(files_in(root, &id), vec!["UI- pass 2-final.md"]);
}

#[test]
fn a_rename_is_the_one_thing_that_moves_the_prompts_path() {
    // DHS-FR-06 (DRS-FR-11, DRS-FR-14): the draft still holds exactly one
    // file afterwards, and it is the one the record names.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    save_draft_file_impl(root, &id, "spec.md", "# body").unwrap();

    let record = rename_draft_impl(root, &id, "overview").unwrap();
    assert_eq!(record.prompt_path.as_deref(), Some("overview.md"));
    assert_eq!(files_in(root, &id), vec!["overview.md"]);
    assert_eq!(require_prompt(root, &id).unwrap(), "overview.md");
    assert_eq!(
        load_draft_file_impl(root, &id, "overview.md").unwrap().body,
        "# body",
        "the content moved with the file rather than being recreated empty",
    );
}

#[test]
fn a_rename_can_never_collide_because_the_draft_holds_nothing_else() {
    // DRS-FR-09: the rename fails only for a name that is empty or for an
    // I/O failure — there is no other file for the derived name to be taken
    // by (DRS-FR-11).
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    for name in ["notes", "spec", "notes"] {
        rename_draft_impl(root, &id, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(files_in(root, &id).len(), 1);
    }
}

#[test]
fn a_prompt_removed_out_of_band_leaves_an_inconsistent_draft_rather_than_an_empty_one() {
    // DRS-FR-13 / DRS-FR-15: nothing here can remove a draft's prompt, so a
    // draft holding no file at all got there from outside the application —
    // and it is reported rather than treated as an ordinary empty draft.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    std::fs::remove_file(draft_dir(root, &id).unwrap().join(FILES_DIR).join("spec.md"))
        .unwrap();

    assert!(listed(root)[0].inconsistent);
    assert_eq!(require_prompt(root, &id).unwrap_err(), ERR_NOT_SINGLE_FILE);
    assert_eq!(open_draft_impl(root, &id).unwrap_err(), ERR_NOT_SINGLE_FILE);
}

#[test]
fn a_primary_path_that_escapes_the_draft_is_read_as_no_binding_at_all() {
    // DRS-FR-16: `draft.toml` is a file an author can edit, and this is the
    // one path here that arrives from one.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    let victim = root.join("outside.md");
    std::fs::write(&victim, "not the draft's").unwrap();

    for escape in ["../../outside.md", "/etc/passwd", ""] {
        let mut record = read_record(root, &id).unwrap();
        record.prompt_path = Some(escape.to_string());
        write_record(root, &record).unwrap();

        let read_back = read_record(root, &id).unwrap();
        assert_eq!(read_back.prompt_path, None, "{escape:?} was kept");

        // And a rename touches nothing outside the draft.
        rename_draft_impl(root, &id, "renamed").unwrap();
        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            "not the draft's"
        );
    }
}

#[test]
fn a_case_only_rename_is_a_rename_rather_than_a_collision() {
    // DRS-FR-26 refuses a path occupied by *another* file. On a
    // case-insensitive filesystem the destination of a capitalisation change
    // is the source itself, and refusing it would make a draft's
    // capitalisation impossible to correct.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "naw spec");

    let record = rename_draft_impl(root, &id, "NAW spec").unwrap();
    assert_eq!(record.name, "NAW spec");
    assert_eq!(record.prompt_path.as_deref(), Some("NAW spec.md"));
    let tree = files_in(root, &id);
    assert_eq!(tree.len(), 1, "one file, not two: {tree:?}");
    assert_eq!(tree[0], "NAW spec.md");

    // And back down again, which is the same act the other way.
    let record = rename_draft_impl(root, &id, "naw SPEC").unwrap();
    assert_eq!(record.prompt_path.as_deref(), Some("naw SPEC.md"));
    assert_eq!(files_in(root, &id), vec!["naw SPEC.md"]);
    // No staging name was left behind by the two-step.
    assert!(
        !draft_dir(root, &id)
            .unwrap()
            .join(FILES_DIR)
            .join("naw SPEC.md.synthesis-rename")
            .exists()
    );
}

#[test]
fn a_case_only_rename_folds_beyond_ascii() {
    // APFS and NTFS fold the whole range, so a non-ASCII capitalisation is
    // the same self-collision an ASCII one is. The outcome asserted here is
    // the contract on every filesystem; which *branch* produced it is the
    // subject of the two tests below.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "café spec");

    let record = rename_draft_impl(root, &id, "CAFÉ SPEC").unwrap();

    assert_eq!(record.name, "CAFÉ SPEC");
    assert_eq!(record.prompt_path.as_deref(), Some("CAFÉ SPEC.md"));
    assert_eq!(files_in(root, &id).len(), 1, "one file, not two");
}

#[test]
fn the_case_only_decision_is_about_two_strings_and_no_filesystem_at_all() {
    // The predicate the staging two-step turns on, tested where it can be
    // tested completely. Reached only through a rename, the Unicode half of
    // it is invisible on a case-sensitive filesystem — `café`/`CAFÉ` are
    // simply two names there, the rename goes straight through, and a CI on
    // ext4 reports a pass for a fold it never performed. Here both halves
    // are checked on every platform.
    assert!(is_case_only_rename("spec", "Spec"), "ASCII");
    assert!(is_case_only_rename("café spec", "CAFÉ SPEC"), "beyond ASCII");
    assert!(!is_case_only_rename("spec", "spec"), "no change at all is not a rename");
    assert!(!is_case_only_rename("spec", "notes"), "a different name is not a capitalisation");
    assert!(!is_case_only_rename("spec", "Spec notes"), "a longer name is not a capitalisation");
    // `ß` lowercases to itself while `SS` lowercases to `ss`, so this pair
    // is two names rather than one capitalisation — and treating it as a
    // case-only change would send a genuine rename through the staging
    // path and skip the collision check that protects the other file.
    assert!(!is_case_only_rename("straße", "STRASSE"), "a fold that changes length is a different name");
}

#[test]
fn a_case_only_rename_needs_its_staging_name_exactly_where_the_filesystem_folds() {
    // Why the two-step exists, stated for BOTH filesystems instead of for
    // whichever one the suite happens to be running on.
    //
    // The branch itself is chosen by `is_case_only_rename`, a question about
    // two strings — so the staging path runs on *every* filesystem. What
    // differs is whether it was necessary: below, the direct rename the
    // two-step replaces is attempted by hand, and it is refused where the
    // two names are one file and succeeds where they are two. That refusal
    // is the entire reason the branch exists, and it is observable on one
    // filesystem only, so each run asserts the answer it can see and names
    // which one it was.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    let files = draft_dir(root, &id).unwrap().join(FILES_DIR);

    let direct = root.rename_under(&files, "spec.md", "Spec.md");
    if crate::fs::case_probe::folds_case(dir.path()) {
        assert!(
            matches!(direct, Err(crate::fs::FsError::AlreadyExists { .. })),
            "a folding filesystem refuses the direct rename — that refusal \
             is the whole reason for the staging name: {direct:?}"
        );
    } else {
        assert!(
            direct.is_ok(),
            "a case-sensitive filesystem renames straight across: {direct:?}"
        );
        // Put back, so the author's rename below starts from the same state
        // the folding branch starts from.
        root.rename_under(&files, "Spec.md", "spec.md").unwrap();
    }

    // And on either filesystem the act the author performs succeeds, ends
    // with the new capitalisation, and leaves exactly one file.
    let record = rename_draft_impl(root, &id, "Spec").unwrap();
    assert_eq!(record.prompt_path.as_deref(), Some("Spec.md"));
    assert_eq!(files_in(root, &id), vec!["Spec.md"]);
}

#[test]
fn a_rename_interrupted_between_its_two_legs_is_finished_rather_than_unbound() {
    // The staging file of a case-only rename is the author's file under a
    // name nothing points at. Reading that as a deleted file would end the
    // binding (DRS-FR-15) for a file that is still there.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    let files = draft_dir(root, &id).unwrap().join(FILES_DIR);
    std::fs::write(files.join("spec.md"), "the author's words").unwrap();
    // Exactly the state a process killed between the two legs leaves.
    std::fs::rename(files.join("spec.md"), files.join(".spec.md.synthesis-rename")).unwrap();

    let record = rename_draft_impl(root, &id, "Spec").unwrap();

    assert_eq!(record.name, "Spec");
    assert_eq!(record.prompt_path.as_deref(), Some("Spec.md"));
    assert_eq!(
        load_draft_file_impl(root, &id, "Spec.md").unwrap().body,
        "the author's words",
        "the file was recovered rather than abandoned"
    );
    assert_eq!(files_in(root, &id), vec!["Spec.md"]);
}

#[test]
fn the_staging_name_of_a_case_only_rename_is_never_shown_in_the_rail() {
    // Two syscalls wide, but a process killed inside it must not leave
    // something that reads as one of the author's files (DRS-FR-11).
    assert!(staging_name("Spec.md").starts_with('.'));
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    let files = draft_dir(root, &id).unwrap().join(FILES_DIR);
    std::fs::write(files.join(staging_name("d.md")), "").unwrap();

    // DRS-FR-11: a dot-entry is skipped, so the draft still reads as the one
    // prompt it holds rather than as two files.
    assert_eq!(require_prompt(root, &id).unwrap(), "d.md");
    assert!(!listed(root)[0].inconsistent);
}

#[test]
fn a_name_beginning_with_a_dot_still_yields_a_file_the_rail_can_show() {
    // The rail skips dot-entries, so a primary file named `.x.md` would be a
    // file the draft holds and no surface can reach.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = create_draft_at_root(root, Some(".claude notes")).unwrap();

    assert_eq!(created.file, "-claude notes.md");
    assert_eq!(
        files_in(root, &created.draft.id),
        vec!["-claude notes.md"],
        "the prompt is not a dot-entry, so it is the draft's one file",
    );
    assert!(!listed(root)[0].inconsistent);
}


#[test]
fn a_draft_renamed_into_the_untitled_slot_pushes_the_next_default_along() {
    // DRS-FR-27 reads the names in force rather than a counter.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");
    rename_draft_impl(root, &id, "Untitled").unwrap();

    assert_eq!(create_draft_at_root(root, None).unwrap().draft.name, "Untitled 2");
}

#[test]
fn a_name_the_filesystem_will_not_take_leaves_no_half_built_draft_behind() {
    // DRS-FR-06: the folder and the record arrive together or not at all.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let refused = create_draft_at_root(root, Some(&"x".repeat(300)));

    assert!(refused.is_err(), "a 303-byte filename is not creatable");
    assert!(listed(root).is_empty());
    // The whole directory the scaffold had begun — `files/`, `comments/`,
    // `proposals/` and the empty `history/` alike (DHS-FR-01) — goes with
    // it: a directory with no record is not a draft, and one left behind
    // would accumulate under `.synthesis/drafts/` where nothing lists it.
    // DRS-FR-DGMI / DRS-FR-JDRY: the drafts root's own `.gitattributes` and
    // `.gitignore` are established before the first draft is created, so
    // neither is part of the scaffold a refused creation takes back.
    let left: Vec<_> = std::fs::read_dir(root.join(DRAFTS_REL))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name != ".gitattributes" && name != ".gitignore")
                .collect()
        })
        .unwrap_or_default();
    assert!(left.is_empty(), "a folder was left behind: {left:?}");
}

#[test]
fn two_drafts_may_carry_the_same_name_and_stay_distinct() {
    // DRS-FR-09 / NAW-FR-04: a draft is identified by its id, never by what
    // it is called.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let a = draft(root, "same");
    let b = draft(root, "same");

    assert_ne!(a, b);
    assert_eq!(listed(root).len(), 2);
}


#[test]
fn the_prompt_path_survives_a_round_trip_through_the_record() {
    // DRS-FR-03: `draft.toml` carries it, so the binding outlives the
    // process that made it rather than being held in memory.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "spec");

    let toml = std::fs::read_to_string(draft_dir(root, &id).unwrap().join(RECORD_FILE)).unwrap();
    assert!(toml.contains("promptPath"), "not written to disk: {toml}");
    assert_eq!(
        read_record(root, &id).unwrap().prompt_path.as_deref(),
        Some("spec.md")
    );

    // And a record written before this field existed still reads: the draft
    // simply has no file bound to its name (DRS-FR-15's end state).
    let legacy = format!(
        "id = \"{id}\"\nname = \"spec\"\nstatus = \"active\"\ncreatedAt = \"2026-07-31T08:00:00Z\"\nupdatedAt = \"2026-07-31T08:00:00Z\"\n"
    );
    std::fs::write(draft_dir(root, &id).unwrap().join(RECORD_FILE), legacy).unwrap();
    assert_eq!(read_record(root, &id).unwrap().prompt_path, None);
}
