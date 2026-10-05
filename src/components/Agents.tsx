/**
 * Agents — `specifications/ui/AGT-agents.md`.
 *
 * Three surfaces and one shared vocabulary:
 *
 * - the top-chrome control and its **roster** (AGT-FR-02 … AGT-FR-08), which
 *   reports who this project can talk to and who is answering right now;
 * - the Global settings **Agents** section and its persona editor
 *   (AGT-FR-09 … AGT-FR-21), the only place a persona is authored;
 * - the Project settings **Agents** section (AGT-FR-22, AGT-FR-23), which
 *   decides only who is in this project.
 *
 * The `@nickname` syntax and the mention picker live here too, because the tag
 * and the roster are the same fact seen from two places.
 *
 * One rule cuts across all of it (AGT-FR-01, AGT-FR-32): an agent is presented
 * as a nickname, a model, and a reasoning level, and nothing here dispatches a
 * turn, cancels one, or renders a reply. A conversation is begun and read where
 * the agent was addressed (`CMT-comments.md`).
 *
 * This is also the **only** surface where the model and the reasoning an agent
 * runs on are chosen (AGT-FR-01). An agent takes the endpoint and the key of the
 * provider it names from the AI API section and nothing else: that section's own
 * model and reasoning selections serve the graduation loop alone and are never
 * what an agent answers on (`AII-ai-integrations.md` AII-FR-54).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../api";
import { onAgentTurnStateChanged } from "../events";
import {
  publishProjectAgents,
  useAgentRegistryRevision,
} from "../state/agentRegistry";
import {
  aiErrorMessage,
  } from "./AiIntegrations";
import { useActiveAiApiCatalog } from "../hooks/useActiveAiApiCatalog";
import { ALL_HANDLE, } from "./agentTags";
import { Icon } from "./icons";
import { AGENT_ERRORS } from "../types";
import type {
  Agent,
  AgentAvailability,
  ActiveAiApiCatalog,
  AgentTurn,
  AiApiCatalog,
  ProjectAgent,
} from "../types";

// ---------------------------------------------------------------------------
// Shared vocabulary
// ---------------------------------------------------------------------------

/**
 * AGT-FR-10 / AGT-FR-22 / AGT-FR-26: why an agent cannot currently be spoken
 * to, said plainly and naming where it is corrected.
 *
 * An empty string for `ready`, so a caller can render the note unconditionally
 * and get nothing when there is nothing wrong.
 */
export function availabilityNote(availability: AgentAvailability): string {
  switch (availability) {
    case "provider_unconfigured":
      return "No AI API provider is configured. Configure one in Global settings → AI API.";
    case "provider_unverified":
      return "No AI API provider is active for this project. Verify and activate one in Global settings → AI API.";
    case "model_unavailable":
      return "The active AI API provider does not offer this model. Choose another model in the editor.";
    default:
      return "";
  }
}

/** The short form the roster and the picker render on one line. */
export function availabilityShort(availability: AgentAvailability): string {
  switch (availability) {
    case "provider_unconfigured":
      return "provider not configured";
    case "provider_unverified":
      return "provider not verified";
    case "model_unavailable":
      return "model unavailable";
    default:
      return "";
  }
}

/**
 * AGT-FR-PRVQ: the availability of a global agent, computed from the catalog
 * of the AI API provider that is active for the open project.
 *
 * A null `active` means the read has not landed yet, and an agent is then
 * presented as ready rather than flashing a warning that may not be true.
 */
export function availabilityFromCatalog(
  agent: Agent,
  active: ActiveAiApiCatalog | null,
): AgentAvailability {
  if (active === null) return "ready";
  if (active.catalog === null) {
    return active.resolution === "none_configured"
      ? "provider_unconfigured"
      : "provider_unverified";
  }
  return active.catalog.models.some((m) => m.id === agent.modelId)
    ? "ready"
    : "model_unavailable";
}

/**
 * AGT-FR-01 / AGT-FR-04: how an agent is described everywhere — the model it
 * runs on, and the reasoning that model is asked for where it has one. The line
 * names no provider.
 *
 * `catalog` supplies the model's *label*, so a row reads `Claude Opus 5`
 * rather than `anthropic/claude-opus-5` when the record knows a friendlier name.
 * Falling back to the identifier matters: an agent whose model the active
 * provider does not offer still has to render a row (AGR-FR-17).
 */
export function agentModelLine(
  agent: Agent,
  catalog: AiApiCatalog | null,
): string {
  const model = catalog?.models.find((m) => m.id === agent.modelId);
  const name = model?.label ?? agent.modelId;
  if (!agent.reasoning) return name;
  const reasoning =
    agent.reasoning.kind === "effort" ? agent.reasoning.effort : agent.reasoning.kind;
  return `${name} · ${reasoning}`;
}

// ---------------------------------------------------------------------------
// In-flight turns, shared by the roster and the rail
// ---------------------------------------------------------------------------

/**
 * AGC-FR-22 / AGT-FR-05: the turns currently in flight, read once when the
 * consumer mounts and followed by the event thereafter.
 *
 * `enabled` is what makes the roster read only while it is open (AGT
 * non-functional requirements: the roster does not re-read the registry more
 * than once per opening). The subscription is unconditional, because an event
 * that arrives while a surface is closed costs nothing and a surface that
 * unsubscribed would have to re-read on every open to catch up.
 */
export function useAgentTurns(enabled: boolean): AgentTurn[] {
  const [turns, setTurns] = useState<AgentTurn[]>([]);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    api
      .listAgentTurns(null)
      .then((list) => {
        // AGT-FR-05: answering means `running`. `list_agent_turns` also returns
        // turns awaiting a reply (AGC-FR-22), and an agent waiting on a person
        // is not working — nor would the marker ever clear, retirement emitting
        // no event. The event path below filters the same way.
        if (!cancelled)
          setTurns((list ?? []).filter((t) => t.state === "running"));
      })
      .catch(() => {
        // A roster that cannot read the in-flight set still renders its agents;
        // the marker is the only thing missing, and it corrects itself on the
        // next event.
        if (!cancelled) setTurns([]);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onAgentTurnStateChanged((turn) => {
      setTurns((prev) => {
        const without = prev.filter((t) => t.id !== turn.id);
        // AGC-FR-21: a turn leaves the in-flight set the moment it reaches a
        // terminal state, which is exactly when `endedAt` is stamped.
        return turn.state === "running" ? [turn, ...without] : without;
      });
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return turns;
}

// ---------------------------------------------------------------------------
// The chrome control and its roster (AGT-FR-02 … AGT-FR-08)
// ---------------------------------------------------------------------------

export interface AgentsChromeControlProps {
  /**
   * SNV-FR-56: opening the roster closes every other floating overlay of the
   * main window, and this is how it says so.
   */
  onOverlayOpening: () => void;
  /**
   * AGT-FR-06 / AGT-FR-07: where a row and the add action send the author — the
   * Global settings window, on its Agents section, with an editor open. `null`
   * opens a fresh one.
   */
  onOpenAgentInSettings: (agentId: string | null) => void;
  /** AGT-FR-08: where the empty state names agents as being enrolled. */
  onOpenProjectSettings: () => void;
  /**
   * SNV-FR-56: the roster's open state, owned by the shell.
   *
   * Held above this control for the same reason the universal search overlay's
   * is: mutual exclusion is the *window's* rule, and a control that closed only
   * on its own outside-pointer-down would stay mounted beside a chrome dropdown
   * opened from the keyboard, which raises no pointer event at all.
   */
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AgentsChromeControl({
  onOverlayOpening,
  onOpenAgentInSettings,
  onOpenProjectSettings,
  open,
  onOpenChange,
}: AgentsChromeControlProps) {
  const setOpen = onOpenChange;
  const [agents, setAgents] = useState<ProjectAgent[] | null>(null);
  // AGT-FR-08's empty state says "enrol one", which is advice — and advice is
  // wrong when the roster could not be read at all. Kept apart from `agents` so
  // a read that failed cannot be mistaken for a project that enrolled nobody.
  const [error, setError] = useState("");
  const rootRef = useRef<HTMLDivElement | null>(null);
  const turns = useAgentTurns(open);
  const active = useActiveAiApiCatalog(open);
  // AGT-FR-02: the count is a fact about the open project, and it changes from
  // surfaces this control cannot see — the Project settings section enrols, the
  // Global settings section deletes. A control that read only on mount would go
  // on reporting a number that is no longer true.
  const revision = useAgentRegistryRevision();
  /**
   * Whether this mount has published anything yet.
   *
   * Distinguishes "the control just mounted, so the shared roster belongs to the
   * project we have left" from "an agent changed in the project we are in", which
   * the effect below has to treat differently.
   */
  const published = useRef(false);

  // AGT-FR-02: the control carries the count whether or not the roster has been
  // opened, so the roster's size is legible without opening it. A project
  // enrolling none carries the control with a count of none rather than no
  // control at all.
  useEffect(() => {
    let cancelled = false;
    // On *mount* the published roster still describes whatever project was open
    // before: `App` keys this whole subtree by project path, so a switch remounts
    // this control while the module-level roster survives it. Cleared before the
    // read is issued so a peer reading it renders a tag as prose for that moment
    // rather than bolding against the previous project's enrolment — an agent
    // this project never enrolled, marked as though it were a participant.
    //
    // Only on mount, not on every revision: a bump means an agent was created or
    // enrolled *in this project*, and clearing there would unbold every tag in
    // sight for one IPC round trip to no purpose.
    if (!published.current) {
      published.current = true;
      publishProjectAgents([]);
    }
    api
      .listProjectAgents()
      .then((list) => {
        if (cancelled) return;
        setAgents(list ?? []);
        setError("");
        // Published for the surfaces the specs give no call of their own — today
        // the Comments panel, which needs the roster to bold a tag (AGT-FR-29)
        // and is allowed exactly one list call of its own (CMP-FR-18).
        publishProjectAgents(list ?? []);
      })
      .catch((e) => {
        if (cancelled) return;
        setAgents([]);
        setError(agentErrorMessage(e));
        publishProjectAgents([]);
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  // AGT-FR-03: dismisses on Escape and on an outside pointer-down, exactly as
  // every other chrome dropdown does.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, setOpen]);

  const answering = useMemo(() => {
    const ids = new Set<string>();
    for (const turn of turns) ids.add(turn.agentId);
    return ids;
  }, [turns]);

  const toggle = () => {
    if (!open) {
      // SNV-FR-56: every other overlay closes first, then this one opens. The
      // order matters — the shell's handler is what closes the rest, and it
      // must not be able to close this one on the way in.
      onOverlayOpening();
      // AGT-FR-02: re-read on opening, so a persona enrolled in another tab is
      // present. Once per opening, not per render.
      api
        .listProjectAgents()
        .then((list) => {
          setAgents(list ?? []);
          setError("");
        })
        // Both halves, as on mount: an error rendered above the rows of the
        // last successful read describes two different moments at once, and the
        // count beside the control would still be the old one.
        .catch((e) => {
          setAgents([]);
          setError(agentErrorMessage(e));
        });
    }
    setOpen(!open);
  };

  const count = agents?.length ?? 0;

  return (
    <div ref={rootRef} style={{ position: "relative", display: "inline-flex" }}>
      <button
        className="btn btn--ghost btn--sm"
        data-testid="chrome-agents-control"
        aria-haspopup="true"
        aria-expanded={open}
        aria-label={`Agents (${count})`}
        title="Agents"
        onClick={toggle}
      >
        <Icon.Agents size={14} /> {count}
      </button>

      {open && (
        <div
          className="card"
          role="dialog"
          aria-label="Agents"
          data-testid="agents-roster"
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            right: 0,
            width: 260,
            zIndex: 40,
            padding: 0,
            display: "flex",
            flexDirection: "column",
          }}
        >
          <div style={{ maxHeight: 280, overflowY: "auto", padding: 6 }}>
            {/* A roster that could not be read says so rather than advising the
                author to enrol an agent they may already have enrolled. */}
            {error && (
              <div
                data-testid="agents-roster-error"
                style={{ padding: "16px 10px", color: "var(--danger, #b00)" }}
                className="t-ui-sm"
              >
                {error}
              </div>
            )}

            {/* AGT-FR-08: a first-class empty state, not an error. */}
            {!error && count === 0 && (
              <div
                data-testid="agents-roster-empty"
                style={{ padding: "16px 10px", color: "var(--fg-3)" }}
                className="t-ui-sm"
              >
                This project has no agents yet. Enrol one in{" "}
                <button
                  className="btn btn--ghost btn--sm"
                  style={{ padding: 0, textDecoration: "underline" }}
                  onClick={() => {
                    setOpen(false);
                    onOpenProjectSettings();
                  }}
                >
                  Project settings → Agents
                </button>
                .
              </div>
            )}

            {(agents ?? []).map((entry) => {
              const busy = answering.has(entry.agent.id);
              const warning = availabilityShort(entry.availability);
              return (
                <button
                  key={entry.agent.id}
                  className="btn btn--ghost btn--sm"
                  data-testid="agents-roster-row"
                  // AGT-FR-06: the roster is how an agent is reached to be
                  // looked at or changed; it performs no edit itself.
                  onClick={() => {
                    setOpen(false);
                    onOpenAgentInSettings(entry.agent.id);
                  }}
                  style={{
                    display: "flex",
                    width: "100%",
                    textAlign: "left",
                    flexDirection: "column",
                    alignItems: "stretch",
                    gap: 2,
                    padding: "6px 8px",
                    height: "auto",
                    minWidth: 0,
                    overflow: "hidden",
                  }}
                >
                  <span
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 8,
                      minWidth: 0,
                    }}
                  >
                    <span className="u-ellipsis" style={{ fontWeight: 600 }}>
                    @{entry.agent.nickname}
                  </span>
                    {busy && (
                      <span
                        className="t-ui-sm"
                        data-testid="agents-roster-answering"
                        style={{ color: "var(--accent)" }}
                      >
                        answering
                      </span>
                    )}
                    {!busy && warning && (
                      <span className="t-ui-sm" style={{ color: "var(--warn, #b8860b)" }}>
                        ⚠ {warning}
                      </span>
                    )}
                  </span>
                  <span className="t-ui-sm t-muted u-ellipsis">
                    {agentModelLine(entry.agent, active?.catalog ?? null)}
                  </span>
                </button>
              );
            })}
          </div>

          {/* AGT-FR-07: pinned below the rows, outside anything that scrolls. */}
          <div style={{ borderTop: "1px solid var(--border-1)", padding: 6 }}>
            <button
              className="btn btn--ghost btn--sm"
              data-testid="agents-roster-add"
              style={{ width: "100%", justifyContent: "flex-start" }}
              onClick={() => {
                setOpen(false);
                onOpenAgentInSettings(null);
              }}
            >
              <Icon.Plus size={12} /> Add an agent
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The persona editor (AGT-FR-11 … AGT-FR-19)
// ---------------------------------------------------------------------------

/**
 * AGT-FR-19: what a typed refusal from the registry says to the author.
 *
 * Matched on rather than printed, for the same reason `aiErrorMessage` matches
 * its own: `nickname_taken` is corrected in one field while
 * `provider_not_verified` is corrected in another section entirely. Anything
 * this list does not know falls through to the AI vocabulary, which is where the
 * selection errors come from (AAP-FR-33).
 */
export function agentErrorMessage(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  switch (raw) {
    case AGENT_ERRORS.nicknameEmpty:
      return "A nickname is required.";
    case AGENT_ERRORS.nicknameInvalid:
      return "A nickname cannot contain spaces or @.";
    case AGENT_ERRORS.nicknameReserved:
      return `“@${ALL_HANDLE}” addresses every agent the project has enrolled.`;
    case AGENT_ERRORS.nicknameTaken:
      return "Another agent already answers to that nickname.";
    case AGENT_ERRORS.providerUnconfigured:
      return "No AI API provider is configured. Configure one in Global settings → AI API.";
    case AGENT_ERRORS.providerNotVerified:
      return "No AI API provider is active for this project. Verify and activate one in Global settings → AI API.";
    case AGENT_ERRORS.agentNotFound:
      return "That agent no longer exists.";
    case AGENT_ERRORS.agentUnavailable:
      return "That agent's provider or model no longer serves it.";
    case AGENT_ERRORS.noProjectOpen:
      return "No project is open.";
    default:
      return aiErrorMessage(e);
  }
}

/** AGT-FR-13: the nickname rule the confirm action is gated on, client-side. */
export function nicknameProblem(nickname: string): string {
  if (nickname.length === 0) return "A nickname is required.";
  if (/\s/.test(nickname)) return "A nickname cannot contain spaces.";
  if (nickname.includes("@")) return "A nickname cannot contain @.";
  // AGT-FR-41: the handle is reserved, so no agent carries it as a nickname and
  // it means one thing in every project. Stated rather than merely refused —
  // AGT-FR-13 has the field say *why* the name is unavailable, so an author
  // reads that `all` is already spoken for rather than only that it is rejected.
  if (nickname.toLowerCase() === ALL_HANDLE) {
    return `“@${ALL_HANDLE}” addresses every agent the project has enrolled.`;
  }
  // Mirrors `NICKNAME_MAX` in `src-tauri/src/agents.rs`. Without it the confirm
  // action would make a call the backend refuses, which is exactly what
  // AGT-FR-13 exists to prevent.
  if ([...nickname].length > NICKNAME_MAX) {
    return `A nickname is at most ${NICKNAME_MAX} characters.`;
  }
  return "";
}

/** Kept in step with `NICKNAME_MAX` in `src-tauri/src/agents.rs`. */
const NICKNAME_MAX = 64;

export * from "./AgentSettings";
export * from "./AgentMention";
