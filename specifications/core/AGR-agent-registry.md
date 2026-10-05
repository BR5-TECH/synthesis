# Agent registry

**Spec code:** `AGR`

## Intent
The backend home for the author's **conversational agents**: the named personas Synthesis may hold a conversation with on their behalf, each carrying a nickname it is addressed by, optionally a title naming the role it is there to take, a model, the reasoning that model can be asked for, and optionally the instructions it works under. An agent names no provider: the model is one that the AI API provider active for the open project serves (`AAP-ai-api-integrations.md` AAP-FR-APRV), so changing the active provider moves every agent to the new provider's endpoint and credential at once. A persona is defined once for the machine and enrolled into the projects that want it, because the reviewer an author has taught to argue about specifications is worth having in every repository they open rather than re-describing per project, while which projects it actually participates in is a decision that differs from one to the next. This module owns the definitions, the per-project enrolment, the uniqueness of a nickname, and the resolution of a typed nickname to the agent a project means by it; the conversations themselves are `AGC-agent-conversations.md`'s. Out of scope: holding a conversation, assembling a request, and calling a model, none of which happens here — this module describes who may be spoken to and never speaks to them; anything an agent could *do* beyond answering, since a persona here carries no tool, no filesystem reach, and no ability to run a command, which is what separates it from the agent backends of `AIC-agentic-integrations.md`; and any credential, since the key behind the active provider is `AAP-ai-api-integrations.md`'s alone.

## Contract surface
The module owns the agent registry inside the user-global store (`GSS-global-settings-storage.md` GSS-FR-30), the per-project enrolment in that store's per-project slot (GSS-FR-31), and the Tauri commands below. Names match `../ui/AGT-agents.md` byte-for-byte.

### The agent record

```
Agent {
  id,                   // opaque, generated at creation, stable for the agent's lifetime
  nickname,             // the handle the agent is addressed by; unique across the registry
  title:                string,                   // the role it takes, e.g. "UI/UX designer";
                                                  //   trimmed, and "" when the author named none
  model_id:             string,                   // a model of the active AI API provider; no provider id is stored
  reasoning:            ReasoningChoice | null,   // AAP's shape; null means the model's own default
  instructions:         string,                   // free text; empty when the author supplied none
  created_at,           // RFC 3339 UTC
  updated_at            // RFC 3339 UTC
}
```

`AgentDraft` is that same record without `id`, `created_at`, and `updated_at`, and is what `create_agent` and `update_agent` carry.

### Availability and the project record
An agent can only be spoken to while an AI API provider resolves for the open project and that provider still serves the model the agent chose. That is a property of the AI API registry rather than of the agent, so it is computed at read time rather than stored:

```
AgentAvailability =
  | "ready"
  | "provider_unconfigured"      // no AI API provider is configured at all
  | "provider_unverified"        // providers are configured, but none resolves for the project
  | "model_unavailable"          // the active provider does not offer this model

ProjectAgent {
  agent:                Agent,
  availability:         AgentAvailability
}
```

### Tauri commands

Registry (per `../ui/AGT-agents.md` Global settings section):
- `"list agents"` → `list_agents()` → `Agent[]`, every agent in the user-global registry ordered by nickname, case-insensitively. Reads the registry and makes no network request.
- `"create agent"` → `create_agent(draft)` → `Agent`, or a typed error (`nickname_empty`, `nickname_invalid`, `nickname_reserved`, `nickname_taken`, `provider_unconfigured`, `provider_not_verified`, `unknown_model`, `reasoning_unsupported`, `unknown_effort`, `reasoning_mandatory`).
- `"update agent"` → `update_agent(id, draft)` → `Agent`, or a typed `agent_not_found` alongside every error `create_agent` returns.
- `"delete agent"` → `delete_agent(id)` → `Agent[]`, the registry as it stands afterwards.

Project enrolment (per `../ui/AGT-agents.md` Project settings section and `../ui/SET-project-settings.md` SET-FR-15):
- `"list project agents"` → `list_project_agents()` → `ProjectAgent[]`, the open project's enrolled agents ordered by nickname, each carrying its availability.
- `"enrol project agent"` → `enrol_project_agent(agent_id)` → `ProjectAgent[]`, or a typed `agent_not_found`.
- `"remove project agent"` → `remove_project_agent(agent_id)` → `ProjectAgent[]`.

Every project command errors with a typed `no_project_open` when no project is open.

### Internal (no UI consumer; not a Tauri command and unreachable from the frontend)
- `resolve_project_agent(nickname)` — returns the `Agent` the open project's enrolment resolves that nickname to, or a typed refusal (`agent_not_found`, `agent_unavailable`). Called by `AGC-agent-conversations.md` when a turn is dispatched, which then resolves the provider through `resolve_agent_provider` (`AAP-ai-api-integrations.md` AAP-FR-APRV).

## Functional requirements
1. **AGR-FR-01** An agent is a conversational persona and nothing more: a nickname, an optional title, a model, a reasoning choice, and optional instructions. No field of it names a tool, a filesystem path, a command, or a permission, so nothing defined here can act on a project — the whole of what an agent does is answer, which is what distinguishes it from the agent backends of `AIC-agentic-integrations.md`.
2. **AGR-FR-02** The registry is user-global. A definition lives in `app_data_dir()/synthesis.toml` (`GSS-global-settings-storage.md` GSS-FR-30) beside the AI API providers it is served by, is never written into a project's `.synthesis/`, and is available before any project has been opened, so `list_agents` answers on a machine with no project open.
3. **AGR-FR-03** An agent's `id` is opaque, generated at creation, and stable for its lifetime. It is derived from neither the nickname nor any other field, so renaming an agent leaves every enrolment naming it intact.
4. **AGR-FR-04** A nickname is non-empty, carries no whitespace and no `@`, is not the reserved handle `all` in any case, and is stored exactly as the author typed it. Every part of the constraint exists because a nickname is typed inline into a message to address the agent (per `../ui/AGT-agents.md` AGT-FR-24): one carrying a space could not be told from the words after it, one carrying an `@` could not be told from the sigil that introduces it, and `all` is the handle that addresses every agent a project has enrolled (per `../ui/AGT-agents.md` AGT-FR-35), which one agent holding that nickname would make ambiguous everywhere it is written. A nickname carrying whitespace or an `@` is `nickname_invalid`, an empty one is `nickname_empty`, and `all` in any case is `nickname_reserved`.
5. **AGR-FR-05** A nickname is unique across the whole registry, compared without regard to case, so a tag naming a nickname resolves to exactly one agent in every project that could hold it. A create or update that would collide returns `nickname_taken` and writes nothing.
6. **AGR-FR-06** An agent names a model that the AI API provider active for the open project currently offers, and names no provider. On every create and update the registry resolves that provider with `resolve_agent_provider` (`AAP-ai-api-integrations.md` AAP-FR-APRV) and validates the model and the reasoning through `validate_ai_api_selection` (AAP-FR-33), which is the same rule that governs the provider's own selection, so an agent can never be saved pointing at a model the active endpoint does not serve. With no provider resolved, a create or update returns `provider_unconfigured` where nothing is configured and `provider_not_verified` otherwise, and writes nothing.
7. **AGR-FR-07** An agent's reasoning choice is validated against its chosen model by those same rules: `reasoning_unsupported` for a model declaring no reasoning, `unknown_effort` for a level absent from that model's ladder, and `reasoning_mandatory` for switching reasoning off on a model that always reasons. A null choice means the model's own default and is always accepted.
8. **AGR-FR-08** Instructions are free text stored verbatim, and an empty value is an ordinary state rather than an unconfigured one. This module never parses, truncates, or interprets them; composing them into the prompt a turn works under is `../ai/CVL-conversation-loop.md`'s (CVL-FR-04).
9. **AGR-FR-09** `create_agent(draft)` validates the whole draft before writing anything and returns the created record with its generated id and timestamps. A draft failing any validation writes nothing, so a rejected creation leaves the registry byte-for-byte as it was.
10. **AGR-FR-10** `update_agent(id, draft)` replaces every field of the draft, the nickname included, under exactly the validation `create_agent` applies, and refreshes `updated_at` while leaving `created_at` untouched. An id naming no agent returns `agent_not_found`.
11. **AGR-FR-11** `delete_agent(id)` removes the definition and removes it from every project's enrolment in the same operation, so no project is left enrolling an agent that no longer exists. It is idempotent, and it cancels every turn in flight for that agent (per `AGC-agent-conversations.md` AGC-FR-20).
12. **AGR-FR-12** Enrolment is the per-project set of agent ids that may be addressed in that project. It is written into the user-global per-project slot (`GSS-global-settings-storage.md` GSS-FR-31), keyed by project rather than by directory (per `GSS-global-settings-storage.md` GSS-FR-18), so it is one fact for the repository, survives a change of active worktree, and is never written into a project's `.synthesis/`, which is committed content one clone would carry to everyone.
13. **AGR-FR-13** `list_project_agents()` returns only the enrolled agents, never the whole registry, because enrolment is what decides who is addressable in this project. The list is ordered by nickname without regard to case, and a project enrolling nothing returns an empty list rather than an error.
14. **AGR-FR-14** `enrol_project_agent(agent_id)` adds the agent to the open project's enrolment and is idempotent — enrolling one already enrolled succeeds and changes nothing. An id naming no agent returns `agent_not_found`.
15. **AGR-FR-15** `remove_project_agent(agent_id)` removes the agent from the open project's enrolment, leaves its definition untouched, and is idempotent. It cancels every turn in flight for that agent in that project (per `AGC-agent-conversations.md` AGC-FR-20), because an agent withdrawn from a conversation should not go on answering it.
16. **AGR-FR-16** `availability` is computed at read time from the AI API registry and the provider the open project resolves to: `provider_unconfigured` when no provider is configured at all, `provider_unverified` when providers are configured but none resolves, `model_unavailable` when the resolved provider does not offer the agent's model, and `ready` otherwise. The same agent therefore changes availability when the active provider changes, and returns to `ready` when a provider that serves its model is active again. The computation reads the registry alone — it makes no network request and reads no key — so a roster renders offline.
17. **AGR-FR-17** An agent whose availability is anything but `ready` stays defined and stays enrolled rather than being pruned, on the same principle a degraded integration is retained (per `AAP-ai-api-integrations.md` AAP-FR-09): an author is better served by a row naming what is wrong than by a persona that silently vanished when a key expired. A change of the active provider never rewrites a stored model or reasoning; an agent whose model the new provider does not offer is retained as `model_unavailable` until the author selects an available model.
18. **AGR-FR-18** `resolve_project_agent(nickname)` matches the nickname without regard to case against the open project's **enrolment alone**, so an agent defined on the machine but not enrolled here is not addressable here. A nickname matching nothing refuses `agent_not_found`; one matching an agent whose availability is not `ready` refuses `agent_unavailable`. The reserved handle is a nickname no agent can carry (AGR-FR-04), so it matches nothing here and every dispatch that reaches this module names one real agent — expanding `all` into the agents it stands for happens in the surface that read the tag (per `../ui/AGT-agents.md` AGT-FR-36) and never here. It is not registered as a Tauri command, so no frontend call can reach it.
19. **AGR-FR-19** An agent enrolled in several projects is one definition rather than a copy per project: editing it is felt at once by every project that enrolled it, and there is no per-project override of its model, reasoning, or instructions. A persona that must differ between projects is a second agent with its own nickname.
20. **AGR-FR-20** No secret material is held, returned, or accepted anywhere in this module, and no provider id is stored in an agent. The key behind the active provider is held in the application secret vault and read only by `AAP-ai-api-integrations.md` (AAP-FR-07), so `synthesis.toml` gains no credential by gaining an agent. A stored record written with a `provider` field loads without error; the field is ignored and is dropped on the next write of that record.
21. **AGR-FR-21** Every command in the contract surface exists with a typed payload in the walking-skeleton build. A stub may keep the registry and the enrolment in memory and report every agent `ready`, provided it returns the documented shapes.
22. **AGR-FR-22** Every project-scoped command errors with a typed `no_project_open` when no project is open, while the registry commands answer regardless, because a definition belongs to the machine and only an enrolment belongs to a project.
23. **AGR-FR-23** An agent's **title** is the role it is there to take — `UI/UX designer`, `Developer` — free text carrying no constraint beyond being text, and it is normalized by **trimming**: leading and trailing whitespace is removed under the platform's standard string-trimming semantics before validation and before anything is written, while internal whitespace, casing, and every other character the author typed — braces included — survive byte-for-byte. A title that is unset, empty, or nothing but whitespace normalizes to `""`, which is an ordinary state rather than an unconfigured one: no validation refuses it, no error `create_agent` or `update_agent` returns names the title, and an agent carrying none is created, updated, enrolled, addressed, and answered exactly as one carrying one. This module neither parses, bounds, nor interprets it — what a title means to a conversation is decided elsewhere, a comment snapshotting the value that stood when it was written (per `CMS-comments-storage.md` CMS-FR-65) and a prompt substituting it (per `../ai/CVL-conversation-loop.md` CVL-FR-04).
24. **AGR-FR-24** A stored record carrying no title at all reads back with a title of `""` (per `GSS-global-settings-storage.md` GSS-FR-30), which is the same value an author who named none stores. Nothing distinguishes the two, and no migration of the store is performed to tell them apart, so a registry written by a build that never held titles is read by this one without a step of its own.

## Non-functional requirements
- Every command completes without network access: the registry, the enrolment, and the availability computation are all reads of stored state, so an agent roster renders instantly and offline.
- A registry holding no agent and a project enrolling none are ordinary states, not errors. Every command answers on a fresh machine before anything has been defined.
- Validation is performed before any write, so a rejected create or update leaves the store byte-for-byte as it was and a failed edit never degrades a working agent.
- The registry is plain descriptive data in the user-global store, so `synthesis.toml` can still be read, copied, or attached to a bug report without disclosing a credential.
- An agent's instructions may be long; nothing here bounds them, because what a request can carry is a property of the model call rather than of the definition (per `AGC-agent-conversations.md` AGC-FR-11).
