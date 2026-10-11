# Agent registry

**Spec code:** `AGR`

## Intent
The backend home for the author's **conversational agents**: the named personas Synthesis may hold a conversation with on their behalf, each carrying a nickname it is addressed by, optionally a title naming the role it is there to take, a model, the reasoning that model can be asked for, and optionally the instructions it works under. An agent names no provider: the model is one that the AI API provider active for the open project serves (`AAP-ai-api-integrations.md` AAP-FR-APRV), so changing the active provider moves every agent to the new provider's endpoint and credential at once. A persona has one of two **scopes**. A **Project agent** is stored in the project, is committed with it, and is shared by every contributor, so a team argues with the same reviewer under the same instructions. A **Personal agent** is stored on the author's machine and follows the author into every project, because the reviewer an author has taught to argue about specifications is worth having in every repository they open. This module owns both kinds of definition, the uniqueness of a nickname inside a scope, the rule that a Project agent shadows a Personal agent of the same nickname, and the resolution of a typed nickname to the agent a project means by it; the conversations themselves are `AGC-agent-conversations.md`'s. Out of scope: holding a conversation, assembling a request, and calling a model, none of which happens here — this module describes who may be spoken to and never speaks to them; anything an agent could *do* beyond answering, since a persona here carries no tool, no filesystem reach, and no ability to run a command, which is what separates it from the agent backends of `AIC-agentic-integrations.md`; and any credential, since the key behind the active provider is `AAP-ai-api-integrations.md`'s alone.

## Contract surface
The module owns the Personal agent registry inside the user-global store (`GSS-global-settings-storage.md` GSS-FR-30), the Project agent files inside the project's `.synthesis/` (`PSS-project-settings-storage.md` PSS-FR-AGFL), and the Tauri commands below. Names match `../ui/AGT-agents.md` byte-for-byte.

### The agent record

```
Agent {
  id,                   // opaque, generated at creation, stable for the agent's lifetime
  nickname,             // the handle the agent is addressed by; unique inside its scope
  title:                string,                   // the role it takes, e.g. "UI/UX designer";
                                                  //   trimmed, and "" when the author named none
  model_id:             string,                   // a model of the active AI API provider; no provider id is stored
  reasoning:            ReasoningChoice | null,   // AAP's shape; null means the model's own default
  instructions:         string,                   // free text; empty when the author supplied none
  created_at,           // RFC 3339 UTC; "" for a Project agent
  updated_at            // RFC 3339 UTC; "" for a Project agent
}
```

`AgentDraft` is that same record without `id`, `created_at`, and `updated_at`, and is what `create_agent` and `update_agent` carry.

### Scope, availability, and the roster record
An agent has one scope. A Project agent is read from the open project; a Personal agent is read from the user-global store.

```
AgentScope = "project" | "personal"
```

An agent can only be spoken to while an AI API provider resolves for the open project, that provider still serves the model the agent chose, and no Project agent of the same nickname shadows it. The first two are properties of the AI API registry and the third is a property of the open project, so all three are computed at read time and never stored:

```
AgentAvailability =
  | "ready"
  | "provider_unconfigured"      // no AI API provider is configured at all
  | "provider_unverified"        // providers are configured, but none resolves for the project
  | "model_unavailable"          // the active provider does not offer this model
  | "shadowed"                   // a Personal agent hidden by a Project agent of the same nickname

ProjectAgent {
  agent:                Agent,
  scope:                AgentScope,
  availability:         AgentAvailability
}
```

### Tauri commands

Reads:
- `"list project agents"` → `list_project_agents()` → `ProjectAgent[]`, the **roster** of the open project: every Project agent and every Personal agent, each with its scope and its availability, ordered by nickname case-insensitively, a Project agent before a Personal agent of the same nickname. Errors with a typed `no_project_open` when no project is open.
- `"list personal agents"` → `list_personal_agents()` → `ProjectAgent[]`, every Personal agent ordered by nickname case-insensitively. Answers with no project open. While a project is open, a Personal agent that a Project agent shadows reports `shadowed`.

Writes (the scope names the store the write goes to):
- `"create agent"` → `create_agent(scope, draft)` → `Agent`, or a typed error (`nickname_empty`, `nickname_invalid`, `nickname_reserved`, `nickname_taken`, `provider_unconfigured`, `provider_not_verified`, `unknown_model`, `reasoning_unsupported`, `unknown_effort`, `reasoning_mandatory`, and `no_project_open` for the Project scope).
- `"update agent"` → `update_agent(scope, id, draft)` → `Agent`, or a typed `agent_not_found` alongside every error `create_agent` returns.
- `"delete agent"` → `delete_agent(scope, id)` → `ProjectAgent[]`, the agents of that scope as they stand afterwards, in the order of the matching read.

### Events
- `"agents changed"` — carries no payload, emitted after a create, an update, or a delete of an agent of either scope succeeds (AGR-FR-ZRJD). It names no agent and carries no agent content.

### Internal (no UI consumer; not a Tauri command and unreachable from the frontend)
- `resolve_project_agent(nickname)` — returns the `Agent` the open project's roster resolves that nickname to, or a typed refusal (`agent_not_found`, `agent_unavailable`). Called by `AGC-agent-conversations.md` when a turn is dispatched, which then resolves the provider through `resolve_agent_provider` (`AAP-ai-api-integrations.md` AAP-FR-APRV).

## Functional requirements
1. **AGR-FR-01** An agent is a conversational persona and nothing more: a nickname, an optional title, a model, a reasoning choice, and optional instructions. No field of it names a tool, a filesystem path, a command, or a permission, so nothing defined here can act on a project — the whole of what an agent does is answer, which is what distinguishes it from the agent backends of `AIC-agentic-integrations.md`.
2. **AGR-FR-02** The Personal agents are user-global. A definition lives in `app_data_dir()/synthesis.toml` (`GSS-global-settings-storage.md` GSS-FR-30) beside the AI API providers it is served by, is never written into a project's `.synthesis/`, and is available before any project has been opened, so `list_personal_agents` answers on a machine with no project open.
3. **AGR-FR-03** An agent's `id` is opaque, generated at creation, and stable for its lifetime. It is derived from neither the nickname nor any other field, so renaming an agent leaves its id unchanged.
4. **AGR-FR-04** A nickname is non-empty, carries no whitespace and no `@`, is not the reserved handle `all` in any case, and is stored exactly as the author typed it. The constraint applies to both scopes. Every part of it exists because a nickname is typed inline into a message to address the agent (per `../ui/AGT-agents.md` AGT-FR-24): one carrying a space could not be told from the words after it, one carrying an `@` could not be told from the sigil that introduces it, and `all` is the handle that addresses every agent on the project's roster (per `../ui/AGT-agents.md` AGT-FR-35). A nickname carrying whitespace or an `@` is `nickname_invalid`, an empty one is `nickname_empty`, and `all` in any case is `nickname_reserved`.
5. **AGR-FR-05** A nickname is unique inside one scope, compared without regard to case. A create or update that would collide with another agent of the same scope returns `nickname_taken` and writes nothing. An update that leaves the nickname unchanged is never refused for it, so two files merged with the same nickname stay editable. A Project agent and a Personal agent may carry the same nickname (AGR-FR-15).
6. **AGR-FR-06** An agent names a model that the AI API provider active for the open project currently offers, and names no provider. On every create and update the registry resolves that provider with `resolve_agent_provider` (`AAP-ai-api-integrations.md` AAP-FR-APRV) and validates the model and the reasoning through `validate_ai_api_selection` (AAP-FR-33), which is the same rule that governs the provider's own selection, so an agent can never be saved pointing at a model the active endpoint does not serve. With no provider resolved, a create or update returns `provider_unconfigured` where nothing is configured and `provider_not_verified` otherwise, and writes nothing.
7. **AGR-FR-07** An agent's reasoning choice is validated against its chosen model by those same rules: `reasoning_unsupported` for a model declaring no reasoning, `unknown_effort` for a level absent from that model's ladder, and `reasoning_mandatory` for switching reasoning off on a model that always reasons. A null choice means the model's own default and is always accepted.
8. **AGR-FR-08** Instructions are free text stored verbatim, and an empty value is an ordinary state rather than an unconfigured one. This module never parses, truncates, or interprets them; composing them into the prompt a turn works under is `../ai/CVL-conversation-loop.md`'s (CVL-FR-04).
9. **AGR-FR-09** `create_agent(scope, draft)` validates the whole draft before writing anything and returns the created record with its generated id. A Personal agent also carries its timestamps. A draft failing any validation writes nothing, so a rejected creation leaves the store byte-for-byte as it was.
10. **AGR-FR-10** `update_agent(scope, id, draft)` replaces every field of the draft, the nickname included, under exactly the validation `create_agent` applies. For a Personal agent it refreshes `updated_at` and leaves `created_at` untouched. An id naming no agent of that scope returns `agent_not_found`.
11. **AGR-FR-11** `delete_agent(scope, id)` removes the definition of one agent of that scope. Deleting a Project agent removes its file and cancels every turn in flight for that agent in the open project. Deleting a Personal agent removes it from every project and cancels every turn in flight for that agent in every project (per `AGC-agent-conversations.md` AGC-FR-20). It is idempotent.
12. **AGR-FR-12** A Project agent is stored in the project, in a file of its own under the active worktree's `.synthesis/` (per `PSS-project-settings-storage.md` PSS-FR-AGFL). It is committed with the project and shared by every contributor who syncs. It needs no enrolment: it is addressable in the project at once. It holds persona fields only: nickname, title, model id, reasoning, and instructions.
13. **AGR-FR-13** `list_project_agents()` returns the roster of the open project: every Project agent and every Personal agent, each with its scope and availability. The list is ordered by nickname without regard to case, a Project agent before a Personal agent of the same nickname, and a project with no agent at all returns an empty list rather than an error.
14. **AGR-FR-14** `list_personal_agents()` returns only the Personal agents, ordered by nickname without regard to case, and answers whether or not a project is open. While a project is open, each Personal agent that is shadowed in it reports `shadowed` (AGR-FR-15). With no project open, none is shadowed.
15. **AGR-FR-15** A Personal agent is **shadowed** in a project that holds a Project agent of the same nickname, compared without regard to case. The Project agent wins. The Personal agent reports availability `shadowed`, resolves to nobody there, and cannot be addressed there. It stays defined, nothing is rewritten, and it is addressable in every project with no Project agent of that nickname.
16. **AGR-FR-16** `availability` is computed at read time from the AI API registry and the provider the open project resolves to on this machine: `provider_unconfigured` when no provider is configured, `provider_unverified` when providers are configured but none resolves, `model_unavailable` when the resolved provider lacks the agent's model, and `ready` otherwise. A shadowed Personal agent reports `shadowed`. The computation makes no network request and reads no key.
17. **AGR-FR-17** An agent whose availability is anything but `ready` stays defined rather than being pruned, on the same principle a degraded integration is retained (per `AAP-ai-api-integrations.md` AAP-FR-09): an author is better served by a row naming what is wrong than by a persona that silently vanished when a key expired. A change of the active provider never rewrites a stored model or reasoning; an agent whose model the new provider does not offer is retained as `model_unavailable` until the author selects an available model.
18. **AGR-FR-18** `resolve_project_agent(nickname)` matches the nickname without regard to case against the open project's Project agents first, and a match is the result whatever its availability. Otherwise it matches the Personal agents that are not shadowed. A nickname matching nothing refuses `agent_not_found`. A match whose availability is not `ready` refuses `agent_unavailable`. It is not a Tauri command, so no frontend call reaches it.
19. **AGR-FR-19** A Personal agent is one definition for the machine. Editing it is felt at once by every project, and no project overrides its model, reasoning, or instructions. A Project agent is one definition for the project, and its edit reaches another contributor when that contributor syncs. A persona that must differ in one project is a Project agent of the same nickname, which shadows the Personal agent there.
20. **AGR-FR-20** No secret is held, returned, or accepted anywhere in this module, and no agent stores a provider id. A Project agent file holds no provider, no URL, no key, and no secret. The key behind the active provider lives in the application secret vault (`AAP-ai-api-integrations.md` AAP-FR-07). A stored record with a `provider` field loads without error, and the field is dropped on the next write.
21. **AGR-FR-21** Every command in the contract surface exists with a typed payload in the walking-skeleton build. A stub may keep both scopes in memory and report every agent `ready`, provided it returns the documented shapes.
22. **AGR-FR-22** Every command that reads or writes the Project scope errors with a typed `no_project_open` when no project is open, while the Personal commands answer regardless, because a Personal definition belongs to the machine and a Project definition belongs to a project.
23. **AGR-FR-23** An agent's **title** is the role it is there to take — `UI/UX designer`, `Developer` — free text carrying no constraint beyond being text, and it is normalized by **trimming**: leading and trailing whitespace is removed under the platform's standard string-trimming semantics before validation and before anything is written, while internal whitespace, casing, and every other character the author typed — braces included — survive byte-for-byte. A title that is unset, empty, or nothing but whitespace normalizes to `""`, which is an ordinary state rather than an unconfigured one: no validation refuses it, no error `create_agent` or `update_agent` returns names the title, and an agent carrying none is created, updated, addressed, and answered exactly as one carrying one. This module neither parses, bounds, nor interprets it — what a title means to a conversation is decided elsewhere, a comment snapshotting the value that stood when it was written (per `CMS-comments-storage.md` CMS-FR-65) and a prompt substituting it (per `../ai/CVL-conversation-loop.md` CVL-FR-04).
24. **AGR-FR-24** A stored record carrying no title at all reads back with a title of `""` (per `GSS-global-settings-storage.md` GSS-FR-30), which is the same value an author who named none stores. Nothing distinguishes the two, and no migration of the store is performed to tell them apart, so a registry written by a build that never held titles is read by this one without a step of its own.
25. **AGR-FR-WQBN** A Project agent whose model the provider resolved on this machine does not offer stays defined and reports `model_unavailable`, or the matching provider state. Nothing is rewritten in its committed file. A contributor whose provider offers the model sees the same agent as `ready`.
26. **AGR-FR-HTKX** Conversations that name an agent by nickname keep working when an agent changes scope or storage. A turn resolves its agent by nickname at dispatch (AGR-FR-18), so an agent that moved from one store to another is reached by the same tag.
27. **AGR-FR-ZRJD** A successful create, update, or delete of an agent of either scope emits `"agents changed"` to every window. A failed write emits nothing. Every window that shows agents reads them again, so a change made in the Settings window reaches the roster of the main window.

## Non-functional requirements
- Every command completes without network access: both stores and the availability computation are reads of stored state, so an agent roster renders instantly and offline.
- A scope holding no agent is an ordinary state, not an error. Every read answers on a fresh machine before anything has been defined.
- Validation is performed before any write, so a rejected create or update leaves the store byte-for-byte as it was and a failed edit never degrades a working agent.
- A Project agent file and `synthesis.toml` are plain descriptive data, so either can be read, copied, committed, or attached to a bug report without disclosing a credential.
- An agent's instructions may be long; nothing here bounds them, because what a request can carry is a property of the model call rather than of the definition (per `AGC-agent-conversations.md` AGC-FR-11).
