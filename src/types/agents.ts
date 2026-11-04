// Conversational agents (agents.rs / AGR-agent-registry.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { ReasoningChoice } from "./agentic";

// --- Conversational agents (agents.rs / AGR-agent-registry.md) -------------

/**
 * AGR-FR-01: a described persona and nothing more — a nickname it is addressed
 * by, a model the active AI API provider serves, the reasoning that model can be
 * asked for, and optionally the instructions it works under. An agent stores no
 * provider (AGR-FR-06).
 *
 * No field names a tool, a path, a command, or a permission, which is the whole
 * of what separates an agent from an agentic *backend*: one answers, the other
 * acts. And no field is key-derived, not even a masked hint (AGR-FR-20) — an
 * agent names a provider, and the credential behind that provider is
 * `AiApiIntegration`'s alone.
 *
 * Field names must match `Agent` in `src-tauri/src/agents.rs` byte-for-byte:
 * the Rust struct carries `#[serde(default)]` and does not deny unknown fields,
 * so a misspelled key here is not an error — it is silently `undefined`.
 */
export interface Agent {
  /** Opaque and stable for the agent's lifetime; renaming never changes it. */
  id: string;
  nickname: string;
  /**
   * AGR-FR-23: the role this persona takes — `UI/UX designer`, `Developer`.
   * Stored trimmed, with internal whitespace and casing preserved, and `""`
   * where the author named none. A record written before the field existed
   * reads as `""` too (AGR-FR-24), so this is never `undefined` in practice.
   */
  title: string;
  modelId: string;
  /** AGR-FR-ABCF: a plain voice name. `""` means the provider's default. */
  voice?: string;
  /** Free text. Empty is an ordinary state, not an unconfigured one. */
  instructions: string;
  /**
   * AGR-FR-IGIL: the paths of the project skills a Project agent may use. The
   * backend omits the key while the list is empty, and a Personal agent never
   * holds one (AGR-FR-QKOV), so an absent value means no project skill.
   */
  skills?: string[];
  /**
   * AGR-FR-HMVQ: the paths of the synthesis skills this agent turns off. Both
   * scopes hold it. The backend omits the key while the list is empty, so an
   * absent value turns nothing off (AGR-FR-WDZT).
   */
  disabledSkills?: string[];
  createdAt: string;
  updatedAt: string;
  /** `null` means the model's own default (AGR-FR-07). */
  reasoning: ReasoningChoice | null;
}

/** What `createAgent` and `updateAgent` carry: an `Agent` without the three
 * fields the backend owns. */
export interface AgentDraft {
  nickname: string;
  /** Sent as the author typed it; the backend trims before it stores
   * (AGR-FR-23), so the editor reopens on the trimmed value. */
  title: string;
  modelId: string;
  instructions: string;
  /** AGR-FR-ABCF. */
  voice?: string;
  /** AGR-FR-IGIL: the selected project skill paths. Always empty for a
   * Personal agent (AGT-FR-WTSR). */
  skills?: string[];
  /** AGR-FR-HMVQ: the synthesis skill paths the agent turns off, for either
   * scope (AGT-FR-JRWE). */
  disabledSkills?: string[];
  reasoning: ReasoningChoice | null;
}

/** DSL-FR-PEHR: one synthesis skill or project skill as `listProjectSkills`
 * returns it. Never a built-in skill, which every agent has without a
 * selection. */
export interface ProjectSkill {
  /** The project-relative path of its `SKILL.md`: the skill's identity. */
  path: string;
  ecosystem: "claude" | "codex" | "github" | "opencode" | "synthesis";
  name: string;
  description: string;
}

/**
 * AGR-FR-16: whether an agent can currently be spoken to. Computed at read time
 * from the AI API registry rather than stored, so a key that expired while the
 * application was closed reads as such on the next launch. `shadowed` is a
 * Personal agent hidden by a Project agent of the same nickname (AGR-FR-15).
 */
export type AgentAvailability =
  | "ready"
  | "provider_unconfigured"
  | "provider_unverified"
  | "model_unavailable"
  | "shadowed";

/**
 * AGR-FR-12, AGR-FR-02: where an agent is stored. A Project agent is a file of
 * the project, committed and shared by every contributor. A Personal agent is
 * stored on this machine and is available in every project.
 */
export type AgentScope = "project" | "personal";

/** An agent as a project sees it, with its scope and availability (AGR-FR-13). */
export interface ProjectAgent {
  agent: Agent;
  scope: AgentScope;
  availability: AgentAvailability;
}
