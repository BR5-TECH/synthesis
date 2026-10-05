//! The draft template (DRS-FR-39).

use super::*;

// -- graduation --------------------------------------------------------








// -----------------------------------------------------------------------
// The draft template (DRS-FR-39)
// -----------------------------------------------------------------------

/// Configure `root`'s draft template, or clear it back to unset.
fn set_template(root: &crate::fs::RootFs, text: Option<&str>) {
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: project_settings::LineEndings::Lf,
            draft_template: Some(text.unwrap_or("").to_string()),
                        ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn a_draft_is_created_holding_the_projects_template_or_nothing_where_none_is_set() {
    // DRS-FR-01, DRS-FR-03, DRS-FR-04, DRS-FR-06, DRS-FR-11, DRS-FR-23, DRS-FR-25, DHS-FR-07 / DRS-FR-39, DRS-FR-12: an unset template creates a zero-byte prompt;
    // a configured one is the prompt's starting bytes. The `history/` folder
    // is empty either way — a template is starting content, not a version.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let before = create_draft_at_root(root, Some("spec")).unwrap();
    let before_prompt = draft_dir(root, &before.draft.id)
        .unwrap()
        .join(FILES_DIR)
        .join(&before.file);
    assert_eq!(std::fs::read(&before_prompt).unwrap().len(), 0);

    set_template(root, Some("# Context\n\n## Decision\n"));

    let after = create_draft_at_root(root, Some("second")).unwrap();
    let after_dir = draft_dir(root, &after.draft.id).unwrap();
    assert_eq!(
        std::fs::read_to_string(after_dir.join(FILES_DIR).join(&after.file)).unwrap(),
        "# Context\n\n## Decision\n"
    );
    assert!(
        std::fs::read_dir(after_dir.join(HISTORY_DIR))
            .unwrap()
            .next()
            .is_none(),
        "creating a draft settles no version (DHS-FR-07)"
    );

    // DRS-FR-06, DRS-FR-39, DRS-FR-12 tail: the draft created before the template was configured
    // is still a zero-byte prompt.
    assert_eq!(std::fs::read(&before_prompt).unwrap().len(), 0);
}

#[test]
fn the_template_is_normalised_to_the_projects_line_ending_convention() {
    // DRS-FR-39, per DRS-FR-12: the copy is normalised exactly as a write
    // is, so a template authored under one convention lands under the
    // project's.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: project_settings::LineEndings::Crlf,
            draft_template: Some("# Context\n\n## Decision\n".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();

    let created = create_draft_at_root(root, Some("spec")).unwrap();
    let body = std::fs::read_to_string(
        draft_dir(root, &created.draft.id)
            .unwrap()
            .join(FILES_DIR)
            .join(&created.file),
    )
    .unwrap();
    assert_eq!(body, "# Context\r\n\r\n## Decision\r\n");
}

#[test]
fn the_copy_is_a_snapshot_that_a_later_template_change_does_not_reach() {
    // DRS-FR-39: a template edited, replaced, and then cleared changes
    // nothing in a draft that already exists, and the draft holds no
    // reference back to the template it was created from.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_template(root, Some("# Context"));
    let created = create_draft_at_root(root, Some("spec")).unwrap();
    let prompt = draft_dir(root, &created.draft.id)
        .unwrap()
        .join(FILES_DIR)
        .join(&created.file);

    for step in [Some("# Edited"), Some("# Replaced"), None] {
        set_template(root, step);
        assert_eq!(
            std::fs::read_to_string(&prompt).unwrap(),
            "# Context",
            "an existing draft's prompt is untouched by a template change"
        );
    }

    let record = std::fs::read_to_string(
        draft_dir(root, &created.draft.id).unwrap().join(RECORD_FILE),
    )
    .unwrap();
    assert!(
        !record.contains("template"),
        "no draft carries a reference back to the template: {record}"
    );

    // And with the template cleared, the next draft's prompt is empty.
    let next = create_draft_at_root(root, Some("after")).unwrap();
    assert_eq!(
        std::fs::read(
            draft_dir(root, &next.draft.id)
                .unwrap()
                .join(FILES_DIR)
                .join(&next.file)
        )
        .unwrap()
        .len(),
        0
    );
}

#[test]
fn a_malformed_project_config_fails_the_creation_rather_than_creating_an_empty_draft() {
    // DRS-FR-39, DRS-FR-06, DRS-FR-07 head: a template that cannot be read is the typed error of
    // PSS-FR-10, and creates no draft — rather than quietly creating one on
    // the empty-prompt terms and withholding the starting content the
    // project asked for.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = = toml").unwrap();

    let err = create_draft_at_root(root, Some("spec")).unwrap_err();
    assert_eq!(err, project_settings::ERR_MALFORMED_PROJECT_CONFIG);
    assert!(
        !root.join(DRAFTS_REL).exists() || listed(root).is_empty(),
        "nothing remains under the drafts root"
    );
}

#[test]
fn a_bad_folder_outranks_a_malformed_store_and_neither_creates_anything() {
    // DRS-FR-07 / DRS-FR-39: a caller wrong about both is told about the
    // folder they named rather than about a store they said nothing about,
    // and either way nothing is created.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = = toml").unwrap();

    let err = create_draft_impl(root, Some("spec"), Some("nope")).unwrap_err();
    assert_ne!(err, project_settings::ERR_MALFORMED_PROJECT_CONFIG);
    assert!(err.to_lowercase().contains("nope"), "{err}");
    assert!(listed(root).is_empty());
}

#[test]
fn a_draft_template_key_that_is_not_a_string_reads_as_unset() {
    // PSS-FR-21: one odd value is not a damaged store — a malformed *file*
    // is the typed error of PSS-FR-10, and this is not that. The prompt is
    // created empty rather than the creation failing.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "draftTemplate = 42\n",
    )
    .unwrap();

    let created = create_draft_at_root(root, Some("spec")).unwrap();
    assert_eq!(
        std::fs::read(
            draft_dir(root, &created.draft.id)
                .unwrap()
                .join(FILES_DIR)
                .join(&created.file)
        )
        .unwrap()
        .len(),
        0
    );
}

#[test]
fn both_drafts_panel_routes_create_a_draft_holding_the_same_template() {
    // DRS-FR-39, DRS-FR-06, DRS-FR-07 tail / DRS-FR-03: the pinned affordance and a folder's
    // **New Draft** run the same creation — the same template copied in —
    // and differ only in the drafts folder each is filed under. Neither
    // record holds a project destination of any kind.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_template(root, Some("# Context"));
    create_drafts_folder_impl(root, "", "UI").unwrap();

    let at_root = create_draft_impl(root, Some("a"), None).unwrap();
    let in_folder = create_draft_impl(root, Some("b"), Some("UI")).unwrap();

    for created in [&at_root, &in_folder] {
        assert_eq!(
            std::fs::read_to_string(
                draft_dir(root, &created.draft.id)
                    .unwrap()
                    .join(FILES_DIR)
                    .join(&created.file)
            )
            .unwrap(),
            "# Context"
        );
    }

    let listed = listed(root);
    let a = listed.iter().find(|d| d.id == at_root.draft.id).unwrap();
    let b = listed.iter().find(|d| d.id == in_folder.draft.id).unwrap();
    assert_eq!(a.folder, "");
    assert_eq!(b.folder, "UI");
    // DRS-FR-07: the record says nothing about where a specification lands.
    let record = std::fs::read_to_string(
        draft_dir(root, &at_root.draft.id).unwrap().join(RECORD_FILE),
    )
    .unwrap();
    assert!(!record.contains("destination"), "{record}");
}
