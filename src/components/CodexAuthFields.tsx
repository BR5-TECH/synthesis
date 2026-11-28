import type { ReactNode } from "react";
import type { AgenticIntegration, CodexAuthMode } from "../types";
import { AgenticAuthTabs, AgenticField, type AuthTab } from "./AgenticAuthTabs";
import { accessTokenValidation, type AgenticDraft } from "./agenticDraft";

/**
 * The Codex tab's two sub-tabs and what each renders below the binary field —
 * `specifications/ui/AII-ai-integrations.md` AII-FR-LOYM, AII-FR-RNSO,
 * AII-FR-XXSQ, and AII-FR-NXAV.
 *
 * Choosing a sub-tab only changes `draft.codexAuthMode`. Verify submits the
 * open sub-tab's fields and no other's.
 */

const SUB_TABS: ReadonlyArray<AuthTab<CodexAuthMode>> = [
  { mode: "login_folder", label: "Login folder" },
  { mode: "access_token", label: "Access token" },
];

/** AII-FR-LOYM: the strip above the binary field. */
export function CodexAuthTabs({
  mode,
  onChoose,
}: {
  mode: CodexAuthMode;
  onChoose: (mode: CodexAuthMode) => void;
}) {
  return (
    <AgenticAuthTabs
      ariaLabel="Codex authentication"
      tabs={SUB_TABS}
      mode={mode}
      onChoose={onChoose}
    />
  );
}

/**
 * AII-FR-RNSO: the one statement the Login folder sub-tab renders below the
 * binary field. It renders no credential field: the login belongs to Codex's
 * own CLI, and the application never reads it.
 */
export function CodexLoginFolderNote() {
  return (
    <div
      className="t-ui-xs t-muted"
      data-testid="codex-login-folder-note"
      style={{ marginBottom: 4 }}
    >
      Codex&apos;s own <code>codex login</code> writes <code>~/.codex</code>, and a
      run mounts that folder.
    </div>
  );
}

/**
 * AII-FR-XXSQ / AII-FR-NXAV: the access token row of the Access token
 * sub-tab, with Verify on it.
 */
export function CodexAccessTokenRow({
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
  const validation = accessTokenValidation(draft.accessToken);
  // AIC-FR-25: in `access_token` mode the record's key fields describe the
  // stored access token.
  const stored = integration.keyState === "set";
  const source = "The token comes from the access-token settings of your ChatGPT workspace.";
  return (
    <AgenticField
      id={`agentic-access-token-${vendor}`}
      label="Access token"
      noteId={`agentic-access-token-note-${vendor}`}
      noteTestId="agentic-access-token-note"
      warn={validation !== ""}
      note={
        validation ||
        (stored
          ? `Leave empty to re-verify with the stored token. ${source}`
          : `Required. ${source}`)
      }
      marginBottom={10}
    >
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <input
          id={`agentic-access-token-${vendor}`}
          type="password"
          className="input input--mono"
          data-testid="agentic-access-token"
          spellCheck={false}
          autoComplete="off"
          aria-invalid={validation !== "" || undefined}
          aria-describedby={`agentic-access-token-note-${vendor}`}
          placeholder={
            stored
              ? integration.maskedHint
                ? `•••• ${integration.maskedHint}`
                : "a token is stored"
              : "required"
          }
          value={draft.accessToken}
          onChange={(e) => onEdit({ accessToken: e.target.value })}
          style={{ flex: 1 }}
        />
        {verifyButton}
      </div>
    </AgenticField>
  );
}
