/**
 * The Terminals panel (`../../../specifications/ui/TRM-terminals.md`
 * TRM-FR-LAZH … TRM-FR-OCSN).
 *
 * It lists every terminal session of the project window, grouped by target,
 * and opens a session's tab on a click. New terminal opens the New terminal
 * menu under the button that was activated. The row context menu renames,
 * re-themes, pins, and closes sessions, and a close that ends a running
 * process asks first.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import {
  MAX_TERMINALS,
  errorCode,
  removeTerminal,
  upsertTerminal,
} from "../../state/terminals";
import {
  closeSet,
  groupTerminals,
  kindName,
  placeName,
  roleName,
  rowOrder,
  rowName,
  runningCount,
  stateName,
  type CloseScope,
} from "../../state/terminalGroups";
import { releaseTerminalView } from "../../state/terminalViews";
import { setTerminalThemeOverride } from "../../state/terminalActions";
import { useTerminalsSnapshot } from "../../hooks/shell/useTerminalTabs";
import type { PanelRevealRequest, TerminalSummary } from "../../types";
import { Icon } from "../icons";
import { EMPTY_BODY_CLASS, PanelEmptyState } from "../PanelEmptyState";
import { EndTerminalsConfirm } from "./EndTerminalsConfirm";
import { NewTerminalMenu } from "./NewTerminalMenu";
import { TerminalLabelField } from "./TerminalLabelField";
import { TerminalPersistMark } from "./TerminalPersistMark";
import { TerminalRowMenu, type TerminalMenuChoice } from "./TerminalRowMenu";

export interface TerminalsPanelProps {
  activeWorktreePath: string | null;
  onOpenTerminal: (session: TerminalSummary) => void;
  reveal?: PanelRevealRequest | null;
  /** SNV-FR-56: a floating overlay of this panel is about to open. */
  onOverlayOpening?: () => void;
}

interface MenuState {
  id: string;
  x: number;
  y: number;
}

interface PendingClose {
  ids: string[];
  running: number;
  rowId: string;
}

/** TRM-FR-OCSN: ends each session and takes it out of the list. */
async function endSessions(ids: readonly string[]): Promise<void> {
  for (const id of ids) {
    try {
      await api.closeTerminal(id);
      logDebug(["frontend"], "terminal closed", { terminalId: id });
    } catch (err) {
      logWarn(["frontend"], "terminal close failed", { terminalId: id, code: errorCode(err) });
      if (errorCode(err) !== "unknown_terminal") continue;
    }
    releaseTerminalView(id, false);
    removeTerminal(id);
  }
}

export function TerminalsPanel({
  activeWorktreePath,
  onOpenTerminal,
  reveal,
  onOverlayOpening,
}: TerminalsPanelProps) {
  const { sessions } = useTerminalsSnapshot();
  const groups = useMemo(() => groupTerminals(sessions, activeWorktreePath), [sessions, activeWorktreePath]);
  const rows = useMemo(() => rowOrder(groups), [groups]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [newAnchor, setNewAnchor] = useState<HTMLElement | null>(null);
  const [pending, setPending] = useState<PendingClose | null>(null);
  /** TRM-FR-SHKD: the row whose label shows the label field. */
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const headerButton = useRef<HTMLButtonElement | null>(null);
  const atLimit = sessions.length >= MAX_TERMINALS;

  // TRM-FR-RCCE / SNV-FR-66: a Terminal tab that became active selects its row.
  useEffect(() => {
    if (!reveal) return;
    if (sessions.some((s) => s.id === reveal.id)) {
      setSelectedId(reveal.id);
      setFocusId(reveal.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reveal?.nonce]);

  const rowElement = (id: string) =>
    Array.from(listRef.current?.querySelectorAll<HTMLElement>("[data-terminal-row]") ?? []).find(
      (el) => el.dataset.terminalRow === id,
    ) ?? null;

  const focusRow = (id: string) => {
    setFocusId(id);
    rowElement(id)?.focus();
  };

  const open = (session: TerminalSummary) => {
    setSelectedId(session.id);
    setFocusId(session.id);
    onOpenTerminal(session);
  };

  const openMenu = (id: string, x: number, y: number) => {
    onOverlayOpening?.();
    setNewAnchor(null);
    setMenu({ id, x, y });
  };

  /** TRM-FR-JINY: a second activation of the same button closes the menu. */
  const toggleNewMenu = (anchor: HTMLElement) => {
    if (newAnchor === anchor) {
      setNewAnchor(null);
      return;
    }
    if (atLimit) return;
    onOverlayOpening?.();
    setMenu(null);
    setNewAnchor(anchor);
  };

  /** TRM-FR-JJEY: a close that ends a running session asks first. */
  const requestClose = (scope: CloseScope, rowId: string) => {
    const set = closeSet(sessions, scope, rowId);
    if (set.length === 0) return;
    const running = runningCount(set);
    const ids = set.map((s) => s.id);
    if (running > 0) {
      onOverlayOpening?.();
      setPending({ ids, running, rowId });
      return;
    }
    void endSessions(ids);
  };

  const choose = (choice: TerminalMenuChoice, session: TerminalSummary) => {
    setMenu(null);
    if (choice === "rename") {
      // TRM-FR-SHKD: the label turns into the label field, which takes the focus.
      setSelectedId(session.id);
      setFocusId(session.id);
      setRenamingId(session.id);
      return;
    }
    if (choice === "pin") {
      // TRM-FR-AHXU: the pin changes nothing else.
      api
        .setTerminalPinned(session.id, !session.pinned)
        .then((summary) => {
          upsertTerminal(summary);
          logDebug(["frontend"], "terminal pin changed", { terminalId: session.id, pinned: summary.pinned });
        })
        .catch((err) => logWarn(["frontend"], "terminal pin failed", { terminalId: session.id, code: errorCode(err) }));
      focusRow(session.id);
      return;
    }
    requestClose(choice, session.id);
  };

  const menuSession = menu ? sessions.find((s) => s.id === menu.id) : undefined;
  // TRM-FR-OXKC: at the limit the button is disabled, and its tooltip says why.
  const newTitle = atLimit
    ? `The limit of ${MAX_TERMINALS} terminals is reached`
    : "New terminal";
  const newButton = (
    <span className="terminals-panel__new-wrap" title={newTitle}>
      <button
        ref={headerButton}
        type="button"
        className="icon-btn terminals-panel__new"
        aria-label="New terminal"
        aria-haspopup="menu"
        aria-expanded={newAnchor !== null && newAnchor === headerButton.current}
        aria-description={atLimit ? newTitle : undefined}
        disabled={atLimit}
        onClick={(e) => toggleNewMenu(e.currentTarget)}
      >
        <Icon.Plus size={14} aria-hidden="true" />
      </button>
    </span>
  );

  return (
    <div className="terminals-panel">
      <div className="panel-header">
        <span className="panel-header__title">Terminals</span>
        <div className="panel-header__actions">{newButton}</div>
      </div>

      {sessions.length === 0 ? (
        <div className={EMPTY_BODY_CLASS}>
          <PanelEmptyState
            line="No terminals"
            action={
              atLimit
                ? undefined
                : {
                    label: "New terminal",
                    icon: <Icon.Plus size={12} aria-hidden="true" />,
                    onClick: (e) => toggleNewMenu(e.currentTarget),
                  }
            }
          >
            A terminal is a shell or an agent CLI session in a worktree or a work stream.
          </PanelEmptyState>
        </div>
      ) : (
        <div
          ref={listRef}
          className="vpanel__body terminals-panel__list"
          role="listbox"
          aria-label="Terminals"
          onKeyDown={(e) => {
            // TRM-FR-UIWG: the arrow keys move between rows, across groups.
            if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
            // TRM-FR-SHKD: the label field keeps its own keys.
            if (renamingId !== null) return;
            e.preventDefault();
            const from = (e.target as HTMLElement).closest<HTMLElement>("[data-terminal-row]")?.dataset.terminalRow;
            const at = rows.findIndex((r) => r.id === (from ?? focusId));
            const next = e.key === "ArrowDown" ? Math.min(rows.length - 1, at + 1) : Math.max(0, at - 1);
            if (rows[next]) focusRow(rows[next].id);
          }}
        >
          {groups.map((group) => (
            <div key={group.key} role="group" aria-label={group.name} className="terminals-group">
              <div className="terminals-group__head">
                <Icon.Branch size={11} aria-hidden="true" />
                <span className="terminals-group__name">{group.name}</span>
                {group.branch && group.branch !== group.name && (
                  <span className="terminals-group__branch">{group.branch}</span>
                )}
                <span className="terminals-group__role">{roleName(group.role)}</span>
              </div>
              {group.sessions.map((session) => {
                const tabbable = (focusId ?? rows[0]?.id) === session.id;
                const renaming = renamingId === session.id;
                return (
                  <div
                    key={session.id}
                    role="option"
                    aria-selected={selectedId === session.id}
                    aria-label={rowName(session)}
                    tabIndex={tabbable ? 0 : -1}
                    className="terminal-row"
                    data-terminal-row={session.id}
                    data-state={session.state}
                    data-pinned={session.pinned ? "true" : undefined}
                    onClick={() => {
                      if (!renaming) open(session);
                    }}
                    onFocus={() => setFocusId(session.id)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      openMenu(session.id, e.clientX, e.clientY);
                    }}
                    onKeyDown={(e) => {
                      if (renaming) return;
                      if (e.key === "Enter") {
                        e.preventDefault();
                        open(session);
                      } else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
                        e.preventDefault();
                        const box = e.currentTarget.getBoundingClientRect();
                        openMenu(session.id, box.left + 16, box.bottom);
                      }
                    }}
                  >
                    <span className="terminal-row__marks">
                      <span className="terminal-row__pin" aria-hidden="true">
                        {session.pinned && <Icon.Pin size={10} />}
                      </span>
                      <span className="terminal-row__persist">
                        {session.persistent && <TerminalPersistMark className="terminal-persist" />}
                      </span>
                    </span>
                    <span className="terminal-row__main">
                      <span className="terminal-row__line">
                        {renaming ? (
                          <TerminalLabelField
                            terminalId={session.id}
                            label={session.label}
                            className="terminal-row__label-field"
                            onDone={(restoreFocus) => {
                              setRenamingId(null);
                              if (restoreFocus) focusRow(session.id);
                            }}
                          />
                        ) : (
                          <span className="terminal-row__label">{session.label}</span>
                        )}
                        {session.busyRunId && (
                          <span
                            className="terminal-busy"
                            title="A graduation run is working in this target"
                            aria-label="A graduation run is working in this target"
                          >
                            <Icon.AlertTriangle size={11} aria-hidden="true" />
                          </span>
                        )}
                        <span className="terminal-row__state" data-state={session.state}>
                          {stateName(session)}
                        </span>
                      </span>
                      <span className="terminal-row__detail">
                        {kindName(session.kind, session.vendor)} · {placeName(session.runsIn, session.environmentName)}
                      </span>
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      )}

      {menu && menuSession && (
        <TerminalRowMenu
          x={menu.x}
          y={menu.y}
          terminalLabel={menuSession.label}
          pinned={menuSession.pinned}
          theme={menuSession.theme ?? null}
          othersDisabled={closeSet(sessions, "others", menuSession.id).length === 0}
          allDisabled={closeSet(sessions, "all", menuSession.id).length === 0}
          onChoose={(choice) => choose(choice, menuSession)}
          onChooseTheme={(theme) => {
            // TRM-FR-QXIY: a choice closes both menus, and the focus goes back to the row.
            const id = menuSession.id;
            setMenu(null);
            focusRow(id);
            void setTerminalThemeOverride(id, theme);
          }}
          onClose={(restoreFocus) => {
            const id = menu.id;
            setMenu(null);
            if (restoreFocus) focusRow(id);
          }}
        />
      )}

      {newAnchor && (
        <NewTerminalMenu
          anchor={newAnchor}
          onClose={(restoreFocus) => {
            const anchor = newAnchor;
            setNewAnchor(null);
            if (restoreFocus) anchor.focus();
          }}
          onCreated={(session) => {
            setNewAnchor(null);
            open(session);
          }}
        />
      )}

      {pending && (
        <EndTerminalsConfirm
          running={pending.running}
          returnFocus={rowElement(pending.rowId)}
          onCancel={() => setPending(null)}
          onConfirm={() => {
            const ids = pending.ids;
            setPending(null);
            void endSessions(ids);
          }}
        />
      )}
    </div>
  );
}
