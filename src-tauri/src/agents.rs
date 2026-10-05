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
//! Two storage facts shape everything below:
//!
//! - **Definitions are user-global** (AGR-FR-02). A persona describes how its
//!   author wants to be argued with, so it belongs to the machine and is
//!   available before any project is open.
//! - **Enrolment is per project** (AGR-FR-12), and lives in the user-global
//!   per-project slot rather than in the project's committed `.synthesis/` —
//!   it names records that exist only here, and a committed enrolment would
//!   carry one author's personas to everyone who clones the repository.
//!
//! This module describes *who may be spoken to* and never speaks to them.
//! Holding the conversation is `crate::agent_conversations`'.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ai_api::{self, ReasoningChoice};
use crate::global_settings::GlobalSettingsStore;
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;

// ---------------------------------------------------------------------------
// Typed errors (AGR contract surface)
// ---------------------------------------------------------------------------

/// The nickname field was empty (AGR-FR-04).
pub const ERR_NICKNAME_EMPTY: &str = "nickname_empty";
/// The nickname carries whitespace or an `@` (AGR-FR-04).
pub const ERR_NICKNAME_INVALID: &str = "nickname_invalid";
/// The nickname is the reserved handle that addresses every enrolled agent
/// (AGR-FR-04).
pub const ERR_NICKNAME_RESERVED: &str = "nickname_reserved";
/// Another agent already answers to that nickname (AGR-FR-05).
pub const ERR_NICKNAME_TAKEN: &str = "nickname_taken";
/// No AI API provider is configured at all (AGR-FR-06).
pub const ERR_PROVIDER_UNCONFIGURED: &str = "provider_unconfigured";
/// Providers are configured, but none resolves for the project (AGR-FR-06).
pub const ERR_PROVIDER_NOT_VERIFIED: &str = "provider_not_verified";
/// No agent carries that id (AGR-FR-10 / AGR-FR-14).
pub const ERR_AGENT_NOT_FOUND: &str = "agent_not_found";
/// The agent resolves, but no provider resolves or the active provider does not
/// serve its model (AGR-FR-18).
pub const ERR_AGENT_UNAVAILABLE: &str = "agent_unavailable";
/// A project-scoped command was called with no project open (AGR-FR-22).
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";

/// The longest nickname this module will store. Not a spec requirement — a
/// nickname is typed inline into a message, so an unbounded one is a way to
/// make a store unreadable rather than a way to name a persona.
const NICKNAME_MAX: usize = 64;

/// AGR-FR-04: the handle that addresses every agent a project has enrolled
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
    /// renaming an agent leaves every enrolment naming it intact.
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
    pub created_at: String,
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
}

/// An enrolled agent as a project sees it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgent {
    pub agent: Agent,
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

/// AGR-FR-13 / AGR-FR-09: the stable order every listing returns.
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

/// AGR-FR-02: the whole registry, ordered by nickname without regard to case.
/// Answers regardless of whether a project is open, because a definition belongs
/// to the machine (AGR-FR-22).
pub fn list_agents_impl(store: &GlobalSettingsStore) -> Result<Vec<Agent>, String> {
    let mut agents = store.load_agent_registry()?;
    sort_by_nickname(&mut agents);
    Ok(agents)
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

/// The whole of AGR-FR-04 through AGR-FR-07, run against a registry that does
/// **not** yet contain the draft. `exclude` is the id being updated, so an agent
/// keeping its own nickname is not a collision with itself.
fn validate_draft(
    store: &GlobalSettingsStore,
    project_key: &str,
    agents: &[Agent],
    draft: &AgentDraft,
    exclude: Option<&str>,
) -> Result<(), String> {
    validate_nickname(&draft.nickname)?;
    if agents
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
    ai_api::validate_ai_api_selection(
        store,
        &provider,
        &draft.model_id,
        draft.reasoning.as_ref(),
    )
    .map_err(|e| map_selection_error(&e).to_string())
}

/// AGR-FR-09: validate the whole draft before writing anything, so a rejected
/// creation leaves the registry byte-for-byte as it was.
pub fn create_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    draft: &AgentDraft,
) -> Result<Agent, String> {
    let mut agents = store.load_agent_registry()?;
    validate_draft(store, project_key, &agents, draft, None)?;
    let now = now_rfc3339();
    let agent = Agent {
        id: new_note_id(),
        nickname: draft.nickname.clone(),
        // AGR-FR-23: normalised before it is written, so what the registry
        // holds is what every reader of it — a comment's snapshot, a compiled
        // prompt, the editor reopened — sees.
        title: normalise_title(&draft.title),
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

/// AGR-FR-10: replace every field of the draft under exactly the validation
/// `create_agent` applies, refreshing `updated_at` and leaving `created_at` and
/// the id untouched.
pub fn update_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    id: &str,
    draft: &AgentDraft,
) -> Result<Agent, String> {
    let mut agents = store.load_agent_registry()?;
    let index = agents
        .iter()
        .position(|a| a.id == id)
        .ok_or(ERR_AGENT_NOT_FOUND)?;
    validate_draft(store, project_key, &agents, draft, Some(id))?;
    let updated = Agent {
        id: agents[index].id.clone(),
        nickname: draft.nickname.clone(),
        title: normalise_title(&draft.title),
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

/// AGR-FR-11: remove the definition and, in the same operation, every
/// enrolment naming it — the prune is `save_agent_registry`'s (GSS-FR-31), so
/// no project is ever observable enrolling an agent that no longer exists.
/// Idempotent.
pub fn delete_agent_impl(store: &GlobalSettingsStore, id: &str) -> Result<Vec<Agent>, String> {
    let mut agents = store.load_agent_registry()?;
    agents.retain(|a| a.id != id);
    store.save_agent_registry(agents)?;
    list_agents_impl(store)
}

/// AGR-FR-13: only the enrolled agents, each carrying its availability, ordered
/// by nickname without regard to case. A project enrolling nothing returns an
/// empty list rather than an error.
pub fn list_project_agents_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<Vec<ProjectAgent>, String> {
    let enrolled = store.load_project_agent_enrolment(project_key)?;
    let mut agents: Vec<Agent> = store
        .load_agent_registry()?
        .into_iter()
        .filter(|a| enrolled.iter().any(|id| id == &a.id))
        .collect();
    sort_by_nickname(&mut agents);
    Ok(agents
        .into_iter()
        .map(|agent| ProjectAgent {
            availability: availability_of(store, project_key, &agent),
            agent,
        })
        .collect())
}

/// AGR-FR-14: idempotent. An id naming no agent is refused rather than stored,
/// which is what keeps GSS-FR-31's invariant true from the write side as well
/// as the prune side.
pub fn enrol_project_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    agent_id: &str,
) -> Result<Vec<ProjectAgent>, String> {
    if !store.load_agent_registry()?.iter().any(|a| a.id == agent_id) {
        return Err(ERR_AGENT_NOT_FOUND.into());
    }
    let mut enrolled = store.load_project_agent_enrolment(project_key)?;
    if !enrolled.iter().any(|id| id == agent_id) {
        enrolled.push(agent_id.to_string());
        store.save_project_agent_enrolment(project_key, enrolled)?;
    }
    list_project_agents_impl(store, project_key)
}

/// AGR-FR-15: withdraw the agent from this project, leaving its definition
/// untouched. Idempotent.
pub fn remove_project_agent_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    agent_id: &str,
) -> Result<Vec<ProjectAgent>, String> {
    let mut enrolled = store.load_project_agent_enrolment(project_key)?;
    if enrolled.iter().any(|id| id == agent_id) {
        enrolled.retain(|id| id != agent_id);
        store.save_project_agent_enrolment(project_key, enrolled)?;
    }
    list_project_agents_impl(store, project_key)
}

/// AGR-FR-18: resolve a typed nickname to the agent this project means by it.
///
/// Matched without regard to case against the project's **enrolment alone**, so
/// an agent described on this machine but not enrolled here is not addressable
/// here — which is what makes the same text address nobody in a project that
/// did not enrol it.
///
/// The reserved handle is a nickname no agent can carry (AGR-FR-04), so it
/// matches nothing here and every dispatch reaching this module names one real
/// agent. Expanding `all` into the agents it stands for happens in the surface
/// that read the tag (`../ui/AGT-agents.md` AGT-FR-36) and never here.
///
/// Deliberately **not** a `#[tauri::command]`: it is the read path a dispatch
/// takes (`crate::agent_conversations`), and registering it would let the
/// frontend enumerate agents a project never enrolled.
#[allow(dead_code)]
pub fn resolve_project_agent(
    store: &GlobalSettingsStore,
    project_key: &str,
    nickname: &str,
) -> Result<Agent, String> {
    let enrolled = list_project_agents_impl(store, project_key)?;
    let found = enrolled
        .into_iter()
        .find(|p| same_nickname(&p.agent.nickname, nickname))
        .ok_or(ERR_AGENT_NOT_FOUND)?;
    match found.availability {
        AgentAvailability::Ready => Ok(found.agent),
        _ => Err(ERR_AGENT_UNAVAILABLE.into()),
    }
}

/// AGR-FR-22: a project-scoped command answers only while a project is open,
/// while the registry commands answer regardless.
fn require_project(project: &ProjectState) -> Result<String, String> {
    project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.into())
}

// ---------------------------------------------------------------------------
// Tauri commands (AGR contract surface)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_agents(store: State<'_, GlobalSettingsStore>) -> Result<Vec<Agent>, String> {
    list_agents_impl(&store)
}

#[tauri::command]
pub fn create_agent(
    draft: AgentDraft,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Agent, String> {
    create_agent_impl(&store, &project.slot_key(), &draft)
}

#[tauri::command]
pub fn update_agent(
    id: String,
    draft: AgentDraft,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Agent, String> {
    update_agent_impl(&store, &project.slot_key(), &id, &draft)
}

/// AGR-FR-11: deleting an agent also cancels every turn in flight for it
/// (AGC-FR-20) — an agent that no longer exists should not go on answering.
#[tauri::command]
pub fn delete_agent<R: tauri::Runtime>(
    id: String,
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    turns: State<'_, crate::agent_conversations::TurnRegistry>,
    progress: State<'_, crate::progress::ProgressRegistry>,
) -> Result<Vec<Agent>, String> {
    let agents = delete_agent_impl(&store, &id)?;
    crate::agent_conversations::cancel_turns_for_agent(&app, &turns, &progress, &id, None);
    Ok(agents)
}

#[tauri::command]
pub fn list_project_agents(
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Vec<ProjectAgent>, String> {
    let key = require_project(&project)?;
    list_project_agents_impl(&store, &key)
}

#[tauri::command]
pub fn enrol_project_agent(
    agent_id: String,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Vec<ProjectAgent>, String> {
    let key = require_project(&project)?;
    enrol_project_agent_impl(&store, &key, &agent_id)
}

/// AGR-FR-15: withdrawing an agent from a project also cancels every turn in
/// flight for it *in that project* (AGC-FR-20). Turns it has running elsewhere
/// are untouched: it was withdrawn from this conversation, not from the machine.
#[tauri::command]
pub fn remove_project_agent<R: tauri::Runtime>(
    agent_id: String,
    app: tauri::AppHandle<R>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
    turns: State<'_, crate::agent_conversations::TurnRegistry>,
    progress: State<'_, crate::progress::ProgressRegistry>,
) -> Result<Vec<ProjectAgent>, String> {
    let key = require_project(&project)?;
    let agents = remove_project_agent_impl(&store, &key, &agent_id)?;
    crate::agent_conversations::cancel_turns_for_agent(
        &app,
        &turns,
        &progress,
        &agent_id,
        Some(&key),
    );
    Ok(agents)
}

#[cfg(test)]
mod tests;
