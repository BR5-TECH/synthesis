//! The GitHub polling settings and the GitHub pending claims
//! (PSS-FR-OPQD, PSS-FR-CNZO, PSS-FR-TXBB, PSS-FR-TXJV).
//!
//! The settings are project-public: the selected GitHub Project and the
//! polling interval are the same for every checkout of the project. The
//! pending claims are project-local: they record what this machine has done
//! and must never reach a commit.
//!
//! `GPP-github-polling.md` (GPP-FR-NLPG, GPP-FR-DHQM) is the only caller, and
//! no function here is a Tauri command.

use serde::{Deserialize, Serialize};

use super::{load_local, load_public, local_store_lock, local_toml_path, save_public, write_local};

/// PSS-FR-OPQD: the project-public table the polling settings live in.
const GITHUB_POLLING_KEY: &str = "githubPolling";
const PROJECT_NODE_ID_KEY: &str = "projectNodeId";
const INTERVAL_MINUTES_KEY: &str = "intervalMinutes";
/// PSS-FR-TXBB: the project-local list of pending claims.
const PENDING_CLAIMS_KEY: &str = "github_pending_claims";

/// GPP-FR-HZDD / PSS-FR-CNZO: the intervals a stored value may hold.
pub const GITHUB_POLLING_INTERVALS: [u32; 5] = [1, 5, 15, 30, 60];

/// PSS-FR-OPQD: the GitHub polling settings. Each value is unset or stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPollingSettings {
    pub project_node_id: Option<String>,
    pub interval_minutes: Option<u32>,
}

/// PSS-FR-TXBB: one pending claim, in the shape `GPP-github-polling.md`
/// declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPendingClaim {
    /// The normalized host of the repository. Absent in a record stored before
    /// hosts existed, which reads as `github.com` (GTS-FR-VRYL).
    #[serde(default = "crate::github_tokens::default_host")]
    pub repository_host: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub issue_number: u64,
    pub issue_url: String,
    pub project_node_id: String,
    /// `None` until the shadow draft exists (GPP-FR-CWGH). Serialised as
    /// `null` on the wire; the TOML encoder omits an absent value.
    #[serde(default)]
    pub draft_id: Option<String>,
    /// RFC 3339 UTC.
    pub claimed_at: String,
}

impl GithubPendingClaim {
    /// Whether this claim names issue `number` of `owner/name` on `host`. The
    /// host and repository matches are case-insensitive. The same issue number
    /// on another host is another issue.
    pub fn names(&self, host: &str, owner: &str, name: &str, number: u64) -> bool {
        self.issue_number == number
            && self.repository_host.eq_ignore_ascii_case(host)
            && self.repository_owner.eq_ignore_ascii_case(owner)
            && self.repository_name.eq_ignore_ascii_case(name)
    }

    /// PSS-FR-TXJV: a record with every required value present.
    fn is_complete(&self) -> bool {
        !self.repository_owner.is_empty()
            && !self.repository_name.is_empty()
            && self.issue_number > 0
            && !self.issue_url.is_empty()
            && !self.project_node_id.is_empty()
            && !self.claimed_at.is_empty()
            && self.draft_id.as_deref().is_none_or(|id| !id.is_empty())
    }
}

/// PSS-FR-OPQD / PSS-FR-CNZO: the stored settings, each value repaired to
/// unset where it is not valid. A malformed `project.toml` is the typed error
/// of PSS-FR-10.
pub fn load_github_polling_settings_from(
    root: &crate::fs::RootFs,
) -> Result<GithubPollingSettings, String> {
    let table = load_public(root)?;
    let Some(section) = table.get(GITHUB_POLLING_KEY).and_then(|v| v.as_table()) else {
        return Ok(GithubPollingSettings::default());
    };
    let project_node_id = section
        .get(PROJECT_NODE_ID_KEY)
        .and_then(|v| v.as_str())
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string);
    let interval_minutes = section
        .get(INTERVAL_MINUTES_KEY)
        .and_then(|v| v.as_integer())
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| GITHUB_POLLING_INTERVALS.contains(n));
    Ok(GithubPollingSettings { project_node_id, interval_minutes })
}

/// PSS-FR-OPQD: persist the settings. An unset value has no key, and every
/// other project-public section is carried through unchanged (PSS-FR-17).
pub fn save_github_polling_settings_to(
    root: &crate::fs::RootFs,
    settings: &GithubPollingSettings,
) -> Result<(), String> {
    let mut table = load_public(root)?;
    let mut section = toml::Table::new();
    if let Some(id) = settings.project_node_id.as_deref().filter(|id| !id.is_empty()) {
        section.insert(PROJECT_NODE_ID_KEY.to_string(), toml::Value::String(id.to_string()));
    }
    if let Some(minutes) = settings.interval_minutes {
        section.insert(INTERVAL_MINUTES_KEY.to_string(), toml::Value::Integer(i64::from(minutes)));
    }
    match section.is_empty() {
        true => {
            table.remove(GITHUB_POLLING_KEY);
        }
        false => {
            table.insert(GITHUB_POLLING_KEY.to_string(), toml::Value::Table(section));
        }
    }
    save_public(root, &table)
}

/// PSS-FR-TXBB / PSS-FR-TXJV: the pending claims of this checkout. A missing
/// list reads as empty, and a malformed or incomplete record is dropped.
pub fn load_github_pending_claims_from(root: &crate::fs::RootFs) -> Vec<GithubPendingClaim> {
    claims_of(&load_local(root))
}

fn claims_of(config: &toml::Table) -> Vec<GithubPendingClaim> {
    let Some(items) = config.get(PENDING_CLAIMS_KEY).and_then(|v| v.as_array()).cloned() else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| item.try_into::<GithubPendingClaim>().ok())
        .filter(GithubPendingClaim::is_complete)
        .collect()
}

/// PSS-FR-TXBB: persist the whole list into the project-local store, carrying
/// every other project-local section through unchanged. An empty list has no
/// key. The project-public store is never written (PSS-FR-TXJV).
pub fn save_github_pending_claims_to(
    root: &crate::fs::RootFs,
    claims: &[GithubPendingClaim],
) -> Result<(), String> {
    update_github_pending_claims(root, |stored| *stored = claims.to_vec())
}

/// PSS-FR-TXBB: read, change, and write the pending claims as one act. The
/// process-wide `local.toml` lock is held across the read and the write, so
/// two claims cannot lose each other's record.
///
/// A `local.toml` that exists and does not parse is refused rather than
/// repaired: repairing it here would replace every other section with
/// defaults to record one claim.
pub fn update_github_pending_claims(
    root: &crate::fs::RootFs,
    change: impl FnOnce(&mut Vec<GithubPendingClaim>),
) -> Result<(), String> {
    let _guard = local_store_lock();
    let mut config = match root.read_toml::<toml::Table>(local_toml_path(root)) {
        Ok(table) => table,
        Err(crate::fs::FsError::NotFound { .. }) => toml::Table::new(),
        Err(_) => return Err("the project-local store is malformed".to_string()),
    };
    let mut claims = claims_of(&config);
    change(&mut claims);
    if claims.is_empty() {
        config.remove(PENDING_CLAIMS_KEY);
    } else {
        let items = claims
            .iter()
            .map(toml::Value::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "failed to encode the GitHub pending claims".to_string())?;
        config.insert(PENDING_CLAIMS_KEY.to_string(), toml::Value::Array(items));
    }
    write_local(root, &config)
}
