/**
 * Project settings → **MCP servers** (`MSS-mcp-servers-settings.md`, per
 * `SET-project-settings.md` SET-FR-06).
 *
 * The list of the project's MCP servers in two groups, the team servers and
 * the author's own (MSS-FR-POYD), with the status of each, the actions of each
 * row, and the add, edit, trust, and remove dialogs. Each action writes at
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
import {
  errorCode,
  logCode,
  PERSONAL_GROUP,
  refusalText,
  removeConfirmText,
  TEAM_GROUP,
} from "../state/mcpServerText";
import type { McpScope, McpServer, McpServerEntry, McpServerStatus } from "../types";
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

interface RemoveRequest extends IdRequest {
  scope: McpScope;
}

/** MCS-FR-FIEE: the order of the scopes in the list. */
const SCOPE_RANK: Record<McpScope, number> = { team: 0, personal_project: 1, personal_all: 2 };

/** One row of the list: an ID can be in two rows when one of them is shadowed. */
const rowKey = (entry: McpServerEntry) => `${entry.scope}:${entry.id}`;

/** MCS-FR-PQHI: the active entry of an ID is the first listed entry of it. */
const isActive = (entry: McpServerEntry, list: McpServerEntry[]) =>
  list.find((e) => e.id === entry.id) === entry;

export function McpServersSettings() {
  const [entries, setEntries] = useState<McpServerEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  const [tests, setTests] = useState<Record<string, McpTestView>>({});
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  const [trustFor, setTrustFor] = useState<IdRequest | null>(null);
  const [removeFor, setRemoveFor] = useState<RemoveRequest | null>(null);
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

  /** Keep the records of the active entries. A shadowed entry has no record. */
  const remember = (list: McpServerEntry[]) => {
    const withRecord = new Set(list.filter((e) => e.record).map((e) => e.id));
    for (const entry of list) {
      if (entry.record) confirmed.current.set(entry.id, entry.record);
      else if (!withRecord.has(entry.id)) confirmed.current.delete(entry.id);
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
        // MCS-FR-PQHI: an event is of the active entry of its ID.
        setEntries(list.map((entry) => (isActive(entry, list) ? withNewerStatus(entry, since) : entry)));
        logInfo(["frontend"], "mcp servers loaded", { count: list.length });
      })
      .catch((e) => {
        if (!alive.current || seq !== loadSeq.current) return;
        const code = errorCode(e);
        setLoadError(refusalText(code));
        logWarn(["frontend"], "mcp servers could not be loaded", { error: logCode(code) });
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  /** Read the list again with no loading line, after the active row of an ID changed. */
  const refreshList = () => {
    const since = eventSeq.current;
    api
      .listMcpServers()
      .then((list) => {
        if (!alive.current) return;
        remember(list);
        setEntries(list.map((entry) => (isActive(entry, list) ? withNewerStatus(entry, since) : entry)));
      })
      .catch((e) => {
        logWarn(["frontend"], "mcp servers could not be read again", { error: logCode(errorCode(e)) });
      });
  };

  /** The section held tools that are older than the status. Read them again. */
  const refreshTools = useCallback((id: string) => {
    api
      .listMcpServers()
      .then((list) => {
        if (!alive.current) return;
        const fresh = list.find((e) => e.id === id);
        if (!fresh) return;
        setEntries((prev) =>
          prev?.map((e) =>
            rowKey(e) === rowKey(fresh) ? { ...e, tools: fresh.tools, secrets: fresh.secrets } : e,
          ) ?? prev,
        );
      })
      .catch((e) => {
        logWarn(["frontend"], "mcp server tools could not be read", { id, error: logCode(errorCode(e)) });
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
      // MCS-FR-PQHI: the event is of the active entry of the ID.
      setEntries(
        (prev) =>
          prev?.map((entry) => (entry.id === id && isActive(entry, prev) ? { ...entry, status } : entry)) ??
          prev,
      );
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
        logWarn(["frontend"], "mcp server status events not available", { error: logCode(errorCode(e)) });
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
      const key = rowKey(entry);
      return prev.some((e) => rowKey(e) === key)
        ? prev.map((e) => (rowKey(e) === key ? entry : e))
        : [...prev, entry];
    });
  };

  /** Show a record on the active row of its ID. A personal switch is its `enabled`. */
  const showRecord = (id: string, record: McpServer) =>
    setEntries(
      (prev) =>
        prev?.map((e) =>
          e.id === id && isActive(e, prev)
            ? { ...e, record, enabledHere: e.scope === "team" ? e.enabledHere : record.enabled }
            : e,
        ) ?? prev,
    );

  const showEnabledHere = (id: string, enabledHere: boolean) =>
    setEntries(
      (prev) => prev?.map((e) => (e.id === id && isActive(e, prev) ? { ...e, enabledHere } : e)) ?? prev,
    );

  /**
   * MSS-FR-GBOM, MSS-FR-MMLA: save a changed record at once. A refused save
   * puts the last stored record back.
   */
  const writeRecord = (
    entry: McpServerEntry,
    change: (record: McpServer) => McpServer,
    action: string,
    send: (next: McpServer) => Promise<McpServerEntry> = (next) => api.saveMcpServer(next, entry.scope),
  ) => {
    const { id } = entry;
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
        return send(next);
      })
      .then((saved) => {
        if (saved.record) confirmed.current.set(id, saved.record);
        if (latest.current.get(id) !== next) return;
        latest.current.delete(id);
        if (alive.current) placeEntry(saved, since);
      })
      .catch((e) => {
        const code = errorCode(e);
        logWarn(["frontend"], "mcp server save refused", { id, action, error: logCode(code) });
        if (latest.current.get(id) === next) {
          latest.current.delete(id);
          const back = confirmed.current.get(id);
          if (back && alive.current) showRecord(id, back);
        }
        if (alive.current) setRowError(id, refusalText(code));
      });
  };

  /**
   * MSS-FR-GBOM: the switch on this computer. A team switch is no record
   * field; a personal switch is the record's `enabled`, so it goes with the
   * other record writes. A refusal puts the switch back.
   */
  const toggleHere = (entry: McpServerEntry, enabled: boolean) => {
    const action = enabled ? "enable" : "disable";
    if (entry.scope !== "team") {
      writeRecord(entry, (r) => ({ ...r, enabled }), action, () =>
        api.setMcpServerEnabled(entry.id, enabled),
      );
      return;
    }
    const { id } = entry;
    const before = entry.enabledHere;
    showEnabledHere(id, enabled);
    setRowError(id, null);
    logInfo(["frontend"], "mcp server switch started", { id, action });
    let since = eventSeq.current;
    writes.current = writes.current
      .catch(() => undefined)
      .then(() => {
        since = eventSeq.current;
        return api.setMcpServerEnabled(id, enabled);
      })
      .then((saved) => {
        if (alive.current) placeEntry(saved, since);
      })
      .catch((e) => {
        const code = errorCode(e);
        logWarn(["frontend"], "mcp server switch refused", { id, action, error: logCode(code) });
        if (!alive.current) return;
        showEnabledHere(id, before);
        setRowError(id, refusalText(code));
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
      logWarn(["frontend"], "mcp server action refused", { id, action, error: logCode(code) });
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
      logWarn(["frontend"], "mcp server test refused", { id, error: logCode(code) });
      if (alive.current) {
        setTests((prev) => ({ ...prev, [id]: { phase: "refused", text: refusalText(code) } }));
      }
    }
  };

  /** MSS-FR-KQNF */
  const confirmRemove = async () => {
    if (!removeFor) return;
    const { id, scope } = removeFor;
    const removed = entries?.find((e) => e.id === id && e.scope === scope) ?? null;
    const wasActive = removed !== null && entries !== null && isActive(removed, entries);
    setRemoving(true);
    setRemoveError(null);
    logInfo(["frontend"], "mcp server remove started", { id, scope });
    try {
      await api.removeMcpServer(id, scope);
      if (!alive.current) return;
      setEntries((prev) => prev?.filter((e) => !(e.id === id && e.scope === scope)) ?? prev);
      // A shadowed row shares its ID with the active row, which keeps its state.
      if (wasActive) {
        // MCS-FR-PQHI: a row of another scope with this ID is active now.
        if (entries?.some((e) => e.id === id && e.scope !== scope)) refreshList();
        confirmed.current.delete(id);
        latest.current.delete(id);
        setTests((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
        setRowError(id, null);
      }
      setRemoveFor(null);
      setPendingFocus("add");
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server remove refused", { id, scope, error: logCode(code) });
      if (alive.current) setRemoveError(refusalText(code));
    } finally {
      if (alive.current) setRemoving(false);
    }
  };

  const entryOf = (id: string) => entries?.find((e) => e.id === id) ?? null;
  const removeEntry = removeFor
    ? (entries?.find((e) => e.id === removeFor.id && e.scope === removeFor.scope) ?? null)
    : null;
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

      {entries !== null && entries.length > 0 &&
        groups(entries).map((group) => (
          <div key={group.key} className="mcp-servers__group" data-testid={`mcp-group-${group.key}`}>
            <h3 className="mcp-servers__group-title">{group.text.heading}</h3>
            <p className="t-ui-sm t-muted">{group.text.sentence}</p>
            {group.entries.length === 0 ? (
              <p className="t-ui-sm t-muted" data-testid={`mcp-group-${group.key}-empty`}>
                {group.text.empty}
              </p>
            ) : (
              <ul className="mcp-servers__list" aria-label={group.text.heading}>
                {group.entries.map((entry) => (
                <McpServerRow
                  key={rowKey(entry)}
                  entry={entry}
                  test={isActive(entry, entries) ? tests[entry.id] : undefined}
                  error={isActive(entry, entries) ? (rowErrors[entry.id] ?? null) : null}
                  busy={isActive(entry, entries) ? (busy[entry.id] ?? false) : false}
                  onToggleEnabled={(enabled) => toggleHere(entry, enabled)}
                  onToggleTool={(tool, enabled) =>
                    writeRecord(
                      entry,
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
                    setRemoveFor({ id: entry.id, scope: entry.scope, opener, since: eventSeq.current });
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
          </div>
        ))}

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
              {removeConfirmText(removeFor.scope, removeEntry?.record?.name ?? removeFor.id)}
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

/** MSS-FR-POYD: the team group and the personal group, each in list order. */
function groups(entries: McpServerEntry[]) {
  const personal = entries
    .filter((e) => e.scope !== "team")
    .sort((a, b) => SCOPE_RANK[a.scope] - SCOPE_RANK[b.scope]);
  return [
    { key: "team", text: TEAM_GROUP, entries: entries.filter((e) => e.scope === "team") },
    { key: "personal", text: PERSONAL_GROUP, entries: personal },
  ];
}
