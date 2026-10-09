import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import * as api from "../api";
import {
  type AgenticTurnKind,
  type AgenticIntegration,
  type AgenticVendorId,
  type AgenticVerifyConfig,
} from "../types";
import { logInfo, logWarn } from "../logging";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { aiErrorMessage, isGatewayCheckFailure } from "./aiErrorMessage";
import { GatewayCheckFailedDialog } from "./GatewayCheckFailedDialog";
import { FilterableSelect, type FilterableOption } from "./FilterableSelect";
import {
  BACKEND_DEFAULT_LABEL,
  SAME_AS_DEFAULT_LABEL,
  modelsOriginLabel,
  toneColor,
  typedFailureCode,
  type Busy,
} from "./aiIntegrationsShared";
import { PerTaskSection } from "./AgenticPerTaskSection";
import { ClaudeAuthTabs, ClaudeCredentialRows } from "./ClaudeCodeAuthFields";
import {
  agenticStatus,
  cliConfigFor,
  defaultAgenticBaseUrl,
  draftForIntegration,
  emptyDraft,
  rendersOauthTokenField,
  type AgenticDraft,
} from "./agenticDraft";

/**
 * The **Agentic AI** level of `specifications/ui/AII-ai-integrations.md`
 * (AII-FR-16 .. AII-FR-30, AII-FR-QDLW, AII-FR-49 .. AII-FR-53). It shares no
 * state, no request, and no control with the AI API level (AII-FR-02).
 */

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
  /**
   * AII-FR-ZQTB: the gateway verification whose check failed, while the author
   * decides whether to accept the gateway anyway. It holds a typed token only
   * until that decision (AII-FR-53).
   */
  const [gatewayRetry, setGatewayRetry] = useState<{
    target: AgenticVendorId;
    config: AgenticVerifyConfig;
    message: string;
  } | null>(null);
  /** Vendors detection has already run for, so it runs once per tab. */
  const [detected, setDetected] = useState<Record<string, boolean>>({});
  /** What detection returned per vendor, so the field can say so (AII-FR-17). */
  const [detectedPaths, setDetectedPaths] = useState<Record<string, string>>({});
  /** Vendors whose detection came back with nothing (AII-FR-17). */
  const [detectionEmpty, setDetectionEmpty] = useState<Record<string, boolean>>({});
  const [loadError, setLoadError] = useState("");

  // AII-FR-53 / AII-FR-30: neither credential field is ever populated from a
  // record. A stored key or token is represented by its masked hint alone.
  const draftFor = draftForIntegration;

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

  // AII-FR-20 / FR-21: the explicit action, and the only one that commits.
  // Resolves with the rejection, or with null on success.
  const submitVerify = async (
    target: AgenticVendorId,
    config: AgenticVerifyConfig,
  ): Promise<{ error: unknown } | null> => {
    setBusy((prev) => ({ ...prev, [target]: "verifying" }));
    setConfigErrors((prev) => ({ ...prev, [target]: "" }));
    try {
      const updated = await api.verifyAgenticIntegration(target, config);
      replace(updated);
      // Clears the token field along with the rest of the candidate: the
      // submission is over, so the value has no reason to still be here
      // (AII-FR-53).
      setDrafts((prev) => ({ ...prev, [target]: draftFor(updated) }));
      return null;
    } catch (e) {
      setConfigErrors((prev) => ({ ...prev, [target]: aiErrorMessage(e) }));
      return { error: e };
    } finally {
      setBusy((prev) => ({ ...prev, [target]: "idle" }));
    }
  };

  // The config carries the fields of the tab's own kind and no others
  // (AII-FR-16).
  const onVerify = async (target: AgenticVendorId, kind: "cli" | "api") => {
    const draft = drafts[target] ?? emptyDraft();
    const key = draft.apiKey.trim();
    const stored = integrations?.find((i) => i.vendor === target);
    // AIC-FR-26: `{ path, oauthToken }` when the author supplied a new token,
    // `{ path }` when they are keeping the stored one — which the section can
    // send without ever having read it. Claude Code adds the open sub-tab's
    // shape (AII-FR-IUUM, AII-FR-DKDC).
    const config: AgenticVerifyConfig =
      kind === "cli"
        ? cliConfigFor(stored, draft)
        : { baseUrl: draft.baseUrl, apiKey: key === "" ? null : key };
    const failure = await submitVerify(target, config);
    // AII-FR-ZQTB: a failed gateway check, and only that failure, asks the
    // author whether to accept the gateway without the check.
    if (
      failure &&
      config.authMode === "custom_gateway" &&
      isGatewayCheckFailure(failure.error)
    ) {
      logInfo(["frontend"], "gateway check failed; asking the author", {
        vendor: target,
      });
      setGatewayRetry({ target, config, message: aiErrorMessage(failure.error) });
    }
  };

  // AII-FR-ZQTB: Accept anyway sends the same payload with the check skipped
  // (AIC-FR-KWMV). Every other route invokes nothing and keeps the failure.
  const onGatewayRetrySettle = (accept: boolean) => {
    const pending = gatewayRetry;
    setGatewayRetry(null);
    if (!pending) return;
    // Focus goes back to the row the author was working on, and not to the
    // start of the window.
    document.getElementById(`agentic-gateway-token-${pending.target}`)?.focus();
    logInfo(["frontend"], accept ? "gateway accepted without the check" : "gateway check failure kept", {
      vendor: pending.target,
    });
    if (accept) {
      void submitVerify(pending.target, { ...pending.config, skipGatewayCheck: true });
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
        [target]: { ...emptyDraft(), baseUrl: defaultAgenticBaseUrl(target) },
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
              {/* AII-FR-IUUM: Claude Code chooses between its two credential
                  shapes above the binary field. */}
              {hasTokenField && (
                <ClaudeAuthTabs
                  mode={draft.authMode}
                  // AII-FR-53: a token typed under the sub-tab being left
                  // goes with it, so neither field outlives the view it was
                  // typed in.
                  onChoose={(authMode) =>
                    onEditDraft(
                      current.vendor,
                      authMode === "custom_gateway"
                        ? { authMode, oauthToken: "" }
                        : { authMode, gatewayToken: "" },
                    )
                  }
                />
              )}
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

              {/* AII-FR-49 .. AII-FR-53, AII-FR-PHFX, AII-FR-EJMG: the open
                  sub-tab's credential row and the shared environment field.
                  Rendered by this tab alone. */}
              {hasTokenField && (
                <ClaudeCredentialRows
                  integration={current}
                  draft={draft}
                  busy={vendorBusy === "working" ? "idle" : vendorBusy}
                  onEdit={(patch) => onEditDraft(current.vendor, patch)}
                  onVerify={() => void onVerify(current.vendor, "cli")}
                />
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
              current.keyState !== "unset" ||
              (current.gatewayKeyState ?? "unset") !== "unset") && (
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
                  ? "Point Synthesis at its binary, give it an OAuth token or a gateway, and verify to use this integration."
                  : "Point Synthesis at its binary and verify it to use this integration."
                : "Give Synthesis its endpoint and verify it to use this integration."}
            </div>
          )}
        </div>
      )}
      {gatewayRetry && (
        <GatewayCheckFailedDialog
          message={gatewayRetry.message}
          onSettle={onGatewayRetrySettle}
        />
      )}
    </section>
  );
}
