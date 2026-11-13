/**
 * One row of the MCP servers section (`MSS-mcp-servers-settings.md`
 * MSS-FR-XALV through MSS-FR-MMLA): the name, the ID, the transport badge, the
 * status chip, the line under the row, the actions of its status, the test
 * result, and the tools disclosure.
 *
 * The row writes nothing itself: each action is a callback of the section,
 * which holds the writes and the refusals.
 */
import { useState } from "react";

import {
  firstLine,
  NEEDS_SECRET_TEXT,
  rowProblemText,
  SCOPE_BADGE,
  statusChipText,
  statusChipTone,
  switchNoteText,
  testResultText,
  WAITING_FOR_BROWSER_TEXT,
} from "../state/mcpServerText";
import type { McpServerEntry, McpTestResult } from "../types";

/** MSS-FR-VWKQ: the test of a row, from its start to its result. */
export type McpTestView =
  | { phase: "running" }
  | { phase: "done"; result: McpTestResult }
  | { phase: "refused"; text: string };

/** The actions of a row. Each that opens a dialog gets the control that opened it. */
export interface McpRowActions {
  onToggleEnabled: (enabled: boolean) => void;
  onToggleTool: (tool: string, enabled: boolean) => void;
  onTest: () => void;
  onEdit: (opener: HTMLElement) => void;
  onSetSecrets: (opener: HTMLElement) => void;
  onRemove: (opener: HTMLElement) => void;
  onTrust: (opener: HTMLElement) => void;
  onSignIn: () => void;
  onCancelSignIn: () => void;
  onSignOut: () => void;
  onReconnect: () => void;
}

export interface McpServerRowProps extends McpRowActions {
  entry: McpServerEntry;
  test?: McpTestView;
  /** The refusal of the last action of this row. */
  error?: string | null;
  /** An action of this row is in flight. */
  busy?: boolean;
}

const TONE_CLASS = {
  ok: "badge--ok",
  warn: "badge--warn",
  danger: "badge--danger",
  neutral: "",
} as const;

/** MSS-FR-HXVP: whether the row offers `Sign in`. */
export function offersSignIn(entry: McpServerEntry): boolean {
  const { status } = entry;
  if (status.state === "needs_sign_in") return true;
  const transport = entry.record?.transport;
  return (
    status.state === "failed" &&
    (status.failure === "oauth_failed" || status.failure === "rejected") &&
    transport?.kind === "http" &&
    transport.auth === "oauth2"
  );
}

/** MSS-FR-ANUU: whether the row offers `Sign out`. */
export function offersSignOut(entry: McpServerEntry): boolean {
  const transport = entry.record?.transport;
  return entry.status.state === "ready" && transport?.kind === "http" && transport.auth === "oauth2";
}

function TestResultView({ test }: { test: McpTestView }) {
  if (test.phase === "running") {
    return (
      <p className="mcp-server__test t-ui-xs t-muted" role="status" data-testid="mcp-test-result">
        Testing…
      </p>
    );
  }
  if (test.phase === "refused") {
    return (
      <div className="mcp-server__test mcp-server__test--failed" role="status" data-testid="mcp-test-result">
        <p className="t-ui-xs">{test.text}</p>
      </div>
    );
  }
  const { result } = test;
  return (
    <div
      className={`mcp-server__test${result.ok ? "" : " mcp-server__test--failed"}`}
      role="status"
      data-testid="mcp-test-result"
    >
      <p className="t-ui-xs">{testResultText(result)}</p>
      {!result.ok && result.stderrTail.length > 0 && (
        <pre className="mcp-server__stderr" data-testid="mcp-test-stderr">
          {result.stderrTail.join("\n")}
        </pre>
      )}
    </div>
  );
}

export function McpServerRow({ entry, test, error, busy = false, ...actions }: McpServerRowProps) {
  const [toolsOpen, setToolsOpen] = useState(false);
  const { status, record } = entry;
  const name = record?.name ?? entry.id;
  const transport = record?.transport.kind ?? null;
  const problem = rowProblemText(entry);
  const ready = status.state === "ready" ? status : null;
  const disabledTools = new Set(record?.disabledTools ?? []);
  const switchNote = switchNoteText(entry);
  // MSS-FR-GBOM, MSS-FR-RAYC: a personal record whose ID another server has
  // can only be removed.
  const removeOnly = entry.scope !== "team" && entry.invalid === "duplicate_id";

  return (
    <li
      className="mcp-server"
      data-testid="mcp-server-row"
      data-server-id={entry.id}
      data-scope={entry.scope}
      aria-label={name}
      tabIndex={-1}
    >
      <div className="mcp-server__head">
        <span className="mcp-server__name">{name}</span>
        <span className="mcp-server__id">{entry.id}</span>
        {transport && (
          <span className="badge" data-testid="mcp-transport-badge">
            {transport === "stdio" ? "Local" : "Remote"}
          </span>
        )}
        {/* MSS-FR-SAKG */}
        {entry.scope !== "team" && (
          <span className="badge mcp-server__scope" data-testid="mcp-scope-badge">
            {SCOPE_BADGE[entry.scope]}
          </span>
        )}
        <span
          className={`badge mcp-server__chip ${TONE_CLASS[statusChipTone(status.state)]}`}
          data-testid="mcp-status-chip"
          data-state={status.state}
        >
          <span className="mcp-server__dot" aria-hidden="true" />
          {statusChipText(status)}
        </span>
      </div>

      {/* MSS-FR-YIMR: the failure class, or the validation code. */}
      {problem && (
        <p className="mcp-server__note mcp-server__note--danger" data-testid="mcp-server-problem">
          {problem}
        </p>
      )}
      {/* MSS-FR-OOUI */}
      {switchNote && (
        <p className="mcp-server__note" data-testid="mcp-switch-note">
          {switchNote}
        </p>
      )}
      {/* MSS-FR-PDVK */}
      {status.state === "needs_secret" && (
        <p className="mcp-server__note" data-testid="mcp-needs-secret">
          {NEEDS_SECRET_TEXT}
        </p>
      )}
      {/* MSS-FR-HXVP */}
      {status.state === "signing_in" && (
        <p className="mcp-server__note" data-testid="mcp-signing-in">
          {WAITING_FOR_BROWSER_TEXT}
        </p>
      )}

      <div className="mcp-server__actions">
        {/* MSS-FR-GBOM: an invalid record has no record to switch. */}
        {!removeOnly && (
          <label className="mcp-server__switch t-ui-sm">
            <input
              type="checkbox"
              role="switch"
              checked={entry.enabledHere}
              disabled={record === null}
              onChange={(event) => actions.onToggleEnabled(event.target.checked)}
            />
            On on this computer
          </label>
        )}
        {status.state === "untrusted" && (
          <button type="button" className="btn btn--default btn--sm" disabled={busy} onClick={(e) => actions.onTrust(e.currentTarget)}>
            Trust…
          </button>
        )}
        {status.state === "needs_secret" && (
          <button type="button" className="btn btn--default btn--sm" onClick={(e) => actions.onSetSecrets(e.currentTarget)}>
            Set secrets
          </button>
        )}
        {offersSignIn(entry) && (
          <button type="button" className="btn btn--default btn--sm" disabled={busy} onClick={actions.onSignIn}>
            Sign in
          </button>
        )}
        {status.state === "signing_in" && (
          <button type="button" className="btn btn--ghost btn--sm" disabled={busy} onClick={actions.onCancelSignIn}>
            Cancel
          </button>
        )}
        {status.state === "failed" && (
          <button type="button" className="btn btn--default btn--sm" disabled={busy} onClick={actions.onReconnect}>
            Reconnect
          </button>
        )}
        {!removeOnly && (
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={test?.phase === "running"}
            onClick={actions.onTest}
          >
            Test
          </button>
        )}
        {!removeOnly && (
          <button type="button" className="btn btn--ghost btn--sm" onClick={(e) => actions.onEdit(e.currentTarget)}>
            Edit
          </button>
        )}
        {offersSignOut(entry) && (
          <button type="button" className="btn btn--ghost btn--sm" disabled={busy} onClick={actions.onSignOut}>
            Sign out
          </button>
        )}
        <button type="button" className="btn btn--ghost btn--sm" onClick={(e) => actions.onRemove(e.currentTarget)}>
          Remove
        </button>
      </div>

      {error && (
        <p className="mcp-server__note mcp-server__note--danger" role="alert" data-testid="mcp-row-error">
          {error}
        </p>
      )}

      {/* MSS-FR-VWKQ: under the row, until the next test or until the section closes. */}
      {test && <TestResultView test={test} />}

      {/* MSS-FR-QMTG, MSS-FR-MMLA */}
      {ready && (
        <div className="mcp-server__tools">
          <button
            type="button"
            className="mcp-server__disclosure"
            aria-expanded={toolsOpen}
            onClick={() => setToolsOpen((open) => !open)}
          >
            <span aria-hidden="true">{toolsOpen ? "▾" : "▸"}</span> Tools ({ready.toolCount})
          </button>
          {toolsOpen && (
            <ul className="mcp-server__tool-list" data-testid="mcp-tool-list">
              {entry.tools.map((tool) => (
                <li key={tool.name} className="mcp-tool">
                  <div className="mcp-tool__text">
                    <span className="mcp-tool__name">{tool.name}</span>
                    {tool.description && (
                      <span className="mcp-tool__description">{firstLine(tool.description)}</span>
                    )}
                  </div>
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label={tool.name}
                    checked={!disabledTools.has(tool.name)}
                    disabled={record === null}
                    onChange={(event) => actions.onToggleTool(tool.name, event.target.checked)}
                  />
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </li>
  );
}
