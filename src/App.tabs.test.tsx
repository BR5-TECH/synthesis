/**
 * The tab strip through the shell: selection following the active tab, the tab
 * context menu, and a work stream merge started from a tab.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { mintAddress } from "./state/notificationAddress";
import { NOTIFICATION_ACTIVATED } from "./events";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { pickSelector } from "./test/selectors";
import { makeMergeRun, makeQueue, makeRun } from "./test/graduationFixtures";
import { resetPanelReveals } from "./state/panelReveal";
import {
  activeWorktreePath,
  createDraftFromPanel,
  defaultInvoke,
  enterIde,
  resetAppFixture,
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

/**
 * SNV-FR-64 … SNV-FR-69, end to end through the shell: the half no unit test
 * reaches, which is the vertical panel actually *becoming* the target surface —
 * and opening when it was hidden (SNV-FR-45).
 */
describe("Selection follows tab (SNV-FR-64 / SNV-FR-69)", () => {
  /**
   * The default `list_drafts` is empty (a worktree with nothing in it), which is
   * the right default for most of this file — but the panel has to actually hold
   * the row for a reveal to land on. This wires the one draft `create_draft`
   * hands back.
   */
  function withOneDraft() {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_drafts")
        return {
          folders: [],
          drafts: [
            {
              id: "d1",
              name: "Untitled",
              status: "active",
              folder: "",
              updatedAt: "2026-07-31T10:00:00Z",
            },
          ],
        };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
  }

  /** The title in the vertical panel's own header, i.e. which surface is on. */
  const panelTitle = () =>
    document.querySelector(".panel-header__title")?.textContent?.trim() ?? null;

  it("SNV-FR-64, SNV-FR-66, SNV-FR-68, DRP-FR-34: opening a draft makes Drafts the active surface and selects its row", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    // OVW-FR-05: the panel opens on Project.
    expect(panelTitle()).toBe("Project");

    // File → New Artifact creates a draft and opens its New Artifact tab, which
    // is an activation of a different tab (SNV-FR-65).
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });

    await waitFor(() => expect(panelTitle()).toBe("Drafts"));
    const row = await screen.findByRole("treeitem", { name: /^Draft Untitled/ });
    expect(row).toHaveAttribute("aria-selected", "true");
  });

  it("SNV-FR-64, SNV-FR-66, SNV-FR-45, LIB-FR-18: a hidden panel is opened by the follow, on the target surface", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    await waitFor(() => expect(panelTitle()).toBe("Drafts"));

    // SNV-FR-45: re-activating the active surface's toggle hides the panel.
    await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
    await waitFor(() => expect(screen.queryByTestId("vpanel-resizer")).toBeNull());

    // Leave for a tab that names nothing, then come back to the draft's tab.
    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("Dashboard"));
    await userEvent.click(within(strip).getByText("Untitled"));

    // The panel opened again, on Drafts, with the row selected.
    await waitFor(() =>
      expect(screen.getByTestId("vpanel-resizer")).toBeInTheDocument(),
    );
    expect(panelTitle()).toBe("Drafts");
    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft Untitled/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("SNV-FR-66: a Dashboard tab, or a settings window, leaves the panel exactly as it is", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    await waitFor(() => expect(panelTitle()).toBe("Drafts"));

    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("Dashboard"));
    expect(panelTitle()).toBe("Drafts");

    // SWN-FR-01 / SNV-FR-64: a settings window is no tab, so opening one is no
    // tab activation and the panel follows nothing.
    await userEvent.click(screen.getByRole("button", { name: "Global settings" }));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "open_settings_window"),
      ).toBe(true),
    );
    expect(panelTitle()).toBe("Drafts");
  });

  it("NTF-FR-18, NTF-FR-17, SNV-FR-64, SNV-FR-69: an activated notification's tab is an activation like any other", async () => {
    // NTF-FR-18: the activation performs exactly the navigation the author's own
    // equivalent gesture performs — including what that gesture carries with it.
    withOneDraft();
    render(<App />);
    await enterIde();
    expect(panelTitle()).toBe("Project");

    // A draft address for the open project on its active worktree — the two the
    // address is keyed by (NTF-FR-04), so it resolves rather than being stated
    // as unreachable.
    fireBusEvent(NOTIFICATION_ACTIVATED, {
      id: "n1",
      key: "k",
      payload: mintAddress("~/dev/acme", activeWorktreePath, {
        kind: "draft",
        draftId: "d1",
      }),
    });

    // Either the address resolved and the panel followed, or it did not resolve
    // at all — and a test that cannot tell the two apart is worth nothing, so
    // assert the tab opened first.
    const strip = await screen.findByTestId("tabstrip");
    await waitFor(() =>
      expect(within(strip).queryByText("Untitled")).toBeInTheDocument(),
    );
    await waitFor(() => expect(panelTitle()).toBe("Drafts"));
  });

  it("SNV-FR-67: a tab whose draft is gone leaves the panel and its selection alone", async () => {
    // The panel must be neither opened nor switched, which cannot be decided by
    // the panel itself — it is not mounted until the switch, and the switch is
    // what the requirement forbids. `list_drafts` returns nothing, so the shell
    // has positive evidence the draft is absent.
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_drafts") return { folders: [], drafts: [] };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    // A New Artifact tab is opened, so the strip holds one bound to draft `d1`.
    // The backend lists no drafts here, so the created draft has no row and
    // no name field opens on it (DRP-FR-36).
    await createDraftFromPanel({ listed: false });
    await screen.findByRole("button", { name: "Draft actions" });

    // Park the panel somewhere with a surface of its own, and leave for a tab
    // that names nothing.
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));
    await waitFor(() => expect(panelTitle()).toBe("Notes"));
    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("Dashboard"));

    // Back to the draft's tab. Its draft is not in the listing, so nothing moves.
    await userEvent.click(within(strip).getByText("Untitled"));

    expect(panelTitle()).toBe("Notes");
    // And nothing was rendered to explain it — there was no question asked.
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("SNV-FR-67: a hidden panel is not opened for a target that does not resolve", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_drafts") return { folders: [], drafts: [] };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    // The backend lists no drafts here, so the created draft has no row and
    // no name field opens on it (DRP-FR-36).
    await createDraftFromPanel({ listed: false });
    await screen.findByRole("button", { name: "Draft actions" });

    // Creating the draft made Drafts the active surface (SNV-FR-64). Hide the
    // panel outright from there (SNV-FR-45), then leave and come back.
    await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
    await waitFor(() => expect(screen.queryByTestId("vpanel-resizer")).toBeNull());
    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("Dashboard"));
    await userEvent.click(within(strip).getByText("Untitled"));

    // "neither opened nor hidden": it stays hidden.
    expect(screen.queryByTestId("vpanel-resizer")).toBeNull();
  });

  it("SNV-FR-69, SNV-FR-64, GLS-FR-28: with the preference off, no tab change touches the panel", async () => {
    // GSS-FR-33: the stored record says off, which the shell seeds at startup.
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_app_preferences")
        return { theme: "system", selectionFollowsTab: false };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();
    expect(panelTitle()).toBe("Project");

    await createDraftFromPanel();

    // The tab opened and is active. Park the panel back on Project — reaching
    // the affordance is what put it on Drafts, not the tab activation.
    const strip = screen.getByTestId("tabstrip");
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Project" }));
    await waitFor(() => expect(panelTitle()).toBe("Project"));

    // With the preference off, no tab change moves it.
    await userEvent.click(within(strip).getByText("Dashboard"));
    expect(panelTitle()).toBe("Project");
    await userEvent.click(within(strip).getByText("Untitled"));
    expect(panelTitle()).toBe("Project");
  });
});

/**
 * The tab context menu wired through the real shell.
 *
 * `TabStrip.test.tsx` renders the strip with `massCloseTargets` stubbed, and
 * `useShellSession.test.tsx` calls `closeTabGroup` directly — so each half is
 * proved against a fake of the other, and a wiring mistake in `App.tsx`
 * (argument order on `onCloseGroup`, `pinned` bound to the wrong set) is caught
 * by neither. This is the one place the two meet.
 */
describe("tab context menu through the shell (TAB-FR-32 / TAB-FR-37 / TAB-FR-39)", () => {
  const strip = () => screen.getByTestId("tabstrip");
  const labels = () =>
    Array.from(strip().querySelectorAll(".tab .tab__label")).map(
      (el) => el.textContent ?? "",
    );
  const tabByLabel = (label: string) =>
    Array.from(strip().querySelectorAll<HTMLElement>("[data-tab-id]")).find(
      (el) => el.querySelector(".tab__label")?.textContent === label,
    )!;

  /**
   * Dashboard plus two Editor tabs.
   *
   * Two real artifacts rather than the settings surfaces this used to open:
   * neither settings surface is a tab any more (SWN-FR-01 / TAB-FR-03), so a
   * strip built out of them would be a strip the application cannot produce.
   */
  async function threeTabs() {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("CHG-changes.md"));
    await within(strip()).findByText("CHG-changes.md");
    // The second file is unclassified, so it is reachable under the Library's
    // "All files" lens alone (LIB-FR-12).
    await pickSelector("Filter by type", "files");
    const panel = document.querySelector(".vpanel") as HTMLElement;
    await userEvent.click(await within(panel).findByText("main.rs"));
    await within(strip()).findByText("main.rs");
    expect(labels()).toEqual(["Dashboard", "CHG-changes.md", "main.rs"]);
  }

  // TAB-FR-37 end to end: the eligible set is computed by the shell, the entry
  // is rendered from it, and choosing it runs the close the same rule produced.
  it("closes other tabs, keeping the context-clicked tab and every pinned tab", async () => {
    await threeTabs();

    // Pin the Dashboard, so the mass close has a pinned tab to spare and an
    // unpinned one to take.
    fireEvent.contextMenu(tabByLabel("Dashboard"));
    await userEvent.click(await screen.findByTestId("tabmenu-pin"));
    await waitFor(() =>
      expect(tabByLabel("Dashboard")).toHaveAttribute("data-pinned", "true"),
    );

    fireEvent.contextMenu(tabByLabel("main.rs"));
    await userEvent.click(await screen.findByTestId("tabmenu-others"));

    await waitFor(() => expect(labels()).toEqual(["Dashboard", "main.rs"]));
  });

  // TAB-FR-39 composed: the derivation and the rendering meet, so a greyed
  // entry here reflects the real strip rather than a stub's answer.
  it("greys the mass closes the real strip has no eligible tab for", async () => {
    await threeTabs();

    // Nothing stands to the left of the Dashboard, whatever the pins.
    fireEvent.contextMenu(tabByLabel("Dashboard"));
    const menu = await screen.findByTestId("tab-context-menu");
    expect(screen.getByTestId("tabmenu-left")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    // Two unpinned tabs stand to its right, so those two entries act.
    expect(screen.getByTestId("tabmenu-right")).not.toHaveAttribute(
      "aria-disabled",
    );
    expect(screen.getByTestId("tabmenu-others")).not.toHaveAttribute(
      "aria-disabled",
    );
    expect(menu).toBeInTheDocument();
  });

  // TAB-FR-33, TAB-FR-36: the entry acts on the tab that was context-clicked, not on the
  // active one — the whole point of opening a menu on a tab you are not reading.
  it("closes the context-clicked tab rather than the active one", async () => {
    await threeTabs();
    // `main.rs` is the active tab, having just been opened.
    fireEvent.contextMenu(tabByLabel("CHG-changes.md"));
    await userEvent.click(await screen.findByTestId("tabmenu-close"));

    await waitFor(() => expect(labels()).toEqual(["Dashboard", "main.rs"]));
  });

  /**
   * A right-click as the browser delivers it: `mousedown` with the secondary
   * button, then `contextmenu`. Both matter — the dropdowns of SNV-FR-56 take
   * themselves down on an outside `mousedown`, so a test firing `contextmenu`
   * alone would report an exclusivity failure the real window does not have.
   */
  const rightClick = (el: HTMLElement) => {
    fireEvent.mouseDown(el, { button: 2 });
    fireEvent.contextMenu(el);
  };

  // TAB-FR-32 / SNV-FR-56, in both directions, through the real overlay wiring.
  it("is mutually exclusive with the window's other overlays", async () => {
    await threeTabs();

    // A dropdown open, then a right-click on a tab: the menu opens and the
    // dropdown is gone.
    await userEvent.click(screen.getByTestId("project-switcher"));
    await screen.findByTestId("project-switcher-menu");
    rightClick(tabByLabel("Dashboard"));
    await screen.findByTestId("tab-context-menu");
    await waitFor(() =>
      expect(screen.queryByTestId("project-switcher-menu")).toBeNull(),
    );

    // And the other way: the menu open, then a dropdown.
    await userEvent.click(screen.getByTestId("project-switcher"));
    await screen.findByTestId("project-switcher-menu");
    await waitFor(() =>
      expect(screen.queryByTestId("tab-context-menu")).toBeNull(),
    );
  });
});

/**
 * CMW-FR-KRVP / WSS-FR-TQBN: the seam between the commit message window and the
 * merge it takes a message for.
 *
 * Both halves mock the other in their own suites, so this is the one place that
 * drives the whole route: a merge may spend three agent turns, and the defect
 * this covers was the window awaiting one and trapping the application behind a
 * scrim nothing could dismiss.
 */
describe("a work stream merge started from the shell (CMW-FR-KRVP, WSS-FR-TQBN)", () => {
  const streamSummary = {
    stream: {
      id: "w1",
      projectKey: "p",
      name: "test stream",
      branch: "synthesis/stream/test-stream",
      worktreePath: "/store/w/w1",
      baseBranch: "main",
      baseRevision: "abc",
      createdAt: "2026-09-06T10:00:00Z",
      busyRunId: null,
      isMissing: false,
    },
    queuedRunCount: 0,
    aheadOfBase: 2,
  };

  const openMergeWindow = async () => {
    await enterIde();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const menu = await screen.findByTestId("stream-menu");
    await userEvent.click(within(menu).getByRole("button", { name: "Merge…" }));
    const row = await screen.findByTestId("stream-row-w1");
    await userEvent.click(within(row).getByRole("button", { name: "Commit…" }));
    return screen.findByRole("dialog");
  };

  it("CMW-FR-KRVP: confirming closes the window at once and starts the merge", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_work_streams") return [streamSummary];
      // The merge call never answers, so the row is looked at while it is out.
      if (cmd === "merge_work_stream") return new Promise(() => {});
      return defaultInvoke(cmd, args);
    });
    render(<App />);
    const dialog = await openMergeWindow();

    // CMW-FR-ZDMT: it names the stream and counts no files it was never given.
    expect(within(dialog).getByText(/Merge message · test stream/)).toBeInTheDocument();
    expect(screen.queryByText(/Commit 0 files/)).toBeNull();

    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "merge the specs",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Merge/ }));

    // The window is gone even though the merge has not answered, and the merge
    // is running with the message it was given.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const merges = invokeMock.mock.calls.filter(([c]) => c === "merge_work_stream");
    expect(merges).toHaveLength(1);
    expect(merges[0][1]).toEqual({
      streamId: "w1",
      publication: { kind: "commit", message: "merge the specs" },
    });
    // WSS-FR-HGWL: and the stream says it is merging. The dropdown closed when
    // the author moved into the window, so this is also what the merge looks
    // like to someone coming back to it. The row offers no cancel for it.
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const row = await screen.findByTestId("stream-row-w1");
    expect(await within(row).findByRole("status")).toHaveTextContent(/Merging/);
    expect(within(row).queryByRole("button", { name: /Cancel/ })).toBeNull();
  });

  it("WSS-FR-AWRS, GRU-FR-PAHN: Open in Runs opens the Runs panel on the graduation section with the merge run selected", async () => {
    const mergeRun = makeMergeRun("m-77", "working", {
      streamId: "w1",
      streamName: "test stream",
      merge: { name: "Merge test stream" },
    });
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_work_streams")
        return [
          {
            ...streamSummary,
            behindBase: 0,
            baseTipRevision: "abc",
            missingCommits: [],
            mergeRun: { runId: "m-77", name: "Merge test stream", state: "working" },
          },
        ];
      if (cmd === "list_graduation_queue")
        return makeQueue([makeRun("r-1", "working"), mergeRun]);
      if (cmd === "get_graduation_capacity")
        return { limit: "unlimited", inUse: 1, waitingForSlot: [] };
      return defaultInvoke(cmd, args);
    });
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByTestId("stream-selector"));
    const row = await screen.findByTestId("stream-row-w1");
    await userEvent.click(within(row).getByRole("button", { name: "Open in Runs…" }));

    // The route is the one a `run` address takes: the panel shows Runs, its
    // graduation section is active, and the named run is the selected one.
    const region = await screen.findByLabelText("The selected run");
    await waitFor(() => expect(region).toHaveAttribute("data-run", "m-77"));
    expect(within(region).getByText("Merge test stream")).toBeInTheDocument();
    expect(screen.queryByTestId("stream-menu")).toBeNull();
  });

  it("CMW-FR-09, WSS-FR-TQBN: dismissing the window starts no merge", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_work_streams") return [streamSummary];
      if (cmd === "merge_work_stream") return new Promise(() => {});
      return defaultInvoke(cmd, args);
    });
    render(<App />);
    const dialog = await openMergeWindow();
    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "never sent",
    );
    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(
      invokeMock.mock.calls.filter(([c]) => c === "merge_work_stream"),
    ).toHaveLength(0);
  });
});
