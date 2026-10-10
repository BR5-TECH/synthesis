//! GitHub-shadow drafts (`DRS-draft-storage.md` DRS-FR-INCJ, DRS-FR-WFLY,
//! DRS-FR-XDWS, DRS-FR-QPSC, DRS-FR-JYIO, DRS-FR-EZDB).
//!
//! A GitHub-shadow draft is the draft a claim of a ready GitHub Task creates
//! (`GPP-github-polling.md` GPP-FR-XPUO). It is an ordinary draft folder with
//! one difference: its record carries a [`GithubIssueLink`]. The link makes the
//! draft immutable for its whole life, while every read and the graduation
//! start answer for it as for any other draft.

use super::*;

/// DRS-FR-QPSC: the typed refusal every mutation returns for a shadow draft.
pub const ERR_GITHUB_SHADOW: &str = "draft_github_shadow";

/// The longest draft name, in bytes, that a GitHub title is cut to. The name
/// also names the prompt file (DRS-FR-25), and a filesystem refuses a file
/// name longer than 255 bytes.
const SHADOW_NAME_MAX_BYTES: usize = 200;

/// DRS-FR-QPSC: refuse a record that carries a GitHub issue link.
pub(super) fn refuse_shadow_record(record: &DraftRecord) -> Result<(), String> {
    match record.github_issue {
        Some(_) => Err(ERR_GITHUB_SHADOW.to_string()),
        None => Ok(()),
    }
}

/// DRS-FR-XDWS: the issue link of a draft, or `None` where the draft carries
/// none or its record cannot be read.
pub fn github_issue_link(root: &fs::RootFs, id: &str) -> Option<GithubIssueLink> {
    read_record(root, id).ok().and_then(|record| record.github_issue)
}

/// DRS-FR-XDWS: whether the draft is a GitHub-shadow draft.
pub fn is_github_shadow(root: &fs::RootFs, id: &str) -> bool {
    github_issue_link(root, id).is_some()
}

/// DRS-FR-QPSC: the guard every mutating command applies before it changes
/// anything. A draft that does not resolve passes, so the command reports its
/// own "not found" rather than this refusal.
pub fn require_not_github_shadow(root: &fs::RootFs, id: &str) -> Result<(), String> {
    match is_github_shadow(root, id) {
        true => Err(ERR_GITHUB_SHADOW.to_string()),
        false => Ok(()),
    }
}

/// DRS-FR-INCJ: the one path to a GitHub-shadow draft.
///
/// The draft is created at the drafts root with status `github_shadow`, the
/// given name, and `link` in its record. Its one prompt holds `prompt`
/// normalised to the project's line-ending convention, and the draft template
/// is not applied. Registered as no Tauri command, so no frontend call can
/// create one.
pub fn create_github_shadow_draft(
    root: &fs::RootFs,
    name: &str,
    prompt: &str,
    link: GithubIssueLink,
) -> Result<DraftCreated, String> {
    let now = now_rfc3339();
    let parent = resolve_drafts_folder(root, "")?;
    let name = match shadow_name(name) {
        Some(name) => name,
        None => default_draft_name(root),
    };
    let primary = prompt_file_name(&name);
    let record = DraftRecord {
        id: new_draft_id(),
        name,
        prompt_path: Some(primary.clone()),
        status: DraftStatus::GithubShadow,
        created_at: now.clone(),
        updated_at: now,
        github_issue: Some(link),
    };
    let starting = project_settings::line_endings_for(root).normalize(prompt);
    scaffold_draft(root, &parent, &record, &primary, &starting)?;
    Ok(DraftCreated {
        draft: record,
        file: primary,
    })
}

/// The draft name a GitHub title gives, trimmed and cut on a character
/// boundary, or `None` for a title with no text.
fn shadow_name(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() <= SHADOW_NAME_MAX_BYTES {
        return Some(trimmed.to_string());
    }
    let mut end = SHADOW_NAME_MAX_BYTES;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    Some(trimmed[..end].trim_end().to_string())
}

/// DRS-FR-XDWS: every GitHub-shadow draft of the worktree, whatever its
/// status, from one walk of the drafts root.
pub fn github_shadow_drafts(root: &fs::RootFs) -> Vec<DraftSummary> {
    list_drafts_impl(root)
        .drafts
        .into_iter()
        .filter(|draft| draft.github_issue.is_some())
        .collect()
}

/// GPP-FR-XPUO: the shadow draft that names issue `number` of `owner/name` on
/// `host`, if one exists.
pub fn find_github_shadow(
    root: &fs::RootFs,
    host: &str,
    owner: &str,
    name: &str,
    number: u64,
) -> Option<DraftSummary> {
    github_shadow_drafts(root).into_iter().find(|draft| {
        draft
            .github_issue
            .as_ref()
            .is_some_and(|link| link.names(host, owner, name, number))
    })
}

/// Give an existing draft a GitHub issue link, so a test can exercise the
/// guards against a draft that already holds proposals or assets.
#[cfg(test)]
pub fn mark_github_shadow_for_test(root: &fs::RootFs, id: &str, link: GithubIssueLink) {
    let dir = draft_dir(root, id).expect("the draft resolves");
    let mut record = read_record_at(root, &dir, id).expect("the record reads");
    record.github_issue = Some(link);
    record.status = DraftStatus::GithubShadow;
    write_record_at(root, &dir, &record).expect("the record writes");
}
