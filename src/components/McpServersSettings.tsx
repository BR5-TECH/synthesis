/**
 * Project settings → **MCP servers** (`MSS-mcp-servers-settings.md`, per
 * `SET-project-settings.md` SET-FR-06).
 *
 * The list of the project's MCP servers with the status of each, the actions of
 * each row, and the add, edit, trust, and remove dialogs. Each action writes at
 * once, so the section holds no dirty state (MSS-FR-GMHC). The status of a row
 * follows `mcp-server-status-changed` while the section is open (MSS-FR-CLQR).
 *
 * The Settings window shows no toast, so the outcome of an action shows in its
 * row: the chip, the line under the row, the test result, or the refusal.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onMcpServerStatusChanged } from "../events";
import { logInfo, logWarn } from "../logging";
import { errorCode, refusalText } from "../state/mcpServerText";
import type { McpServer, McpServerEntry, McpServerStatus } from "../types";
import { Modal } from "./DraftsPanelParts";
import { McpServerDialog } from "./McpServerDialog";
import { McpServerRow, type McpTestView } from "./McpServerRow";
import { McpTrustDialog } from "./McpTrustDialog";

interface DialogRequest {
  /** Null for `Add server`. */
  entry: McpServerEntry | null;
  focusMissingSecret: boolean;
  opener: HTMLElement | null;
  /** The status event count when the dialog opened (see `withNewerStatus`). */
  since: number;
}

interface IdRequest {
  id: string;
  opener: HTMLElement | null;
  since: number;
}

export function McpServersSettings() {
  const [entries, setEntries] = useState<McpServerEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  const [tests, setTests] = useState<Record<string, McpTestView>>({});
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  const [trustFor, setTrustFor] = useState<IdRequest | null>(null);
  const [removeFor, setRemoveFor] = useState<IdRequest | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  /**
   * Where focus goes after an action removed the control that had it: the row
   * of a server (after a trust), or the Add server button (after a remove).
   */
  const [pendingFocus, setPendingFocus] = useState<{ row: string } | "add" | null>(null);
  useEffect(() => {
    if (!pendingFocus) return;
    setPendingFocus(null);
    if (pendingFocus === "add") {
      addButton.current?.focus();
      return;
    }
    const rowElement = Array.from(
      document.querySelectorAll<HTMLElement>("[data-testid='mcp-server-row']"),
    ).find((element) => element.dataset.serverId === pendingFocus.row);
    rowElement?.focus();
  }, [pendingFocus]);
  const alive = useRef(true);
  /** The last record of each server that the backend stored, which a refusal restores. */
  const confirmed = useRef(new Map<string, McpServer>());
  /** The record of each server that a write in flight shows. */
  const latest = useRef(new Map<string, McpServer>());
  /** MSS-FR-GBOM, MSS-FR-MMLA: the record writes, one after the other. */
  const writes = useRef<Promise<unknown>>(Promise.resolve());
  const loadSeq = useRef(0);
  /** The entries of the last render, for the event handler. */
  const entriesRef = useRef<McpServerEntry[] | null>(null);
  entriesRef.current = entries;
  /** MSS-FR-CLQR: the count of status events, and the last event of each server. */
  const eventSeq = useRef(0);
  const lastEvent = useRef(new Map<string, { seq: number; status: McpServerStatus }>());

  /**
   * A command answers with the status it saw when it ran. An event that came
   * after the command started is newer, so it wins over that answer.
   */
  const withNewerStatus = (entry: McpServerEntry, since: number): McpServerEntry => {
    const event = lastEvent.current.get(entry.id);
    return event && event.seq > since ? { ...entry, status: event.status } : entry;
  };

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const remember = (list: McpServerEntry[]) => {
    for (const entry of list) {
      if (entry.record) confirmed.current.set(entry.id, entry.record);
      else confirmed.current.delete(entry.id);
    }
  };

  /** MSS-FR-RYEQ: load the list, and show the loading line until it arrives. */
  const load = useCallback(() => {
    const seq = ++loadSeq.current;
    const since = eventSeq.current;
    setLoadError(null);
    setEntries(null);
    logInfo(["frontend"], "mcp servers load started", {});
    api
      .listMcpServers()
      .then((list) => {
        if (!alive.current || seq !== loadSeq.current) return;
        remember(list);
        setEntries(list.map((entry) => withNewerStatus(entry, since)));
        logInfo(["frontend"], "mcp servers loaded", { count: list.length });
      })
      .catch((e) => {
        if (!alive.current || seq !== loadSeq.current) return;
        const code = errorCode(e);
        setLoadError(refusalText(code));
        logWarn(["frontend"], "mcp servers could not be loaded", { error: code });
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  /** The section held tools that are older than the status. Read them again. */
  const refreshTools = useCallback((id: string) => {
    api
      .listMcpServers()
      .then((list) => {
        if (!alive.current) return;
        const fresh = list.find((e) => e.id === id);
        if (!fresh) return;
        setEntries((prev) =>
          prev?.map((e) => (e.id === id ? { ...e, tools: fresh.tools, secrets: fresh.secrets } : e)) ?? prev,
        );
      })
      .catch((e) => {
        logWarn(["frontend"], "mcp server tools could not be read", { id, error: errorCode(e) });
      });
  }, []);

  // MSS-FR-CLQR: the chip and the line of one row follow the event, with no
  // reload of the list.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onMcpServerStatusChanged(({ id, status }) => {
      eventSeq.current += 1;
      lastEvent.current.set(id, { seq: eventSeq.current, status });
      const held = entriesRef.current?.find((entry) => entry.id === id);
      setEntries((prev) => prev?.map((entry) => (entry.id === id ? { ...entry, status } : entry)) ?? prev);
      // A server that is ready now can offer other tools than the section holds.
      if (held && status.state === "ready" && held.tools.length !== status.toolCount) {
        refreshTools(id);
      }
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e) => {
        logWarn(["frontend"], "mcp server status events not available", { error: errorCode(e) });
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshTools]);

  const setRowError = (id: string, text: string | null) =>
    setRowErrors((prev) => {
      const next = { ...prev };
      if (text === null) delete next[id];
      else next[id] = text;
      return next;
    });

  /** Put the entry the backend returned in place of its row, or at the end. */
  const placeEntry = (answer: McpServerEntry, since: number) => {
    remember([answer]);
    const entry = withNewerStatus(answer, since);
    setEntries((prev) => {
      if (!prev) return [entry];
      return prev.some((e) => e.id === entry.id)
        ? prev.map((e) => (e.id === entry.id ? entry : e))
        : [...prev, entry];
    });
  };

  const showRecord = (id: string, record: McpServer) =>
    setEntries((prev) => prev?.map((e) => (e.id === id ? { ...e, record } : e)) ?? prev);

  /**
   * MSS-FR-GBOM, MSS-FR-MMLA: save a changed record at once. A refused save
   * puts the last stored record back.
   */
  const writeRecord = (id: string, change: (record: McpServer) => McpServer, action: string) => {
    const base = latest.current.get(id) ?? confirmed.current.get(id);
    if (!base) return;
    const next = change(base);
    latest.current.set(id, next);
    showRecord(id, next);
    setRowError(id, null);
    logInfo(["frontend"], "mcp server save started", { id, action });
    let since = eventSeq.current;
    writes.current = writes.current
      .catch(() => undefined)
      .then(() => {
        since = eventSeq.current;
        return api.saveMcpServer(next);
      })
      .then((saved) => {
        if (saved.record) confirmed.current.set(id, saved.record);
        if (latest.current.get(id) !== next) return;
        latest.current.delete(id);
        if (alive.current) placeEntry(saved, since);
      })
      .catch((e) => {
        const code = errorCode(e);
        logWarn(["frontend"], "mcp server save refused", { id, action, error: code });
        if (latest.current.get(id) === next) {
          latest.current.delete(id);
          const back = confirmed.current.get(id);
          if (back && alive.current) showRecord(id, back);
        }
        if (alive.current) setRowError(id, refusalText(code));
      });
  };

  /** One action of a row: log its start, put a returned entry in place, show a refusal. */
  const runAction = async (
    id: string,
    action: string,
    call: () => Promise<McpServerEntry | void>,
  ) => {
    setRowError(id, null);
    setBusy((prev) => ({ ...prev, [id]: true }));
    logInfo(["frontend"], "mcp server action started", { id, action });
    const since = eventSeq.current;
    try {
      const result = await call();
      if (result && alive.current) placeEntry(result, since);
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server action refused", { id, action, error: code });
      if (alive.current) setRowError(id, refusalText(code));
    } finally {
      if (alive.current) setBusy((prev) => ({ ...prev, [id]: false }));
    }
  };

  /** MSS-FR-VWKQ */
  const runTest = async (id: string) => {
    setTests((prev) => ({ ...prev, [id]: { phase: "running" } }));
    logInfo(["frontend"], "mcp server test started", { id });
    try {
      const result = await api.testMcpServer(id);
      if (!result.ok) {
        logWarn(["frontend"], "mcp server test failed", { id, error: result.failure });
      } else {
        logInfo(["frontend"], "mcp server test passed", { id, tools: result.tools.length });
      }
      if (alive.current) setTests((prev) => ({ ...prev, [id]: { phase: "done", result } }));
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server test refused", { id, error: code });
      if (alive.current) {
        setTests((prev) => ({ ...prev, [id]: { phase: "refused", text: refusalText(code) } }));
      }
    }
  };

  /** MSS-FR-KQNF */
  const confirmRemove = async () => {
    if (!removeFor) return;
    const { id } = removeFor;
    setRemoving(true);
    setRemoveError(null);
    logInfo(["frontend"], "mcp server remove started", { id });
    try {
      await api.removeMcpServer(id);
      if (!alive.current) return;
      confirmed.current.delete(id);
      latest.current.delete(id);
      setEntries((prev) => prev?.filter((e) => e.id !== id) ?? prev);
      setTests((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setRowError(id, null);
      setRemoveFor(null);
      setPendingFocus("add");
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server remove refused", { id, error: code });
      if (alive.current) setRemoveError(refusalText(code));
    } finally {
      if (alive.current) setRemoving(false);
    }
  };

  const entryOf = (id: string) => entries?.find((e) => e.id === id) ?? null;
  const removeEntry = removeFor ? entryOf(removeFor.id) : null;
  // MSS-FR-YFPL: a Local command and a Remote server alike.
  const trustRecord = trustFor ? (entryOf(trustFor.id)?.record ?? null) : null;

  return (
    <section className="mcp-servers" data-testid="mcp-servers-settings" aria-label="MCP servers">
      {entries === null && !loadError && (
        <p className="t-ui-sm t-muted" role="status">
          Loading MCP servers…
        </p>
      )}

      {loadError && (
        <div className="mcp-servers__load-error" role="alert">
          <p className="t-ui-sm">The MCP servers could not be loaded: {loadError}</p>
          <button type="button" className="btn btn--default btn--sm" onClick={load}>
            Try again
          </button>
        </div>
      )}

      {entries !== null && entries.length === 0 && (
        <p className="t-ui-sm t-muted" data-testid="mcp-servers-empty">
          No MCP servers yet. Add one to give the agents of this project its tools.
        </p>
      )}

      {entries !== null && entries.length > 0 && (
        <ul className="mcp-servers__list">
          {entries.map((entry) => (
            <McpServerRow
              key={entry.id}
              entry={entry}
              test={tests[entry.id]}
              error={rowErrors[entry.id] ?? null}
              busy={busy[entry.id] ?? false}
              onToggleEnabled={(enabled) =>
                writeRecord(entry.id, (r) => ({ ...r, enabled }), enabled ? "enable" : "disable")
              }
              onToggleTool={(tool, enabled) =>
                writeRecord(
                  entry.id,
                  (r) => ({
                    ...r,
                    disabledTools: enabled
                      ? r.disabledTools.filter((name) => name !== tool)
                      : r.disabledTools.includes(tool)
                        ? r.disabledTools
                        : [...r.disabledTools, tool],
                  }),
                  enabled ? "enable tool" : "disable tool",
                )
              }
              onTest={() => void runTest(entry.id)}
              onEdit={(opener) =>
                setDialog({ entry, focusMissingSecret: false, opener, since: eventSeq.current })
              }
              onSetSecrets={(opener) =>
                setDialog({ entry, focusMissingSecret: true, opener, since: eventSeq.current })
              }
              onRemove={(opener) => {
                setRemoveError(null);
                setRemoveFor({ id: entry.id, opener, since: eventSeq.current });
              }}
              onTrust={(opener) => setTrustFor({ id: entry.id, opener, since: eventSeq.current })}
              onSignIn={() => void runAction(entry.id, "sign in", () => api.startMcpSignIn(entry.id))}
              onCancelSignIn={() =>
                void runAction(entry.id, "cancel sign in", () => api.cancelMcpSignIn(entry.id))
              }
              onSignOut={() => void runAction(entry.id, "sign out", () => api.signOutMcpServer(entry.id))}
              onReconnect={() =>
                void runAction(entry.id, "reconnect", () => api.reconnectMcpServer(entry.id))
              }
            />
          ))}
        </ul>
      )}

      {entries !== null && (
        <div className="mcp-servers__add">
          <button
            ref={addButton}
            type="button"
            className="btn btn--default"
            onClick={() =>
              setDialog({
                entry: null,
                focusMissingSecret: false,
                opener: addButton.current,
                since: eventSeq.current,
              })
            }
          >
            + Add server
          </button>
        </div>
      )}

      {dialog && (
        <McpServerDialog
          entry={dialog.entry}
          knownIds={(entries ?? []).map((e) => e.id)}
          focusMissingSecret={dialog.focusMissingSecret}
          returnFocus={dialog.opener}
          onClose={() => setDialog(null)}
          onSaved={(entry) => {
            if (!alive.current) return;
            latest.current.delete(entry.id);
            placeEntry(entry, dialog.since);
            setRowError(entry.id, null);
          }}
        />
      )}

      {trustFor && trustRecord && (
        <McpTrustDialog
          record={trustRecord}
          returnFocus={trustFor.opener}
          onClose={() => setTrustFor(null)}
          onTrusted={(entry) => {
            if (!alive.current) return;
            placeEntry(entry, trustFor.since);
            setPendingFocus({ row: entry.id });
          }}
        />
      )}

      {removeFor && (
        <Modal label="Remove MCP server" onClose={() => setRemoveFor(null)} returnFocus={removeFor.opener}>
          <div className="modal__head">
            <div className="modal__title">Remove MCP server</div>
          </div>
          <div className="modal__body" data-testid="mcp-remove-dialog">
            <p className="t-ui-sm">
              Remove {removeEntry?.record?.name ?? removeFor.id}? Its secrets and its sign-in on this
              computer are removed too.
            </p>
            {removeError && (
              <p className="mcp-dialog__problem" role="alert">
                {removeError}
              </p>
            )}
          </div>
          <div className="modal__actions">
            <button type="button" className="btn btn--ghost" onClick={() => setRemoveFor(null)}>
              Cancel
            </button>
            <button
              type="button"
              className="btn btn--danger"
              disabled={removing}
              onClick={() => void confirmRemove()}
            >
              Remove
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
