//! The states of the original draft that refuse every new-draft operation
//! (DCP-FR-LTEZ).

use super::*;

/// DCP-FR-LTEZ: refuse while the original draft is a GitHub-shadow draft,
/// carries a `superseded` marker, is `graduated`, is `archived`, or is held by
/// a non-terminal graduation run. Each case has its own typed error.
pub fn require_open_original<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    draft_id: &str,
) -> Result<drafts::DraftRecord, String> {
    let record =
        drafts::read_draft_record(root, draft_id).map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    if record.github_issue.is_some() || record.status == drafts::DraftStatus::GithubShadow {
        return Err(ERR_DRAFT_GITHUB_SHADOW.into());
    }
    if record.superseded.is_some() {
        return Err(ERR_DECOMPOSITION_CONFIRMED.into());
    }
    if record.status == drafts::DraftStatus::Graduated {
        return Err(ERR_DRAFT_GRADUATED.into());
    }
    if record.status == drafts::DraftStatus::Archived {
        return Err(ERR_DRAFT_ARCHIVED.into());
    }
    crate::graduation::require_unlocked_draft(app, draft_id)
        .map_err(|_| ERR_DRAFT_LOCKED.to_string())?;
    Ok(record)
}
