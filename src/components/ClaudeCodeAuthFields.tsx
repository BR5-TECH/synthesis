import type { ReactNode } from "react";
import type { AgenticIntegration, ClaudeAuthMode } from "../types";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { toneColor, type Busy } from "./aiIntegrationsShared";
import { canVerifyAgentic, oauthTokenValidation, type AgenticDraft } from "./agenticDraft";
import {
  gatewayTokenValidation,
  gatewayTokenVarValidation,
  gatewayUrlValidation,
  parseEnvText,
} from "./claudeGateway";

/**
 * The Claude Code tab's two sub-tabs and the credential rows under them —
 * `specifications/ui/AII-ai-integrations.md` AII-FR-IUUM, AII-FR-49 through
 * AII-FR-53, AII-FR-PHFX, AII-FR-EJMG, AII-FR-HOKG.
 *
 * Choosing a sub-tab only changes the candidate in `draft.authMode`. Nothing is
 * committed until Verify succeeds, and Verify submits the open sub-tab's fields
 * and no other's (AII-FR-IUUM).
 */

const SUB_TABS: ReadonlyArray<{ mode: ClaudeAuthMode; label: string }> = [
  { mode: "subscription", label: "Subscription" },
  { mode: "custom_gateway", label: "Custom Gateway" },
];

/** AII-FR-IUUM: the strip that chooses between the two credential shapes. */
export function ClaudeAuthTabs({
  mode,
  onChoose,
}: {
  mode: ClaudeAuthMode;
  onChoose: (mode: ClaudeAuthMode) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="Claude Code authentication"
      style={{ ...SETTINGS_TABLIST_STYLE, marginBottom: 14 }}
    >
      {SUB_TABS.map((tab) => (
        <button
          key={tab.mode}
          role="tab"
          aria-selected={tab.mode === mode}
          className="btn btn--ghost btn--sm"
          data-testid={`agentic-auth-tab-${tab.mode}`}
          style={settingsTabStyle(tab.mode === mode)}
          onClick={() => onChoose(tab.mode)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

/** One labelled text field in the shape the other agentic fields use. */
function Field({
  id,
  label,
  note,
  noteId,
  noteTestId,
  warn,
  marginBottom = 10,
  children,
}: {
  id: string;
  label: string;
  note?: string;
  noteId?: string;
  noteTestId?: string;
  warn?: boolean;
  marginBottom?: number;
  children: ReactNode;
}) {
  return (
    <div className="picker-field" style={{ marginBottom }}>
      <label className="picker-field__label" htmlFor={id}>
        {label}
      </label>
      {children}
      {note && (
        <div
          id={noteId}
          className="t-ui-xs"
          data-testid={noteTestId}
          style={{ marginTop: 4, color: toneColor(warn ? "warn" : "muted") }}
        >
          {note}
        </div>
      )}
    </div>
  );
}

/**
 * The rows beneath the binary field: the open sub-tab's credential fields with
 * Verify on the token row, then the Environment variables field both sub-tabs
 * share.
 */
export function ClaudeCredentialRows({
  integration,
  draft,
  busy,
  onEdit,
  onVerify,
}: {
  integration: AgenticIntegration;
  draft: AgenticDraft;
  busy: Busy;
  onEdit: (patch: Partial<AgenticDraft>) => void;
  onVerify: () => void;
}) {
  const vendor = integration.vendor;
  const verifying = busy === "verifying" || busy === "detecting";
  const verifyDisabled = verifying || !canVerifyAgentic(integration, draft);
  const verifyButton = (
    <button
      className="btn btn--default btn--sm"
      // AII-FR-51 / AII-FR-HOKG: unavailable until the open sub-tab can produce
      // a complete configuration, so nothing the backend would refuse as a
      // missing or malformed field is ever sent.
      disabled={verifyDisabled}
      onClick={onVerify}
    >
      {busy === "verifying" ? "Verifying…" : "Verify"}
    </button>
  );
  const env = parseEnvText(draft.envText);

  return (
    <div role="tabpanel" aria-label={draft.authMode === "custom_gateway" ? "Custom Gateway" : "Subscription"}>
      {draft.authMode === "subscription" ? (
        <SubscriptionRow
          integration={integration}
          draft={draft}
          onEdit={onEdit}
          verifyButton={verifyButton}
        />
      ) : (
        <GatewayRows
          integration={integration}
          draft={draft}
          onEdit={onEdit}
          verifyButton={verifyButton}
        />
      )}

      {/* AII-FR-EJMG: optional, and the same in both sub-tabs. */}
      <Field
        id={`agentic-env-${vendor}`}
        label="Environment variables"
        noteId={`agentic-env-note-${vendor}`}
        noteTestId="agentic-env-note"
        warn={env.error !== ""}
        note={
          env.error ||
          "One NAME=value on each line. Stored in the settings file, so put credentials in the token field."
        }
        marginBottom={4}
      >
        <textarea
          id={`agentic-env-${vendor}`}
          className="textarea input--mono"
          data-testid="agentic-env-vars"
          spellCheck={false}
          autoComplete="off"
          rows={3}
          aria-invalid={env.error !== "" || undefined}
          aria-describedby={`agentic-env-note-${vendor}`}
          placeholder="HTTPS_PROXY=http://proxy.example:3128"
          value={draft.envText}
          onChange={(e) => onEdit({ envText: e.target.value })}
        />
      </Field>
    </div>
  );
}

/** AII-FR-49 .. AII-FR-52: the OAuth token row of the Subscription sub-tab. */
function SubscriptionRow({
  integration,
  draft,
  onEdit,
  verifyButton,
}: {
  integration: AgenticIntegration;
  draft: AgenticDraft;
  onEdit: (patch: Partial<AgenticDraft>) => void;
  verifyButton: ReactNode;
}) {
  const vendor = integration.vendor;
  const validation = oauthTokenValidation(draft.oauthToken);
  return (
    <Field
      id={`agentic-token-${vendor}`}
      label="OAuth token"
      noteId={`agentic-token-note-${vendor}`}
      noteTestId="agentic-token-note"
      warn={validation !== ""}
      // AII-FR-50: structural only, and quoting nothing the author typed.
      // AII-FR-52: an empty field is not an error where a token is already held
      // — it re-verifies with it.
      note={
        validation ||
        (integration.keyState === "set"
          ? "Leave empty to re-verify with the stored token."
          : `Required — ${integration.displayName} runs where it cannot sign in for itself.`)
      }
      marginBottom={10}
    >
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <input
          id={`agentic-token-${vendor}`}
          type="password"
          className="input input--mono"
          data-testid="agentic-oauth-token"
          spellCheck={false}
          autoComplete="off"
          aria-invalid={validation !== "" || undefined}
          aria-describedby={`agentic-token-note-${vendor}`}
          // Keyed on `keyState` alone, exactly as the note below is: a record
          // reporting a stored token with no hint to describe it still has one,
          // and a placeholder saying "required" beside a note saying "leave
          // empty" would have the tab contradicting itself.
          placeholder={
            integration.keyState === "set"
              ? integration.maskedHint
                ? `•••• ${integration.maskedHint}`
                : "a token is stored"
              : "required"
          }
          value={draft.oauthToken}
          onChange={(e) => onEdit({ oauthToken: e.target.value })}
          style={{ flex: 1 }}
        />
        {verifyButton}
      </div>
    </Field>
  );
}

/** AII-FR-PHFX: the base URL, the token variable name, and the token. */
function GatewayRows({
  integration,
  draft,
  onEdit,
  verifyButton,
}: {
  integration: AgenticIntegration;
  draft: AgenticDraft;
  onEdit: (patch: Partial<AgenticDraft>) => void;
  verifyButton: ReactNode;
}) {
  const vendor = integration.vendor;
  // AII-FR-HOKG: an empty URL is named at once, in the quiet tone of a field
  // not yet filled in rather than the warning tone of a wrong value.
  const urlComplaint = gatewayUrlValidation(draft.gatewayUrl);
  const varComplaint = gatewayTokenVarValidation(draft.gatewayTokenVar);
  const tokenComplaint = gatewayTokenValidation(draft.gatewayToken);
  const rowComplaint = varComplaint || tokenComplaint;
  const stored = integration.gatewayKeyState === "set";
  return (
    <>
      <Field
        id={`agentic-gateway-url-${vendor}`}
        label="Base URL"
        note={urlComplaint}
        noteTestId="agentic-gateway-url-note"
      >
        <input
          id={`agentic-gateway-url-${vendor}`}
          className="input input--mono"
          data-testid="agentic-gateway-url"
          spellCheck={false}
          autoComplete="off"
          placeholder="https://gateway.example.com"
          value={draft.gatewayUrl}
          onChange={(e) => onEdit({ gatewayUrl: e.target.value })}
        />
      </Field>
      {/* AII-FR-PHFX: the variable name and the token write one variable,
          `NAME=token`, so they are one frame joined by the `=` that writes
          them, like the image name and tag of SET-FR-21. The name takes the
          width it needs and the token takes the rest. */}
      <div className="picker-field" style={{ marginBottom: 10 }}>
        <span className="picker-field__label" id={`agentic-gateway-token-label-${vendor}`}>
          Token
        </span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <div
            className="joined-field"
            role="group"
            aria-labelledby={`agentic-gateway-token-label-${vendor}`}
            data-testid="agentic-gateway-credential"
          >
            <input
              id={`agentic-gateway-var-${vendor}`}
              className="joined-field__part joined-field__part--var"
              data-testid="agentic-gateway-token-var"
              aria-label="Token variable name"
              spellCheck={false}
              autoComplete="off"
              aria-invalid={varComplaint !== "" || undefined}
              aria-describedby={`agentic-gateway-note-${vendor}`}
              value={draft.gatewayTokenVar}
              onChange={(e) => onEdit({ gatewayTokenVar: e.target.value })}
            />
            <span className="joined-field__sep" aria-hidden="true">
              =
            </span>
            <input
              id={`agentic-gateway-token-${vendor}`}
              type="password"
              className="joined-field__part"
              data-testid="agentic-gateway-token"
              aria-label="Gateway token"
              spellCheck={false}
              autoComplete="off"
              aria-invalid={tokenComplaint !== "" || undefined}
              aria-describedby={`agentic-gateway-note-${vendor}`}
              placeholder={
                stored
                  ? integration.gatewayMaskedHint
                    ? `•••• ${integration.gatewayMaskedHint}`
                    : "a token is stored"
                  : "required"
              }
              value={draft.gatewayToken}
              onChange={(e) => onEdit({ gatewayToken: e.target.value })}
            />
          </div>
          {verifyButton}
        </div>
        {/* AII-FR-HOKG: one note for the row. A wrong name is named first,
            then a wrong token, then what an empty token field means. */}
        <div
          id={`agentic-gateway-note-${vendor}`}
          className="t-ui-xs"
          data-testid="agentic-gateway-note"
          style={{ marginTop: 4, color: toneColor(rowComplaint ? "warn" : "muted") }}
        >
          {rowComplaint ||
            (stored
              ? "Leave empty to re-verify with the stored token."
              : "Required — passed to Claude Code under the variable name on the left.")}
        </div>
      </div>
    </>
  );
}
