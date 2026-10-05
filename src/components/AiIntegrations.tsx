import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import * as api from "../api";
import {
  AGENTIC_TURN_KINDS,
  AI_ERRORS,
  type AgenticTurnKind,
  type AgenticIntegration,
  type AgenticVendorId,
  type AiApiIntegration,
  type AiApiProviderId,
  type ModelOption,
  type ModelsOrigin,
  type ProjectAgenticIntegration,
  type ProjectAiApiIntegration,
  type ReasoningChoice,
  isValidClaudeOauthToken,
} from "../types";
import { logWarn } from "../logging";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { AiApiTurnTimeoutRow } from "./AiApiTurnTimeoutRow";
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

/** The entry meaning "whatever the provider itself would pick" (AII-FR-12). */
export const PROVIDER_DEFAULT_LABEL = "Provider default";
/** The entry meaning "whatever the backend itself would pick" (AII-FR-23/24). */
export const BACKEND_DEFAULT_LABEL = "Backend default";
/** The entry a per-task selector shows while it follows the default (AII-FR-23/24). */
export const SAME_AS_DEFAULT_LABEL = "Same as default";

/** The typed distinctions this module knows how to name (AII-FR-28). */
const KNOWN_AI_ERRORS = new Set<string>(Object.values(AI_ERRORS));

/**
 * The typed distinction behind a rejection, as a value that is safe to log.
 *
 * A backend rejection carries a code from a closed set, but an error that
 * reached here by another route can carry anything — a URL, a request body, a
 * credential the client echoed back. Nothing downstream redacts a log record, so
 * a code outside the set is reported by its *shape* rather than its value: which
 * failure it was is what a reader needs, and an unrecognised one only needs to
 * be distinguishable from the rest.
 */
function typedFailureCode(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  return KNOWN_AI_ERRORS.has(raw) ? raw : "unrecognised";
}

/**
 * An agentic tab's single per-task section — AII-FR-QDLW / AII-FR-PMZK.
 *
 * The tab holds **one** of these, below both default selectors rather than
 * under either one of them: the two defaults are the pair of values the backend
 * runs on, and this is where a single kind of work departs from that pair. Each
 * row is one kind of work followed by its own model selector and its own effort
 * selector on one line, so what a turn resolves is read across a row rather than
 * matched between two lists.
 *
 * The two selectors of a row are independent of each other and of every other
 * row. Nothing here couples them: each renders from its own override map and
 * invokes its own operation, so returning one to "same as default" returns that
 * kind to one default and never to two.
 *
 * The summary counts **rows**, not selectors (AII-FR-PMZK) — a kind of work that
 * overrides its model, its effort, or both counts once — so the line answers
 * "how many kinds of work are configured apart?" rather than "how many controls
 * were touched?".
 *
 * The rows are in the order a run reaches them and are labelled for the author
 * rather than by the identifier each carries — the last of them is the
 * semantic-rebase turn under the name the rest of the application gives it.
 */
function PerTaskSection({
  modelOverrides,
  effortOverrides,
  hasEffortColumn,
  renderModel,
  renderEffort,
}: {
  /**
   * The turn kinds the model selection differs on. Never null-valued: a kind
   * that follows the default is absent from it rather than mapped to null.
   */
  modelOverrides: Record<string, string>;
  /** The same, for the effort selection. */
  effortOverrides: Record<string, string>;
  /**
   * Whether the vendor declares effort levels at all. A vendor that declares
   * none renders no effort column and no disabled control in its place
   * (AII-FR-24), and a stored effort override it cannot show is not counted —
   * a summary must never report a difference the author has no control for.
   */
  hasEffortColumn: boolean;
  renderModel: (kind: (typeof AGENTIC_TURN_KINDS)[number]) => ReactNode;
  renderEffort: (kind: (typeof AGENTIC_TURN_KINDS)[number]) => ReactNode;
}) {
  const total = AGENTIC_TURN_KINDS.length;
  const differing = AGENTIC_TURN_KINDS.filter(
    (kind) =>
      modelOverrides[kind.id] !== undefined ||
      (hasEffortColumn && effortOverrides[kind.id] !== undefined),
  ).length;
  return (
    <details className="agentic-per-task" data-testid="agentic-per-task">
      {/* The summary counts rather than names, so a backend configured one way
          for everything reads as the two defaults and one closed line. */}
      <summary data-testid="agentic-per-task-summary">
        Per task ·{" "}
        {differing === 0
          ? `${total} follow the defaults`
          : `${differing} of ${total} ${
              differing === 1 ? "differs" : "differ"
            } from the defaults`}
      </summary>
      <div
        className={`agentic-per-task__grid${
          hasEffortColumn ? "" : " agentic-per-task__grid--no-effort"
        }`}
      >
        {/* The column headings name the default each column follows, so a
            column reads down as one dimension and a row reads across as one
            kind of work. The first cell is the task-label column's own. */}
        <span />
        <span className="agentic-per-task__heading">Model</span>
        {hasEffortColumn && (
          <span className="agentic-per-task__heading">Effort</span>
        )}
        {AGENTIC_TURN_KINDS.map((kind) => (
          <Fragment key={kind.id}>
            <span className="agentic-per-task__task">{kind.label}</span>
            <div className="agentic-per-task__cell">{renderModel(kind)}</div>
            {hasEffortColumn && (
              <div className="agentic-per-task__cell">{renderEffort(kind)}</div>
            )}
          </Fragment>
        ))}
      </div>
    </details>
  );
}

/**
 * Render a typed backend rejection as text that says what to do about it.
 *
 * The failures are deliberately distinguishable — a wrong path, a permissions
 * problem, an unreachable host, and a refused key each call for a different
 * response from the author (AII-FR-09 / AII-FR-20).
 */
export function aiErrorMessage(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  switch (raw) {
    case AI_ERRORS.pathEmpty:
      return "Enter the path to the CLI before verifying.";
    case AI_ERRORS.notFound:
      return "Nothing exists at that path.";
    case AI_ERRORS.notExecutable:
      return "That file cannot be run. Check its permissions.";
    case AI_ERRORS.tokenMissing:
      return "This integration needs an OAuth token. Enter one and verify again.";
    case AI_ERRORS.tokenMalformed:
      // Deliberately says nothing about the value that provoked it (AII-FR-50).
      return "That is not a valid OAuth token.";
    case AI_ERRORS.notTheExpectedCli:
      return "That program is not this vendor's CLI.";
    case AI_ERRORS.executionFailed:
      return "That file could not be run at all.";
    case AI_ERRORS.baseUrlEmpty:
      return "Enter the endpoint's base URL before verifying.";
    case AI_ERRORS.baseUrlInvalid:
      return "That is not a valid URL. It should start with http:// or https://.";
    case AI_ERRORS.keyMissing:
      return "This provider needs an API key.";
    case AI_ERRORS.unreachable:
      return "That endpoint could not be reached. Check the URL and your network.";
    case AI_ERRORS.rejected:
      return "The endpoint refused that key.";
    case AI_ERRORS.notAnAiEndpoint:
      return "That URL answered, but not as a conversational API.";
    case AI_ERRORS.notAnAgentEndpoint:
      return "That URL answered, but not as an agent-execution endpoint.";
    case AI_ERRORS.timedOut:
      return "It did not answer in time and was stopped.";
    case AI_ERRORS.keychainUnavailable:
      return "The system keychain would not answer, so the key was not stored.";
    case AI_ERRORS.wrongConfigKind:
      return "That configuration does not match this integration's kind.";
    case AI_ERRORS.notACliIntegration:
      return "This integration has no binary to look for.";
    case AI_ERRORS.unknownModel:
      return "That model is not one this backend offers.";
    case AI_ERRORS.unknownEffort:
      return "That reasoning effort is not one this backend offers.";
    case AI_ERRORS.unknownTurnKind:
      return "That is not a kind of task this backend is given.";
    case AI_ERRORS.noModelSelected:
      return "Choose a model before setting how much it should reason.";
    case AI_ERRORS.reasoningUnsupported:
      return "That model does not support reasoning.";
    case AI_ERRORS.reasoningMandatory:
      return "That model always reasons and cannot be turned off.";
    case AI_ERRORS.notVerified:
      return "Verify this integration before using it.";
    case AI_ERRORS.turnTimeoutOutOfRange:
      return "Enter a whole number of seconds from 30 to 3600.";
    case AI_ERRORS.unknownVendor:
    case AI_ERRORS.unknownProvider:
      return "That integration is not one Synthesis supports.";
    default:
      return raw || "operation failed";
  }
}

/** What a verification status line says, and how it reads. */
export interface Status {
  text: string;
  tone: "ok" | "warn" | "muted";
}

/** AII-FR-12 / FR-23: where a model list came from, said plainly. */
export function modelsOriginLabel(origin: ModelsOrigin, source: string): string {
  return origin === "probed"
    ? `from ${source}`
    : "from Synthesis's bundled list";
}

type Busy = "idle" | "detecting" | "verifying" | "working";

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

/** The same, for the agentic level's API-kind vendors (AII-FR-19). */
function defaultAgenticBaseUrl(vendor: AgenticVendorId): string {
  return vendor === "claude_agent_api" ? "https://api.anthropic.com/v1" : "";
}

// ---------------------------------------------------------------------------
// The Agentic level
// ---------------------------------------------------------------------------

/** The candidate configuration of an agentic tab, whichever kind it is. */
export interface AgenticDraft {
  path: string;
  baseUrl: string;
  apiKey: string;
  /**
   * Claude Code's OAuth token (AII-FR-49). Lives here and nowhere else, and only
   * until the submission carrying it resolves — nothing persists it, carries it
   * across a tab change, or restores it when the tab reopens (AII-FR-53).
   */
  oauthToken: string;
}

/**
 * AII-FR-49: does this tab render an OAuth token field?
 *
 * Read off the record rather than compared against a vendor id, so the rule is
 * "the vendor says it needs a credential" rather than "the vendor is called
 * Claude Code" — which is the same fact the backend derives its own behaviour
 * from (AIC-FR-26), and keeps the two from drifting apart.
 */
export function rendersOauthTokenField(i: AgenticIntegration): boolean {
  return i.kind === "cli" && i.keyRequired;
}

/**
 * AII-FR-50: the inline structural complaint, or `""` when there is nothing to
 * say. An empty field is not an error — it means "keep the stored token", which
 * AII-FR-51 judges separately.
 *
 * Never quotes the value it rejected: a near-miss token is one character from
 * the real thing, and echoing it into the DOM would put it exactly where the
 * rest of this section takes care never to.
 */
export function oauthTokenValidation(value: string): string {
  if (value.trim() === "") return "";
  return isValidClaudeOauthToken(value.trim())
    ? ""
    : "A token starts with sk-ant-oat01- followed by letters, digits, hyphens, or underscores.";
}

/**
 * AII-FR-51: may Verify be activated in this tab?
 *
 * For Claude Code the tab must be able to produce a complete configuration —
 * either a token that passes AII-FR-50, or an empty field with a token already
 * in the keychain. Anything else would submit a payload the backend refuses as
 * `token_missing` or `token_malformed`, so the guard is here rather than in the
 * error handler. Every other tab has nothing extra to satisfy.
 */
export function canVerifyAgentic(
  i: AgenticIntegration,
  draft: AgenticDraft,
): boolean {
  if (!rendersOauthTokenField(i)) return true;
  const typed = draft.oauthToken.trim();
  if (typed !== "") return isValidClaudeOauthToken(typed);
  return i.keyState === "set";
}

/**
 * AII-FR-20 / FR-21 / FR-22 / FR-27: the Agentic level's verification status
 * line. Same precedence rules as `apiStatus`, and the same reason for them.
 *
 * Pure and exported so both kinds' precedence is testable directly.
 */
export function agenticStatus(
  integration: AgenticIntegration,
  draft: AgenticDraft,
  busy: Busy,
  error: string,
  detectionFoundNothing: boolean,
  detectedPath: string | null = null,
): Status {
  if (busy === "verifying") return { text: "Verifying…", tone: "muted" };
  if (busy === "detecting") return { text: "Looking for the binary…", tone: "muted" };
  if (error) return { text: error, tone: "warn" };

  if (integration.kind === "cli") {
    const stored = integration.binaryPath ?? "";
    const candidate = draft.path.trim();
    // A newly typed token is an edit like any other: it returns the tab to an
    // unverified presentation until Verify commits it (AII-FR-22).
    const edited =
      candidate !== stored ||
      (rendersOauthTokenField(integration) && draft.oauthToken.trim() !== "");

    if (edited || integration.state === "unconfigured") {
      if (candidate === "") {
        return detectionFoundNothing
          ? {
              text: "No binary found. Enter the path to it, or browse for it.",
              tone: "muted",
            }
          : { text: "No binary configured yet.", tone: "muted" };
      }
      // AII-FR-17: a value detection produced is marked as detected. It is still
      // only a candidate — detection persists nothing — so it says so in the
      // same breath, but the author should not have to wonder where it came from.
      if (detectedPath !== null && candidate === detectedPath.trim()) {
        return { text: "Detected — verify this path before use.", tone: "muted" };
      }
      return { text: "Not verified — verify this path before use.", tone: "muted" };
    }

    // AII-FR-27: a stored configuration that has degraded keeps everything it
    // had and says so, rather than being cleared or silently re-detected.
    if (integration.state === "missing") {
      return {
        text: "The binary is no longer at this path. Verify it again once it is back.",
        tone: "warn",
      };
    }
    if (integration.state === "key_unavailable") {
      return {
        text: "The stored OAuth token can no longer be read. Enter one and verify again.",
        tone: "warn",
      };
    }

    const origin =
      integration.pathOrigin === "detected" ? "detected" : "you supplied this path";
    const version = integration.version ? ` · ${integration.version}` : "";
    return { text: `${origin} · verified${version}`, tone: "ok" };
  }

  // API kind.
  const stored = integration.baseUrl ?? "";
  const urlEdited = draft.baseUrl.trim() !== stored;
  const keyEdited = draft.apiKey.trim() !== "";

  if (urlEdited || keyEdited || integration.state === "unconfigured") {
    if (draft.baseUrl.trim() === "") {
      return { text: "No endpoint configured yet.", tone: "muted" };
    }
    return {
      text: "Not verified — verify this endpoint before use.",
      tone: "muted",
    };
  }

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
 * The **Agentic AI** section of Global settings — the agentic level of
 * AII-FR-01, and the counterpart to {@link AiApiIntegrations}. It shares no
 * state, no request, and no control with that section (AII-FR-02): the two are
 * separate sections precisely so that neither can be mistaken for a
 * continuation of the other.
 */
export function AgenticAiIntegrations() {
  const [integrations, setIntegrations] = useState<AgenticIntegration[] | null>(
    null,
  );
  const [vendor, setVendor] = useState<AgenticVendorId | null>(null);
  const [drafts, setDrafts] = useState<Record<string, AgenticDraft>>({});
  const [configErrors, setConfigErrors] = useState<Record<string, string>>({});
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<Record<string, Busy>>({});
  /** Vendors detection has already run for, so it runs once per tab. */
  const [detected, setDetected] = useState<Record<string, boolean>>({});
  /** What detection returned per vendor, so the field can say so (AII-FR-17). */
  const [detectedPaths, setDetectedPaths] = useState<Record<string, string>>({});
  /** Vendors whose detection came back with nothing (AII-FR-17). */
  const [detectionEmpty, setDetectionEmpty] = useState<Record<string, boolean>>({});
  const [loadError, setLoadError] = useState("");

  // AII-FR-53 / AII-FR-30: neither credential field is ever populated from a
  // record. A stored key or token is represented by its masked hint alone.
  const draftFor = (i: AgenticIntegration): AgenticDraft => ({
    path: i.binaryPath ?? "",
    baseUrl: i.baseUrl ?? defaultAgenticBaseUrl(i.vendor),
    apiKey: "",
    oauthToken: "",
  });

  useEffect(() => {
    let cancelled = false;
    api
      .listAgenticIntegrations()
      .then((list) => {
        if (cancelled) return;
        const safe = Array.isArray(list) ? list : [];
        setIntegrations(safe);
        setDrafts(Object.fromEntries(safe.map((i) => [i.vendor, draftFor(i)])));
        setVendor((current) => current ?? safe[0]?.vendor ?? null);
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

  const current = integrations?.find((i) => i.vendor === vendor) ?? null;

  /** AII-FR-21: leaving a tab discards its unverified candidate. */
  const onSelectVendor = (next: AgenticVendorId) => {
    const leaving = current;
    if (leaving && leaving.vendor !== next) {
      setDrafts((prev) => ({ ...prev, [leaving.vendor]: draftFor(leaving) }));
      setConfigErrors((prev) => ({ ...prev, [leaving.vendor]: "" }));
      setActionErrors((prev) => ({ ...prev, [leaving.vendor]: "" }));
      // With no stored path to fall back to, detection is re-armed so
      // re-entering the tab offers its suggestion again.
      if (leaving.kind === "cli" && !leaving.binaryPath) {
        setDetected((prev) => ({ ...prev, [leaving.vendor]: false }));
        setDetectionEmpty((prev) => ({ ...prev, [leaving.vendor]: false }));
      }
    }
    setVendor(next);
  };

  const replace = useCallback((updated: AgenticIntegration) => {
    setIntegrations((prev) =>
      (prev ?? []).map((i) => (i.vendor === updated.vendor ? updated : i)),
    );
  }, []);

  // AII-FR-17: a CLI tab with no stored path asks the backend to find one, once.
  // Detection persists nothing, so a value it produces is still only a candidate.
  useEffect(() => {
    if (!current || current.kind !== "cli" || detected[current.vendor]) return;
    if (current.binaryPath) return;
    const target = current.vendor;
    setDetected((prev) => ({ ...prev, [target]: true }));
    setBusy((prev) => ({ ...prev, [target]: "detecting" }));
    api
      .detectAgenticCliBinary(target)
      .then((found) => {
        const path = found?.path ?? null;
        if (path) {
          setDrafts((prev) => ({
            ...prev,
            [target]: { ...(prev[target] ?? emptyDraft()), path },
          }));
          setDetectedPaths((prev) => ({ ...prev, [target]: path }));
        } else {
          setDetectionEmpty((prev) => ({ ...prev, [target]: true }));
        }
      })
      .catch(() => {
        // Detection is a convenience; its failure is the same outcome as finding
        // nothing, and never an error the author must clear.
        setDetectionEmpty((prev) => ({ ...prev, [target]: true }));
      })
      .finally(() => setBusy((prev) => ({ ...prev, [target]: "idle" })));
  }, [current, detected]);

  // AII-FR-22: editing returns the tab to an unverified presentation without
  // invoking anything.
  const onEditDraft = (target: AgenticVendorId, patch: Partial<AgenticDraft>) => {
    setDrafts((prev) => ({
      ...prev,
      [target]: { ...(prev[target] ?? emptyDraft()), ...patch },
    }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
  };

  const onBrowse = async (target: AgenticVendorId) => {
    try {
      const result = await api.browseForFile();
      if (result !== "cancelled" && result?.selected?.path) {
        onEditDraft(target, { path: result.selected.path });
      }
    } catch (e) {
      setConfigErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    }
  };

  // AII-FR-20 / FR-21: the explicit action, and the only one that commits. The
  // config carries the fields of the tab's own kind and no others (AII-FR-16).
  const onVerify = async (target: AgenticVendorId, kind: "cli" | "api") => {
    setBusy((prev) => ({ ...prev, [target]: "verifying" }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const draft = drafts[target] ?? emptyDraft();
      const key = draft.apiKey.trim();
      const token = draft.oauthToken.trim();
      // AIC-FR-26: `{ path, oauthToken }` when the author supplied a new token,
      // `{ path }` when they are keeping the stored one — which the section can
      // send without ever having read it.
      const config =
        kind === "cli"
          ? token === ""
            ? { path: draft.path }
            : { path: draft.path, oauthToken: token }
          : { baseUrl: draft.baseUrl, apiKey: key === "" ? null : key };
      const updated = await api.verifyAgenticIntegration(target, config);
      replace(updated);
      // Clears the token field along with the rest of the candidate: the
      // submission is over, so the value has no reason to still be here
      // (AII-FR-53).
      setDrafts((prev) => ({ ...prev, [target]: draftFor(updated) }));
    } catch (e) {
      setConfigErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  // AII-FR-23: selections apply at once. A null `turnKind` sets the default
  // every kind falls back to; a named one sets that kind's own model, and a null
  // `modelId` beside it returns that kind to the default. AII-FR-28: a rejected
  // update leaves the previous value on screen and renders the typed error
  // inline — `replace` is reached on success alone, so a row the backend refused
  // still reads what it read before.
  const onSelectModel = async (
    target: AgenticVendorId,
    turnKind: AgenticTurnKind | null,
    modelId: string | null,
  ) => {
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      replace(await api.setAgenticIntegrationModel(target, turnKind, modelId));
    } catch (e) {
      // The author sees the message inline and reads no further, so without
      // this the refusal leaves nothing behind for whoever is asked later why a
      // turn ran on the wrong model. The kind is what makes the record useful:
      // it says which row of the per-task section was refused.
      logWarn(["frontend"], "agentic model selection refused", {
        vendor: target,
        turnKind: turnKind ?? "default",
        reason: typedFailureCode(e),
      });
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    }
  };

  // AII-FR-24: a null `turnKind` sets the default every kind falls back to; a
  // named one sets that kind's own effort, and a null `effortId` beside it
  // returns that kind to the default.
  const onSelectEffort = async (
    target: AgenticVendorId,
    turnKind: AgenticTurnKind | null,
    effortId: string | null,
  ) => {
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      replace(await api.setAgenticIntegrationEffort(target, turnKind, effortId));
    } catch (e) {
      logWarn(["frontend"], "agentic effort selection refused", {
        vendor: target,
        turnKind: turnKind ?? "default",
        reason: typedFailureCode(e),
      });
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    }
  };

  // AII-FR-25: exactly one integration is active in this level, across kinds.
  const onActivate = async (target: AgenticVendorId) => {
    setBusy((prev) => ({ ...prev, [target]: "working" }));
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const list = await api.setActiveAgenticIntegration(target);
      setIntegrations(Array.isArray(list) ? list : []);
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  // AII-FR-26: back to the unconfigured presentation.
  const onClear = async (target: AgenticVendorId) => {
    setBusy((prev) => ({ ...prev, [target]: "working" }));
    setActionErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const list = await api.clearAgenticIntegration(target);
      setIntegrations(Array.isArray(list) ? list : []);
      setDrafts((prev) => ({
        ...prev,
        [target]: {
          path: "",
          baseUrl: defaultAgenticBaseUrl(target),
          apiKey: "",
          oauthToken: "",
        },
      }));
      setConfigErrors((prev) => ({ ...prev, [target]: "" }));
      // Detection is deliberately suppressed rather than re-armed. AII-FR-26
      // returns the tab to a presentation with no path, and re-detecting would
      // immediately refill the field the author just emptied — undoing the
      // action they took.
      setDetected((prev) => ({ ...prev, [target]: true }));
      setDetectedPaths((prev) => ({ ...prev, [target]: "" }));
      setDetectionEmpty((prev) => ({ ...prev, [target]: false }));
    } catch (e) {
      setActionErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  /** AII-FR-23: the vendor's options, preceded by the backend-default entry. */
  const agenticModelOptions = useMemo<FilterableOption[]>(
    () => [
      { id: "", label: BACKEND_DEFAULT_LABEL },
      ...(current?.models ?? []).map((m) => ({ id: m.id, label: m.label })),
    ],
    [current?.models],
  );

  /**
   * AII-FR-23: the same options a per-task row offers — the selector's list,
   * with "same as default" in place of the backend-default entry, because a row
   * that follows the selector above it is following a choice rather than making
   * the backend's own.
   */
  const agenticTaskModelOptions = useMemo<FilterableOption[]>(
    () => [
      { id: "", label: SAME_AS_DEFAULT_LABEL },
      ...(current?.models ?? []).map((m) => ({ id: m.id, label: m.label })),
    ],
    [current?.models],
  );

  const vendorBusy = current ? (busy[current.vendor] ?? "idle") : "idle";
  const draft = current ? (drafts[current.vendor] ?? emptyDraft()) : emptyDraft();
  const hasTokenField = current ? rendersOauthTokenField(current) : false;
  const tokenValidation = hasTokenField
    ? oauthTokenValidation(draft.oauthToken)
    : "";
  const status = current
    ? agenticStatus(
        current,
        draft,
        vendorBusy === "working" ? "idle" : vendorBusy,
        configErrors[current.vendor] ?? "",
        detectionEmpty[current.vendor] ?? false,
        detectedPaths[current.vendor] ?? null,
      )
    : null;

  return (
    <section data-testid="agentic-level">
      <p className="t-p" style={{ marginBottom: 16 }}>
        The agent backends Synthesis hands work to — a coding-agent CLI installed
        here, or an agent-execution endpoint it reaches over the network.
        Configure as many as you like; one is active at a time, and a project can
        override that choice in its own settings.
      </p>

      {integrations === null && <div className="t-muted">Loading…</div>}

      {/* AII-FR-16: one strip, both kinds. */}
      <div
        role="tablist"
        aria-label="Agentic AI"
        style={SETTINGS_TABLIST_STYLE}
      >
        {(integrations ?? []).map((i) => (
          <button
            key={i.vendor}
            role="tab"
            aria-selected={i.vendor === vendor}
            className="btn btn--ghost btn--sm"
            data-active={i.vendor === vendor}
            data-kind={i.kind}
            style={settingsTabStyle(i.vendor === vendor)}
            onClick={() => onSelectVendor(i.vendor)}
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

      {current && status && (
        <div
          data-testid={`agentic-panel-${current.vendor}`}
          role="tabpanel"
          aria-label={current.displayName}
        >
          {current.active && (
            <div
              className="badge badge--ok"
              style={{ marginBottom: 14 }}
              data-testid="agentic-active-marker"
            >
              ● Active
            </div>
          )}

          {/* AII-FR-16: a tab renders the configuration fields of its own kind
              and no others, in the same position either way. */}
          {current.kind === "cli" ? (
            <>
              <div
                className="picker-field"
                style={{ marginBottom: hasTokenField ? 10 : 4 }}
              >
                <label
                  className="picker-field__label"
                  htmlFor={`agentic-path-${current.vendor}`}
                >
                  Binary
                </label>
                {/* AII-FR-16: Verify occupies the same position in every tab,
                    so this row centres its controls exactly as the token row
                    and the API key row do. Without it Verify sits 3px higher
                    here than on the Claude Code tab. */}
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <input
                    id={`agentic-path-${current.vendor}`}
                    className="input input--mono"
                    spellCheck={false}
                    autoComplete="off"
                    placeholder={`path to the ${current.displayName} CLI`}
                    value={draft.path}
                    onChange={(e) =>
                      onEditDraft(current.vendor, { path: e.target.value })
                    }
                    style={{ flex: 1 }}
                  />
                  {/* AII-FR-18: a path of the author's own, typed or picked. */}
                  <button
                    className="btn btn--ghost btn--sm"
                    onClick={() => void onBrowse(current.vendor)}
                  >
                    Browse…
                  </button>
                  {/* AII-FR-49: where a token field follows, Verify sits on it
                      instead, so the path and the credential are submitted as
                      one act rather than two. */}
                  {!hasTokenField && (
                    <button
                      className="btn btn--default btn--sm"
                      disabled={
                        vendorBusy === "verifying" || vendorBusy === "detecting"
                      }
                      onClick={() => void onVerify(current.vendor, "cli")}
                    >
                      {vendorBusy === "verifying" ? "Verifying…" : "Verify"}
                    </button>
                  )}
                </div>
              </div>

              {/* AII-FR-49: the OAuth token field, rendered by this tab alone.
                  A password-style input that is never populated from the
                  record — a stored token is represented by its masked hint. */}
              {hasTokenField && (
                <div className="picker-field" style={{ marginBottom: 4 }}>
                  <label
                    className="picker-field__label"
                    htmlFor={`agentic-token-${current.vendor}`}
                  >
                    OAuth token
                  </label>
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    <input
                      id={`agentic-token-${current.vendor}`}
                      type="password"
                      className="input input--mono"
                      data-testid="agentic-oauth-token"
                      spellCheck={false}
                      autoComplete="off"
                      aria-invalid={tokenValidation !== "" || undefined}
                      aria-describedby={`agentic-token-note-${current.vendor}`}
                      // Keyed on `keyState` alone, exactly as the note below
                      // is: a record reporting a stored token with no hint to
                      // describe it still has one, and a placeholder saying
                      // "required" beside a note saying "leave empty" would
                      // have the tab contradicting itself.
                      placeholder={
                        current.keyState === "set"
                          ? current.maskedHint
                            ? `•••• ${current.maskedHint}`
                            : "a token is stored"
                          : "required"
                      }
                      value={draft.oauthToken}
                      onChange={(e) =>
                        onEditDraft(current.vendor, {
                          oauthToken: e.target.value,
                        })
                      }
                      style={{ flex: 1 }}
                    />
                    <button
                      className="btn btn--default btn--sm"
                      // AII-FR-51: unavailable until the tab can produce a
                      // complete configuration, so nothing the backend would
                      // refuse as token_missing or token_malformed is ever sent.
                      disabled={
                        vendorBusy === "verifying" ||
                        vendorBusy === "detecting" ||
                        !canVerifyAgentic(current, draft)
                      }
                      onClick={() => void onVerify(current.vendor, "cli")}
                    >
                      {vendorBusy === "verifying" ? "Verifying…" : "Verify"}
                    </button>
                  </div>
                  <div
                    id={`agentic-token-note-${current.vendor}`}
                    className="t-ui-xs"
                    data-testid="agentic-token-note"
                    // The same two tones the verification status line uses, via
                    // the same function — a token spelled by hand here would
                    // resolve to nothing and silently inherit.
                    style={{
                      marginTop: 4,
                      color: toneColor(tokenValidation ? "warn" : "muted"),
                    }}
                  >
                    {/* AII-FR-50: structural only, and quoting nothing the
                        author typed. AII-FR-52: an empty field is not an error
                        where a token is already held — it re-verifies with it. */}
                    {tokenValidation ||
                      (current.keyState === "set"
                        ? "Leave empty to re-verify with the stored token."
                        : `Required — ${current.displayName} runs where it cannot sign in for itself.`)}
                  </div>
                </div>
              )}
            </>
          ) : (
            <>
              {/* AII-FR-19: prefilled for the named vendor, empty for Custom
                  agent API, which is how a self-hosted deployment is pointed at. */}
              <div className="picker-field" style={{ marginBottom: 10 }}>
                <label
                  className="picker-field__label"
                  htmlFor={`agentic-url-${current.vendor}`}
                >
                  Base URL
                </label>
                <input
                  id={`agentic-url-${current.vendor}`}
                  className="input input--mono"
                  spellCheck={false}
                  autoComplete="off"
                  placeholder={
                    defaultAgenticBaseUrl(current.vendor) ||
                    "https://your-agent.example/v1"
                  }
                  value={draft.baseUrl}
                  onChange={(e) =>
                    onEditDraft(current.vendor, { baseUrl: e.target.value })
                  }
                />
              </div>
              <div className="picker-field" style={{ marginBottom: 4 }}>
                <label
                  className="picker-field__label"
                  htmlFor={`agentic-key-${current.vendor}`}
                >
                  API key
                </label>
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <input
                    id={`agentic-key-${current.vendor}`}
                    type="password"
                    className="input input--mono"
                    spellCheck={false}
                    autoComplete="off"
                    placeholder={
                      current.maskedHint
                        ? `•••• ${current.maskedHint}`
                        : current.keyRequired
                          ? "required"
                          : "optional"
                    }
                    value={draft.apiKey}
                    onChange={(e) =>
                      onEditDraft(current.vendor, { apiKey: e.target.value })
                    }
                    style={{ flex: 1 }}
                  />
                  <button
                    className="btn btn--default btn--sm"
                    disabled={vendorBusy === "verifying"}
                    onClick={() => void onVerify(current.vendor, "api")}
                  >
                    {vendorBusy === "verifying" ? "Verifying…" : "Verify"}
                  </button>
                </div>
              </div>
            </>
          )}

          <div
            className="t-ui-sm"
            data-testid="agentic-status"
            style={{ marginBottom: 18, color: toneColor(status.tone) }}
          >
            {status.text}
          </div>

          {/* AII-FR-23: the same filterable selector the API level renders
              (AII-FR-36), so nothing about choosing a model is learned twice. */}
          <div className="picker-field" style={{ marginBottom: 4 }}>
            <label className="picker-field__label">Model</label>
            <FilterableSelect
              label="Model"
              testId="agentic-model"
              value={current.selectedModel ?? ""}
              options={agenticModelOptions}
              onChange={(id) => void onSelectModel(current.vendor, null, id || null)}
            />
          </div>
          <div
            className="t-ui-xs t-muted"
            data-testid="agentic-models-origin"
            style={{ marginBottom: 4 }}
          >
            {modelsOriginLabel(
              current.modelsOrigin,
              current.kind === "cli" ? "the installed CLI" : "the endpoint",
            )}
          </div>

          {/* AII-FR-24: a vendor that declares no effort levels renders no
              effort selector at all. */}
          {current.reasoningEfforts.length > 0 && (
            <div className="picker-field" style={{ marginBottom: 4 }}>
              <label
                className="picker-field__label"
                htmlFor={`agentic-effort-${current.vendor}`}
              >
                Reasoning effort
              </label>
              <select
                id={`agentic-effort-${current.vendor}`}
                className="select"
                value={current.selectedEffort ?? ""}
                onChange={(e) =>
                  void onSelectEffort(current.vendor, null, e.target.value || null)
                }
              >
                <option value="">{BACKEND_DEFAULT_LABEL}</option>
                {current.reasoningEfforts.map((eff) => (
                  <option key={eff.id} value={eff.id}>
                    {eff.label}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* AII-FR-QDLW: one section below both defaults, holding one row per
              kind of work a run hands to this backend. Every agentic tab
              renders it, whichever vendor the tab is for; a vendor that
              declares no effort levels simply has no effort column in it. */}
          <div style={{ marginBottom: 18 }}>
            <PerTaskSection
              // A record written before any kind was distinguished carries no
              // map at all, which reads back as one that follows the default
              // everywhere rather than as a tab that cannot render.
              modelOverrides={current.modelOverrides ?? {}}
              effortOverrides={current.effortOverrides ?? {}}
              hasEffortColumn={current.reasoningEfforts.length > 0}
              renderModel={(kind) => (
                <FilterableSelect
                  // The accessible name carries the row's own label and the
                  // dimension it selects: two comboboxes named "Review" in one
                  // panel would be one control to anybody who cannot see which
                  // column they are standing in.
                  label={`${kind.label} model`}
                  testId={`agentic-model-${kind.id}`}
                  value={(current.modelOverrides ?? {})[kind.id] ?? ""}
                  options={agenticTaskModelOptions}
                  onChange={(id) =>
                    void onSelectModel(current.vendor, kind.id, id || null)
                  }
                />
              )}
              renderEffort={(kind) => (
                <select
                  id={`agentic-effort-${current.vendor}-${kind.id}`}
                  className="select"
                  aria-label={`${kind.label} effort`}
                  value={(current.effortOverrides ?? {})[kind.id] ?? ""}
                  onChange={(e) =>
                    void onSelectEffort(
                      current.vendor,
                      kind.id,
                      e.target.value || null,
                    )
                  }
                >
                  <option value="">{SAME_AS_DEFAULT_LABEL}</option>
                  {current.reasoningEfforts.map((eff) => (
                    <option key={eff.id} value={eff.id}>
                      {eff.label}
                    </option>
                  ))}
                </select>
              )}
            />
          </div>

          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            {!current.active && (
              <button
                className="btn btn--primary btn--sm"
                // Only the integration's own state gates this. The level stays
                // interactive while a verification is in flight.
                disabled={current.state !== "verified"}
                onClick={() => void onActivate(current.vendor)}
              >
                Use this integration
              </button>
            )}
            {/* Offered whenever there is anything to clear — which includes a
                keychain entry the registry has no record of. Gating on `state`
                alone would hide the only control that can purge a stored
                credential from a tab that reads `unconfigured`, leaving the
                author no way to reach it short of the OS keychain. */}
            {(current.state !== "unconfigured" ||
              current.keyState !== "unset") && (
              <button
                className="btn btn--ghost btn--sm"
                onClick={() => void onClear(current.vendor)}
              >
                Clear
              </button>
            )}
          </div>

          {actionErrors[current.vendor] && (
            <span
              className="picker-error"
              style={{ display: "block", marginTop: 10 }}
              data-testid="agentic-action-error"
            >
              ✗ {actionErrors[current.vendor]}
            </span>
          )}

          {current.state === "unconfigured" && (
            <div
              className="t-ui-sm t-muted"
              data-testid="agentic-empty"
              style={{ marginTop: 14 }}
            >
              {current.displayName} is not set up yet.{" "}
              {current.kind === "cli"
                ? hasTokenField
                  ? "Point Synthesis at its binary, give it an OAuth token, and verify to use this integration."
                  : "Point Synthesis at its binary and verify it to use this integration."
                : "Give Synthesis its endpoint and verify it to use this integration."}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function emptyDraft(): AgenticDraft {
  return { path: "", baseUrl: "", apiKey: "", oauthToken: "" };
}

function toneColor(tone: Status["tone"]): string {
  return tone === "ok"
    ? "var(--ok, var(--accent))"
    : tone === "warn"
      ? "var(--danger, var(--fg-2))"
      : "var(--fg-3)";
}

// ---------------------------------------------------------------------------
// The Project settings controls
// ---------------------------------------------------------------------------

/**
 * One resolved-integration line: what the project resolves to, how it resolved,
 * and one action to change it (AII-FR-31..35).
 *
 * It renders no URL, no key, no masked hint, no path, no model list, and no
 * verification state — those belong to the Global settings section, and
 * repeating them here would invite editing them from a surface that does not own
 * them (AII-FR-35).
 */
function ProjectIntegrationControl<Id extends string>(props: {
  title: string;
  blurb: string;
  testId: string;
  /**
   * The Global settings section that configures this level, named verbatim so
   * the pointer an author is given is one they can follow. The two levels live
   * in two sections (GLS-FR-16), so "Global settings → AI integrations" would
   * now send them somewhere that does not exist.
   */
  settingsSection: string;
  /** The id in effect, the recorded override, and how it resolved. */
  resolved: {
    id: Id | null;
    overrideId: Id | null;
    resolution: string;
  } | null;
  /** Choosable options: configured *and* verified only (AII-FR-32). */
  choosable: { id: Id; displayName: string }[];
  nameOf: (id: Id | null) => string;
  onChoose: (id: Id | null) => void;
  error: string;
}) {
  const { resolved, choosable, nameOf } = props;
  const [choosing, setChoosing] = useState(false);

  return (
    <div
      className="card"
      style={{ padding: "14px 16px", marginBottom: 16 }}
      data-testid={props.testId}
    >
      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
        {props.title}
      </div>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
        {props.blurb}
      </div>

      {resolved === null && !props.error && (
        <div className="t-muted t-ui-sm">Loading…</div>
      )}

      {resolved && (
        <div data-testid={`${props.testId}-resolution`}>
          {resolved.resolution === "inherited" && (
            <div style={{ fontSize: "var(--fs-ui-md)", color: "var(--fg-1)" }}>
              {nameOf(resolved.id)}
              <span className="t-ui-sm t-muted">
                {" "}
                · inherited from your global choice
              </span>
            </div>
          )}
          {resolved.resolution === "overridden" && (
            <div style={{ fontSize: "var(--fs-ui-md)", color: "var(--fg-1)" }}>
              {nameOf(resolved.id)}
              <span className="t-ui-sm t-muted"> · overrides the global choice</span>
            </div>
          )}
          {/* AII-FR-33: a dead override is named as dead, and what is actually in
              effect is named beside it — presenting it as the project's choice
              would be a lie the author acts on. */}
          {resolved.resolution === "override_unavailable" && (
            <div className="t-ui-sm">
              {nameOf(resolved.overrideId)} was chosen for this project but is no
              longer available.{" "}
              {resolved.id
                ? `${nameOf(resolved.id)} is in effect instead.`
                : "Nothing is in effect."}
            </div>
          )}
          {resolved.resolution === "none_selected" && (
            <div className="t-ui-sm">
              Nothing is active. Choose one for this project, or activate one in
              Global settings → {props.settingsSection}.
            </div>
          )}
          {/* AII-FR-34: nothing configured at all names where one is set up,
              exactly as the GitHub token control names its own section. */}
          {resolved.resolution === "none_configured" && (
            <div className="t-ui-sm" data-testid={`${props.testId}-none`}>
              None is configured. Set one up in Global settings →{" "}
              {props.settingsSection}.
            </div>
          )}
        </div>
      )}

      {/* Nothing to choose between until at least one integration verifies. */}
      {resolved && choosable.length > 0 && !choosing && (
        <button
          className="btn btn--ghost btn--sm btn--inline-start"
          style={{ marginTop: 10 }}
          onClick={() => setChoosing(true)}
        >
          Change…
        </button>
      )}

      {choosing && (
        <select
          className="select"
          style={{ marginTop: 10 }}
          aria-label={props.title}
          defaultValue={resolved?.overrideId ?? ""}
          onChange={(e) => {
            setChoosing(false);
            props.onChoose((e.target.value || null) as Id | null);
          }}
        >
          {/* AII-FR-32: the entry meaning "inherit the global choice", which
              clears the override rather than naming one. */}
          <option value="">Inherit the global choice</option>
          {choosable.map((i) => (
            <option key={i.id} value={i.id}>
              {i.displayName}
            </option>
          ))}
        </select>
      )}

      {props.error && (
        <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
          ✗ {props.error}
        </span>
      )}
    </div>
  );
}

/**
 * AII-FR-31..35: the Project section's two lines — one per level — naming the
 * integration the open project resolves to and how it resolved, each with the
 * action that changes it.
 *
 * Two independent controls rather than one combined choice, because the levels
 * are chosen independently: overriding one leaves the other's resolution
 * untouched (AII-FR-32).
 */
export function ProjectAiIntegrations() {
  const [agentic, setAgentic] = useState<ProjectAgenticIntegration | null>(null);
  const [apiResolved, setApiResolved] = useState<ProjectAiApiIntegration | null>(
    null,
  );
  const [agenticList, setAgenticList] = useState<AgenticIntegration[]>([]);
  const [apiList, setApiList] = useState<AiApiIntegration[]>([]);
  const [agenticError, setAgenticError] = useState("");
  const [apiError, setApiError] = useState("");

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      api.getProjectAgenticIntegration(),
      api.listAgenticIntegrations(),
    ])
      .then(([current, list]) => {
        if (cancelled) return;
        setAgentic(current ?? null);
        setAgenticList(Array.isArray(list) ? list : []);
        setAgenticError("");
      })
      .catch((e) => {
        if (cancelled) return;
        setAgentic(null);
        setAgenticError(aiErrorMessage(e));
      });
    void Promise.all([
      api.getProjectAiApiIntegration(),
      api.listAiApiIntegrations(),
    ])
      .then(([current, list]) => {
        if (cancelled) return;
        setApiResolved(current ?? null);
        setApiList(Array.isArray(list) ? list : []);
        setApiError("");
      })
      .catch((e) => {
        if (cancelled) return;
        setApiResolved(null);
        setApiError(aiErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onChooseAgentic = async (vendor: AgenticVendorId | null) => {
    setAgenticError("");
    try {
      setAgentic((await api.setProjectAgenticIntegration(vendor)) ?? null);
    } catch (e) {
      setAgenticError(aiErrorMessage(e));
    }
  };

  const onChooseApi = async (provider: AiApiProviderId | null) => {
    setApiError("");
    try {
      setApiResolved((await api.setProjectAiApiIntegration(provider)) ?? null);
      // AII-FR-54: the project override decides which provider serves agents.
      notifyAgentRegistryChanged();
    } catch (e) {
      setApiError(aiErrorMessage(e));
    }
  };

  return (
    <>
      <ProjectIntegrationControl<AgenticVendorId>
        title="Agentic integration"
        blurb="The agent backend this project hands work to."
        testId="settings-agentic-integration"
        settingsSection="Agentic AI"
        resolved={
          agentic && {
            id: agentic.vendor,
            overrideId: agentic.overrideVendor,
            resolution: agentic.resolution,
          }
        }
        choosable={agenticList
          .filter((i) => i.state === "verified")
          .map((i) => ({ id: i.vendor, displayName: i.displayName }))}
        nameOf={(id) =>
          agenticList.find((i) => i.vendor === id)?.displayName ?? id ?? ""
        }
        onChoose={(id) => void onChooseAgentic(id)}
        error={agenticError}
      />
      <ProjectIntegrationControl<AiApiProviderId>
        title="AI API integration"
        blurb="The API endpoint this project calls."
        testId="settings-ai-api-integration"
        settingsSection="AI API"
        resolved={
          apiResolved && {
            id: apiResolved.provider,
            overrideId: apiResolved.overrideProvider,
            resolution: apiResolved.resolution,
          }
        }
        choosable={apiList
          .filter((i) => i.state === "verified")
          .map((i) => ({ id: i.provider, displayName: i.displayName }))}
        nameOf={(id) =>
          apiList.find((i) => i.provider === id)?.displayName ?? id ?? ""
        }
        onChoose={(id) => void onChooseApi(id)}
        error={apiError}
      />
    </>
  );
}
