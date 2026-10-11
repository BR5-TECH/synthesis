//! The conversational agent registry — `specifications/core/AGR-agent-registry.md`.
//!
//! An **agent** here is a persona and nothing more: a nickname it is addressed
//! by, optionally a title naming the role it takes, a model, the reasoning that
//! model can be asked for, and optionally the instructions it works under
//! (AGR-FR-01). It names no provider: the model is read against the AI API
//! provider active for the open project (AGR-FR-06). No field of it names a tool, a path,
//! a command, or a permission, which is the whole of what separates it from the
//! agent *backends* of `crate::agentic`: one answers, the other acts.
//!
//! A persona has one of two **scopes**:
//!
//! - A **Personal agent** is user-global (AGR-FR-02). A persona describes how
//!   its author wants to be argued with, so it belongs to the machine, is
//!   available before any project is open, and follows the author into every
//!   project.
//! - A **Project agent** lives in the project (AGR-FR-12): one committed file
//!   per agent under the active worktree's `.synthesis/agents/`
//!   ([`project_store`]), shared by every contributor who syncs the project.
//!   The file holds persona fields only and never a provider, a URL, or a key.
//!
//! A Project agent shadows a Personal agent of the same nickname in its project
//! (AGR-FR-15). This module describes *who may be spoken to* and never speaks
//! to them. Holding the conversation is `crate::agent_conversations`'.

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::ai_api::{self, ReasoningChoice};
use crate::fs::RootFs;
use crate::global_settings::GlobalSettingsStore;
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;

pub mod project_store;

/// AGR-FR-ZRJD: emitted after a create, an update, or a delete of an agent of
/// either scope succeeds, so every window showing agents reads them again. It
/// carries no payload and no agent content.
pub const AGENTS_CHANGED: &str = "agents-changed";

// ---------------------------------------------------------------------------
// Typed errors (AGR contract surface)
// ---------------------------------------------------------------------------

/// The nickname field was empty (AGR-FR-04).
pub const ERR_NICKNAME_EMPTY: &str = "nickname_empty";
/// The nickname carries whitespace or an `@` (AGR-FR-04).
pub const ERR_NICKNAME_INVALID: &str = "nickname_invalid";
/// The nickname is the reserved handle that addresses every agent on the
/// roster (AGR-FR-04).
pub const ERR_NICKNAME_RESERVED: &str = "nickname_reserved";
/// Another agent already answers to that nickname (AGR-FR-05).
pub const ERR_NICKNAME_TAKEN: &str = "nickname_taken";
/// No AI API provider is configured at all (AGR-FR-06).
pub const ERR_PROVIDER_UNCONFIGURED: &str = "provider_unconfigured";
/// Providers are configured, but none resolves for the project (AGR-FR-06).
pub const ERR_PROVIDER_NOT_VERIFIED: &str = "provider_not_verified";
/// No agent of that scope carries that id (AGR-FR-10).
pub const ERR_AGENT_NOT_FOUND: &str = "agent_not_found";
/// The agent resolves, but no provider resolves or the active provider does not
/// serve its model (AGR-FR-18).
pub const ERR_AGENT_UNAVAILABLE: &str = "agent_unavailable";
/// A command of the Project scope was called with no project open (AGR-FR-22).
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";

/// The longest nickname this module will store. Not a spec requirement — a
/// nickname is typed inline into a message, so an unbounded one is a way to
/// make a store unreadable rather than a way to name a persona.
const NICKNAME_MAX: usize = 64;

/// AGR-FR-04: the handle that addresses every agent on a project's roster
/// (`../ui/AGT-agents.md` AGT-FR-35). No agent carries it as a nickname, so the
/// handle means one thing in every project and no author describes a persona
/// they would then be unable to summon.
pub const RESERVED_NICKNAME: &str = "all";

// ---------------------------------------------------------------------------
// Wire and persisted shapes (AGR contract surface)
// ---------------------------------------------------------------------------

/// One described persona. The same shape on the wire and on disk, because it
/// carries no secret in any form and names no provider (AGR-FR-20). A stored
/// record from an earlier build that still has a `provider` field loads, the
/// field being ignored, and the next write of the record omits it.
///
/// `#[serde(default)]` matters for the same reason it does on every other
/// record in `synthesis.toml`: a store written before a field existed must load
/// rather than sending the whole file through GSS-FR-13's repair-to-defaults,
/// which would wipe recents and every registry.
///
/// Field order is load-bearing for TOML: every scalar is declared before
/// `reasoning`, which serialises as a table.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Agent {
    /// Opaque, generated at creation, stable for the agent's lifetime
    /// (AGR-FR-03). Derived from neither the nickname nor any other field, so
    /// renaming an agent leaves its id unchanged. For a Project agent it is the
    /// name of the agent's file.
    pub id: String,
    pub nickname: String,
    /// AGR-FR-23: the role this persona takes — `UI/UX designer`, `Developer`.
    /// Stored **trimmed**, with internal whitespace and casing preserved, and
    /// `""` where the author named none. `#[serde(default)]` on the struct is
    /// what makes AGR-FR-24 hold: a record written before the field existed
    /// loads with `""` rather than sending the store through GSS-FR-13's
    /// repair-to-defaults.
    pub title: String,
    pub model_id: String,
    /// Free text, stored verbatim and never parsed here (AGR-FR-08). An empty
    /// value is an ordinary state rather than an unconfigured one.
    pub instructions: String,
    /// RFC 3339 UTC. Empty for a Project agent, whose file carries no timestamp.
    pub created_at: String,
    /// RFC 3339 UTC. Empty for a Project agent.
    pub updated_at: String,
    /// `None` means the model's own default (AGR-FR-07). Serialises as a table,
    /// so it sits after every scalar above — TOML assigns a bare `key = value`
    /// to the most recently opened table, and a scalar declared after this one
    /// would land inside it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningChoice>,
}

/// What `create_agent` and `update_agent` carry: an [`Agent`] without the three
/// fields this module owns.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentDraft {
    pub nickname: String,
    /// AGR-FR-23: carried as the author typed it and normalised by
    /// [`normalise_title`] before validation and before anything is written.
    pub title: String,
    pub model_id: String,
    pub instructions: String,
    pub reasoning: Option<ReasoningChoice>,
}

/// Whether an agent can currently be spoken to (AGR-FR-16).
///
/// A property of the AI API registry rather than of the agent, so it is computed
/// at read time rather than stored: a key that expired while the application was
/// closed must read as such on the next launch without anything having written
/// that fact down.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAvailability {
    #[default]
    Ready,
    /// No AI API provider is configured at all.
    ProviderUnconfigured,
    /// Providers are configured, but none resolves for the project.
    ProviderUnverified,
    /// The active provider does not offer this model.
    ModelUnavailable,
    /// A Personal agent that a Project agent of the same nickname hides in the
    /// open project (AGR-FR-15).
    Shadowed,
}

/// Where an agent is stored (AGR-FR-02, AGR-FR-12).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentScope {
    /// Stored in the project, committed, shared by every contributor.
    Project,
    /// Stored on this machine, available in every project.
    Personal,
}

/// An agent as a project sees it: with its scope and its availability.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgent {
    pub agent: Agent,
    pub scope: AgentScope,
    pub availability: AgentAvailability,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// AGR-FR-04: a nickname is typed inline into a message to address its agent
/// (`../ui/AGT-agents.md` AGT-FR-24), and every constraint follows from that —
/// one carrying a space could not be told from the words after it, one carrying
/// an `@` could not be told from the sigil that introduces it, and `all` is the
/// handle that addresses every agent a project has enrolled, which one agent
/// holding that nickname would make ambiguous everywhere it is written.
pub fn validate_nickname(nickname: &str) -> Result<(), &'static str> {
    if nickname.is_empty() {
        return Err(ERR_NICKNAME_EMPTY);
    }
    if nickname.chars().count() > NICKNAME_MAX
        || nickname.chars().any(|c| c.is_whitespace() || c == '@')
    {
        return Err(ERR_NICKNAME_INVALID);
    }
    // Compared the way every other nickname comparison is (AGR-FR-05), so
    // `ALL` and `All` are refused alongside `all` — a reservation that held for
    // one spelling alone would be no reservation at all.
    if same_nickname(nickname, RESERVED_NICKNAME) {
        return Err(ERR_NICKNAME_RESERVED);
    }
    Ok(())
}

/// AGR-FR-05: nicknames are compared without regard to case, so a tag resolves
/// to exactly one agent in every project that could hold it.
fn same_nickname(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// AGR-FR-13 / AGR-FR-14: the stable order a listing of one scope returns.
fn sort_by_nickname(agents: &mut [Agent]) {
    agents.sort_by(|a, b| {
        a.nickname
            .to_lowercase()
            .cmp(&b.nickname.to_lowercase())
            // Two agents cannot share a nickname (AGR-FR-05), but a store that
            // was hand-edited could; the id breaks the tie so the order is
            // still total rather than dependent on the file's own.
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Map `crate::ai_api`'s validation vocabulary onto this module's.
///
/// The two differ deliberately: AAP answers about a *provider*, this module
/// answers about an *agent*, and `provider_not_verified` reads correctly on a
/// row that names a persona where AAP's bare `not_verified` would not.
fn map_selection_error(err: &str) -> &'static str {
    match err {
        ai_api::ERR_PROVIDER_UNCONFIGURED | ai_api::ERR_UNKNOWN_PROVIDER => {
            ERR_PROVIDER_UNCONFIGURED
        }
        ai_api::ERR_NOT_VERIFIED => ERR_PROVIDER_NOT_VERIFIED,
        ai_api::ERR_UNKNOWN_MODEL => ai_api::ERR_UNKNOWN_MODEL,
        ai_api::ERR_REASONING_UNSUPPORTED => ai_api::ERR_REASONING_UNSUPPORTED,
        ai_api::ERR_UNKNOWN_EFFORT => ai_api::ERR_UNKNOWN_EFFORT,
        ai_api::ERR_REASONING_MANDATORY => ai_api::ERR_REASONING_MANDATORY,
        // A store error rather than a validation verdict. Reported as
        // unconfigured, which is the conservative reading: nothing usable was
        // found behind that provider.
        _ => ERR_PROVIDER_UNCONFIGURED,
    }
}

/// AGR-FR-16: availability, computed from the AI API registry alone, against the
/// provider the open project resolves to (AAP-FR-APRV).
///
/// Asking `validate_ai_api_selection` with **no** reasoning is what makes this
/// exactly the provider-and-model question: a null choice is the model's own
/// default and is always serviceable (AAP-FR-27), so the only outcomes left are
/// the three degraded provider states and `ready`. Reasoning is validated when
/// an agent is *saved*, not when it is listed — a stored choice cannot have gone
/// stale without the model list having changed, which `model_unavailable`
/// already reports.
///
/// Reads the registry and nothing else: no network request, no keychain access
/// (AAP-FR-33), so a roster renders offline and while the keychain is locked.
pub fn availability_of(
    store: &GlobalSettingsStore,
    project_key: &str,
    agent: &Agent,
) -> AgentAvailability {
    let provider = match ai_api::resolve_agent_provider(store, project_key) {
        Ok(provider) => provider,
        Err(e) => {
            return match map_selection_error(&e) {
                ERR_PROVIDER_UNCONFIGURED => AgentAvailability::ProviderUnconfigured,
                _ => AgentAvailability::ProviderUnverified,
            }
        }
    };
    match ai_api::validate_ai_api_selection(store, &provider, &agent.model_id, None) {
        Ok(()) => AgentAvailability::Ready,
        Err(e) => match map_selection_error(&e) {
            ERR_PROVIDER_UNCONFIGURED => AgentAvailability::ProviderUnconfigured,
            ERR_PROVIDER_NOT_VERIFIED => AgentAvailability::ProviderUnverified,
            _ => AgentAvailability::ModelUnavailable,
        },
    }
}

// ---------------------------------------------------------------------------
// Implementations (testable without a Tauri runtime)
// ---------------------------------------------------------------------------

/// AGR-FR-02: every Personal agent, ordered by nickname without regard to case.
fn sorted_personal(store: &GlobalSettingsStore) -> Result<Vec<Agent>, String> {
    let mut agents = store.load_agent_registry()?;
    sort_by_nickname(&mut agents);
    Ok(agents)
}

/// AGR-FR-15: whether a Project agent of the same nickname hides this agent.
fn is_shadowed(project: &[Agent], agent: &Agent) -> bool {
    project.iter().any(|p| same_nickname(&p.nickname, &agent.nickname))
}

/// AGR-FR-16: one agent with its scope and availability. A shadowed agent
/// reports `shadowed` in place of every provider state.
fn describe(
    store: &GlobalSettingsStore,
    project_key: &str,
    agent: Agent,
    scope: AgentScope,
    shadowed: bool,
) -> ProjectAgent {
    let availability = if shadowed {
        AgentAvailability::Shadowed
    } else {
        availability_of(store, project_key, &agent)
    };
    ProjectAgent { agent, scope, availability }
}

/// AGR-FR-14: the Personal agents, answering whether or not a project is open.
/// With a worktree, the agents a Project agent shadows report `shadowed`.
pub fn list_personal_agents_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: Option<&RootFs>,
) -> Result<Vec<ProjectAgent>, String> {
    // A folder that cannot be read costs the shadow marks and nothing else: the
    // Personal agents are the author's own and are listed regardless.
    let project = match worktree {
        Some(root) => project_store::list(root).map(|f| f.agents).unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(sorted_personal(store)?
        .into_iter()
        .map(|agent| {
            let shadowed = is_shadowed(&project, &agent);
            describe(store, project_key, agent, AgentScope::Personal, shadowed)
        })
        .collect())
}

/// AGR-FR-13: the roster of the open project, together with the names of the
/// Project agent files that were skipped (PSS-FR-AGRD). Ordered by nickname
/// without regard to case, a Project agent before a Personal agent of the same
/// nickname.
pub fn roster_of(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: &RootFs,
) -> Result<(Vec<ProjectAgent>, Vec<String>), String> {
    let files = project_store::list(worktree)?;
    let mut roster: Vec<ProjectAgent> = Vec::new();
    for agent in &files.agents {
        roster.push(describe(store, project_key, agent.clone(), AgentScope::Project, false));
    }
    for agent in sorted_personal(store)? {
        let shadowed = is_shadowed(&files.agents, &agent);
        roster.push(describe(store, project_key, agent, AgentScope::Personal, shadowed));
    }
    roster.sort_by(|a, b| {
        a.agent
            .nickname
            .to_lowercase()
            .cmp(&b.agent.nickname.to_lowercase())
            .then_with(|| scope_rank(a.scope).cmp(&scope_rank(b.scope)))
            .then_with(|| a.agent.id.cmp(&b.agent.id))
    });
    Ok((roster, files.skipped))
}

fn scope_rank(scope: AgentScope) -> u8 {
    match scope {
        AgentScope::Project => 0,
        AgentScope::Personal => 1,
    }
}

/// AGR-FR-13: the roster alone.
pub fn list_project_agents_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: &RootFs,
) -> Result<Vec<ProjectAgent>, String> {
    roster_of(store, project_key, worktree).map(|(roster, _)| roster)
}

/// The agents of one scope, in the order of the matching read.
fn list_scope_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: Option<&RootFs>,
    scope: AgentScope,
) -> Result<Vec<ProjectAgent>, String> {
    match scope {
        AgentScope::Personal => list_personal_agents_impl(store, project_key, worktree),
        AgentScope::Project => {
            let root = require_worktree(worktree)?;
            let mut roster = list_project_agents_impl(store, project_key, root)?;
            roster.retain(|p| p.scope == AgentScope::Project);
            Ok(roster)
        }
    }
}

/// AGR-FR-22: a command of the Project scope answers only while a project is open.
fn require_worktree(worktree: Option<&RootFs>) -> Result<&RootFs, String> {
    worktree.ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}

/// AGR-FR-23: the stored form of a supplied title.
///
/// Trimming is the whole of the normalisation: leading and trailing whitespace
/// goes under Rust's `str::trim` — the platform's standard semantics, which
/// covers every Unicode `White_Space` codepoint rather than ASCII blanks alone —
/// and internal whitespace, casing, and every other character the author typed
/// survive byte-for-byte. Braces included: a title reading `{{ agent-title }}`
/// is stored as that text and substituted as that text, never re-expanded
/// (CVL-FR-04).
///
/// Whitespace-only collapses to `""`, which is the same value an author who
/// named none stores. Nothing distinguishes the two, and nothing needs to: an
/// empty title is an ordinary state, refused by no validation and named by no
/// error.
pub fn normalise_title(title: &str) -> String {
    title.trim().to_string()
}

/// The whole of AGR-FR-04 through AGR-FR-07, run against the agents of **one
/// scope** that do not yet contain the draft. `exclude` is the id being
/// updated, so an agent keeping its own nickname is not a collision with itself.
///
/// An update that leaves the nickname as it stands is never `nickname_taken`
/// (AGR-FR-05). Two Project agent files merged from two branches can hold the
/// same nickname, and refusing every edit of either would leave the author no
/// way to correct one of them.
fn validate_draft(
    store: &GlobalSettingsStore,
    project_key: &str,
    agents: &[Agent],
    draft: &AgentDraft,
    exclude: Option<&str>,
) -> Result<(), String> {
    validate_nickname(&draft.nickname)?;
    let keeps_nickname = agents.iter().any(|a| {
        Some(a.id.as_str()) == exclude && a.nickname.eq_ignore_ascii_case(&draft.nickname)
    });
    if !keeps_nickname
        && agents
            .iter()
            .any(|a| Some(a.id.as_str()) != exclude && same_nickname(&a.nickname, &draft.nickname))
    {
        return Err(ERR_NICKNAME_TAKEN.into());
    }
    // AGR-FR-06 / AGR-FR-07: the model and the reasoning are checked by the same
    // rule that governs a provider's own selection, against the provider the
    // open project resolves to, so an agent can never be saved pointing at a
    // model the active endpoint does not serve.
    let provider = ai_api::resolve_agent_provider(store, project_key)
        .map_err(|e| map_selection_error(&e).to_string())?;
    ai_api::validate_ai_api_selection(store, &provider, &draft.model_id, draft.reasoning.as_ref())
        .map_err(|e| map_selection_error(&e).to_string())
}

/// AGR-FR-09: validate the whole draft before writing anything, so a rejected
/// creation leaves the store byte-for-byte as it was.
pub fn create_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: Option<&RootFs>,
    scope: AgentScope,
    draft: &AgentDraft,
) -> Result<Agent, String> {
    // AGR-FR-23: normalised before it is written, so what the store holds is
    // what every reader of it — a comment's snapshot, a compiled prompt, the
    // editor reopened — sees.
    let title = normalise_title(&draft.title);
    match scope {
        AgentScope::Personal => {
            let mut agents = store.load_agent_registry()?;
            validate_draft(store, project_key, &agents, draft, None)?;
            let now = now_rfc3339();
            let agent = Agent {
                id: new_note_id(),
                nickname: draft.nickname.clone(),
                title,
                model_id: draft.model_id.clone(),
                instructions: draft.instructions.clone(),
                created_at: now.clone(),
                updated_at: now,
                reasoning: draft.reasoning.clone(),
            };
            agents.push(agent.clone());
            store.save_agent_registry(agents)?;
            Ok(agent)
        }
        AgentScope::Project => {
            let root = require_worktree(worktree)?;
            let existing = project_store::list(root)?.agents;
            validate_draft(store, project_key, &existing, draft, None)?;
            let id = new_note_id();
            project_store::write(root, &id, draft, &title)?;
            Ok(Agent {
                id,
                nickname: draft.nickname.clone(),
                title,
                model_id: draft.model_id.clone(),
                instructions: draft.instructions.clone(),
                created_at: String::new(),
                updated_at: String::new(),
                reasoning: draft.reasoning.clone(),
            })
        }
    }
}

/// AGR-FR-10: replace every field of the draft under exactly the validation
/// `create_agent` applies. A Personal agent refreshes `updated_at` and keeps
/// `created_at`; the id never changes.
pub fn update_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: Option<&RootFs>,
    scope: AgentScope,
    id: &str,
    draft: &AgentDraft,
) -> Result<Agent, String> {
    let title = normalise_title(&draft.title);
    match scope {
        AgentScope::Personal => {
            let mut agents = store.load_agent_registry()?;
            let index = agents.iter().position(|a| a.id == id).ok_or(ERR_AGENT_NOT_FOUND)?;
            validate_draft(store, project_key, &agents, draft, Some(id))?;
            let updated = Agent {
                id: agents[index].id.clone(),
                nickname: draft.nickname.clone(),
                title,
                model_id: draft.model_id.clone(),
                instructions: draft.instructions.clone(),
                created_at: agents[index].created_at.clone(),
                updated_at: now_rfc3339(),
                reasoning: draft.reasoning.clone(),
            };
            agents[index] = updated.clone();
            store.save_agent_registry(agents)?;
            Ok(updated)
        }
        AgentScope::Project => {
            let root = require_worktree(worktree)?;
            let existing = project_store::list(root)?.agents;
            if !existing.iter().any(|a| a.id == id) {
                return Err(ERR_AGENT_NOT_FOUND.into());
            }
            validate_draft(store, project_key, &existing, draft, Some(id))?;
            project_store::write(root, id, draft, &title)?;
            Ok(Agent {
                id: id.to_string(),
                nickname: draft.nickname.clone(),
                title,
                model_id: draft.model_id.clone(),
                instructions: draft.instructions.clone(),
                created_at: String::new(),
                updated_at: String::new(),
                reasoning: draft.reasoning.clone(),
            })
        }
    }
}

/// AGR-FR-11: remove one definition of one scope and return the agents of that
/// scope as they stand afterwards. Idempotent. The caller cancels the turns.
pub fn delete_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: Option<&RootFs>,
    scope: AgentScope,
    id: &str,
) -> Result<Vec<ProjectAgent>, String> {
    match scope {
        AgentScope::Personal => {
            let mut agents = store.load_agent_registry()?;
            agents.retain(|a| a.id != id);
            store.save_agent_registry(agents)?;
        }
        AgentScope::Project => {
            project_store::remove(require_worktree(worktree)?, id)?;
        }
    }
    list_scope_impl(store, project_key, worktree, scope)
}

/// AGR-FR-18: resolve a typed nickname to the agent this project means by it.
///
/// Matched without regard to case against the project's Project agents first —
/// a match there is the result, whatever its availability — and then against
/// the Personal agents that no Project agent shadows. The roster is already
/// ordered with a Project agent before a Personal one of the same nickname, and
/// a shadowed agent is skipped, so the first match is the right one.
///
/// The reserved handle is a nickname no agent can carry (AGR-FR-04), so it
/// matches nothing here and every dispatch reaching this module names one real
/// agent. Expanding `all` into the agents it stands for happens in the surface
/// that read the tag (`../ui/AGT-agents.md` AGT-FR-36) and never here.
///
/// Deliberately **not** a `#[tauri::command]`: it is the read path a dispatch
/// takes (`crate::agent_conversations`).
#[allow(dead_code)]
pub fn resolve_project_agent(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: &RootFs,
    nickname: &str,
) -> Result<Agent, String> {
    let roster = list_project_agents_impl(store, project_key, worktree)?;
    let found = roster
        .into_iter()
        .filter(|p| p.availability != AgentAvailability::Shadowed)
        .find(|p| same_nickname(&p.agent.nickname, nickname))
        .ok_or(ERR_AGENT_NOT_FOUND)?;
    match found.availability {
        AgentAvailability::Ready => Ok(found.agent),
        _ => Err(ERR_AGENT_UNAVAILABLE.into()),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (AGR contract surface)
// ---------------------------------------------------------------------------

/// Record the outcome of a write. Only the operation, the scope, the agent's id,
/// and a typed error code are logged — never a nickname, a title, or the
/// instructions, which are the author's own content.
fn log_write<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &str,
    scope: AgentScope,
    id: &str,
    outcome: &Result<(), &str>,
) {
    use crate::logging::{log_info, log_warn, Domain, BUFFER};
    let scope = match scope {
        AgentScope::Project => "project",
        AgentScope::Personal => "personal",
    };
    match outcome {
        Ok(()) => log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "agent written",
            crate::log_fields! { "operation" => operation, "scope" => scope, "agentId" => id },
        ),
        Err(code) => log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "agent write refused",
            crate::log_fields! { "operation" => operation, "scope" => scope, "reason" => *code },
        ),
    }
}

/// The open project's worktree, or `None` when no project is open.
fn open_worktree(project: &ProjectState) -> Option<RootFs> {
    project.require_root().ok()
}

#[tauri::command]
pub fn list_personal_agents(
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Vec<ProjectAgent>, String> {
    list_personal_agents_impl(&store, &project.slot_key(), open_worktree(&project).as_ref())
}

#[tauri::command]
pub fn list_project_agents<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Vec<ProjectAgent>, String> {
    let key = require_project(&project)?;
    let worktree = open_worktree(&project).ok_or(ERR_NO_PROJECT_OPEN)?;
    let (roster, skipped) = roster_of(&store, &key, &worktree)?;
    // PSS-FR-AGRD: a skipped file is handled, so it is invisible unless it is
    // recorded. Only the file names are logged, never their content.
    for name in skipped {
        crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "a project agent file was skipped",
            crate::log_fields! { "file" => name },
        );
    }
    Ok(roster)
}

#[tauri::command]
pub fn create_agent<R: tauri::Runtime>(
    scope: AgentScope,
    draft: AgentDraft,
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Agent, String> {
    let result = create_agent_impl(
        &store,
        &project.slot_key(),
        open_worktree(&project).as_ref(),
        scope,
        &draft,
    );
    let id = result.as_ref().map(|a| a.id.as_str()).unwrap_or("");
    log_write(&app, "create", scope, id, &result.as_ref().map(|_| ()).map_err(String::as_str));
    let agent = result?;
    let _ = app.emit(AGENTS_CHANGED, ());
    Ok(agent)
}

#[tauri::command]
pub fn update_agent<R: tauri::Runtime>(
    scope: AgentScope,
    id: String,
    draft: AgentDraft,
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Agent, String> {
    let result = update_agent_impl(
        &store,
        &project.slot_key(),
        open_worktree(&project).as_ref(),
        scope,
        &id,
        &draft,
    );
    log_write(&app, "update", scope, &id, &result.as_ref().map(|_| ()).map_err(String::as_str));
    let agent = result?;
    let _ = app.emit(AGENTS_CHANGED, ());
    Ok(agent)
}

/// AGR-FR-11: deleting an agent also cancels every turn in flight for it
/// (AGC-FR-20) — an agent that no longer exists should not go on answering. A
/// Project agent's turns are cancelled in the open project; a Personal agent's
/// in every project.
#[tauri::command]
pub fn delete_agent<R: tauri::Runtime>(
    scope: AgentScope,
    id: String,
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    turns: State<'_, crate::agent_conversations::TurnRegistry>,
    progress: State<'_, crate::progress::ProgressRegistry>,
) -> Result<Vec<ProjectAgent>, String> {
    let key = project.slot_key();
    let result = delete_agent_impl(&store, &key, open_worktree(&project).as_ref(), scope, &id);
    log_write(&app, "delete", scope, &id, &result.as_ref().map(|_| ()).map_err(String::as_str));
    let agents = result?;
    let cancel_in = match scope {
        AgentScope::Project => Some(key.as_str()),
        AgentScope::Personal => None,
    };
    crate::agent_conversations::cancel_turns_for_agent(&app, &turns, &progress, &id, cancel_in);
    let _ = app.emit(AGENTS_CHANGED, ());
    Ok(agents)
}

/// AGR-FR-22: the roster answers only while a project is open.
fn require_project(project: &ProjectState) -> Result<String, String> {
    project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.into())
}

#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod tests;
