//! The per-project slot of the user-global store (GSS-FR-16 to GSS-FR-18).
//!
//! One part of `global_settings.rs`.

use serde::{Deserialize, Serialize};

/// The per-project slot (GSS-FR-16 / GSS-FR-17 / GSS-FR-18).
///
/// Two facts about a project that cannot live inside the project on disk.
/// `.synthesis/` is committed content, so a repository holds one copy per
/// worktree — and both of these have to be stable across worktrees:
///
/// - **layout** (GSS-FR-17): panel sizing/side/collapse/hide plus the main
///   window's geometry. Per-worktree storage would reshuffle the shell's shape
///   on every switch.
/// - **active worktree** (GSS-FR-16): which checkout roots the project. It has
///   to be readable *before* any worktree has been chosen, so it cannot live in
///   the worktree it names.
///
/// The slot is keyed by project (the repository's primary worktree, or the
/// project root when it is not in a repository), never by directory, so a
/// repository with several worktrees shares one slot (GSS-FR-18).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectSlot {
    /// Absolute path of the worktree the project was last working in
    /// (GSS-FR-16). `None` until a worktree has been activated.
    ///
    /// Declared before `layout` deliberately: TOML requires a table's scalar
    /// values to be emitted before its sub-tables, and serde serialises fields
    /// in declaration order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_worktree: Option<String>,
    /// GSS-FR-23: the id of the GitHub token registry record this project's
    /// authenticated operations use. `None` until one is chosen.
    ///
    /// User-global rather than project-local because `.synthesis/project.toml`
    /// is committed content — a binding stored there would carry one author's
    /// choice to everyone who clones the repository. Like the two fields
    /// around it, it is a scalar and so must stay declared *before* `layout`:
    /// TOML requires a table's scalars to be emitted before its sub-tables.
    ///
    /// This module stores whatever id it is given and never checks that the
    /// record still exists; a binding pointing at a removed record is resolved
    /// by `crate::github_tokens` (GTS-FR-10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_token_id: Option<String>,
    /// GSS-FR-26: the vendor whose agentic integration this project uses in
    /// place of the user-global active one. `None` until one is chosen.
    ///
    /// User-global for the same reason as `github_token_id` above:
    /// `.synthesis/project.toml` is committed content, and an override stored
    /// there would carry one author's choice of local tooling to everyone who
    /// clones the repository. Also a scalar, so it must stay declared *before*
    /// `layout` — TOML requires a table's scalars to be emitted before its
    /// sub-tables.
    ///
    /// Stored as given and never checked for usability; an override naming a
    /// cleared or unverified integration is resolved by `crate::agentic`
    /// (AIC-FR-16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agentic_vendor: Option<String>,
    /// GSS-FR-28: the provider whose AI API integration this project uses in
    /// place of the user-global active one. `None` until one is chosen.
    ///
    /// Independent of `agentic_vendor` above: a project may override one level,
    /// both, or neither, so the two are separate fields rather than one
    /// combined choice. Another scalar, so it too stays declared *before*
    /// `layout`.
    ///
    /// Stored as given and never checked for usability; an override naming a
    /// cleared or unverified provider is resolved by `crate::ai_api`
    /// (AAP-FR-16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_api_provider: Option<String>,
    /// GSS-FR-31: the earlier per-project enrolment of agents. Read so that a
    /// store written by an earlier version loads, never written, and cleared by
    /// the one-time migration of [`GlobalSettingsStore::with_path`]. The slot
    /// holds no agent data.
    #[serde(rename = "enrolledAgents", skip_serializing)]
    pub(super) legacy_enrolled_agents: Vec<String>,
    /// GSS-FR-DSUG: whether embeddings improve this project's search —
    /// `use_global` when unset. User-global and keyed by project (GSS-FR-18),
    /// so each user chooses for self and every worktree reads one value. A
    /// scalar, so it stays before the arrays and tables below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddings_mode: Option<crate::embeddings::ProjectEmbeddingsMode>,
    /// GSS-FR-ARIP: whether the project's messages are answered as agent groups
    /// (advisors and Synthesis). Unset reads as off. User-global because the
    /// project's committed files never hold it. A scalar, so it stays before the
    /// arrays and tables below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_group_enabled: Option<bool>,
    /// GSS-FR-ARIP: the join timeout of the project's agent groups, in seconds.
    /// Unset reads as 60.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_group_join_timeout_secs: Option<u32>,
    /// GSS-FR-CDYK: the reference documents this project selects — an ordered
    /// list of `{ kind, path }`, empty until a source is added. References only:
    /// never a document's content, its extracted text, or an index.
    ///
    /// User-global because a selected path may lie outside the project, and keyed
    /// by project (GSS-FR-18, GSS-FR-ZWJW) so every worktree of a repository reads
    /// the same list. An array of tables, so it stays after every scalar field
    /// and beside `layout`, which TOML also requires to come last.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub document_sources: Vec<crate::documents::StoredSource>,
    /// GSS-FR-DAKR: the model downgrade suppressions, an array of tables, so after the scalars.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub model_downgrade_suppressions: Vec<crate::model_downgrades::ModelDowngradeSuppression>,
    /// GSS-FR-UTCM: MCP server id to trusted command fingerprint, a table, so after the scalars.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub mcp_trust: std::collections::BTreeMap<String, String>,
    /// GSS-FR-KNUB: team MCP server id to its switch on this computer, a table.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub mcp_switches: std::collections::BTreeMap<String, bool>,
    /// GSS-FR-EAJV: the personal MCP server records of this project, an array
    /// of tables in the shape of `.synthesis/project.toml`. Kept as tables, so
    /// a record that does not decode stays as it was.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub personal_mcp_servers: Vec<toml::Table>,
    /// Layout preferences (GSS-FR-17). `None` until the shell first persists.
    /// Skipped when absent — the TOML serialiser rejects a bare `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::layout::LayoutPreferences>,
}
