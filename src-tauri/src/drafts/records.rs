//! The wire shapes a draft is described by (`DRS-draft-storage.md`).

use super::*;

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// NAW-FR-16 / DRS-FR-10: a draft is `active` or `archived`, and moving between
/// them is reversible at any time — archiving retires a draft from the Drafts
/// panel's default view and takes nothing off disk.
///
/// DRS-FR-VKQO: `published` is a fourth position, set by exactly one path —
/// `set_draft_published`, which `GHP-github-publication.md` alone calls once a
/// GitHub issue exists. It is **not** read-only: a published draft edits,
/// renames, graduates, and archives exactly as an active one does
/// (DRS-FR-OGZC).
///
/// DRS-FR-20: `graduated` is a third position and a *terminal* one, reached by
/// exactly one path — `set_draft_graduated`, which `GRD-graduation.md` alone
/// calls when a run publishes or is accepted. `set_draft_status` cannot reach
/// it, nothing clears it, and it is neither archiving nor deletion: everything
/// the draft holds is retained and every read answers as it always did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Active,
    Archived,
    Graduated,
    Published,
    /// DRS-FR-WFLY: the status of a GitHub-shadow draft, set by
    /// `create_github_shadow_draft` alone. No caller can set it, and no caller
    /// can move a draft out of it.
    GithubShadow,
}

impl Default for DraftStatus {
    fn default() -> Self {
        DraftStatus::Active
    }
}

/// Read a record's status, taking anything this enum does not admit as
/// `active`.
///
/// A `draft.toml` written before the two positions were named carries a value
/// serde cannot parse, and a parse failure fails the **whole record**:
/// `read_record` reports it as "draft not found", and `list_drafts_impl` — which
/// skips a draft it cannot read (DRS-FR-08) — drops it from the panel with no
/// error anywhere. A draft that silently vanishes from the list reads as data
/// loss, so an unreadable status is answered with the position every draft is
/// created in (DRS-FR-06) rather than with the loss of the draft. `active` is
/// also the right answer on the merits: archiving is the only position that
/// hides anything, and nothing in an older record ever meant that.
pub(super) fn status_or_active<'de, D>(de: D) -> Result<DraftStatus, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(de)?;
    Ok(match raw.as_str() {
        "archived" => DraftStatus::Archived,
        "graduated" => DraftStatus::Graduated,
        // DRS-FR-ULKN: a record written before publication existed carries no
        // such value, so this arm only ever reads one this application wrote.
        "published" => DraftStatus::Published,
        // DRS-FR-WFLY: written by `create_github_shadow_draft` alone.
        "github_shadow" => DraftStatus::GithubShadow,
        _ => DraftStatus::Active,
    })
}

/// The persisted record. Serialised as TOML into `<draft>/draft.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRecord {
    pub id: String,
    pub name: String,
    /// DRS-FR-25: the draft-relative path of the draft's one prompt, derived
    /// from the name.
    ///
    /// Optional on the wire and in TOML only so that a record which predates the
    /// single-prompt invariant, or one whose `files/` has been made
    /// inconsistent by hand (DRS-FR-15), still reads back rather than failing
    /// the whole record and dropping the draft out of the panel. Every product
    /// operation checks the invariant itself (see [`require_prompt`]), so a
    /// record with no prompt is reported and never acted on.
    ///
    /// The `primaryPath` alias reads a record written before the field was
    /// named for what it holds; nothing writes that spelling.
    #[serde(
        default,
        alias = "primaryPath",
        alias = "primary_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_path: Option<String>,
    #[serde(default, deserialize_with = "status_or_active")]
    pub status: DraftStatus,
    pub created_at: String,
    pub updated_at: String,
    /// DRS-FR-XDWS: the GitHub issue a GitHub-shadow draft was claimed from.
    /// Present on a shadow draft alone, and never removed or rewritten.
    ///
    /// The last field on purpose: it serialises as a TOML table, and a table
    /// must follow every plain value of the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_issue: Option<GithubIssueLink>,
}

/// DRS-FR-XDWS: the claim state a [`GithubIssueLink`] records. One value
/// today; carried rather than assumed so a later state does not have to
/// migrate every stored record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GithubClaimState {
    Claimed,
}

/// DRS-FR-INCJ / DRS-FR-XDWS: the link from a GitHub-shadow draft to the issue
/// it was claimed from (`GPP-github-polling.md` GPP-FR-XPUO).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubIssueLink {
    pub repository_owner: String,
    pub repository_name: String,
    pub issue_number: u64,
    pub issue_url: String,
    pub project_node_id: String,
    pub claim_state: GithubClaimState,
}

impl GithubIssueLink {
    /// Whether this link names the issue `number` of `owner/name`. The
    /// repository match is case-insensitive, as GitHub's own is.
    pub fn names(&self, owner: &str, name: &str, number: u64) -> bool {
        self.issue_number == number
            && self.repository_owner.eq_ignore_ascii_case(owner)
            && self.repository_name.eq_ignore_ascii_case(name)
    }
}

/// One row of the Drafts panel (DRP-FR-03). Carries no contents at all, so the
/// panel renders from one call and never reads a draft's prompt (DRP
/// non-functional requirements).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSummary {
    pub id: String,
    pub name: String,
    pub status: DraftStatus,
    /// DRS-FR-29: the drafts-root-relative path of the folder this draft is
    /// filed in, `""` being the implicit root. Not held in the record: it *is*
    /// where the draft's directory sits, so there is no second copy of it to
    /// fall out of step with the disk.
    pub folder: String,
    /// DRS-FR-15: the draft's `files/` is not the single prompt DRS-FR-11
    /// requires, so it cannot be opened, edited, or graduated.
    ///
    /// Carried on the row rather than discovered by trying to open the draft,
    /// because the panel has to render such a draft as itself — listed, named,
    /// and offering Delete alone (DRP-FR-33) — rather than dropping it, which
    /// would read as data loss for material that is still on disk.
    pub inconsistent: bool,
    pub updated_at: String,
    /// DRS-FR-41: the filesystem modification time of the draft's **one prompt
    /// file**, RFC 3339 UTC — read from disk here rather than held in
    /// `draft.toml`, so a prompt rewritten by an external editor carries exactly
    /// the activity a prompt rewritten through `save_draft_file_contents`
    /// carries.
    ///
    /// Separate from `updated_at` (DRS-FR-24) and never a substitute for it: a
    /// rename, a status change, a move between drafts folders, and a write under
    /// `assets/`, `comments/`, `proposals/`, `history/`, or `conversation.jsonl`
    /// each move `updated_at` and leave this exactly where it was. That is the
    /// whole reason it exists — the Dashboard's Active workstreams widget orders
    /// by *prompt* activity (`PST-project-storage.md` PST-FR-32), which is what
    /// the author was actually working on.
    ///
    /// `None` for a draft that is inconsistent (DRS-FR-15): there is no single
    /// prompt to stat, which is one more thing such a draft cannot answer rather
    /// than a case to guess at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_activity_at: Option<String>,
    /// DRS-FR-23: always idle. No operation in this module starts a build, so
    /// the field records that fact rather than reporting one.
    pub build: BuildState,
    /// DRP-FR-19: whether an agent has proposed a change to this draft that the
    /// author has not yet decided (DCP-FR-04).
    ///
    /// Carried on the summary rather than fetched per row, because the Drafts
    /// panel's cost claim is **one call** however many drafts the worktree holds
    /// — and this walk already opens each draft's folder, so the answer costs a
    /// directory read inside a walk that was happening anyway.
    pub has_pending_proposal: bool,
    /// DRS-FR-18: the graduation run the project's queue holds for this draft —
    /// its id, its current state, and whether that state locks the draft — or
    /// `None` where the queue holds none.
    ///
    /// Read from `GRD-graduation.md` GRD-FR-LGDV's queue and never cached, so what
    /// the Drafts panel shows is what the queue holds. Filled in by the command
    /// rather than by the walk, because the queue is one read for the whole
    /// list and the walk is per draft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graduation: Option<crate::graduation::DraftGraduation>,
    /// DRS-FR-XDWS: the issue link of a GitHub-shadow draft, or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_issue: Option<GithubIssueLink>,
}

/// One of the author's organising directories under the drafts root
/// (DRS-FR-29).
///
/// It carries no id: a drafts folder's identity **is** its path, because the
/// directory structure is the whole of the organisation and there is nothing
/// beside it to hold an id in. That is also why a rename or a move changes the
/// identity of everything beneath it, and why nothing outside this module
/// addresses one — every consumer addresses a draft by the id it keeps through
/// any move (DRS-FR-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftFolder {
    /// Drafts-root-relative, e.g. `UI/Components`.
    pub path: String,
    /// Drafts-root-relative path of the containing folder; `""` for a top-level
    /// folder.
    pub parent: String,
}

/// DRS-FR-08: the active worktree's whole drafts organisation, from one walk.
///
/// Folders and drafts travel together rather than in two calls because the
/// Drafts panel's cost is one list call however deep the tree runs
/// (`../ui/DRP-drafts-panel.md` DRP-FR-20), and the walk that finds the drafts
/// has already visited every folder on the way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftHierarchy {
    /// Every folder, at every depth, empty ones included.
    pub folders: Vec<DraftFolder>,
    pub drafts: Vec<DraftSummary>,
}

/// DRS-FR-23: whether an agent is working on a draft right now, which the Drafts
/// panel renders as a build indicator (DRP-FR-04). Nothing here produces the
/// running position yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BuildState {
    Idle,
    Running { started_at: String },
}

/// DRS-FR-06: what `create_draft` hands back — the record, and the draft-relative
/// path of the one file it was created holding, which the workspace opens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftCreated {
    pub draft: DraftRecord,
    pub file: String,
}

/// DRS-FR-17: why a draft survived the Drafts panel's text filter. The panel
/// annotates a row matched on its contents (DRP-FR-17), so the distinction is
/// carried rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftMatchedIn {
    Name,
    Contents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftMatch {
    pub draft_id: String,
    pub matched_in: DraftMatchedIn,
}

/// Body plus checksum, on the same contract `load_artifact_contents_by_id`
/// serves (PST-FR-15), so the editing surface's dirty/baseline handling is the
/// same one it uses for an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftContents {
    pub body: String,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSaveResult {
    pub checksum: String,
}
