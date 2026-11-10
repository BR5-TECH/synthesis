//! FTS-FR-TPWE, FTS-FR-OBPX, FTS-FR-ALOJ: the pure cores of the commands that
//! are not a sync: one tracker item as a link's base, the address check before
//! a URL goes to the browser, and the settings a caller may save.

use crate::flow_validation::model::{FlowDocument, RefKind, SyncBase, TrackerRef};
use crate::project_settings::FlowTrackerSettings;

use super::adapter::TrackerAdapter;
use super::records::*;
use super::values::Repo;

/// FTS-FR-TPWE: issue or milestone `number` of the bound repository, with
/// its values on the terms of FTS-FR-PNMA.
pub fn snapshot(
    adapter: &dyn TrackerAdapter,
    doc: &FlowDocument,
    kind: RefKind,
    number: u64,
) -> Result<TrackerItemSnapshot, FlowSyncError> {
    let (host, owner, repo) = adapter.repository();
    let bound = Repo { host: host.to_string(), owner: owner.to_string(), repo: repo.to_string() };
    let linked = doc.sync.iter().flat_map(|s| s.links.iter()).any(|l| {
        l.reference.kind == kind && l.reference.number == number && bound.holds(&l.reference)
    });
    if linked {
        return Err(FlowSyncError::of(ERR_ALREADY_LINKED));
    }
    let not_found = || FlowSyncError::of(ERR_TRACKER_ITEM_NOT_FOUND);
    let reference = |url: &str| TrackerRef {
        kind,
        host: bound.host.clone(),
        owner: bound.owner.clone(),
        repo: bound.repo.clone(),
        number,
        url: url.to_string(),
    };
    match kind {
        RefKind::Issue => {
            let issue = adapter
                .read_issues(&[number])
                .map_err(FlowSyncError::of)?
                .into_iter()
                .find(|(n, _)| *n == number)
                .and_then(|(_, i)| i)
                .filter(|i| bound.holds_key(&i.key()))
                .ok_or_else(not_found)?;
            Ok(TrackerItemSnapshot {
                reference: reference(&issue.url),
                values: SyncBase {
                    name: issue.title.clone(),
                    description: issue.body.clone(),
                    state: issue.state.clone(),
                    r#type: Some(issue.issue_type.clone()),
                    parent: Some(issue.parent.as_ref().map(|p| bound.issue_ref(p))),
                    milestone: Some(issue.milestone),
                    blocked_by: Some(issue.blocked_by.iter().map(|k| bound.issue_ref(k)).collect()),
                    due: None,
                },
            })
        }
        RefKind::Milestone => {
            let m = adapter
                .read_milestones(&[number])
                .map_err(FlowSyncError::of)?
                .into_iter()
                .find(|(n, _)| *n == number)
                .and_then(|(_, m)| m)
                .ok_or_else(not_found)?;
            Ok(TrackerItemSnapshot {
                reference: reference(&m.url),
                values: SyncBase {
                    name: m.title.clone(),
                    description: m.description.clone(),
                    state: m.state.clone(),
                    r#type: None,
                    parent: None,
                    milestone: None,
                    blocked_by: None,
                    due: Some(m.due.clone()),
                },
            })
        }
    }
}

/// FTS-FR-OBPX: whether `url` is the HTTPS address of an issue or a
/// milestone of the repository `owner/repo` on `host`: no user, no port, no
/// query, no fragment, and nothing after the number.
pub fn is_tracker_url(url: &str, host: &str, owner: &str, repo: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    if rest.contains(['?', '#', '@', '\\']) || rest.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let mut parts = rest.split('/');
    let (Some(h), Some(o), Some(r), Some(kind), Some(number), None) =
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    h.eq_ignore_ascii_case(host)
        && o.eq_ignore_ascii_case(owner)
        && r.eq_ignore_ascii_case(repo)
        && matches!(kind, "issues" | "milestone")
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && !number.starts_with('0')
}

/// FTS-FR-ALOJ: the four Types, trimmed, or `invalid_flow_tracker_settings`
/// where one is empty. No Type is checked against GitHub.
pub fn validated_settings(
    epic_type: &str,
    feature_type: &str,
    task_type: &str,
    defect_type: &str,
) -> Result<FlowTrackerSettings, FlowSyncError> {
    let names = [epic_type, feature_type, task_type, defect_type].map(|n| n.trim().to_string());
    if names.iter().any(String::is_empty) {
        return Err(FlowSyncError::of(ERR_INVALID_SETTINGS));
    }
    let [epic_type, feature_type, task_type, defect_type] = names;
    Ok(FlowTrackerSettings { epic_type, feature_type, task_type, defect_type })
}
