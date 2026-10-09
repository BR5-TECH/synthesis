import { useEffect, useState } from "react";
import * as api from "../api";
import {
  type AgenticIntegration,
  type AgenticVendorId,
  type AiApiIntegration,
  type AiApiProviderId,
  type ProjectAgenticIntegration,
  type ProjectAiApiIntegration,
} from "../types";
import { aiErrorMessage } from "./aiErrorMessage";
import { notifyAgentRegistryChanged } from "../state/agentRegistry";

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
