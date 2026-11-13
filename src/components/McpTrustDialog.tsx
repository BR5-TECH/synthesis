/**
 * The trust dialog of an MCP server (`MSS-mcp-servers-settings.md`
 * MSS-FR-VJPZ, MSS-FR-SCFN, MSS-FR-GJVD, MSS-FR-UVWX).
 *
 * For a `Local command` it shows the command and its arguments as one line,
 * the working folder, each plain environment variable with its value, and the
 * names of the secret variables. For a `Remote (HTTP)` server it shows the URL,
 * the authentication, and the names of the secret headers. It shows no secret
 * value: the vault values never reach the UI.
 *
 * `Trust and start` sends the record the dialog shows, so the backend trusts
 * only what the author saw (MCS-FR-EVRS).
 */
import { useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import { AUTH_LABEL, errorCode, logCode, refusalText } from "../state/mcpServerText";
import type { McpHttpTransport, McpServer, McpServerEntry, McpStdioTransport } from "../types";
import { Modal } from "./DraftsPanelParts";

export interface McpTrustDialogProps {
  /** The record of the row, as the section holds it. */
  record: McpServer;
  returnFocus?: HTMLElement | null;
  onClose: () => void;
  onTrusted: (entry: McpServerEntry) => void;
}

const TITLE = "Trust this server?";

/** One argument as the author would type it in a shell, so spaces stay visible. */
function shellWord(word: string): string {
  return word === "" || /[\s"'\\]/.test(word) ? JSON.stringify(word) : word;
}

/** MSS-FR-VJPZ: the command and its arguments as one line. */
export function commandLine(transport: McpStdioTransport): string {
  return [transport.command, ...transport.args].map(shellWord).join(" ");
}

/** MSS-FR-VJPZ: each plain environment variable with its value. */
export function plainEnvironment(transport: McpStdioTransport): string[] {
  return Object.entries(transport.env).map(([name, value]) => `${name}=${value}`);
}

const listOrNone = (items: string[]) => (items.length > 0 ? items.join(", ") : "(none)");

function StdioFacts({ transport }: { transport: McpStdioTransport }) {
  return (
    <>
      <p className="t-ui-sm">This project runs this command on your computer:</p>
      <div className="mcp-trust__facts">
        <code className="mcp-trust__command" data-testid="mcp-trust-command">
          {commandLine(transport)}
        </code>
        <p className="t-ui-sm" data-testid="mcp-trust-cwd">
          Working folder: {transport.cwd ?? "(project root)"}
        </p>
        <p className="t-ui-sm" data-testid="mcp-trust-env">
          Environment: {listOrNone(plainEnvironment(transport))}
        </p>
        <p className="t-ui-sm" data-testid="mcp-trust-secret-env">
          Secret environment: {listOrNone(transport.secretEnv)}
        </p>
      </div>
    </>
  );
}

function HttpFacts({ transport }: { transport: McpHttpTransport }) {
  return (
    <>
      <p className="t-ui-sm">This project sends your credentials for this server to:</p>
      <div className="mcp-trust__facts">
        <code className="mcp-trust__command" data-testid="mcp-trust-url">
          {transport.url}
        </code>
        <p className="t-ui-sm" data-testid="mcp-trust-auth">
          Authentication: {AUTH_LABEL[transport.auth]}
          {transport.auth === "oauth2" && transport.clientId ? ` (client ${transport.clientId})` : ""}
        </p>
        <p className="t-ui-sm" data-testid="mcp-trust-secret-headers">
          Secret headers: {listOrNone(transport.secretHeaders)}
        </p>
      </div>
      {/* MSS-FR-VNIK: the stored credentials do not follow a new destination. */}
      <p className="t-ui-sm" data-testid="mcp-trust-credentials-removed">
        Stored credentials for this server are removed. Enter them again after you trust it.
      </p>
    </>
  );
}

export function McpTrustDialog({ record, returnFocus, onClose, onTrusted }: McpTrustDialogProps) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // MSS-FR-GJVD: the record the dialog shows, held for the whole life of the
  // dialog, so a newer list does not change what the author is asked to trust.
  const [shown] = useState(record);
  const { id, transport } = shown;

  /** MSS-FR-GJVD: trust the shown record, then close. A refusal stays in the dialog. */
  const trust = async () => {
    setBusy(true);
    setError(null);
    logInfo(["frontend"], "mcp server trust started", { id, transport: transport.kind });
    try {
      const entry = await api.trustMcpServer(id, shown);
      onTrusted(entry);
      onClose();
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server trust refused", { id, error: logCode(code) });
      // MSS-FR-UVWX: `record_changed` reads its own text, and the dialog stays open.
      setError(refusalText(code));
      setBusy(false);
    }
  };

  return (
    <Modal label={TITLE} onClose={onClose} returnFocus={returnFocus}>
      <div className="modal__head">
        <div className="modal__title">{TITLE}</div>
      </div>
      <div className="modal__body" data-testid="mcp-trust-dialog">
        {transport.kind === "stdio" ? (
          <StdioFacts transport={transport} />
        ) : (
          <HttpFacts transport={transport} />
        )}
        {error && (
          <p className="mcp-dialog__problem" role="alert">
            {error}
          </p>
        )}
      </div>
      <div className="modal__actions">
        <button type="button" className="btn btn--ghost" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="btn btn--primary" disabled={busy} onClick={() => void trust()}>
          Trust and start
        </button>
      </div>
    </Modal>
  );
}
