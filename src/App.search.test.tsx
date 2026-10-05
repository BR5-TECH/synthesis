/**
 * Universal search, end to end (SCH-search.md / TAB-tabs.md / EDT-editor.md).
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
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { pickSelector } from "./test/selectors";
import { resetPanelReveals } from "./state/panelReveal";
import {
  defaultInvoke,
  enterIde,
  resetAppFixture,
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

/** Stream one batch plus the terminal event for the most recent dispatch. */
function streamSearch(searchId: string, hits: unknown[]) {
  fireBusEvent("search-results", { searchId, hits });
  fireBusEvent("search-ended", { searchId, reason: "completed" });
}

/** A Files-group hit for a file the scan surfaces with no artifact type. */
const plainFileHit = {
  id: "src/main.rs",
  name: "main.rs",
  path: "src/main.rs",
  ordinal: 0,
  group: "file",
  matchKind: "content",
  line: 87,
  snippet: "// find me: the walker starts here",
};

describe("search click-through to a plain text file (SCH-FR-09, ESH-FR-ATDS, TAB-FR-02 / TAB-FR-04, TAB-FR-05 / ESH-FR-VASA, ESH-FR-BLTT, EDT-FR-16, EDT-FR-70, EDT-FR-28, EDT-FR-29)", () => {
  /** Ids `start_search` handed back, in dispatch order. */
  let dispatched: string[] = [];

  beforeEach(() => {
    dispatched = [];
    let counter = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "start_search") {
        const id = `search-${counter++}`;
        dispatched.push(id);
        return id;
      }
      if (cmd === "load_artifact_contents_by_id")
        return { body: "fn main() {}", checksum: "ck-rs" };
      return defaultInvoke(cmd);
    });
  });

  async function searchForPlainFile() {
    render(<App />);
    await enterIde();
    const input = screen.getByPlaceholderText(/Search artifacts/i);
    await userEvent.type(input, "find me");
    await waitFor(() => expect(dispatched.length).toBeGreaterThan(0));
    act(() => streamSearch(dispatched[dispatched.length - 1], [plainFileHit]));
    const overlay = document.querySelector(".search-overlay") as HTMLElement;
    return within(overlay).findByText("main.rs");
  }

  it("SCH-FR-09, ESH-FR-ATDS, TAB-FR-02 / ESH-FR-VASA, ESH-FR-BLTT, EDT-FR-16, EDT-FR-70, EDT-FR-28, EDT-FR-29: opens an Editor tab on the file with the ordinary cluster", async () => {
    const result = await searchForPlainFile();
    // SCH-FR-17: the snippet renders with its line number beneath the result.
    expect(screen.getByText("87")).toBeInTheDocument();

    await userEvent.click(result);

    // An Editor tab on the file, rendering its content.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "load_artifact_contents_by_id" && c[1]?.id === "src/main.rs",
        ),
      ).toBe(true),
    );
    expect(tabLabels()).toContain("main.rs");
    // EDT-FR-16 / ESH-FR-ROWK: the cluster holds the same two controls it holds
    // for a typed artifact, and no artifact-scoped action of any kind.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Inject" })).toBeNull(),
    );
    expect(screen.queryByText(/last injected/i)).toBeNull();
  });

  it("TAB-FR-02, TAB-FR-04, TAB-FR-05: activating the same file from the Library focuses the search-opened tab", async () => {
    // The point of the scenario is that two DIFFERENT surfaces converge on one
    // stable key (TAB-FR-04): search opens it, then the Library's "All files"
    // lens reaches the same file and must find the tab already there.
    const result = await searchForPlainFile();
    await userEvent.click(result);
    await waitFor(() => expect(tabLabels()).toContain("main.rs"));
    const tabsAfterFirst = tabLabels().length;

    // Dismiss the overlay, then reach the same file from the Library.
    await userEvent.keyboard("{Escape}");
    await pickSelector("Filter by type", "files");
    const panel = document.querySelector(".vpanel") as HTMLElement;
    await userEvent.click(await within(panel).findByText("main.rs"));

    // TAB-FR-04 / TAB-FR-05: focus jumps, no second tab.
    expect(tabLabels()).toHaveLength(tabsAfterFirst);
    expect(tabLabels().filter((l) => l === "main.rs")).toHaveLength(1);
  });
});

describe("overlay and full results tab are two searches (SCH-FR-18, SCH-FR-20)", () => {
  let dispatched: Array<{ id: string; scope: string }> = [];

  beforeEach(() => {
    dispatched = [];
    let counter = 0;
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "start_search") {
          const id = `search-${counter++}`;
          dispatched.push({ id, scope: args?.scope as string });
          return id;
        }
        return defaultInvoke(cmd);
      },
    );
  });

  it("SCH-FR-18, SCH-FR-20: dismissing the overlay cancels only the overlay's capped search", async () => {
    render(<App />);
    await enterIde();
    const input = screen.getByPlaceholderText(/Search artifacts/i);
    await userEvent.type(input, "find me");
    await waitFor(() => expect(dispatched).toHaveLength(1));
    // SCH-FR-18: the overlay's search is capped.
    expect(dispatched[0].scope).toBe("capped");

    // Enter opens the results tab, which dispatches its own full sweep.
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(dispatched).toHaveLength(2));
    expect(dispatched[1].scope).toBe("full");

    // Opening the tab dismissed the overlay, which cancels ITS search — and
    // only its: the tab's full sweep is left alone.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "cancel_search" && c[1]?.searchId === dispatched[0].id,
        ),
      ).toBe(true),
    );
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "cancel_search" && c[1]?.searchId === dispatched[1].id,
      ),
    ).toBe(false);
    // And the tab is open on that query.
    expect(tabLabels()).toContain("Search: find me");
  });
});
