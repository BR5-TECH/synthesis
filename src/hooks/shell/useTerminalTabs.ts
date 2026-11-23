/**
 * The Terminal tabs of the strip (`../../../specifications/ui/TAB-tabs.md`
 * TAB-FR-VKPS, TAB-FR-JFZR, TAB-FR-KIIW; `TRM-terminals.md`).
 *
 * A Terminal tab is a view onto a session that outlives it. So the tab's pinned
 * state is the session's, a tab that leaves the strip by any route only
 * detaches its session, and a session that is closed takes its tab away.
 */
import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import type { Dispatch, SetStateAction } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import { isWindowsPlatform } from "../../platform";
import {
  ensureTerminalsFollowed,
  errorCode,
  subscribeTerminals,
  terminalsSnapshot,
  upsertTerminal,
  type TerminalsSnapshot,
} from "../../state/terminals";
import { hasTerminalView, releaseTerminalView, terminalViewIds } from "../../state/terminalViews";
import type { Tab, TerminalSummary } from "../../types";
import { terminalTabId } from "./tabRecords";

/** The terminal list of the window, as React state. */
export function useTerminalsSnapshot(): TerminalsSnapshot {
  return useSyncExternalStore(subscribeTerminals, terminalsSnapshot, terminalsSnapshot);
}

/**
 * TAB-FR-KIIW: the pinned set the strip draws and the mass closes read. A
 * Terminal tab is pinned while its session is pinned.
 */
export function useTerminalPins(
  tabs: readonly Tab[],
  pinnedTabs: ReadonlySet<string>,
): ReadonlySet<string> {
  const { sessions } = useTerminalsSnapshot();
  return useMemo(() => {
    const pinnedTerminals = tabs.filter(
      (t) => t.kind === "terminal" && sessions.some((s) => s.id === t.terminalId && s.pinned),
    );
    if (pinnedTerminals.length === 0) return pinnedTabs;
    const next = new Set(pinnedTabs);
    for (const t of pinnedTerminals) next.add(t.id);
    return next;
  }, [tabs, pinnedTabs, sessions]);
}

interface TerminalTabDeps {
  active: boolean;
  tabs: readonly Tab[];
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  activateTab: (id: string) => void;
  closeTab: (id: string) => Promise<boolean>;
  setTabPinned: (id: string, pinned: boolean) => void;
}

export interface TerminalTabs {
  /** TRM-FR-PZJI, TAB-FR-VKPS: opens the session's tab, or focuses it where open. */
  openTerminalTab: (session: TerminalSummary) => void;
  /** TAB-FR-KIIW: Pin tab and Unpin tab, which on a Terminal tab pin the session. */
  setTabPinned: (id: string, pinned: boolean) => void;
}

export function useTerminalTabs({
  active,
  tabs,
  setTabs,
  activateTab,
  closeTab,
  setTabPinned,
}: TerminalTabDeps): TerminalTabs {
  const snapshot = useTerminalsSnapshot();
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const closeRef = useRef(closeTab);
  closeRef.current = closeTab;

  // TRM-FR-LAZH: the list is read while a project window shows its IDE.
  useEffect(() => {
    if (active && !isWindowsPlatform()) ensureTerminalsFollowed();
  }, [active]);

  // TAB-FR-JFZR / TRM-FR-JXLA: a Terminal tab that left the strip, by any
  // route, detaches its session and ends nothing.
  useEffect(() => {
    const open = new Set(tabs.filter((t) => t.kind === "terminal").map((t) => t.terminalId));
    for (const id of terminalViewIds()) {
      if (!open.has(id)) releaseTerminalView(id, terminalsSnapshot().sessions.some((s) => s.id === id));
    }
  }, [tabs]);

  // TRM-FR-OCSN: a closed session takes its tab away.
  useEffect(() => {
    if (!snapshot.loaded) return;
    for (const tab of tabsRef.current) {
      if (tab.kind !== "terminal" || !tab.terminalId) continue;
      if (snapshot.sessions.some((s) => s.id === tab.terminalId)) continue;
      if (hasTerminalView(tab.terminalId)) releaseTerminalView(tab.terminalId, false);
      void closeRef.current(tab.id);
    }
  }, [snapshot]);

  // TRM-FR-DPWA: the tab's title follows each change of the session's label.
  useEffect(() => {
    const labels = new Map(snapshot.sessions.map((s) => [s.id, s.label]));
    const stale = (t: Tab) =>
      t.kind === "terminal" && !!t.terminalId && labels.has(t.terminalId) && labels.get(t.terminalId) !== t.label;
    if (!tabsRef.current.some(stale)) return;
    setTabs((current) => current.map((t) => (stale(t) ? { ...t, label: labels.get(t.terminalId!)! } : t)));
  }, [snapshot, setTabs]);

  const openTerminalTab = useCallback(
    (session: TerminalSummary) => {
      const id = terminalTabId(session.id);
      setTabs((current) =>
        current.some((t) => t.id === id)
          ? current
          : [...current, { id, label: session.label, kind: "terminal", terminalId: session.id }],
      );
      activateTab(id);
    },
    [setTabs, activateTab],
  );

  const pin = useCallback(
    (id: string, pinned: boolean) => {
      const tab = tabsRef.current.find((t) => t.id === id);
      if (tab?.kind !== "terminal" || !tab.terminalId) {
        setTabPinned(id, pinned);
        return;
      }
      const terminalId = tab.terminalId;
      api
        .setTerminalPinned(terminalId, pinned)
        .then((summary) => {
          upsertTerminal(summary);
          logDebug(["frontend"], "terminal pin changed from tab", { terminalId, pinned });
        })
        .catch((err) =>
          logWarn(["frontend"], "terminal pin failed", { terminalId, code: errorCode(err) }),
        );
    },
    [setTabPinned],
  );

  return { openTerminalTab, setTabPinned: pin };
}
