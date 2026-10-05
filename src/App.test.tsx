/**
 * The application shell itself: the vertical panel splitter, the settings
 * child windows and the rehearsal they ask for, the in-window project switch,
 * and the end-to-end worktree switch.
 *
 * The rest of the shell suite is in the `App.<topic>.test.tsx` siblings.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { mintAddress } from "./state/notificationAddress";
import {
  resetNotifications,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
} from "./state/notifications";
import { REHEARSAL_DELAY_MS } from "./components/NotificationSettings";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import {
  ACTIVITY_BAR_PX,
  DEFAULT_VPANEL_FRACTION,
  fractionToPx,
  VPANEL_MIN_PX,
} from "./state/panelLayout";
import { resetPanelReveals } from "./state/panelReveal";
import {
  activeWorktreePath,
  defaultInvoke,
  enterIde,
  resetAppFixture,
  ROSTER_AGENT_ID,
  tabLabels,
} from "./test/appFixtures";

const invokeMock = vi.fn();
const confirmMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
// The Library panel (mounted in the IDE) subscribes to the backend
// `"project tree changed"` event, and the File menu reaches the frontend via
// Tauri events. Stub the event bus with a capturing registry so the real Tauri
// runtime is never required and tests can fire menu events deterministically.
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
const { emitMock } = vi.hoisted(() => ({ emitMock: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  // SWN-FR-01: three windows now, so this window both announces and listens.
  emit: (...args: unknown[]) => emitMock(...args),
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
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
    LogicalSize,
    getCurrentWindow: () => win,
    availableMonitors: async () => [
      {
        name: "primary",
        size: { width: 2560, height: 1440 },
        position: { x: 0, y: 0 },
        workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
        scaleFactor: 1,
      },
    ],
  };
});
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  resetAppFixture();
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) =>
      defaultInvoke(cmd, args),
  );
  // The app-preferences record is cached at module scope for the application's
  // life (so a theme write can carry the full-screen flag through, GSS-FR-20).
  // Reset it between tests, or the first test's persisted theme is served to
  // every later one.
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  confirmMock.mockReset();
  emitMock.mockReset();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("confirm", confirmMock);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Vertical panel splitter wiring (SNV-FR-33 / SNV-FR-37)", () => {
  it("renders a resize separator on the panel's edge in the IDE shell", async () => {
    render(<App />);
    await enterIde();

    const resizer = screen.getByTestId("vpanel-resizer");
    expect(resizer).toBeInTheDocument();
    // Announced as what it is, not as an unlabelled div.
    expect(resizer).toHaveAttribute("role", "separator");
    expect(resizer).toHaveAttribute("aria-orientation", "vertical");
    expect(resizer).toHaveAttribute("data-dragging", "false");
  });

  it("drives the shell's panel grid track from the resolved width", async () => {
    // The panel column must come from the hook rather than staying at the
    // hard-coded 280px in kit.css, or dragging would move a handle that
    // resizes nothing. Assert the actual number: an implementation stuck at
    // the clamp floor (or at any constant) passes a shape-only regex.
    render(<App />);
    await enterIde();

    const shell = document.querySelector(".shell") as HTMLElement;
    expect(shell).not.toBeNull();
    // jsdom reports a zero-width element, so the hook falls back to
    // window.innerWidth (1024 by default) for the shell measurement.
    const inner = window.innerWidth - ACTIVITY_BAR_PX;
    const expected = fractionToPx(DEFAULT_VPANEL_FRACTION, inner);
    expect(shell.style.gridTemplateColumns).toBe(`44px ${expected}px 1fr`);
    expect(expected).toBeGreaterThan(VPANEL_MIN_PX);
  });

  it("resizes the panel from a real pointer drag on the handle (SNV-FR-33, SNV-FR-36)", async () => {
    // Every other drag test calls the hook's `onResizeStart` directly. This one
    // goes through the DOM, so a wiring mistake in PanelResizer — onMouseDown
    // instead of onPointerDown, a handler that never receives clientX — is
    // caught rather than shipped.
    render(<App />);
    await enterIde();

    const shell = document.querySelector(".shell") as HTMLElement;
    const before = shell.style.gridTemplateColumns;
    const resizer = screen.getByTestId("vpanel-resizer");

    await act(async () => {
      resizer.dispatchEvent(
        new MouseEvent("pointerdown", { bubbles: true, clientX: 225 }),
      );
    });
    expect(shell.getAttribute("style")).toContain("44px");
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 344 }));
    });

    const during = shell.style.gridTemplateColumns;
    expect(during).not.toBe(before);
    // 344 client-x minus the 44px activity bar = a 300px panel.
    expect(during).toBe("44px 300px 1fr");

    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });

    const saves = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saves).toHaveLength(1);
    const payload = (
      saves[0][1] as { preferences: Record<string, unknown> }
    ).preferences;
    expect(payload.verticalPanelFraction).toBeCloseTo(
      300 / (window.innerWidth - ACTIVITY_BAR_PX),
      5,
    );
  });

  it("has no splitter on the Project picker", async () => {
    render(<App />);
    await screen.findByText("acme");
    expect(screen.queryByTestId("vpanel-resizer")).toBeNull();
  });
});

describe("the notification rehearsal the settings window asks for (GLS-FR-27)", () => {
  const posts = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "post_notification")
      .map((c) => (c[1] as { request: Record<string, unknown> }).request);

  const address = () =>
    mintAddress("~/dev/acme", activeWorktreePath, {
      kind: "settings",
      which: "global",
    });

  beforeEach(() => {
    resetNotifications();
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
  });
  afterEach(() => {
    vi.useRealTimers();
    resetNotifications();
  });

  it("NTF-FR-25, NTF-FR-17, GLS-FR-27: posts after the delay, even once that window has gone", async () => {
    // The whole point of the rehearsal is that the author leaves before it
    // fires, and closing the Global settings window is one of the ways they may
    // leave (SWN-FR-01). The delay and the raise therefore live HERE rather
    // than in the section that asks for them — a timer held in that window
    // would go with it and take the notification with it.
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    render(<App />);
    await enterIde();
    // Installed only once the shell is up: `enterIde` waits on real timers.
    vi.useFakeTimers();

    act(() => {
      fireBusEvent("settings-window:rehearsal-requested", { address: address() });
      // …and the window closes before the delay elapses.
      fireBusEvent("settings-window:changed", { open: null });
    });
    expect(posts()).toHaveLength(0);

    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS);
    });

    expect(posts()).toHaveLength(1);
    expect(posts()[0].key).toBe("notifications:rehearsal");
    expect(posts()[0].payload).toBe(address());
  });

  it("NTF-FR-25, NTF-FR-08: posts nothing while that window keeps focus for the whole delay", async () => {
    // NTF-FR-08: the address names what the author is looking at, and a
    // settings window holding focus is a window of the application holding it.
    // Both halves matter — a snapshot that read only THIS window's focus would
    // post, because the settings window is the one that has it.
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    render(<App />);
    await enterIde();
    // Installed only once the shell is up: `enterIde` waits on real timers.
    vi.useFakeTimers();

    act(() => {
      fireBusEvent("settings-window:changed", { open: "global" });
      fireBusEvent("settings-window:focus", { focused: true });
      fireBusEvent("settings-window:rehearsal-requested", { address: address() });
    });

    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS);
    });

    expect(posts()).toHaveLength(0);
  });

  it("posts when the OTHER settings window is the one open (SWN-FR-05)", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    render(<App />);
    await enterIde();
    // Installed only once the shell is up: `enterIde` waits on real timers.
    vi.useFakeTimers();

    act(() => {
      fireBusEvent("settings-window:changed", { open: "project" });
      fireBusEvent("settings-window:focus", { focused: true });
      fireBusEvent("settings-window:rehearsal-requested", { address: address() });
    });

    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS);
    });

    expect(posts()).toHaveLength(1);
  });

  it("takes its pending rehearsal with it when the shell goes", async () => {
    // A timer left armed would fire against whatever backend mock is installed
    // by then, which reads as a notification appearing from nowhere.
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const { unmount } = render(<App />);
    await enterIde();
    vi.useFakeTimers();

    act(() => {
      fireBusEvent("settings-window:rehearsal-requested", { address: address() });
    });
    unmount();

    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS * 2);
    });

    expect(posts()).toHaveLength(0);
  });
});

describe("the settings windows from the shell (GLS-FR-01, GLS-FR-02, SWN-FR-06 / SET-FR-01, SET-FR-02 / STB-FR-04, SWN-FR-01, SWN-FR-02, SNV-FR-03)", () => {
  it("opens the Global settings child window and creates no tab", async () => {
    render(<App />);
    await enterIde();
    const before = tabLabels();

    await userEvent.click(
      screen.getByRole("button", { name: "Global settings" }),
    );

    // SWN-FR-01 / SWN-FR-18 / TAB-FR-03: a native child window, so nothing is
    // added to the strip and no settings surface is drawn in this window.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "open_settings_window"),
      ).toHaveLength(1),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")![1],
    ).toEqual({ kind: "global", section: null });
    expect(tabLabels()).toEqual(before);
    expect(screen.queryByTestId("global-settings")).toBeNull();
  });

  it("opens the Project settings child window and creates no tab either", async () => {
    render(<App />);
    await enterIde();
    const before = tabLabels();

    await userEvent.click(
      screen.getByRole("button", { name: "Project settings" }),
    );

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "open_settings_window"),
      ).toHaveLength(1),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")![1],
    ).toEqual({ kind: "project", section: null });
    expect(tabLabels()).toEqual(before);
  });

  // SWN-FR-01, SWN-FR-18: opening and closing either window leaves the strip exactly as it
  // was — no tab focused, none closed, and no settings tab at any moment.
  it("SWN-FR-01, SWN-FR-18: leaves the tab strip untouched", async () => {
    render(<App />);
    await enterIde();
    // Open a real artifact from the Library so the strip holds more than the
    // Dashboard and something other than it is active.
    await userEvent.click(await screen.findByText("CHG-changes.md"));
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
    const before = tabLabels();
    const activeBefore = document
      .querySelector(".tabstrip .tab[data-active='true'] .tab__label")
      ?.textContent;
    expect(activeBefore).toBe("CHG-changes.md");

    await userEvent.click(
      screen.getByRole("button", { name: "Global settings" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Project settings" }),
    );

    expect(tabLabels()).toEqual(before);
    expect(
      document.querySelector(".tabstrip .tab[data-active='true'] .tab__label")
        ?.textContent,
    ).toBe(activeBefore);
  });

  // AGT-FR-06 / AGT-FR-07: the chrome roster is how an agent is
  // reached to be looked at or changed, and how a new one is described. Both
  // open the Global settings WINDOW on its Agents section with an editor open —
  // and since that window shares no state with this one (SWN-FR-01), the editor
  // has to travel as part of the section address or not at all.
  it("AGT-FR-06, AGT-FR-07: routes a roster row to the Global settings Agents section", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    await userEvent.click(await screen.findByTestId("agents-roster-row"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")?.[1],
      ).toEqual({ kind: "global", section: `agents:${ROSTER_AGENT_ID}` }),
    );
    // AGT-FR-06, AGT-FR-07: …and the roster closed behind it.
    await waitFor(() =>
      expect(screen.queryByTestId("agents-roster")).toBeNull(),
    );
  });

  it("AGT-FR-06, AGT-FR-07: routes Add an agent to the same section with a fresh editor", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    await userEvent.click(await screen.findByTestId("agents-roster-add"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")?.[1],
      ).toEqual({ kind: "global", section: "agents:new" }),
    );
  });

  // SWN-FR-10 / GLS-FR-13: no discard-unsaved-changes prompt exists anywhere in
  // this window on account of a settings window — there is no settings tab to
  // close and nothing here to confirm.
  it("raises no discard prompt on account of a settings window", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(
      screen.getByRole("button", { name: "Global settings" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Project settings" }),
    );

    expect(confirmMock).not.toHaveBeenCalled();
  });
});

describe("In-window project switch (OVW-FR-11 / SNV-FR-19 / SNV-FR-22)", () => {
  function twoRecentInvoke(
    cmd: string,
    args?: { path?: string },
  ): unknown {
    if (cmd === "list_recent_projects") {
      return [
        { name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-06-20T10:00:00Z" },
        { name: "beta", path: "~/dev/beta", lastOpenedAt: "2026-06-19T10:00:00Z" },
      ];
    }
    if (cmd === "open_project_at_path") {
      const path = args?.path ?? "~/dev/acme";
      return { name: path.includes("beta") ? "beta" : "acme", path };
    }
    return defaultInvoke(cmd);
  }

  it("OVW-FR-11: switching to a recent project tears down the window and reopens with Dashboard + Library", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { path?: string }) =>
      twoRecentInvoke(cmd, args),
    );

    const { container } = render(<App />);
    await enterIde();

    // Build up some non-default state for project A: an extra tab, a different
    // vertical-panel surface (Notes), and an open bottom panel.
    await userEvent.click(await screen.findByText("CHG-changes.md"));
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Notes" })).toHaveAttribute(
        "data-active",
        "true",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Runs" }));
    await waitFor(() =>
      expect(container.querySelector(".bottom-panel")).toBeTruthy(),
    );

    // Switch to project B via the top-chrome switcher.
    const treeLoadsBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_project_tree",
    ).length;
    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await userEvent.click(await within(menu).findByText("beta"));

    // The window reopened for B: the switcher trigger shows the new project.
    await waitFor(() =>
      expect(screen.getByTestId("project-switcher")).toHaveTextContent("beta"),
    );
    // open_project_at_path was invoked for B's path (SNV-FR-19).
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "open_project_at_path" && c[1]?.path === "~/dev/beta",
      ),
    ).toBe(true);
    // The previous project's tabs and panel state are discarded (OVW-FR-11):
    // the extra tab is gone, Library is the default panel again, and the bottom
    // panel is hidden.
    expect(tabLabels()).not.toContain("CHG-changes.md");
    expect(
      screen.getByRole("button", { name: "Project" }),
    ).toHaveAttribute("data-active", "true");
    expect(container.querySelector(".bottom-panel")).toBeNull();
    // The main-window subtree remounted for B: project-scoped surfaces re-fetch
    // for the newly-activated project rather than showing A's data (OVW-FR-11).
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree")
          .length,
      ).toBeGreaterThan(treeLoadsBefore),
    );
  });

  it("OVW-FR-02, OVW-FR-11: 'Open Another Project…' returns to the Project picker", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { path?: string }) =>
      twoRecentInvoke(cmd, args),
    );

    render(<App />);
    await enterIde();

    await userEvent.click(screen.getByTestId("project-switcher"));
    await userEvent.click(
      await screen.findByTestId("project-switcher-open-another"),
    );

    // Back on the picker; the main shell is gone.
    expect(await screen.findByTestId("picker-left")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Global settings" }),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// End-to-end worktree switch (OVW-FR-12 / LIB-FR-13 / CHG-FR-28 / GIT-FR-11)
// ---------------------------------------------------------------------------

describe("switching the active worktree from the top chrome (OVW-FR-12)", () => {
  const treeLoads = () =>
    invokeMock.mock.calls.filter(
      (c) => c[0] === "load_project_tree" || c[0] === "rescan_project_tree",
    ).length;

  // LIB-FR-02, LIB-FR-13 / CHG-FR-26, CHG-FR-09: the surfaces bound to the content root reload. This
  // is what the shell subtree's key buys, and nothing else asserts it.
  it("reloads the content-root surfaces and returns the viewport to its fresh state", async () => {
    render(<App />);
    await enterIde();
    await waitFor(() => expect(treeLoads()).toBeGreaterThan(0));

    // Open a tab so there is something for the switch to close, and remember
    // how many times the Library had loaded before it.
    await userEvent.click(await screen.findByText("CHG-changes.md"));
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
    const before = treeLoads();

    // Switch to the other worktree from the chrome selector.
    await userEvent.click(screen.getByTestId("worktree-selector"));
    await userEvent.click(await screen.findByTitle("~/dev/acme-main"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "activate_worktree"),
      ).toHaveLength(1),
    );
    // The Library remounted against the new content root.
    await waitFor(() => expect(treeLoads()).toBeGreaterThan(before));
    // The viewport is back to Dashboard-only (TAB-FR-14)…
    await waitFor(() =>
      expect(tabLabels()).not.toContain("CHG-changes.md"),
    );
    // …the chrome relabelled to the new checkout (WTS-FR-25)…
    await waitFor(() =>
      expect(screen.getByTestId("worktree-selector")).toHaveTextContent("main"),
    );
    // …and the main window was not torn down: same project, still in the IDE.
    expect(screen.getByTestId("project-switcher")).toHaveTextContent("acme");
  });

  // WTC-FR-21 end to end: this is the only place the `canCheckOutBranches`
  // thread from App through BottomPanel into the Git panel is exercised — a
  // mis-wire to a constant would pass every component-level test.
  it("offers no branch checkout anywhere once a linked worktree is active", async () => {
    render(<App />);
    await enterIde();

    // In the repository's own checkout, both routes offer a checkout.
    await userEvent.click(screen.getByTestId("worktree-selector"));
    const group = await screen.findByTestId("branch-group");
    expect(within(group).getByText("develop")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    // Switch to the linked worktree.
    await userEvent.click(screen.getByTestId("worktree-selector"));
    await userEvent.click(await screen.findByTitle("~/dev/acme-main"));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "activate_worktree"),
      ).toHaveLength(1),
    );

    // The selector no longer lists a single branch…
    await userEvent.click(screen.getByTestId("worktree-selector"));
    await screen.findByTestId("worktree-group");
    expect(screen.queryByTestId("branch-group")).toBeNull();
    await userEvent.keyboard("{Escape}");

    // …and the Git panel lists them but offers no checkout.
    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    await userEvent.click(await screen.findByText("Branches"));
    const section = await screen.findByTestId("git-branches");
    expect(within(section).getByText("develop")).toBeInTheDocument();
    expect(within(section).queryByTitle(/^Check out /)).toBeNull();
  });

  // SNV-FR-32, WTS-FR-02, WTS-FR-29 end to end: no repository, no selector.
  it("renders no selector for a project outside a Git repository", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_active_worktree") throw "not a git repository";
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    expect(screen.getByTestId("project-switcher")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByTestId("worktree-selector")).toBeNull(),
    );
  });
});
