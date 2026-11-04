//! The decomposition facts of a draft record (DRS-FR-MXQF, DRS-FR-LPCX,
//! DRS-FR-EMKG, DRS-FR-UBWN, DRS-FR-NAZG, DRS-FR-HIOF).

use super::*;

fn sequence(position: u32, total: u32) -> DraftSequence {
    DraftSequence {
        set_id: "set-1".into(),
        position,
        total,
        original_draft_id: "orig".into(),
        original_name: "relay".into(),
    }
}

fn marker() -> SupersededMarker {
    SupersededMarker {
        set_id: "set-1".into(),
        confirmed_at: "2026-11-03T10:00:00Z".into(),
        drafts: vec![
            SupersedingDraft { id: "a".into(), name: "relay-auth".into() },
            SupersedingDraft { id: "b".into(), name: "relay-sync".into() },
        ],
    }
}

fn summary(root: &crate::fs::RootFs, id: &str) -> DraftSummary {
    listed(root).into_iter().find(|d| d.id == id).expect("the draft is listed")
}

/// DRS-FR-EMKG / DRS-FR-MXQF: the draft takes the given name, folder,
/// sequence, and prompt (normalised), and never the template.
#[test]
fn a_draft_with_a_prompt_is_created_in_the_folder_with_its_sequence() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: project_settings::LineEndings::Crlf,
            draft_template: Some("# Template\n".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    create_drafts_folder_impl(root, "", "UI").unwrap();

    let created =
        create_draft_with_prompt(root, "relay-auth", "UI", "Part one\nbody\n", sequence(1, 3))
            .unwrap();
    assert_eq!(created.draft.name, "relay-auth");
    assert_eq!(created.draft.status, DraftStatus::Active);
    assert_eq!(created.draft.sequence, Some(sequence(1, 3)));
    assert_eq!(folder_of(root, &created.draft.id), "UI");
    let body = std::fs::read_to_string(
        draft_dir(root, &created.draft.id).unwrap().join(FILES_DIR).join(&created.file),
    )
    .unwrap();
    assert_eq!(body, "Part one\r\nbody\r\n");
    // DRS-FR-HIOF: the list carries the sequence as part of the record.
    assert_eq!(summary(root, &created.draft.id).sequence, Some(sequence(1, 3)));
}

/// DRS-FR-EMKG: a folder that does not exist creates nothing.
#[test]
fn a_draft_with_a_prompt_in_a_missing_folder_is_refused() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert!(create_draft_with_prompt(root, "x", "Nope", "body", sequence(1, 2)).is_err());
    assert!(listed(root).is_empty());
}

/// DRS-FR-UBWN / DRS-FR-HIOF: the marker and `archived` land in one write,
/// and the list carries the marker.
#[test]
fn marking_a_draft_superseded_archives_it_and_stores_the_marker() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "relay");
    let record = mark_draft_superseded(root, &id, marker()).unwrap();
    assert_eq!(record.status, DraftStatus::Archived);
    assert_eq!(record.superseded, Some(marker()));
    let listed = summary(root, &id);
    assert_eq!(listed.status, DraftStatus::Archived);
    assert_eq!(listed.superseded, Some(marker()));
}

/// DRS-FR-UBWN: a marker is written once, and a shadow or graduated draft
/// takes none.
#[test]
fn marking_refuses_a_second_marker_a_shadow_and_a_graduated_draft() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "relay");
    mark_draft_superseded(root, &id, marker()).unwrap();
    assert_eq!(mark_draft_superseded(root, &id, marker()), Err(ERR_ALREADY_SUPERSEDED.to_string()));

    let graduated = draft(root, "done");
    set_draft_graduated(root, &graduated, "run-1").unwrap();
    assert_eq!(
        mark_draft_superseded(root, &graduated, marker()),
        Err(ERR_DRAFT_GRADUATED.to_string())
    );
    assert!(read_draft_record(root, &graduated).unwrap().superseded.is_none());

    let shadow = draft(root, "mirror");
    mark_github_shadow_for_test(
        root,
        &shadow,
        GithubIssueLink {
            repository_host: "github.com".into(),
            repository_owner: "o".into(),
            repository_name: "r".into(),
            issue_number: 1,
            issue_url: "https://github.com/o/r/issues/1".into(),
            project_node_id: "P".into(),
            claim_state: GithubClaimState::Claimed,
        },
    );
    assert_eq!(mark_draft_superseded(root, &shadow, marker()), Err(ERR_GITHUB_SHADOW.to_string()));
    // DRS-FR-LPCX: a move keeps the marker.
    create_drafts_folder_impl(root, "", "UI").unwrap();
    move_draft_to_folder_impl(root, &id, "UI").unwrap();
    assert_eq!(read_draft_record(root, &id).unwrap().superseded, Some(marker()));
}

/// DRS-FR-LPCX / DRS-FR-NAZG: a restore stores `active` and keeps the marker;
/// a rename and a save keep both facts.
#[test]
fn restore_rename_and_save_keep_the_sequence_and_the_marker() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "relay");
    mark_draft_superseded(root, &id, marker()).unwrap();
    let restored = set_draft_status_impl(root, &id, DraftStatus::Active).unwrap();
    assert_eq!(restored.status, DraftStatus::Active);
    assert_eq!(restored.superseded, Some(marker()));
    rename_draft_impl(root, &id, "relay two").unwrap();
    assert_eq!(read_draft_record(root, &id).unwrap().superseded, Some(marker()));

    let part = create_draft_with_prompt(root, "p", "", "body", sequence(2, 2)).unwrap().draft;
    rename_draft_impl(root, &part.id, "p2").unwrap();
    let path = read_draft_record(root, &part.id).unwrap().prompt_path.unwrap();
    save_draft_file_impl(root, &part.id, &path, "new body").unwrap();
    assert_eq!(read_draft_record(root, &part.id).unwrap().sequence, Some(sequence(2, 2)));
}

/// DRS-FR-03: a record written before the two fields existed reads with
/// neither.
#[test]
fn a_record_without_the_fields_reads_with_neither() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "plain");
    let record = read_draft_record(root, &id).unwrap();
    assert!(record.sequence.is_none());
    assert!(record.superseded.is_none());
}
