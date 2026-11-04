//! The decomposition facts a draft record carries (`DRS-draft-storage.md`
//! DRS-FR-MXQF, DRS-FR-LPCX, DRS-FR-EMKG, DRS-FR-UBWN, DRS-FR-NAZG).
//!
//! A draft that an accepted new-draft proposal created carries a `sequence`,
//! and a draft whose decomposition the author confirmed carries a `superseded`
//! marker. Both are written once, here, and no other operation rewrites them.

use super::*;

/// DRS-FR-UBWN: a marker is written once.
pub const ERR_ALREADY_SUPERSEDED: &str = "decomposition_confirmed";
/// DRS-FR-UBWN: a graduated draft is not superseded.
pub const ERR_DRAFT_GRADUATED: &str = "draft_graduated";

/// DRS-FR-EMKG: the one path to a draft that carries a `sequence`.
///
/// The draft is created as `create_draft` creates one, in `folder`, but its
/// prompt holds `prompt` normalised to the project's line-ending convention,
/// and the draft template is not applied. Registered as no Tauri command:
/// `crate::new_draft_proposals` calls it when a proposal is accepted.
pub fn create_draft_with_prompt(
    root: &fs::RootFs,
    name: &str,
    folder: &str,
    prompt: &str,
    sequence: DraftSequence,
) -> Result<DraftCreated, String> {
    let now = now_rfc3339();
    let parent = resolve_drafts_folder(root, folder)?;
    let name = match name.trim() {
        "" => default_draft_name(root),
        trimmed => trimmed.to_string(),
    };
    let primary = prompt_file_name(&name);
    let record = DraftRecord {
        id: new_draft_id(),
        name,
        prompt_path: Some(primary.clone()),
        status: DraftStatus::Active,
        created_at: now.clone(),
        updated_at: now,
        github_issue: None,
        sequence: Some(sequence),
        superseded: None,
    };
    let starting = project_settings::line_endings_for(root).normalize(prompt);
    scaffold_draft(root, &parent, &record, &primary, &starting)?;
    Ok(DraftCreated { draft: record, file: primary })
}

/// DCP-FR-KOOA: the drafts-root-relative folder that holds a draft now.
pub fn draft_folder_of(root: &fs::RootFs, id: &str) -> Result<String, String> {
    Ok(find_draft(root, id)?.folder)
}

/// DRS-FR-UBWN: store `archived` and the marker in one write of `draft.toml`.
///
/// The caller emits `"drafts changed"`. It refuses a GitHub-shadow draft, a
/// graduated draft, and a draft that already carries a marker. The graduation
/// lock is the caller's check, because it reads the runs of the application.
pub fn mark_draft_superseded(
    root: &fs::RootFs,
    id: &str,
    marker: SupersededMarker,
) -> Result<DraftRecord, String> {
    let dir = draft_dir(root, id)?;
    let mut record = read_record_at(root, &dir, id)?;
    refuse_shadow_record(&record)?;
    if record.status == DraftStatus::Graduated {
        return Err(ERR_DRAFT_GRADUATED.to_string());
    }
    if record.superseded.is_some() {
        return Err(ERR_ALREADY_SUPERSEDED.to_string());
    }
    record.status = DraftStatus::Archived;
    record.superseded = Some(marker);
    record.updated_at = now_rfc3339();
    write_record_at(root, &dir, &record)?;
    Ok(record)
}
