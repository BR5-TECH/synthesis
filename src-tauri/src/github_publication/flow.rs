//! The publication flow itself — everything a command does once the project,
//! the token, and the client are in hand.
//!
//! Pure of Tauri, so the whole of GHP-FR-AKUM through GHP-FR-QLDF is tested
//! against a fake client and a temporary worktree.

use super::client::{GithubIssues, IssueFields, IssueRef};
use super::records::*;
use super::remotes::{self, ConfiguredRemote};
use super::store;
use crate::drafts::DraftStatus;
use crate::fs;
use crate::notes::now_rfc3339;
use crate::project_settings::{clean_type_names, PublicationRemoteSelection};

/// GHP-FR-FQIZ: the fixed marker format. One line, an HTML comment, so it is
/// invisible in the rendered issue and searchable by exact match in its body.
pub const MARKER_PREFIX: &str = "<!-- synthesis-publication-marker: ";
pub const MARKER_SUFFIX: &str = " -->";

/// The whole line an issue body ends with.
pub fn marker_line(marker: &str) -> String {
    format!("{MARKER_PREFIX}{marker}{MARKER_SUFFIX}")
}

static MARKER_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// GHP-FR-FQIZ: an opaque marker, generated once per attempt.
///
/// Time, a process-local counter, and one draw of the hasher's random seed —
/// the same three ingredients a draft id is built from, for the same reason:
/// two attempts started in the same millisecond must not collide, and the value
/// must not be guessable from the draft it belongs to.
pub fn new_marker() -> String {
    use std::hash::{BuildHasher, Hasher};
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let counter = MARKER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(millis);
    hasher.write_u32(counter);
    format!("pub-{millis:011x}{counter:04x}{:08x}", hasher.finish() as u32)
}

/// GHP-FR-XOBH: the title and body one attempt publishes.
pub struct IssueContent {
    pub title: String,
    pub body: String,
}

/// GHP-FR-XOBH: the draft's **current** name and the whole of its **latest**
/// prompt, followed by the marker line and nothing else.
///
/// Read fresh on every attempt, including a retry, so a retry publishes what
/// the draft says now rather than what it said when the attempt started
/// (GHP-FR-OWLB).
pub fn issue_content(
    root: &fs::RootFs,
    draft_id: &str,
    marker: &str,
) -> Result<IssueContent, String> {
    let prompt =
        crate::drafts::read_draft_prompt(root, draft_id).map_err(|_| ERR_DRAFT_NOT_FOUND)?;
    let body = prompt.content.trim_end();
    Ok(IssueContent {
        title: prompt.draft.name,
        body: format!("{body}\n\n{}", marker_line(marker)),
    })
}

/// GHP-FR-AKUM: the image references a v1 publication cannot carry.
///
/// A reference is a **local asset reference** unless it is already an absolute
/// `http` or `https` URL: a draft-owned asset, a relative path, a `file:` URL,
/// and a data URI alike would each render as broken material to whoever picks
/// the issue up, because v1 uploads nothing (GHP-FR-DTVW).
pub fn local_asset_references(root: &fs::RootFs, draft_id: &str) -> Vec<String> {
    crate::draft_assets::read_prompt_images(root, draft_id)
        .into_iter()
        .map(|image| image.reference)
        .filter(|reference| !is_remote_reference(reference))
        .collect()
}

fn is_remote_reference(reference: &str) -> bool {
    let lowered = reference.trim().to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// GHP-FR-GJEO / GHP-FR-KZAP / GHP-FR-AKUM: whether the action is offered, and
/// the exact reason where it is not.
///
/// The remote checks are the caller's, because they cost a network read per
/// GitHub remote; this decides everything that can be decided from disk.
pub fn local_eligibility(
    root: &fs::RootFs,
    draft_id: &str,
    status: DraftStatus,
    attempt: Option<&PublicationAttempt>,
) -> PublicationEligibility {
    // GHP-FR-BKLT: a GitHub-shadow draft is never publishable, whatever its
    // status.
    if crate::drafts::is_github_shadow(root, draft_id) {
        return PublicationEligibility::refused(
            ERR_DRAFT_GITHUB_SHADOW,
            "This draft was made from a GitHub task, so it cannot be published as a new issue.",
        );
    }
    if status == DraftStatus::Archived {
        return PublicationEligibility::refused(
            ERR_DRAFT_ARCHIVED,
            "Restore this draft before you publish it.",
        );
    }
    if attempt.is_some() {
        return PublicationEligibility::refused(
            ERR_ATTEMPT_IN_PROGRESS,
            "A publication attempt for this draft is already in progress.",
        );
    }
    let local_assets = local_asset_references(root, draft_id);
    if !local_assets.is_empty() {
        return PublicationEligibility {
            publishable: false,
            reason_code: Some(ERR_LOCAL_ASSETS.to_string()),
            reason: Some(
                "Local media attachments are not supported. Remove or replace these references \
                 before you publish."
                    .to_string(),
            ),
            local_assets,
        };
    }
    PublicationEligibility::publishable()
}

/// GHP-FR-RUYT: write the attempt record **before** the first GitHub request,
/// holding no publication choice. A record without a choice reads as a root
/// issue with no Type and no milestone (GHP-FR-CDVT).
pub fn open_attempt(
    root: &fs::RootFs,
    draft_id: &str,
    remote: &PublicationRemote,
    marker: String,
) -> Result<PublicationAttempt, String> {
    open_attempt_with(root, draft_id, remote, marker, None)
}

/// GHP-FR-RUYT / GHP-FR-ATCH: write the attempt record and the resolved
/// publication choice **before** the first GitHub mutation.
pub fn open_attempt_with(
    root: &fs::RootFs,
    draft_id: &str,
    remote: &PublicationRemote,
    marker: String,
    choice: Option<PublicationChoice>,
) -> Result<PublicationAttempt, String> {
    let now = now_rfc3339();
    let attempt = PublicationAttempt {
        marker,
        remote_name: remote.name.clone(),
        remote_url: remote.url.clone(),
        repository_owner: remote.repository_owner.clone().unwrap_or_default(),
        repository_name: remote.repository_name.clone().unwrap_or_default(),
        state: AttemptState::Open,
        started_at: now.clone(),
        updated_at: now,
        choice,
    };
    let mut current = store::read_store(root, draft_id)?;
    // GHP-FR-KZAP: one non-terminal attempt per draft, enforced against what is
    // on disk rather than against what the caller believed.
    if current.attempt.is_some() {
        return Err(ERR_ATTEMPT_IN_PROGRESS.to_string());
    }
    current.attempt = Some(attempt.clone());
    store::write_store(root, draft_id, &current)?;
    Ok(attempt)
}

/// DRS-FR-VDQR: move the standing attempt to a state, atomically.
pub fn set_attempt_state(
    root: &fs::RootFs,
    draft_id: &str,
    state: AttemptState,
) -> Result<(), String> {
    let mut current = store::read_store(root, draft_id)?;
    let Some(attempt) = current.attempt.as_mut() else {
        return Err(ERR_NO_ATTEMPT.to_string());
    };
    attempt.state = state;
    attempt.updated_at = now_rfc3339();
    store::write_store(root, draft_id, &current)
}

/// GHP-FR-YPGL: replace the standing attempt with a new one in **one** atomic
/// write, so a failed write leaves the old attempt and its saved choice
/// standing rather than neither.
pub fn restart_attempt(
    root: &fs::RootFs,
    draft_id: &str,
    remote: &PublicationRemote,
    marker: String,
    choice: Option<PublicationChoice>,
) -> Result<PublicationAttempt, String> {
    let mut current = store::read_store(root, draft_id)?;
    if current.attempt.is_none() {
        return Err(ERR_NO_ATTEMPT.to_string());
    }
    let now = now_rfc3339();
    let attempt = PublicationAttempt {
        marker,
        remote_name: remote.name.clone(),
        remote_url: remote.url.clone(),
        repository_owner: remote.repository_owner.clone().unwrap_or_default(),
        repository_name: remote.repository_name.clone().unwrap_or_default(),
        state: AttemptState::Open,
        started_at: now.clone(),
        updated_at: now,
        choice,
    };
    current.attempt = Some(attempt.clone());
    store::write_store(root, draft_id, &current)?;
    Ok(attempt)
}

/// DRS-FR-JOEV: clear the standing attempt, appending nothing.
pub fn clear_attempt(root: &fs::RootFs, draft_id: &str) -> Result<(), String> {
    let mut current = store::read_store(root, draft_id)?;
    if current.attempt.is_none() {
        return Err(ERR_NO_ATTEMPT.to_string());
    }
    current.attempt = None;
    store::write_store(root, draft_id, &current)
}

/// GHP-FR-JAWD: the record one successful publication appends.
pub fn record_of(attempt: &PublicationAttempt, issue: &IssueRef) -> PublicationRecord {
    PublicationRecord {
        provider: "github".to_string(),
        repository_owner: attempt.repository_owner.clone(),
        repository_name: attempt.repository_name.clone(),
        issue_number: issue.number,
        issue_url: issue.url.clone(),
        published_at: now_rfc3339(),
        marker: attempt.marker.clone(),
        choice: attempt.choice.clone(),
    }
}

/// GHP-FR-OHGY: the Type and milestone a create or an edit names.
pub fn fields_of(choice: &PublicationChoice) -> IssueFields {
    IssueFields { issue_type: choice.issue_type.clone(), milestone: choice.milestone_number }
}

/// GHP-FR-PWJG: the saved parent must still be usable before a create or a
/// link. A parent that is missing, closed, a pull request, or in another
/// repository than the attempt's is `parent_issue_unavailable`; a read that
/// cannot answer is its own typed error. Never a reason to publish a root.
fn require_usable_parent(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    choice: &PublicationChoice,
) -> Result<(), String> {
    let Some(number) = choice.parent_issue_number else {
        return Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string());
    };
    let same_repository = choice
        .parent_repository_owner
        .as_deref()
        .is_some_and(|saved| saved.eq_ignore_ascii_case(owner))
        && choice
            .parent_repository_name
            .as_deref()
            .is_some_and(|saved| saved.eq_ignore_ascii_case(repo));
    if !same_repository {
        return Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string());
    }
    match client.get_issue(secret, owner, repo, number)? {
        Some(parent) if parent.open && !parent.is_pull_request => Ok(()),
        _ => Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string()),
    }
}

/// GHP-FR-OHGY / GHP-FR-SWKU: create the issue with the saved Type and
/// milestone, then link it to the saved parent.
fn create_with_choice(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    content: &IssueContent,
    choice: &PublicationChoice,
) -> Result<IssueRef, String> {
    let sub_issue = choice.kind == PublicationKind::SubIssue;
    if sub_issue {
        require_usable_parent(client, secret, owner, repo, choice)?;
    }
    let created = client.create_issue(
        secret,
        owner,
        repo,
        &content.title,
        &content.body,
        &fields_of(choice),
    )?;
    if let (true, Some(parent)) = (sub_issue, choice.parent_issue_number) {
        client.link_sub_issue(secret, owner, repo, parent, created.id, false)?;
    }
    Ok(created)
}

/// GHP-FR-CMPR: what differs between a found issue and the draft's latest
/// title and body and the saved choice. A Type or a milestone the choice does
/// not name is not compared; the parent always is, a root issue expecting none.
pub fn mismatches(
    issue: &IssueRef,
    content: &IssueContent,
    choice: &PublicationChoice,
    owner: &str,
    repo: &str,
) -> Vec<String> {
    let mut differs = Vec::new();
    if issue.title != content.title {
        differs.push("title".to_string());
    }
    if issue.body != content.body {
        differs.push("body".to_string());
    }
    if !parent_matches(issue, choice, owner, repo) {
        differs.push("parent".to_string());
    }
    if !type_matches(issue, choice) {
        differs.push("type".to_string());
    }
    if !milestone_matches(issue, choice) {
        differs.push("milestone".to_string());
    }
    differs
}

fn parent_matches(issue: &IssueRef, choice: &PublicationChoice, owner: &str, repo: &str) -> bool {
    match (choice.kind, &issue.parent, choice.parent_issue_number) {
        (PublicationKind::Root, None, _) => true,
        (PublicationKind::SubIssue, Some(found), Some(number)) => found.names(owner, repo, number),
        _ => false,
    }
}

fn type_matches(issue: &IssueRef, choice: &PublicationChoice) -> bool {
    match &choice.issue_type {
        None => true,
        Some(wanted) => {
            issue.issue_type.as_deref().is_some_and(|found| found.eq_ignore_ascii_case(wanted))
        }
    }
}

fn milestone_matches(issue: &IssueRef, choice: &PublicationChoice) -> bool {
    match choice.milestone_number {
        None => true,
        Some(wanted) => issue.milestone.as_ref().is_some_and(|found| found.number == wanted),
    }
}

/// GHP-FR-OWLB through GHP-FR-TMBK: search for the marker, then create, reuse,
/// or ask.
///
/// The search runs on **every** attempt, first one included: an attempt record
/// is written before the request, so a create whose response was lost is
/// indistinguishable from one that never happened, and only the search can tell
/// the two apart.
///
/// The saved choice (GHP-FR-IBXN) is the only source of the parent, the Type,
/// and the milestone: nothing here reads the settings or the metadata lists.
pub fn publish_with_attempt(
    root: &fs::RootFs,
    draft_id: &str,
    attempt: &PublicationAttempt,
    secret: &str,
    client: &dyn GithubIssues,
) -> Result<PublicationOutcome, String> {
    let content = issue_content(root, draft_id, &attempt.marker)?;
    let owner = &attempt.repository_owner;
    let repo = &attempt.repository_name;
    let choice = attempt.effective_choice();
    let existing = client.find_by_marker(secret, owner, repo, &attempt.marker)?;

    let issue = match existing {
        // GHP-FR-IEQC: nothing carries the marker, so this attempt has not
        // reached GitHub yet.
        None => create_with_choice(client, secret, owner, repo, &content, &choice)?,
        Some(issue) => {
            let differs = mismatches(&issue, &content, &choice, owner, repo);
            if !differs.is_empty() {
                // GHP-FR-HRUN: it exists and differs, which is the author's
                // decision.
                set_attempt_state(root, draft_id, AttemptState::AwaitingChoice)?;
                return Ok(PublicationOutcome::RecoveryRequired {
                    issue_number: issue.number,
                    issue_url: issue.url,
                    marker: attempt.marker.clone(),
                    mismatches: differs,
                });
            }
            // GHP-FR-TMBK: the issue already says exactly what this attempt
            // would say, so it is reused rather than created a second time.
            issue
        }
    };

    let record = store::complete_attempt(root, draft_id, record_of(attempt, &issue))?;
    Ok(PublicationOutcome::Published { record })
}

/// GHP-FR-YPGL / GHP-FR-RCNL: reconcile the found issue to the latest title and
/// body and to the saved choice, then record **one** history entry for the
/// attempt.
pub fn update_existing(
    root: &fs::RootFs,
    draft_id: &str,
    attempt: &PublicationAttempt,
    secret: &str,
    client: &dyn GithubIssues,
) -> Result<PublicationOutcome, String> {
    let content = issue_content(root, draft_id, &attempt.marker)?;
    let owner = &attempt.repository_owner;
    let repo = &attempt.repository_name;
    let choice = attempt.effective_choice();
    let Some(existing) = client.find_by_marker(secret, owner, repo, &attempt.marker)? else {
        // The issue the choice was offered over is gone. Creating one is the
        // same act the attempt was going to perform anyway, and it keeps the
        // marker, so nothing is duplicated.
        let created = create_with_choice(client, secret, owner, repo, &content, &choice)?;
        let record = store::complete_attempt(root, draft_id, record_of(attempt, &created))?;
        return Ok(PublicationOutcome::Published { record });
    };

    let link = choice.kind == PublicationKind::SubIssue
        && !parent_matches(&existing, &choice, owner, repo);
    let unlink = choice.kind == PublicationKind::Root && existing.parent.is_some();
    if link {
        require_usable_parent(client, secret, owner, repo, &choice)?;
    }
    // Only a value that differs is sent, so an edit changes nothing it was not
    // asked to change.
    let fields = IssueFields {
        issue_type: choice.issue_type.clone().filter(|_| !type_matches(&existing, &choice)),
        milestone: choice.milestone_number.filter(|_| !milestone_matches(&existing, &choice)),
    };
    let updated =
        client.update_issue(secret, owner, repo, existing.number, &content.title, &content.body, &fields)?;
    if let (true, Some(parent)) = (link, choice.parent_issue_number) {
        client.link_sub_issue(secret, owner, repo, parent, existing.id, existing.parent.is_some())?;
    }
    if let (true, Some(old)) = (unlink, existing.parent.as_ref()) {
        client.unlink_sub_issue(secret, &old.owner, &old.repo, old.number, existing.id)?;
    }
    let record = store::complete_attempt(root, draft_id, record_of(attempt, &updated))?;
    Ok(PublicationOutcome::Published { record })
}

/// GHP-FR-DZLB: a list read, folded into the state the chooser renders. The
/// client's own error is dropped for the list's fixed code, so no transport
/// message reaches the surface (GHP-FR-DHXK).
fn list_of<T, U>(
    read: Result<Vec<T>, String>,
    code: &str,
    map: impl FnMut(T) -> U,
) -> MetadataList<U> {
    match read {
        Ok(items) => MetadataList::loaded(items.into_iter().map(map).collect()),
        Err(_) => MetadataList::failed(code),
    }
}

/// GHP-FR-ETJD: the configured sub-issue Type in the spelling the Type list
/// holds, or `None` where the list does not hold it or failed.
pub fn resolve_sub_issue_type(
    settings: &GithubPublicationSettings,
    types: &MetadataList<PublicationIssueType>,
) -> SubIssueTypeResolution {
    let resolved = types
        .items
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(&settings.sub_issue_type))
        .map(|t| t.name.clone());
    SubIssueTypeResolution { name: settings.sub_issue_type.clone(), resolved }
}

/// GHP-FR-MDLD: read the three lists. Mutates nothing, and one failed list
/// leaves the others as they are (GHP-FR-DZLB).
pub fn load_metadata(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    settings: GithubPublicationSettings,
) -> PublicationMetadata {
    let issue_types = issue_types_list(client, secret, owner);
    // The parent filter is sent in GitHub's own spelling of each Type where the
    // owner lists it, so a Type saved in another case still finds its issues.
    let wanted: Vec<String> = settings
        .parent_issue_types
        .iter()
        .map(|saved| {
            issue_types
                .items
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(saved))
                .map_or_else(|| saved.clone(), |t| t.name.clone())
        })
        .collect();
    let parents = list_of(
        client.list_parent_issues(secret, owner, repo, &wanted),
        ERR_PARENT_ISSUES_UNREADABLE,
        |issue| PublicationParentIssue {
            number: issue.number,
            title: issue.title,
            issue_type: issue.issue_type.unwrap_or_default(),
            url: issue.url,
            milestone: issue.milestone,
        },
    );
    let milestones = list_of(
        client.list_milestones(secret, owner, repo),
        ERR_MILESTONES_UNREADABLE,
        |milestone| milestone,
    );
    let sub_issue_type = resolve_sub_issue_type(&settings, &issue_types);
    PublicationMetadata {
        repository_owner: owner.to_string(),
        repository_name: repo.to_string(),
        settings,
        parents,
        issue_types,
        milestones,
        sub_issue_type,
    }
}

/// GHP-FR-YSPJ / GHP-FR-PTYL: the Type list of one repository owner.
pub fn issue_types_list(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
) -> MetadataList<PublicationIssueType> {
    list_of(client.list_issue_types(secret, owner), ERR_ISSUE_TYPES_UNREADABLE, |name| {
        PublicationIssueType { name }
    })
}

/// GHP-FR-NQWX: the settings a caller may save, cleaned, or the typed refusal.
pub fn validated_settings(
    parent_issue_types: Vec<String>,
    sub_issue_type: String,
    sub_issue_milestone_policy: MilestonePolicy,
) -> Result<GithubPublicationSettings, String> {
    let parent_issue_types = clean_type_names(parent_issue_types);
    let sub_issue_type = sub_issue_type.trim().to_string();
    if parent_issue_types.is_empty() || sub_issue_type.is_empty() {
        return Err(ERR_INVALID_PUBLICATION_SETTINGS.to_string());
    }
    Ok(GithubPublicationSettings { parent_issue_types, sub_issue_type, sub_issue_milestone_policy })
}

/// GHP-FR-BWNI through GHP-FR-AZPF: turn the caller's choice into the choice an
/// attempt saves. Every read happens here, before the attempt record is written
/// (GHP-FR-ATCH), and a refusal leaves nothing behind.
pub fn resolve_choice(
    input: &PublicationChoiceInput,
    settings: &GithubPublicationSettings,
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
) -> Result<PublicationChoice, String> {
    let requested_type =
        input.issue_type.as_deref().map(str::trim).filter(|name| !name.is_empty());
    let Some(parent_number) = input.parent_issue_number else {
        return resolve_root_choice(input, requested_type, client, secret, owner, repo);
    };
    if requested_type.is_some() {
        return Err(ERR_INVALID_PUBLICATION_CHOICE.to_string());
    }
    let usable = client
        .get_issue(secret, owner, repo, parent_number)?
        .filter(|parent| parent.open && !parent.is_pull_request)
        .filter(|parent| {
            parent.issue_type.as_deref().is_some_and(|found| {
                settings.parent_issue_types.iter().any(|t| t.eq_ignore_ascii_case(found))
            })
        });
    let Some(parent) = usable else {
        return Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string());
    };
    let policy = settings.sub_issue_milestone_policy;
    if input.milestone_number.is_some() && policy != MilestonePolicy::AuthorSelected {
        return Err(ERR_INVALID_PUBLICATION_CHOICE.to_string());
    }
    let milestone = match policy {
        MilestonePolicy::InheritParent => parent.milestone.clone(),
        MilestonePolicy::NoMilestone => None,
        MilestonePolicy::AuthorSelected => {
            open_milestone(client, secret, owner, repo, input.milestone_number)?
        }
    };
    // GHP-FR-ETJD: a Type the owner does not list, or a list that cannot be
    // read, publishes the sub-issue without a Type.
    let issue_type = client.list_issue_types(secret, owner).ok().and_then(|types| {
        types.into_iter().find(|t| t.eq_ignore_ascii_case(&settings.sub_issue_type))
    });
    Ok(PublicationChoice {
        kind: PublicationKind::SubIssue,
        parent_repository_owner: Some(owner.to_string()),
        parent_repository_name: Some(repo.to_string()),
        parent_issue_number: Some(parent_number),
        issue_type,
        milestone_policy: Some(policy),
        milestone_number: milestone.as_ref().map(|m| m.number),
        milestone_title: milestone.map(|m| m.title),
    })
}

fn resolve_root_choice(
    input: &PublicationChoiceInput,
    requested_type: Option<&str>,
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
) -> Result<PublicationChoice, String> {
    let issue_type = match requested_type {
        None => None,
        Some(wanted) => {
            let types = client
                .list_issue_types(secret, owner)
                .map_err(|_| ERR_ISSUE_TYPE_UNAVAILABLE.to_string())?;
            let found = types.into_iter().find(|t| t.eq_ignore_ascii_case(wanted));
            Some(found.ok_or_else(|| ERR_ISSUE_TYPE_UNAVAILABLE.to_string())?)
        }
    };
    let milestone = open_milestone(client, secret, owner, repo, input.milestone_number)?;
    Ok(PublicationChoice {
        issue_type,
        milestone_number: milestone.as_ref().map(|m| m.number),
        milestone_title: milestone.map(|m| m.title),
        ..PublicationChoice::root()
    })
}

/// GHP-FR-LCKZ: the open milestone a number names, or `None` for no number.
fn open_milestone(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    number: Option<u64>,
) -> Result<Option<PublicationMilestone>, String> {
    let Some(number) = number else {
        return Ok(None);
    };
    let milestones = client
        .list_milestones(secret, owner, repo)
        .map_err(|_| ERR_MILESTONE_UNAVAILABLE.to_string())?;
    milestones
        .into_iter()
        .find(|m| m.number == number)
        .map(Some)
        .ok_or_else(|| ERR_MILESTONE_UNAVAILABLE.to_string())
}

/// GHP-FR-HVQG / GHP-FR-CVYK: the remote a publication uses, re-resolved at the
/// moment of the call.
pub fn resolve_remote_for(
    remotes: &[PublicationRemote],
    remote_name: &str,
) -> Result<PublicationRemote, String> {
    let Some(remote) = remotes.iter().find(|r| r.name == remote_name) else {
        return Err(ERR_NO_REMOTE.to_string());
    };
    if remote.eligibility != RemoteEligibility::Eligible {
        return Err(remote.eligibility.error_code().to_string());
    }
    Ok(remote.clone())
}

/// GHP-FR-PWXA: persist the chosen remote, or leave the store untouched.
pub fn persist_choice(
    root: &fs::RootFs,
    remote: &PublicationRemote,
    persist: bool,
) -> Result<(), String> {
    if !persist {
        return Ok(());
    }
    // Mapped to the module's own vocabulary (GHP-FR-PVOA): the settings store
    // phrases its failures with a filesystem path, and a surface renders what
    // this returns unchanged.
    crate::project_settings::save_publication_remote_selection_to(
        root,
        Some(PublicationRemoteSelection {
            name: remote.name.clone(),
            url: remote.url.clone(),
        }),
    )
    .map_err(|_| ERR_STORE_WRITE_FAILED.to_string())
}

/// GHP-FR-MJTB: whether a URL is one this application may hand to the OS.
///
/// The publication store is committed to the project's repository, so a record
/// reaches this machine from whoever else works on the project. Being named by
/// a record is therefore not enough on its own: the address must also be a
/// `github.com` issue address, so a tampered or hostile store cannot make the
/// author open a `file:` URL or a custom scheme by activating an issue link.
pub fn is_openable_issue_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    // No userinfo, and the host itself rather than a look-alike prefix.
    !authority.contains('@')
        && authority.eq_ignore_ascii_case("github.com")
        && path.contains("/issues/")
}

/// GHP-FR-ZRFP / GHP-FR-WKDE: the resolution, from what the repository and the
/// project settings hold.
pub fn resolution_for(
    root: &fs::RootFs,
    configured: &[ConfiguredRemote],
    secret: Option<&str>,
    client: &dyn GithubIssues,
) -> PublicationRemoteResolution {
    let classified = remotes::classify(configured, secret, client);
    let persisted = crate::project_settings::load_publication_remote_selection_from(root);
    remotes::resolve(classified, persisted)
}

/// GHP-FR-BKLT: refuse a GitHub-shadow draft before anything is written or
/// requested.
pub fn require_not_shadow(root: &fs::RootFs, draft_id: &str) -> Result<(), String> {
    match crate::drafts::is_github_shadow(root, draft_id) {
        true => Err(ERR_DRAFT_GITHUB_SHADOW.to_string()),
        false => Ok(()),
    }
}

/// GHP-FR-YDAN: the repository the publication remote resolves to, without a
/// draft, or the GHP-FR-ZRFP refusal.
///
/// The remote is the one the resolution selects: a persisted choice that still
/// applies (GHP-FR-HVQG), else the single eligible remote (GHP-FR-XAUP). Where
/// two or more remotes exist and no choice is persisted, the first eligible
/// remote is used, which is the remote the publish picker offers first. It
/// writes nothing and creates nothing on GitHub.
pub fn resolve_repository_from(
    root: &fs::RootFs,
    configured: &[ConfiguredRemote],
    secret: Option<&str>,
    client: &dyn GithubIssues,
) -> Result<PublicationRepository, String> {
    let resolution = resolution_for(root, configured, secret, client);
    let selected = resolution
        .selection
        .as_deref()
        .and_then(|name| resolution.remotes.iter().find(|remote| remote.name == name));
    match selected {
        Some(PublicationRemote {
            name,
            repository_owner: Some(owner),
            repository_name: Some(repo),
            ..
        }) => Ok(PublicationRepository {
            remote_name: name.clone(),
            repository_owner: owner.clone(),
            repository_name: repo.clone(),
        }),
        _ => Err(remotes::refusal_for(&resolution.remotes).to_string()),
    }
}

/// GHP-FR-MJTB: whether `url` is one this draft holds — in a record of its
/// publication history, or in its GitHub-shadow issue link (DRS-FR-XDWS) —
/// and a `github.com` issue address.
pub fn is_recorded_issue_url(root: &fs::RootFs, draft_id: &str, url: &str) -> Result<bool, String> {
    let publication = store::read_store(root, draft_id)?;
    let recorded = publication.publication.iter().any(|record| record.issue_url == url)
        || crate::drafts::github_issue_link(root, draft_id).is_some_and(|link| link.issue_url == url);
    Ok(recorded && is_openable_issue_url(url))
}
