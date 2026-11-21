// The Terminal tabs of the strip (`TAB-tabs.md` TAB-FR-VKPS, TAB-FR-JFZR,
// TAB-FR-KIIW; `TRM-terminals.md` TRM-FR-DURY, TRM-FR-OCSN). The backend is
// mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useState } from "react";
import { refreshTerminals, removeTerminal } from "../../state/terminals";
import { hasTerminalView, showTerminalView, terminalViewStarted } from "../../state/terminalViews";
import { eligibleMassCloseTargets } from "./tabStripActions";
import { DASHBOARD_TAB } from "./tabRecords";
import type { Tab, TerminalSummary } from "../../types";
import { useTerminalPins, useTerminalTabs } from "./useTerminalTabs";
import { commands, resetTerminalFixtures, serve, session } from "../../components/Terminals/terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("../../components/Terminals/terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../components/Terminals/terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

beforeEach(() => {
  resetTerminalFixtures();
  serve("attach_terminal", { state: "running", historyBytes: 0 });
  serve("detach_terminal", null);
  serve("resize_terminal", null);
});
afterEach(cleanup);

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

function useHarness() {
  const [tabs, setTabs] = useState<Tab[]>([DASHBOARD_TAB]);
  const [active, setActive] = useState("dashboard");
  const [pinnedTabs, setPinnedTabs] = useState<ReadonlySet<string>>(new Set());
  const shown = useTerminalPins(tabs, pinnedTabs);
  const closeTab = async (id: string) => {
    setTabs((ts) => {
      const next = ts.filter((t) => t.id !== id);
      return next.length ? next : [DASHBOARD_TAB];
    });
    return true;
  };
  const setStripPinned = (id: string, pinned: boolean) =>
    setPinnedTabs((cur) => {
      const next = new Set(cur);
      if (pinned) next.add(id);
      else next.delete(id);
      return next;
    });
  const terminals = useTerminalTabs({
    active: true,
    tabs,
    setTabs,
    activateTab: setActive,
    closeTab,
    setTabPinned: setStripPinned,
  });
  return { tabs, setTabs, active, shown, ...terminals };
}

describe("Terminal tabs", () => {
  it("TAB-FR-VKPS: a terminal opens in one tab, identified by its terminal id, and a second open focuses it", async () => {
    const a = session({ id: "a", label: "Host shell 1" });
    await seed([a]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    act(() => result.current.openTerminalTab(a));
    const terminalTabs = result.current.tabs.filter((t) => t.kind === "terminal");
    expect(terminalTabs).toEqual([{ id: "terminal:a", label: "Host shell 1", kind: "terminal", terminalId: "a" }]);
    expect(result.current.active).toBe("terminal:a");
  });

  it("TAB-FR-KIIW, TRM-FR-DURY: Pin tab on a Terminal tab pins the session, and the tab shows it pinned", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    serve("set_terminal_pinned", (args) => ({ ...a, pinned: Boolean(args.pinned) }));
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    act(() => result.current.setTabPinned("terminal:a", true));
    await waitFor(() => expect(result.current.shown.has("terminal:a")).toBe(true));
    expect(commands("set_terminal_pinned")[0].args).toEqual({ terminalId: "a", pinned: true });
  });

  it("TAB-FR-KIIW: a pinned session's tab opens pinned again, and the mass closes skip it", async () => {
    const a = session({ id: "a", pinned: true });
    const b = session({ id: "b", label: "Host shell 2" });
    await seed([a, b]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    act(() => result.current.openTerminalTab(b));
    expect(result.current.shown.has("terminal:a")).toBe(true);
    expect(eligibleMassCloseTargets(result.current.tabs, result.current.shown, "others", "dashboard")).toEqual([
      "terminal:b",
    ]);
    act(() => result.current.setTabs((ts) => ts.filter((t) => t.id !== "terminal:a")));
    act(() => result.current.openTerminalTab(a));
    expect(result.current.shown.has("terminal:a")).toBe(true);
  });

  it("TAB-FR-JFZR, TRM-FR-JXLA: a tab that leaves the strip detaches its session and ends nothing", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    const host = document.createElement("div");
    document.body.appendChild(host);
    showTerminalView("a", host);
    await act(() => terminalViewStarted("a"));
    act(() => result.current.setTabs((ts) => ts.filter((t) => t.id !== "terminal:a")));
    await waitFor(() => expect(commands("detach_terminal")).toHaveLength(1));
    expect(hasTerminalView("a")).toBe(false);
    expect(commands("close_terminal")).toHaveLength(0);
  });

  it("TAB-FR-JFZR, OVW-FR-12: the worktree switch closes every tab and no session ends", async () => {
    const a = session({ id: "a" });
    const b = session({ id: "b", label: "Host shell 2" });
    await seed([a, b]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    act(() => result.current.openTerminalTab(b));
    for (const id of ["a", "b"]) {
      const host = document.createElement("div");
      showTerminalView(id, host);
      await act(() => terminalViewStarted(id));
    }
    // `resetViewport` puts the strip back to the Dashboard alone.
    act(() => result.current.setTabs([DASHBOARD_TAB]));
    await waitFor(() => expect(commands("detach_terminal")).toHaveLength(2));
    expect(commands("close_terminal")).toHaveLength(0);
  });

  it("TRM-FR-OCSN: a closed session takes its tab away", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    act(() => removeTerminal("a"));
    await waitFor(() => expect(result.current.tabs.some((t) => t.id === "terminal:a")).toBe(false));
    expect(commands("detach_terminal")).toHaveLength(0);
  });

  it("TRM-FR-XVJT: a session whose target is gone keeps its tab", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    const { result } = renderHook(useHarness);
    act(() => result.current.openTerminalTab(a));
    await seed([{ ...a, state: "target_gone", endReason: "target_gone" }]);
    expect(result.current.tabs.some((t) => t.id === "terminal:a")).toBe(true);
  });
});
