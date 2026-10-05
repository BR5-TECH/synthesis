/**
 * The Global and Project Agents sections and the persona editor
 * (`specifications/ui/AGT-agents.md` AGT-FR-09 … AGT-FR-23).
 */
import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { useActiveAiApiCatalog } from "../hooks/useActiveAiApiCatalog";
import {
  notifyAgentRegistryChanged,
  useAgentRegistryRevision,
} from "../state/agentRegistry";
import { CUSTOM_GATEWAY_ORIGIN, modelSelectOptions, routeLabel } from "./aiApiCustom";
import {
  reasoningChoiceFor,
  reasoningDefaultNote,
  reasoningOptions,
  reasoningValue,
} from "./AiIntegrations";
import { FilterableSelect } from "./FilterableSelect";
import { Icon } from "./icons";
import type {
  Agent,
  AgentDraft,
  AiApiCatalog,
  ModelOption,
  ProjectAgent,
} from "../types";

import {
  agentErrorMessage,
  agentModelLine,
  availabilityFromCatalog,
  availabilityNote,
  nicknameProblem,
} from "./Agents";

interface PersonaEditorProps {
  /** The agent being edited, or null for a fresh persona. */
  agent: Agent | null;
  /**
   * AGT-FR-14 / AGT-FR-15: the catalog of the provider active for the open
   * project, or null when none resolves. The editor offers its models and
   * nothing else.
   */
  catalog: AiApiCatalog | null;
  onCancel: () => void;
  onSaved: (agent: Agent) => void;
}

function PersonaEditor({ agent, catalog, onCancel, onSaved }: PersonaEditorProps) {
  const [nickname, setNickname] = useState(agent?.nickname ?? "");
  const [title, setTitle] = useState(agent?.title ?? "");
  const models: ModelOption[] = catalog?.models ?? [];
  // AGT-FR-17: `null` means the author has not chosen yet. The model then
  // follows the agent's stored one, but only while the active provider offers
  // it; otherwise nothing is selected and nothing stored is rewritten.
  const [chosenModel, setChosenModel] = useState<string | null>(null);
  const storedOffered =
    !!agent && agent.modelId !== "" && models.some((m) => m.id === agent.modelId);
  const modelId =
    chosenModel !== null
      ? models.some((m) => m.id === chosenModel)
        ? chosenModel
        : ""
      : storedOffered && agent
        ? agent.modelId
        : "";
  const setModelId = (id: string) => setChosenModel(id);
  const [reasoning, setReasoning] = useState(reasoningValue(agent?.reasoning ?? null));
  const [instructions, setInstructions] = useState(agent?.instructions ?? "");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  // AGT-FR-11 / AGT non-functional: every control here is reachable from the
  // keyboard alone, which starts with focus being inside the modal at all —
  // otherwise Tab walks the tab behind it.
  const nicknameRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    nicknameRef.current?.focus();
  }, []);

  const storedUnavailable =
    !!agent && agent.modelId !== "" && !storedOffered && modelId === "";
  const selectedModel = models.find((m) => m.id === modelId) ?? null;
  // AGT-FR-16: rendered only while a model is selected whose record declares
  // reasoning; `null` renders no row at all and the rows beneath close the gap.
  const reasoningEntries = reasoningOptions(selectedModel);

  // AGT-FR-17: changing the model returns the reasoning selector to the
  // model-default entry when the new model cannot honour the previous choice.
  // This keeps the editor from ever holding a combination the backend would
  // reject. A model the active provider does not offer is never held (above).
  useEffect(() => {
    if (!reasoning) return;
    const allowed = reasoningEntries?.some((o) => o.id === reasoning) ?? false;
    if (!allowed) setReasoning("");
  }, [reasoning, reasoningEntries]);

  const nicknameError = nicknameProblem(nickname);
  // AGT-FR-18: an empty instructions field is an ordinary state and does not
  // disable the confirm action.
  // AGT-FR-14 / AGT-FR-17: no provider, or no available model chosen, keeps
  // the confirm action disabled, so no create or update call is made.
  const canConfirm = !nicknameError && catalog !== null && modelId !== "" && !busy;

  const confirm = () => {
    if (!canConfirm) return;
    const draft: AgentDraft = {
      nickname,
      // AGT-FR-42: sent as typed. The trim that decides what is stored is the
      // registry's (AGR-FR-23), so the editor never disagrees with it about
      // what a title with stray whitespace became.
      title,
      modelId,
      instructions,
      reasoning: reasoningChoiceFor(reasoning),
    };
    setBusy(true);
    setError("");
    const call = agent ? api.updateAgent(agent.id, draft) : api.createAgent(draft);
    call
      .then((saved) => onSaved(saved))
      // AGT-FR-19: on a typed failure the editor stays open with its fields
      // intact and the error renders inline, so one value is corrected rather
      // than all of them re-entered.
      .catch((e) => setError(agentErrorMessage(e)))
      .finally(() => setBusy(false));
  };

  return (
    <>
      {/* AGT-FR-11: a modal *over the tab*. Without a backdrop the row beneath
          stays clickable, and activating another row's edit action silently
          swaps the editor — discarding what was typed without a word. */}
      <div
        data-testid="agent-editor-backdrop"
        onMouseDown={onCancel}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 55,
          background: "rgba(0, 0, 0, 0.28)",
        }}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={agent ? `Edit @${agent.nickname}` : "New agent"}
        data-testid="agent-editor"
        className="card"
        // Bounded and scrollable: the field stack is taller than a short window,
        // and a confirm action below the fold in a surface with no other way out
        // is a trap rather than a modal.
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onCancel();
          }
        }}
        style={{
          position: "fixed",
          top: "50%",
          left: "50%",
          transform: "translate(-50%, -50%)",
          // `.card` is content-box — this project declares box sizing per
          // component rather than resetting it globally — so without this the
          // card's own padding is added *outside* the bounds below and the
          // intended 16px margin becomes a 3px overflow at both edges.
          boxSizing: "border-box",
          width: 420,
          maxWidth: "calc(100vw - 32px)",
          maxHeight: "calc(100vh - 32px)",
          overflowY: "auto",
          zIndex: 60,
          padding: 18,
          display: "flex",
          flexDirection: "column",
          gap: 14,
        }}
      >
      <h3 className="t-h3" style={{ margin: 0 }}>
        {agent ? `Edit @${agent.nickname}` : "New agent"}
      </h3>

      {/* AGT-FR-12: exactly five controls, in this order. The model and the
          reasoning among them are this agent's own — nothing here reads the AI
          API section's stored selections, and nothing chosen here reaches a
          graduation run (AGT-FR-01, AAP-FR-19). */}
      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="t-ui-sm">Nickname</span>
        <input
          ref={nicknameRef}
          className="input"
          data-testid="agent-nickname"
          value={nickname}
          onChange={(e) => setNickname(e.target.value)}
        />
        <span className="t-ui-sm t-muted">
          Typed with an @ to address this agent.
        </span>
        {nicknameError && (
          <span className="picker-error" data-testid="agent-nickname-error">
            {nicknameError}
          </span>
        )}
      </label>

      {/* AGT-FR-42: optional, validated by nothing, and never a reason the
          confirm action is disabled. It sits with the nickname because both say
          who this collaborator is, where the three below say what runs it. */}
      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="t-ui-sm">Title</span>
        <input
          className="input"
          data-testid="agent-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <span className="t-ui-sm t-muted">
          Optional. The role this agent takes — “UI/UX designer”, “Developer”.
        </span>
      </label>

      {/* AGT-FR-15: the same filterable selector both AI sections use, so
          choosing a model is the same act here as in Global settings. */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="t-ui-sm">Model</span>
        <FilterableSelect
          label="Model"
          testId="agent-model"
          options={modelSelectOptions(catalog?.provider ?? null, models)}
          value={modelId}
          onChange={setModelId}
        />
        {/* AGT-FR-15: where the active provider is Custom, the list is said to
            come from the Custom gateway. */}
        {catalog?.provider === "custom" && (
          <span className="t-ui-sm t-muted" data-testid="agent-model-origin">
            {CUSTOM_GATEWAY_ORIGIN}
            {selectedModel ? ` · ${routeLabel(selectedModel)}` : ""}
          </span>
        )}
        {/* AGT-FR-17: the stored model is not rewritten until the author
            confirms, so the editor says why nothing is selected. */}
        {storedUnavailable && agent && (
          <span
            className="picker-error"
            data-testid="agent-model-unavailable"
          >
            The stored model ({agent.modelId}) is not offered by the active AI API
            provider. Choose another model to save this agent.
          </span>
        )}
      </div>

      {reasoningEntries && (
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="t-ui-sm">Reasoning</span>
          <FilterableSelect
            label="Reasoning"
            testId="agent-reasoning"
            options={reasoningEntries}
            value={reasoning}
            onChange={setReasoning}
          />
          <span className="t-ui-sm t-muted" data-testid="agent-reasoning-note">
            {reasoningDefaultNote(selectedModel)}
          </span>
        </div>
      )}

      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="t-ui-sm">Instructions</span>
        <textarea
          className="textarea"
          data-testid="agent-instructions"
          rows={5}
          value={instructions}
          onChange={(e) => setInstructions(e.target.value)}
        />
        <span className="t-ui-sm t-muted">Optional.</span>
      </label>

      {error && (
        <span className="picker-error" data-testid="agent-editor-error">
          ✗ {error}
        </span>
      )}

      {/* The editor is taller than the window it opens in — the Global settings
          window is fixed at 800 × 600 (per `SWN-settings-windows.md`
          SWN-FR-03), which is roughly 572 px of body once the title bar is
          taken — so the card scrolls. Its two actions must not scroll away with
          it: a confirm the author cannot see on open reads as a form with no
          way to submit it, and this surface offers no other. Stuck to the foot
          of the scrolling card, on the card's own background so the fields
          passing underneath do not show through. */}
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          gap: 8,
          position: "sticky",
          bottom: -18,
          // Cancels the card's own 18px padding so the strip spans its full
          // width, then puts the padding back inside itself.
          margin: "0 -18px -18px",
          padding: "12px 18px 18px",
          background: "var(--bg-panel)",
          borderTop: "1px solid var(--border-1)",
        }}
      >
        {/* AGT-FR-19: cancelling invokes nothing. */}
        <button className="btn btn--default btn--sm" onClick={onCancel}>
          Cancel
        </button>
        <button
          className="btn btn--primary btn--sm"
          data-testid="agent-editor-confirm"
          disabled={!canConfirm}
          onClick={confirm}
        >
          {agent ? "Save" : "Create"}
        </button>
      </div>
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------
// The Global settings Agents section (AGT-FR-09 … AGT-FR-21)
// ---------------------------------------------------------------------------

/**
 * AGT-FR-06 / AGT-FR-07: a request from the chrome roster to open an editor.
 *
 * A *request* rather than a value, and cleared once honoured, because it
 * describes something the author just did rather than a state the section is
 * in. Held as a value it would reopen the editor every time the section was
 * revisited — and, being read only on mount, would be dropped entirely when the
 * roster addressed a Global settings window that was already open.
 */
export interface AgentEditorRequest {
  /** The agent to edit, or `null` for a fresh persona. */
  agentId: string | null;
  /** Distinguishes two requests for the same agent. */
  nonce: number;
}

export interface GlobalAgentsProps {
  editorRequest?: AgentEditorRequest | null;
  /** Called once the request has been honoured, so it is not honoured twice. */
  onEditorRequestHandled?: () => void;
}

export function GlobalAgents({
  editorRequest,
  onEditorRequestHandled,
}: GlobalAgentsProps) {
  const [agents, setAgents] = useState<Agent[] | null>(null);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState<{ agent: Agent | null } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Agent | null>(null);
  const revision = useAgentRegistryRevision();
  // AGT-FR-14 / AGT-FR-PRVQ: the active provider's catalog, read on mount and
  // again whenever the registry revision changes (a provider switch, a
  // verification, or a clear).
  const active = useActiveAiApiCatalog(true);
  const catalog = active?.catalog ?? null;

  // The agents are read on their own: a catalog that could not be read costs
  // the rows their model labels, not the list.
  useEffect(() => {
    let cancelled = false;
    api
      .listAgents()
      .then((list) => {
        if (cancelled) return;
        setAgents(list ?? []);
        setError("");
      })
      .catch((e) => {
        if (cancelled) return;
        setAgents([]);
        setError(agentErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  // AGT-FR-06 / AGT-FR-07: honour the editor the roster asked for, once the
  // registry is in — and then say so, so the request is not honoured again the
  // next time this section is opened.
  useEffect(() => {
    if (!editorRequest || agents === null) return;
    if (editorRequest.agentId === null) {
      setEditing({ agent: null });
    } else {
      const found = agents.find((a) => a.id === editorRequest.agentId);
      // An agent deleted between the roster reading it and this running is not
      // an error: the request simply has nothing to open.
      if (found) setEditing({ agent: found });
    }
    onEditorRequestHandled?.();
  }, [editorRequest, agents, onEditorRequestHandled]);

  return (
    <section data-testid="agents-section">
      <p className="t-p" style={{ marginBottom: 16 }}>
        The AI collaborators you can talk to. A persona is described once for this
        machine; each project chooses which of them it hears from in its own
        settings.
      </p>

      {agents === null && <div className="t-muted">Loading…</div>}

      {/* AGT-FR-14 / AGT-FR-31: with nothing verified there is no agent worth
          describing, so the section says where one is configured rather than
          offering a create action that could not succeed.

          Withheld while `error` stands, because both of these empty states are
          advice — "configure a provider", "describe an agent" — and a read that
          failed is not evidence for either. Sending an author to AI API to fix
          providers that are fine is worse than saying nothing. */}
      {!error && agents !== null && active !== null && catalog === null && (
        <div
          className="card"
          data-testid="agents-no-provider"
          style={{ padding: "24px 20px", color: "var(--fg-3)" }}
        >
          {active.resolution === "none_configured"
            ? "No AI API provider is configured yet. Configure one in Global settings → AI API, and agents can be described here."
            : "No AI API provider is active for this project. Verify and activate one in Global settings → AI API, and agents can be described here."}
        </div>
      )}

      {!error && agents !== null && agents.length === 0 && catalog !== null && (
        <div
          className="card"
          data-testid="agents-empty"
          style={{ padding: "24px 20px", textAlign: "center", color: "var(--fg-3)" }}
        >
          No agents described yet.
        </div>
      )}

      {agents !== null && agents.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {agents.map((agent) => {
            // AGT-FR-PRVQ: derived from the active provider's catalog.
            const state = availabilityFromCatalog(agent, active);
            const note = availabilityNote(state);
            return (
              <div
                key={agent.id}
                className="card"
                data-testid="agent-row"
                style={{ display: "flex", gap: 12, padding: "10px 14px" }}
              >
                <div style={{ flex: 1 }}>
                  <div
                    data-testid="agent-row-nickname"
                    style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600 }}
                  >
                    @{agent.nickname}
                  </div>
                  <div className="t-ui-sm t-muted">
                    {agentModelLine(agent, catalog)}
                  </div>
                  {/* AGT-FR-10: the row is neither hidden nor cleared for it. */}
                  {note && (
                    <div
                      className="t-ui-sm"
                      data-testid="agent-row-warning"
                      style={{ color: "var(--warn, #b8860b)", marginTop: 4 }}
                    >
                      ⚠ {note}
                    </div>
                  )}
                </div>
                <div style={{ display: "flex", gap: 6, alignItems: "flex-start" }}>
                  <button
                    className="btn btn--default btn--sm"
                    data-testid="agent-edit"
                    onClick={() => setEditing({ agent })}
                  >
                    Edit
                  </button>
                  <button
                    className="btn btn--ghost btn--icon btn--sm"
                    data-testid="agent-delete"
                    aria-label={`Delete @${agent.nickname}`}
                    onClick={() => setConfirmDelete(agent)}
                  >
                    <Icon.X size={12} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <button
        className="btn btn--default btn--sm"
        data-testid="agent-create"
        style={{ alignSelf: "flex-start", marginTop: 12 }}
        disabled={catalog === null}
        onClick={() => setEditing({ agent: null })}
      >
        <Icon.Plus size={12} /> New agent
      </button>

      {error && (
        <span
          className="picker-error"
          data-testid="agents-error"
          style={{ display: "block", marginTop: 8 }}
        >
          ✗ {error}
        </span>
      )}

      {editing && active !== null && (
        <PersonaEditor
          agent={editing.agent}
          catalog={catalog}
          onCancel={() => setEditing(null)}
          onSaved={(saved) => {
            setEditing(null);
            // AGT-FR-19 / AGT-FR-21: the list re-renders from what the operation
            // returned. Applied immediately through its own operation, so the
            // section holds no dirty state.
            setAgents((prev) => {
              const rest = (prev ?? []).filter((a) => a.id !== saved.id);
              return [...rest, saved].sort((a, b) =>
                a.nickname.toLowerCase().localeCompare(b.nickname.toLowerCase()),
              );
            });
            notifyAgentRegistryChanged();
          }}
        />
      )}

      {/* AGT-FR-20: the confirmation names the agent and states that it is
          removed from every project that enrolled it. Dismissing invokes
          nothing. */}
      {confirmDelete && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={`Delete @${confirmDelete.nickname}`}
          data-testid="agent-delete-confirm"
          className="card"
          style={{
            position: "fixed",
            top: "30%",
            left: "50%",
            transform: "translateX(-50%)",
            width: 360,
            zIndex: 60,
            padding: 18,
          }}
        >
          <p className="t-p">
            Delete <strong>@{confirmDelete.nickname}</strong>? It is removed from
            every project that enrolled it.
          </p>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button
              className="btn btn--default btn--sm"
              onClick={() => setConfirmDelete(null)}
            >
              Cancel
            </button>
            <button
              className="btn btn--danger btn--sm"
              data-testid="agent-delete-confirm-ok"
              onClick={() => {
                const id = confirmDelete.id;
                setConfirmDelete(null);
                api
                  .deleteAgent(id)
                  .then((list) => {
                    setAgents(list ?? []);
                    // AGR-FR-11: a deletion clears the agent from every
                    // project's enrolment, so the roster's count moved too.
                    notifyAgentRegistryChanged();
                  })
                  .catch((e) => setError(agentErrorMessage(e)));
              }}
            >
              Delete
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// The Project settings Agents section (AGT-FR-22, AGT-FR-23)
// ---------------------------------------------------------------------------

export function ProjectAgents() {
  const [enrolled, setEnrolled] = useState<ProjectAgent[] | null>(null);
  const [all, setAll] = useState<Agent[] | null>(null);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState("");
  const active = useActiveAiApiCatalog(true);
  const revision = useAgentRegistryRevision();

  // Joined deliberately, unlike the Global section's: which agents are enrolled
  // and which exist are the same question asked twice, and the section can say
  // nothing useful holding one without the other.
  useEffect(() => {
    let cancelled = false;
    Promise.all([api.listProjectAgents(), api.listAgents()])
      .then(([mine, everything]) => {
        if (cancelled) return;
        setEnrolled(mine ?? []);
        setAll(everything ?? []);
        setError("");
      })
      .catch((e) => {
        if (cancelled) return;
        setEnrolled([]);
        setAll([]);
        setError(agentErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  const enrolledIds = new Set((enrolled ?? []).map((e) => e.agent.id));
  const available = (all ?? []).filter((a) => !enrolledIds.has(a.id));

  return (
    <section data-testid="project-agents-section">
      {/* The tab supplies "Who this project can talk to." above this section
          (`SET-project-settings.md`), so this says only what that does not. */}
      <p className="t-p" style={{ marginBottom: 16 }}>
        Personas are described in Global settings → Agents; removing one here
        leaves its description untouched.
      </p>

      {enrolled === null && <div className="t-muted">Loading…</div>}

      {/* AGT-FR-23 / AGT-FR-31: nothing described at all is a different empty
          state from nothing enrolled — one sends the author somewhere to
          describe a persona, the other to choose among the ones they have.
          Both are withheld while `error` stands: a read that failed is evidence
          for neither, and "no agents are described on this machine" is a
          statement of fact rather than advice, so it must not be guessed at. */}
      {!error && enrolled !== null && (all ?? []).length === 0 && (
        <div
          className="card"
          data-testid="project-agents-none-described"
          style={{ padding: "24px 20px", color: "var(--fg-3)" }}
        >
          No agents are described on this machine yet. Describe one in Global
          settings → Agents, then enrol it here.
        </div>
      )}

      {!error && enrolled !== null && enrolled.length === 0 && (all ?? []).length > 0 && (
        <div
          className="card"
          data-testid="project-agents-empty"
          style={{ padding: "24px 20px", textAlign: "center", color: "var(--fg-3)" }}
        >
          This project has enrolled no agents.
        </div>
      )}

      {(enrolled ?? []).map((entry) => {
        const note = availabilityNote(entry.availability);
        return (
          <div
            key={entry.agent.id}
            className="card"
            data-testid="project-agent-row"
            style={{
              display: "flex",
              gap: 12,
              padding: "10px 14px",
              marginBottom: 6,
            }}
          >
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 600 }}>@{entry.agent.nickname}</div>
              <div className="t-ui-sm t-muted">
                {agentModelLine(entry.agent, active?.catalog ?? null)}
              </div>
              {note && (
                <div
                  className="t-ui-sm"
                  data-testid="project-agent-warning"
                  style={{ color: "var(--warn, #b8860b)", marginTop: 4 }}
                >
                  ⚠ {note}
                </div>
              )}
            </div>
            {/* AGT-FR-22: no edit action anywhere in this section — a persona is
                described in Global settings. AGT-FR-23: removal applies at once,
                so this section sits outside the tab's dirty state. */}
            <button
              className="btn btn--ghost btn--icon btn--sm"
              data-testid="project-agent-remove"
              aria-label={`Remove @${entry.agent.nickname}`}
              onClick={() => {
                api
                  .removeProjectAgent(entry.agent.id)
                  .then((list) => {
                    setEnrolled(list ?? []);
                    notifyAgentRegistryChanged();
                  })
                  .catch((e) => setError(agentErrorMessage(e)));
              }}
            >
              <Icon.X size={12} />
            </button>
          </div>
        );
      })}

      <div style={{ position: "relative", display: "inline-block", marginTop: 8 }}>
        <button
          className="btn btn--default btn--sm"
          data-testid="project-agent-add"
          disabled={available.length === 0}
          title={
            available.length === 0 && (all ?? []).length > 0
              ? "Every agent described on this machine is already enrolled."
              : undefined
          }
          onClick={() => setAdding((v) => !v)}
        >
          <Icon.Plus size={12} />{" "}
          {available.length === 0 && (all ?? []).length > 0
            ? "Nothing left to add"
            : "Add an agent"}
        </button>

        {adding && available.length > 0 && (
          <div
            className="card"
            role="listbox"
            aria-label="Agents to enrol"
            data-testid="project-agent-picker"
            style={{
              position: "absolute",
              top: "calc(100% + 4px)",
              left: 0,
              minWidth: 220,
              zIndex: 40,
              padding: 4,
            }}
          >
            {available.map((agent) => (
              <button
                key={agent.id}
                role="option"
                aria-selected={false}
                className="btn btn--ghost btn--sm"
                data-testid="project-agent-option"
                style={{ width: "100%", justifyContent: "flex-start" }}
                onClick={() => {
                  setAdding(false);
                  api
                    .enrolProjectAgent(agent.id)
                    .then((list) => {
                      setEnrolled(list ?? []);
                      notifyAgentRegistryChanged();
                    })
                    .catch((e) => setError(agentErrorMessage(e)));
                }}
              >
                @{agent.nickname}
              </button>
            ))}
          </div>
        )}
      </div>

      {error && (
        <span
          className="picker-error"
          data-testid="project-agents-error"
          style={{ display: "block", marginTop: 8 }}
        >
          ✗ {error}
        </span>
      )}
    </section>
  );
}
