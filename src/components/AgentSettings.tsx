/**
 * The Personal Agents and Project Agents sections and the persona editor
 * (`specifications/ui/AGT-agents.md` AGT-FR-09 … AGT-FR-23).
 *
 * The two sections are one component with a scope. A Personal agent is stored
 * on this machine and follows the author into every project; a Project agent
 * is a file of the project, committed and shared. The editor, the rows, and the
 * actions are the same for both (AGT-FR-11), which is why they are not two
 * copies.
 */
import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { useActiveAiApiCatalog } from "../hooks/useActiveAiApiCatalog";
import { useAgentsChanged } from "../hooks/useAgentsChanged";
import { logWarn } from "../logging";
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
  AgentScope,
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

/** What each scope is called in the editor's title and in the delete confirmation. */
const SCOPE_LABEL: Record<AgentScope, string> = {
  project: "Project",
  personal: "Personal",
};

interface PersonaEditorProps {
  /** The scope of the section the editor was opened from (AGT-FR-11). */
  scope: AgentScope;
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

function PersonaEditor({ scope, agent, catalog, onCancel, onSaved }: PersonaEditorProps) {
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
    const call = agent
      ? api.updateAgent(scope, agent.id, draft)
      : api.createAgent(scope, draft);
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
        aria-label={
          agent ? `Edit @${agent.nickname}` : `New ${SCOPE_LABEL[scope]} agent`
        }
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
        {agent ? `Edit @${agent.nickname}` : `New ${SCOPE_LABEL[scope]} agent`}
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

      {/* The editor is taller than the window it opens in — the Settings window
          opens at 800 × 600 and can be no smaller (per `SWN-settings-windows.md`
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
// The Personal Agents and Project Agents sections (AGT-FR-09 … AGT-FR-23)
// ---------------------------------------------------------------------------

/**
 * AGT-FR-06 / AGT-FR-07: a request from the chrome roster to open an editor.
 *
 * A *request* rather than a value, and cleared once honoured, because it
 * describes something the author just did rather than a state the section is
 * in. Held as a value it would reopen the editor every time the section was
 * revisited — and, being read only on mount, would be dropped entirely when the
 * roster addressed a Settings window that was already open.
 */
export interface AgentEditorRequest {
  /** The agent to edit, or `null` for a fresh persona. */
  agentId: string | null;
  /** Distinguishes two requests for the same agent. */
  nonce: number;
}

export interface AgentsSectionProps {
  /** Which agents the section lists and which store its editor writes (AGT-FR-11). */
  scope: AgentScope;
  editorRequest?: AgentEditorRequest | null;
  /** Called once the request has been honoured, so it is not honoured twice. */
  onEditorRequestHandled?: () => void;
}

/** The two sections of the Settings window that hold agents. */
export function PersonalAgents(props: Omit<AgentsSectionProps, "scope">) {
  return <AgentsSection scope="personal" {...props} />;
}

export function ProjectAgents(props: Omit<AgentsSectionProps, "scope">) {
  return <AgentsSection scope="project" {...props} />;
}

/**
 * AGT-FR-09 / AGT-FR-22: the agents of one scope, read from the operation that
 * serves it. The Personal section answers with no project open; the Project
 * section reads the roster and keeps the Project agents of it.
 */
function readScope(scope: AgentScope): Promise<ProjectAgent[]> {
  if (scope === "personal") return api.listPersonalAgents();
  return api
    .listProjectAgents()
    .then((list) => (list ?? []).filter((entry) => entry.scope === "project"));
}

export function AgentsSection({
  scope,
  editorRequest,
  onEditorRequestHandled,
}: AgentsSectionProps) {
  const [entries, setEntries] = useState<ProjectAgent[] | null>(null);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState<{ agent: Agent | null } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Agent | null>(null);
  const revision = useAgentRegistryRevision();
  // AGR-FR-ZRJD: a write made anywhere reads the list again.
  useAgentsChanged();
  // AGT-FR-14 / AGT-FR-PRVQ: the active provider's catalog, read on mount and
  // again whenever the registry revision changes (a provider switch, a
  // verification, or a clear).
  const active = useActiveAiApiCatalog(true);
  const catalog = active?.catalog ?? null;

  // The agents are read on their own: a catalog that could not be read costs
  // the rows their model labels, not the list.
  useEffect(() => {
    let cancelled = false;
    readScope(scope)
      .then((list) => {
        if (cancelled) return;
        setEntries(list ?? []);
        setError("");
      })
      .catch((e) => {
        if (cancelled) return;
        logWarn(["frontend"], "could not read the agents of a scope", {
          scope,
          reason: e instanceof Error ? e.message : String(e),
        });
        setEntries([]);
        setError(agentErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [scope, revision]);

  // AGT-FR-06 / AGT-FR-07: honour the editor the roster asked for, once the
  // list is in — and then say so, so the request is not honoured again the
  // next time this section is opened.
  useEffect(() => {
    if (!editorRequest || entries === null) return;
    if (editorRequest.agentId === null) {
      setEditing({ agent: null });
    } else {
      const found = entries.find((e) => e.agent.id === editorRequest.agentId);
      // An agent deleted between the roster reading it and this running is not
      // an error: the request simply has nothing to open.
      if (found) setEditing({ agent: found.agent });
    }
    onEditorRequestHandled?.();
  }, [editorRequest, entries, onEditorRequestHandled]);

  const personal = scope === "personal";

  return (
    <section
      data-testid="agents-section"
      data-scope={scope}
    >
      {/* AGT-FR-23: the Project section says where its agents live and how they
          are shared. */}
      <p className="t-p" style={{ marginBottom: 16 }}>
        {personal
          ? "Your AI collaborators. A Personal agent is stored on this machine and is available in every project."
          : "Agents of this project. They are saved in the project’s .synthesis/ folder and are shared with the other contributors when you commit them."}
      </p>

      {entries === null && <div className="t-muted">Loading…</div>}

      {/* AGT-FR-14 / AGT-FR-31: with nothing verified there is no agent worth
          describing, so the section says where one is configured rather than
          offering a create action that could not succeed.

          Withheld while `error` stands, because both of these empty states are
          advice — "configure a provider", "describe an agent" — and a read that
          failed is not evidence for either. Sending an author to AI API to fix
          providers that are fine is worse than saying nothing. */}
      {!error && entries !== null && active !== null && catalog === null && (
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

      {!error && entries !== null && entries.length === 0 && catalog !== null && (
        <div
          className="card"
          data-testid="agents-empty"
          style={{ padding: "24px 20px", textAlign: "center", color: "var(--fg-3)" }}
        >
          {personal
            ? "No Personal agents described yet. Use “New agent” to describe one."
            : "This project has no agents of its own yet. Use “New agent” to describe the first one."}
        </div>
      )}

      {entries !== null && entries.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {entries.map((entry) => {
            const agent = entry.agent;
            // AGT-FR-RWHL: a shadowed agent says so in place of any provider
            // state. Otherwise AGT-FR-PRVQ: derived from the active provider's
            // catalog.
            const state =
              entry.availability === "shadowed"
                ? "shadowed"
                : availabilityFromCatalog(agent, active);
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
          scope={scope}
          agent={editing.agent}
          catalog={catalog}
          onCancel={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            // AGT-FR-19 / AGT-FR-21: applied immediately through its own
            // operation, so the section holds no dirty state. The list is read
            // again, because a write can change what a Project agent shadows.
            notifyAgentRegistryChanged();
          }}
        />
      )}

      {/* AGT-FR-20: the confirmation names the agent and states what removing
          it means in this scope. Dismissing invokes nothing. */}
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
            Delete <strong>@{confirmDelete.nickname}</strong>?{" "}
            {personal
              ? "It is removed from every project."
              : "It is removed from this project and, once you commit the change, from every contributor."}
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
                  .deleteAgent(scope, id)
                  .then((list) => {
                    setEntries(list ?? []);
                    // AGR-FR-11: the roster's count moved too.
                    notifyAgentRegistryChanged();
                  })
                  .catch((e) => {
                    logWarn(["frontend"], "could not delete an agent", {
                      scope,
                      reason: e instanceof Error ? e.message : String(e),
                    });
                    setError(agentErrorMessage(e));
                  });
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
