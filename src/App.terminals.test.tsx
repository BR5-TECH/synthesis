/**
 * The Terminals feature through the shell: the activity-bar toggle, the panel,
 * the New terminal menu, and the Terminal tab, with the real panel and the
 * real tab strip over a mocked backend.
 *
 * `TRM-terminals.md`; `SNV-shell-navigation.md` SNV-FR-PULG, SNV-FR-66;
 * `TAB-tabs.md` TAB-FR-VKPS, TAB-FR-JFZR, TAB-FR-KIIW; `OVW-overview.md` OVW-FR-12.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetLogBufferForTest } from "./logging";
import { resetPanelReveals } from "./state/panelReveal";
import { defaultInvoke, enterIde, resetAppFixture, tabLabels } from "./test/appFixtures";
import { FakeTerminal } from "./test/ghosttyMock";
import type { TerminalSummary, TerminalTarget } from "./types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: (await import("./components/Terminals/terminalFixtures")).FakeChannel,
}));
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async (name: string, handler: (e: { payload?: unknown }) => void) => {
    (eventHandlers[name] ??= []).push(handler);
    return () => {
      eventHandlers[name] = (eventHandlers[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize: class {
      constructor(
        public width: number,
        public height: number,
      ) {}
    },
    getCurrentWindow: () => win,
    availableMonitors: async () => [],
  };
});

let sessions: TerminalSummary[];
const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
const named = (cmd: string) => calls.filter((c) => c.cmd === cmd);

const TARGET: TerminalTarget = {
  target: { kind: "worktree", path: "~/dev/acme" },
  name: "acme",
  branch: "main",
  isActiveWorktree: true,
  busyRunId: null,
  kinds: [{ kind: "host_shell", runsIn: "host", available: true }],
};

function make(id: string): TerminalSummary {
  return {
    id,
    label: "Host shell 1",
    target: TARGET.target,
    targetName: "acme",
    branch: "main",
    kind: "host_shell",
    runsIn: "host",
    state: "running",
    pinned: false,
    busyRunId: null,
    hasHistory: true,
    createdAt: "2026-01-01T00:00:00Z",
  };
}

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  for (const key of Object.keys(eventHandlers)) delete eventHandlers[key];
  calls.length = 0;
  sessions = [];
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
  }));
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case "list_terminals":
        return sessions.map((s) => ({ ...s }));
      case "list_terminal_targets":
        return [TARGET];
      case "create_terminal": {
        const s = make("t1");
        sessions.push(s);
        return { ...s };
      }
      case "set_terminal_pinned": {
        const s = sessions.find((x) => x.id === args.terminalId)!;
        s.pinned = Boolean(args.pinned);
        return { ...s };
      }
      case "attach_terminal":
        return { state: "running", historyBytes: 0 };
      case "detach_terminal":
      case "resize_terminal":
      case "acknowledge_terminal_output":
      case "write_terminal_input":
      case "close_terminal":
        return null;
      default:
        return defaultInvoke(cmd, args);
    }
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  resetLogBufferForTest();
});

const activityBar = () => document.querySelector(".activity-bar") as HTMLElement;
const terminalTab = () =>
  Array.from(document.querySelectorAll<HTMLElement>(".tabstrip .tab")).find((t) =>
    t.textContent?.includes("Host shell 1"),
  );

async function openPanelAndCreate() {
  await userEvent.click(within(activityBar()).getByRole("button", { name: "Terminals" }));
  expect(await screen.findByText("No terminals")).toBeInTheDocument();
  await userEvent.click(screen.getAllByRole("button", { name: "New terminal" })[0]);
  const menu = await screen.findByRole("menu", { name: "New terminal" });
  await userEvent.click(await within(menu).findByRole("menuitem", { name: "Host shell" }));
  await waitFor(() => expect(tabLabels()).toContain("Host shell 1"));
}

describe("Terminals through the shell", () => {
  it("SNV-FR-PULG, TRM-FR-LAZH, TRM-FR-FYYB, TAB-FR-VKPS: the toggle opens the panel, and a created terminal opens in its tab", async () => {
    const focus = vi.spyOn(FakeTerminal.prototype, "focus");
    render(<App />);
    await enterIde();
    await openPanelAndCreate();
    // TRM-FR-FYYB: the new tab takes the focus.
    await waitFor(() => expect(focus).toHaveBeenCalled());
    expect(within(activityBar()).getByRole("button", { name: "Terminals" })).toHaveAttribute("data-active", "true");
    expect(document.querySelector(".terminal-view [data-fake-terminal]")).not.toBeNull();
    expect(named("create_terminal")[0].args).toMatchObject({ kind: "host_shell", target: TARGET.target });
    await waitFor(() => expect(named("attach_terminal")).toHaveLength(1));
    // TAB-FR-VKPS: a second open of the same terminal focuses its tab.
    await userEvent.click(screen.getByRole("option", { name: /^Host shell 1,/ }));
    expect(tabLabels().filter((l) => l === "Host shell 1")).toHaveLength(1);
  });

  it("TAB-FR-KIIW, TRM-FR-DURY: Pin tab pins the terminal, and the row shows the marker", async () => {
    render(<App />);
    await enterIde();
    await openPanelAndCreate();
    fireEvent.contextMenu(terminalTab()!);
    await userEvent.click(await screen.findByRole("menuitem", { name: "Pin tab" }));
    await waitFor(() => expect(named("set_terminal_pinned")[0]?.args).toEqual({ terminalId: "t1", pinned: true }));
    await waitFor(() => expect(terminalTab()).toHaveAttribute("data-pinned", "true"));
    expect(screen.getByRole("option", { name: /^Host shell 1,/ })).toHaveAttribute("data-pinned", "true");
  });

  it("TAB-FR-JFZR, TRM-FR-JXLA: closing the tab detaches the terminal; the row stays and reopens it pinned", async () => {
    render(<App />);
    await enterIde();
    await openPanelAndCreate();
    fireEvent.contextMenu(screen.getByRole("option", { name: /^Host shell 1,/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Pin terminal" }));
    await waitFor(() => expect(terminalTab()).toHaveAttribute("data-pinned", "true"));
    await userEvent.click(screen.getByTestId("close-terminal:t1"));
    await waitFor(() => expect(tabLabels()).not.toContain("Host shell 1"));
    await waitFor(() => expect(named("detach_terminal")).toHaveLength(1));
    expect(named("close_terminal")).toHaveLength(0);
    await userEvent.click(screen.getByRole("option", { name: /^Host shell 1,/ }));
    await waitFor(() => expect(terminalTab()).toHaveAttribute("data-pinned", "true"));
  });

  it("TRM-FR-OCSN: Close terminal ends the session and its tab goes", async () => {
    sessions = [];
    render(<App />);
    await enterIde();
    await openPanelAndCreate();
    sessions[0].state = "exited";
    await act(async () => {
      for (const handler of [...(eventHandlers["terminals-changed"] ?? [])]) handler({ payload: {} });
    });
    fireEvent.contextMenu(await screen.findByRole("option", { name: /^Host shell 1, exited/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close terminal" }));
    await waitFor(() => expect(named("close_terminal")).toHaveLength(1));
    await waitFor(() => expect(tabLabels()).not.toContain("Host shell 1"));
  });
});
