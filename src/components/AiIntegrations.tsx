import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import * as api from "../api";
import {
  type AiApiIntegration,
  type AiApiProviderId,
  type ModelOption,
  type ReasoningChoice,
} from "../types";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { AiApiTurnTimeoutRow } from "./AiApiTurnTimeoutRow";
import { aiErrorMessage } from "./aiErrorMessage";
import {
  CUSTOM_GATEWAY_ORIGIN,
  modelSelectOptions,
  routeLabel,
} from "./aiApiCustom";
import { notifyAgentRegistryChanged } from "../state/agentRegistry";
import {
  FilterableSelect,
  type FilterableOption,
} from "./FilterableSelect";
import {
  PROVIDER_DEFAULT_LABEL,
  modelsOriginLabel,
  toneColor,
  type Busy,
  type Status,
} from "./aiIntegrationsShared";

// The Agentic level and the shared helpers live in files of their own; every
// name this module exported before is still exported from here.
export {
  BACKEND_DEFAULT_LABEL,
  PROVIDER_DEFAULT_LABEL,
  SAME_AS_DEFAULT_LABEL,
  modelsOriginLabel,
  type Status,
} from "./aiIntegrationsShared";
export { AgenticAiIntegrations } from "./AgenticAiIntegrations";
export {
  agenticStatus,
  canVerifyAgentic,
  oauthTokenValidation,
  rendersOauthTokenField,
  type AgenticDraft,
} from "./agenticDraft";

/**
 * The AI integrations section of the Global settings window, and the two
 * project-scoped controls that let one project depart from the global choices —
 * `specifications/ui/AII-ai-integrations.md` (AII-FR-01..35). Its backends are
 * `specifications/core/AAP-ai-api-integrations.md` and
 * `specifications/core/AIC-agentic-integrations.md`; every operation name
 * matches those specs' contract surfaces byte-for-byte.
 *
 * Four invariants shape the surface:
 *
 * - **Two levels, sharing nothing** (AII-FR-02). The AI API level configures the
 *   API endpoints the application calls itself; the Agentic level configures the
 *   backends it hands work to. Each has its own tabs and its own single active
 *   choice, and activating in one never disturbs the other (AII-FR-04). The AI
 *   API level's endpoints and keys carry every call the application makes for
 *   itself, but the model and the reasoning chosen in it are the graduation
 *   loop's alone — an agent runs on what its own description in the Agents
 *   section carries (AII-FR-54, `AGT-agents.md` AGT-FR-01).
 * - **A successful verification is what commits a configuration** (AII-FR-10 /
 *   AII-FR-21). The fields hold *candidates* — `drafts` below — and nothing
 *   reaches the backend until Verify runs. Leaving a tab discards an unverified
 *   candidate rather than half-storing it.
 * - **A key is never rendered back** (AII-FR-30). The key field is a password
 *   input that is never populated from a record; the stored key is represented
 *   only by its masked hint.
 * - **Every action applies at once** (AII-FR-28), so this section holds no dirty
 *   state and never contributes to the window's save-before-close sweep
 *   (GLS-FR-13 / GLS-FR-16).
 */



export { aiErrorMessage };


/** AII-FR-44: the entry meaning "whatever depth the model itself would pick". */
export const MODEL_DEFAULT_LABEL = "Model default";

/**
 * AII-FR-03: the order the AI API level presents its tabs in.
 *
 * The level orders its own tabs rather than rendering whatever order the list
 * operation happened to return, so the presentation is a fact about this
 * surface and not a coupling to the backend's iteration order.
 */
export const API_TAB_ORDER: AiApiProviderId[] = [
  "openrouter",
  "anthropic",
  "openai",
  "custom",
];

/** Put the level's records into tab order, leaving any unknown one at the end. */
export function inTabOrder<T extends { provider: AiApiProviderId }>(
  records: T[],
): T[] {
  const rank = (p: AiApiProviderId) => {
    const i = API_TAB_ORDER.indexOf(p);
    return i === -1 ? API_TAB_ORDER.length : i;
  };
  return [...records].sort((a, b) => rank(a.provider) - rank(b.provider));
}

/**
 * AII-FR-41 .. FR-44: the entries a model's reasoning selector offers, or
 * `null` where it renders none at all.
 *
 * `null` rather than an empty list is the distinction the surface acts on: no
 * reasoning row is rendered at all, and the rows beneath close the gap rather
 * than leaving one. It arises two ways — the provider default entry is
 * selected, so no model's capability is known (AII-FR-41), and the selected
 * model declares no reasoning.
 *
 * Pure and exported so the three shapes are testable without rendering.
 */
export function reasoningOptions(
  model: ModelOption | null | undefined,
): FilterableOption[] | null {
  const reasoning = model?.reasoning;
  if (!reasoning) return null;

  const options: FilterableOption[] = [
    { id: "", label: MODEL_DEFAULT_LABEL },
  ];
  // AII-FR-43: a model that cannot be asked to stop reasoning offers no off
  // entry — in either shape, rather than a disabled control.
  if (!reasoning.mandatory) options.push({ id: "off", label: "Off" });

  const ladder = reasoning.supportedEfforts;
  if (ladder && ladder.length > 0) {
    // AII-FR-42 / FR-47: exactly the levels this model declares, in the order
    // it declares them, labelled by the identifier the record gives them — a
    // level this application has never heard of included.
    for (const effort of ladder) {
      options.push({ id: `effort:${effort}`, label: effort });
    }
  } else {
    // AII-FR-43: reasoning, but no levels of its own.
    options.push({ id: "on", label: "On" });
  }
  return options;
}

/** The selector entry id a stored choice renders as. */
export function reasoningValue(choice: ReasoningChoice | null): string {
  if (!choice) return "";
  return choice.kind === "effort" ? `effort:${choice.effort}` : choice.kind;
}

/** The choice a selector entry id means. `null` is the model's own default. */
export function reasoningChoiceFor(id: string): ReasoningChoice | null {
  if (id === "off") return { kind: "off" };
  if (id === "on") return { kind: "on" };
  if (id.startsWith("effort:")) {
    return { kind: "effort", effort: id.slice("effort:".length) };
  }
  return null;
}

/**
 * AII-FR-44: what the model-default entry does, said plainly beneath the
 * selector, so "model default" is not an unexplained option.
 */
export function reasoningDefaultNote(
  model: ModelOption | null | undefined,
): string {
  const reasoning = model?.reasoning;
  if (!reasoning) return "";
  if (reasoning.mandatory) return "This model always reasons.";
  if (reasoning.defaultEffort) {
    return `This model reasons at ${reasoning.defaultEffort} unless asked otherwise.`;
  }
  if (reasoning.defaultEnabled === false) {
    return "This model does not reason unless asked to.";
  }
  return "This model reasons unless asked not to.";
}

// ---------------------------------------------------------------------------
// The AI API level
// ---------------------------------------------------------------------------

/**
 * AII-FR-09 / FR-10 / FR-11 / FR-15: the API level's verification status line —
 * the single place every configuration fact and every configuration error
 * attaches, which is why no failure here is ever a modal or a transient
 * notification.
 *
 * Pure and exported so the precedence below is testable directly. The order is
 * load-bearing: a fresh error outranks the stored state (the author just acted),
 * and an edited field outranks a stored verification (AII-FR-11) so a tab never
 * claims a configuration is verified while showing a different one.
 */
export function apiStatus(
  integration: AiApiIntegration,
  draftUrl: string,
  draftKey: string,
  busy: Busy,
  error: string,
): Status {
  if (busy === "verifying") return { text: "Verifying…", tone: "muted" };
  if (error) return { text: error, tone: "warn" };

  const stored = integration.baseUrl ?? "";
  const urlEdited = draftUrl.trim() !== stored;
  // A typed key is an edit even when the URL is untouched: it has not been
  // committed, so the tab must not go on claiming the old one is in force.
  const keyEdited = draftKey.trim() !== "";

  if (urlEdited || keyEdited || integration.state === "unconfigured") {
    if (draftUrl.trim() === "") {
      return { text: "No endpoint configured yet.", tone: "muted" };
    }
    return {
      text: "Not verified — verify this endpoint before use.",
      tone: "muted",
    };
  }

  // AII-FR-15: a stored key that can no longer be read keeps its configuration
  // and says so, rather than being cleared or silently re-verified.
  if (integration.state === "key_unavailable") {
    return {
      text: "The stored key can no longer be read. Verify this endpoint again.",
      tone: "warn",
    };
  }

  const when = integration.verifiedAt ? ` · ${integration.verifiedAt}` : "";
  return { text: `verified${when}`, tone: "ok" };
}

/**
 * The **AI API** section of Global settings — the API level of AII-FR-01. It is
 * a whole section rather than half of one: the two levels present the same shape
 * (tabs, a status line, a model selector, an activation control), so side by
 * side they read as one continuous run of form rows whose second half happens to
 * be about agents. Sectioned navigation is what tells the two apart before the
 * author reads a word of either (AII-FR-02).
 */
export function AiApiIntegrations() {
  const [integrations, setIntegrations] = useState<AiApiIntegration[] | null>(null);
  const [provider, setProvider] = useState<AiApiProviderId | null>(null);
  /** Candidate base URLs, per provider. Not stored until Verify succeeds. */
  const [urlDrafts, setUrlDrafts] = useState<Record<string, string>>({});
  /**
   * Candidate keys, per provider. Always starts empty and is emptied again on
   * every commit: a stored key is represented by its masked hint and is never
   * put back into the field (AII-FR-07 / AII-FR-30).
   */
  const [keyDrafts, setKeyDrafts] = useState<Record<string, string>>({});
  /**
   * Errors are kept in two channels because they belong in two places. A
   * configuration error attaches to the verification status line, which the
   * layout notes fix as where every such error goes; an error from a selection
   * or an activation says nothing about the endpoint, so it renders beside the
   * action that caused it.
   */
  const [configErrors, setConfigErrors] = useState<Record<string, string>>({});
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<Record<string, Busy>>({});
  const [loadError, setLoadError] = useState("");
  /** AII-FR-28: a refused turn timeout, per provider, shown beside its field. */
  const [timeoutErrors, setTimeoutErrors] = useState<Record<string, string>>({});
  const onTimeoutError = useCallback(
    (target: AiApiProviderId, message: string) =>
      setTimeoutErrors((prev) => ({ ...prev, [target]: message })),
    [],
  );

  useEffect(() => {
    let cancelled = false;
    api
      .listAiApiIntegrations()
      .then((list) => {
        if (cancelled) return;
        // Defensive: the level must not crash on a backend (or a test double)
        // that answers with nothing where a list was promised.
        const safe = Array.isArray(list) ? list : [];
        setIntegrations(safe);
        setUrlDrafts(
          Object.fromEntries(
            safe.map((i) => [i.provider, i.baseUrl ?? defaultBaseUrl(i.provider)]),
          ),
        );
        // AII-FR-03: the first tab is the level's own first tab, not the first
        // record the backend happened to return.
        setProvider(
          (current) => current ?? inTabOrder(safe)[0]?.provider ?? null,
        );
        setLoadError("");
      })
      .catch((e) => {
        if (cancelled) return;
        setIntegrations([]);
        setLoadError(aiErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const current = integrations?.find((i) => i.provider === provider) ?? null;

  /**
   * AII-FR-10: leaving a tab discards its unverified candidate.
   *
   * A candidate that never verified is not a fact about anything — reopening the
   * tab must show the last configuration that verified rather than an abandoned
   * edit, or the author reads a URL the application would never actually call.
   */
  const onSelectProvider = (next: AiApiProviderId) => {
    const leaving = current;
    if (leaving && leaving.provider !== next) {
      setUrlDrafts((prev) => ({
        ...prev,
        [leaving.provider]: leaving.baseUrl ?? defaultBaseUrl(leaving.provider),
      }));
      setKeyDrafts((prev) => ({ ...prev, [leaving.provider]: "" }));
      setConfigErrors((prev) => ({ ...prev, [leaving.provider]: "" }));
      setActionErrors((prev) => ({ ...prev, [leaving.provider]: "" }));
    }
    setProvider(next);
  };

  /** Replace one record in place, so a change to one tab never disturbs another. */
  const replace = useCallback((updated: AiApiIntegration) => {
    setIntegrations((prev) =>
      (prev ?? []).map((i) => (i.provider === updated.provider ? updated : i)),
    );
  }, []);

  // AII-FR-11: editing returns the tab to an unverified presentation without
  // invoking anything. The stored record is untouched; `apiStatus` reads the
  // drafts against it, so nothing here needs to clear a flag.
  const onEditUrl = (target: AiApiProviderId, value: string) => {
    setUrlDrafts((prev) => ({ ...prev, [target]: value }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
  };

  const onEditKey = (target: AiApiProviderId, value: string) => {
    setKeyDrafts((prev) => ({ ...prev, [target]: value }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
  };

  // AII-FR-09 / FR-10: the explicit action, and the only one that commits.
  const onVerify = async (target: AiApiProviderId) => {
    setBusy((prev) => ({ ...prev, [target]: "verifying" }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const key = (keyDrafts[target] ?? "").trim();
      const updated = await api.verifyAiApiIntegration(
        target,
        urlDrafts[target] ?? "",
        key === "" ? null : key,
      );
      replace(updated);
      // AII-FR-54: the provider that serves agents may have changed state.
      notifyAgentRegistryChanged();
      setUrlDrafts((prev) => ({
        ...prev,
        [target]: updated.baseUrl ?? defaultBaseUrl(target),
      }));
      // The key is committed; the field goes back to empty so nothing but the
      // masked hint represents it from here (AII-FR-07).
      setKeyDrafts((prev) => ({ ...prev, [target]: "" }));
    } catch (e) {
      // The registry and the keychain are exactly as they were (AAP-FR-06), so
      // nothing but the status line changes.
      setConfigErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  /** AII-FR-12: selections apply at once, with no save action in between. */
  const onSelectModel = async (target: AiApiProviderId, modelId: string | null) => {
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      // AII-FR-46: the returned record carries whatever became of the reasoning
      // choice, so a level the new model cannot honour is already gone from what
      // the selector re-renders against — the surface never has to work it out.
      replace(await api.setAiApiModel(target, modelId));
      notifyAgentRegistryChanged();
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    }
  };

  /** AII-FR-45: a reasoning choice applies at once, exactly as a model does. */
  const onSelectReasoning = async (target: AiApiProviderId, entryId: string) => {
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      replace(await api.setAiApiReasoning(target, reasoningChoiceFor(entryId)));
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    }
  };

  // AII-FR-13: exactly one integration is active in this level, so activating
  // one returns the whole list and every other tab renders its control again.
  const onActivate = async (target: AiApiProviderId) => {
    setBusy((prev) => ({ ...prev, [target]: "working" }));
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const list = await api.setActiveAiApiIntegration(target);
      setIntegrations(Array.isArray(list) ? list : []);
      // AII-FR-54: activating a provider changes the endpoint every agent uses.
      notifyAgentRegistryChanged();
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  // AII-FR-14: back to the unconfigured presentation — the provider's default
  // URL, no stored key, no hint, no selection, no activation.
  const onClear = async (target: AiApiProviderId) => {
    setBusy((prev) => ({ ...prev, [target]: "working" }));
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const list = await api.clearAiApiIntegration(target);
      setIntegrations(Array.isArray(list) ? list : []);
      notifyAgentRegistryChanged();
      setUrlDrafts((prev) => ({ ...prev, [target]: defaultBaseUrl(target) }));
      setKeyDrafts((prev) => ({ ...prev, [target]: "" }));
      setConfigErrors((prev) => ({ ...prev, [target]: "" }));
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  /** AII-FR-12: the record's options, preceded by the provider-default entry. */
  const modelOptions = useMemo<FilterableOption[]>(
    () => [
      { id: "", label: PROVIDER_DEFAULT_LABEL },
      ...modelSelectOptions(current?.provider ?? null, current?.models ?? []),
    ],
    [current?.provider, current?.models],
  );

  /** The model the reasoning row describes, or null on the provider default. */
  const selectedModel: ModelOption | null =
    current?.models.find((m) => m.id === current.selectedModel) ?? null;
  const reasoningEntries = reasoningOptions(selectedModel);
  /**
   * AII-FR-48: a model list that predates this application's knowledge of
   * reasoning looks exactly like a list of models that cannot reason, and both
   * render no reasoning row (AII-FR-41). Saying so is what keeps the absent
   * control from reading as a bug.
   */
  const reasoningNeedsReverify =
    current?.provider === "openrouter" &&
    current.models.length > 0 &&
    current.models.every((m) => !m.reasoning);

  const providerBusy = current ? (busy[current.provider] ?? "idle") : "idle";
  const status = current
    ? apiStatus(
        current,
        urlDrafts[current.provider] ?? "",
        keyDrafts[current.provider] ?? "",
        providerBusy,
        configErrors[current.provider] ?? "",
      )
    : null;

  return (
    // The section is named by the Global settings heading above it, so it
    // carries no heading of its own — a second one would name the same thing
    // twice.
    <section data-testid="ai-api-level">
      {/* AII-FR-54: the explanation sits above the tab strip and before any
          field, and says who uses what is chosen here — the one thing about this
          section an author cannot work out from the fields in front of them. It
          names the Agents section as where an agent's model is chosen, and says
          nothing that implies a selection made here changes what an agent runs
          on. The selectors below behave exactly as they otherwise would. */}
      <div style={{ marginBottom: 16 }} data-testid="ai-api-explanation">
        <p className="t-p" style={{ marginBottom: 8 }}>
          The endpoints Synthesis calls itself. Configure as many as you like;
          one is active at a time, and a project can override that choice in its
          own settings. Your key is kept in this machine's keychain and is never
          shown back to you.
        </p>
        <p className="t-p" style={{ margin: 0 }}>
          The model and reasoning here are used only by the graduation loop. An
          agent uses its own model and reasoning, set in the Agents section.
        </p>
        <p className="t-p" style={{ margin: "8px 0 0" }}>
          Agents are served by the provider that is active for the project, so
          activating a provider here changes the endpoint every agent uses.
        </p>
      </div>

      {integrations === null && <div className="t-muted">Loading…</div>}

      <div
        role="tablist"
        // Named for the section it sits in, which is the only name now on
        // screen — announcing "AI API integrations" inside a section headed
        // "AI API" would make a listener map two vocabularies.
        aria-label="AI API"
        style={SETTINGS_TABLIST_STYLE}
      >
        {/* AII-FR-03: OpenRouter first and selected on open, whatever order the
            list operation returned its records in. */}
        {inTabOrder(integrations ?? []).map((i) => (
          <button
            key={i.provider}
            role="tab"
            aria-selected={i.provider === provider}
            className="btn btn--ghost btn--sm"
            data-active={i.provider === provider}
            style={settingsTabStyle(i.provider === provider)}
            onClick={() => onSelectProvider(i.provider)}
          >
            {i.displayName}
            {i.active && (
              <span className="badge badge--ok" style={{ marginLeft: 6 }}>
                active
              </span>
            )}
          </button>
        ))}
      </div>

      {loadError && (
        <span className="picker-error" style={{ display: "block" }}>
          ✗ {loadError}
        </span>
      )}

      {/* AII-FR-05: every tab renders the same rows in the same order — base
          URL, key, verify, status, model, activation. */}
      {current && status && (
        <div
          data-testid={`ai-api-panel-${current.provider}`}
          role="tabpanel"
          aria-label={current.displayName}
        >
          {current.active && (
            <div
              className="badge badge--ok"
              style={{ marginBottom: 14 }}
              data-testid="ai-api-active-marker"
            >
              ● Active
            </div>
          )}

          {/* AII-FR-06: prefilled from the provider's default and editable, so a
              gateway is configurable; empty for Custom, which is how a
              locally-run model is pointed at. */}
          <div className="picker-field" style={{ marginBottom: 10 }}>
            <label
              className="picker-field__label"
              htmlFor={`ai-api-url-${current.provider}`}
            >
              {current.provider === "custom" ? "Gateway URL" : "Base URL"}
            </label>
            <input
              id={`ai-api-url-${current.provider}`}
              className="input input--mono"
              spellCheck={false}
              autoComplete="off"
              placeholder={
                defaultBaseUrl(current.provider) ||
                "https://your-endpoint.example/v1"
              }
              value={urlDrafts[current.provider] ?? ""}
              onChange={(e) => onEditUrl(current.provider, e.target.value)}
            />
            {current.provider === "custom" && (
              <div
                className="t-ui-xs t-muted"
                data-testid="ai-api-url-hint"
                style={{ marginTop: 2 }}
              >
                Host or base URL, without /v1.
              </div>
            )}
          </div>

          {/* AII-FR-07 / FR-30: a password field, never populated with a stored
              key. The masked hint beside it is the only representation of one. */}
          <div className="picker-field" style={{ marginBottom: 4 }}>
            <label
              className="picker-field__label"
              htmlFor={`ai-api-key-${current.provider}`}
            >
              {current.provider === "custom" ? "Secret" : "API key"}
            </label>
            <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
              <input
                id={`ai-api-key-${current.provider}`}
                type="password"
                className="input input--mono"
                spellCheck={false}
                autoComplete="off"
                placeholder={
                  current.maskedHint ? `•••• ${current.maskedHint}` : "required"
                }
                value={keyDrafts[current.provider] ?? ""}
                onChange={(e) => onEditKey(current.provider, e.target.value)}
                style={{ flex: 1 }}
              />
              <button
                className="btn btn--default btn--sm"
                disabled={providerBusy === "verifying"}
                onClick={() => void onVerify(current.provider)}
              >
                {providerBusy === "verifying" ? "Verifying…" : "Verify"}
              </button>
            </div>
          </div>

          {/* AII-FR-08: every tab says its key is required, and Custom says its
              secret is sent as a bearer token. A tab that already holds a key
              also says what the empty field will do, because that is the
              question the author actually has in front of them. */}
          <div
            className="t-ui-xs t-muted"
            data-testid="ai-api-key-requirement"
            style={{ marginBottom: 4 }}
          >
            {current.provider === "custom"
              ? "Required — sent as a bearer token."
              : "This provider requires an API key."}
            {current.keyState === "set" &&
              " Leave empty to verify again with the stored key."}
          </div>

          <div
            className="t-ui-sm"
            data-testid="ai-api-status"
            style={{ marginBottom: 18, color: toneColor(status.tone) }}
          >
            {status.text}
          </div>

          {/* AII-FR-12: the provider's options, preceded by an entry meaning its
              own default — which is what an unset selection renders as. The
              selector is the filterable one of AII-FR-36. */}
          <div className="picker-field" style={{ marginBottom: 4 }}>
            <label className="picker-field__label">Model</label>
            <FilterableSelect
              label="Model"
              testId="ai-api-model"
              value={current.selectedModel ?? ""}
              options={modelOptions}
              onChange={(id) => void onSelectModel(current.provider, id || null)}
            />
          </div>
          <div
            className="t-ui-xs t-muted"
            data-testid="ai-api-models-origin"
            style={{ marginBottom: 18 }}
          >
            {current.provider === "custom" && current.modelsOrigin === "probed"
              ? CUSTOM_GATEWAY_ORIGIN
              : modelsOriginLabel(current.modelsOrigin, "the endpoint")}
            {current.provider === "custom" && selectedModel
              ? ` · ${routeLabel(selectedModel)}`
              : ""}
          </div>

          {/* AII-FR-41: only in the OpenRouter tab, and only while a model is
              selected whose record declares reasoning. A model that declares
              none renders no row at all, and the activation control below
              closes the gap. */}
          {reasoningNeedsReverify && (
            <div
              className="t-ui-xs t-muted"
              data-testid="ai-api-reasoning-stale"
              style={{ marginBottom: 18 }}
            >
              Verify this endpoint again to load the reasoning each model
              supports.
            </div>
          )}

          {current.provider === "openrouter" && reasoningEntries && (
            <>
              <div className="picker-field" style={{ marginBottom: 4 }}>
                <label className="picker-field__label">Reasoning</label>
                <FilterableSelect
                  label="Reasoning"
                  testId="ai-api-reasoning"
                  value={reasoningValue(current.selectedReasoning)}
                  options={reasoningEntries}
                  onChange={(id) => void onSelectReasoning(current.provider, id)}
                />
              </div>
              <div
                className="t-ui-xs t-muted"
                data-testid="ai-api-reasoning-note"
                style={{ marginBottom: 18 }}
              >
                {reasoningDefaultNote(selectedModel)}
              </div>
            </>
          )}

          {/* AII-FR-05: the turn timeout sits beneath the reasoning row and
              before the activation control, on every tab. */}
          {/* Keyed by provider, so a typed entry and an error stay with the
              tab they belong to (AII-FR-28). */}
          <AiApiTurnTimeoutRow
            key={current.provider}
            provider={current.provider}
            turnTimeoutMs={current.turnTimeoutMs ?? null}
            onSaved={replace}
            describeError={aiErrorMessage}
            error={timeoutErrors[current.provider] ?? ""}
            onError={onTimeoutError}
          />

          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            {/* AII-FR-13: enabled only while this tab's integration is verified. */}
            {!current.active && (
              <button
                className="btn btn--primary btn--sm"
                disabled={current.state !== "verified"}
                onClick={() => void onActivate(current.provider)}
              >
                Use this integration
              </button>
            )}
            {current.state !== "unconfigured" && (
              <button
                className="btn btn--ghost btn--sm"
                onClick={() => void onClear(current.provider)}
              >
                Clear
              </button>
            )}
          </div>

          {actionErrors[current.provider] && (
            <span
              className="picker-error"
              style={{ display: "block", marginTop: 10 }}
              data-testid="ai-api-action-error"
            >
              ✗ {actionErrors[current.provider]}
            </span>
          )}

          {/* AII-FR-29: an unconfigured tab names what is needed rather than
              reporting an error. */}
          {current.state === "unconfigured" && (
            <div
              className="t-ui-sm t-muted"
              data-testid="ai-api-empty"
              style={{ marginTop: 14 }}
            >
              {current.displayName} is not set up yet. Give Synthesis its
              endpoint{current.keyRequired ? " and an API key" : ""} and verify
              it to use this integration.
            </div>
          )}
        </div>
      )}
    </section>
  );
}

/**
 * The base URL a tab prefills with before anything is stored (AII-FR-06). The
 * backend owns which providers ship a default; the UI mirrors it here so a fresh
 * tab is one field short of usable rather than blank.
 */
function defaultBaseUrl(provider: AiApiProviderId): string {
  switch (provider) {
    case "anthropic":
      return "https://api.anthropic.com/v1";
    case "openai":
      return "https://api.openai.com/v1";
    case "openrouter":
      return "https://openrouter.ai/api/v1";
    // AII-FR-06: Custom ships none — its whole purpose is a URL only the author
    // knows, which is what makes a locally-run model configurable.
    case "custom":
      return "";
  }
}
export { ProjectAiIntegrations } from "./ProjectAiIntegrations";
