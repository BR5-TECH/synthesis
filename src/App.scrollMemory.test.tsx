/**
 * A tab keeps its scroll position while it is open, end to end through the
 * shell (`EDT-editor.md` EDT-FR-UERT, `TAB-tabs.md` TAB-FR-09).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetLogBufferForTest } from "./logging";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import { pruneTabScroll, resetTabScrollForTest } from "./state/tabScroll";
import { defaultInvoke, enterIde, resetAppFixture } from "./test/appFixtures";
import { installFakeResizeObserver, installFakeScroll } from "./test/fakeScroll";
import { pickSelector } from "./test/selectors";

let restoreScroll: () => void;
let restoreObserver: () => void;

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
  resetTabScrollForTest();
  resetLogBufferForTest();
  restoreScroll = installFakeScroll();
  restoreObserver = installFakeResizeObserver();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  restoreObserver();
  restoreScroll();
  resetLogBufferForTest();
});

const strip = () => screen.getByTestId("tabstrip");
const tabByLabel = (label: string) =>
  Array.from(strip().querySelectorAll<HTMLElement>("[data-tab-id]")).find(
    (el) => el.querySelector(".tab__label")?.textContent === label,
  )!;

async function activate(label: string) {
  await userEvent.click(within(strip()).getByText(label));
}

/** The rich body of the Markdown file on screen. */
const richBody = async () =>
  waitFor(() => {
    const el = document.querySelector(".editor") as HTMLElement | null;
    if (!el || el.style.display === "none") throw new Error("no rich body");
    return el;
  });
/** The source surface of the source file on screen. */
const sourceSurface = async () =>
  (await screen.findByLabelText("Source")) as HTMLTextAreaElement;

function scroll(el: HTMLElement, top: number) {
  el.scrollTop = top;
  fireEvent.scroll(el);
}

async function openTwoFiles() {
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
}

describe("scroll position through the shell (EDT-FR-UERT / TAB-FR-09)", () => {
  it("EDT-FR-UERT, TAB-FR-09: each tab comes back where the author left it", async () => {
    await openTwoFiles();
    scroll(await sourceSurface(), 200);

    await activate("CHG-changes.md");
    const first = await richBody();
    expect(first.scrollTop).toBe(0);
    scroll(first, 600);

    // The offsets are held under the ids of the tabs in the strip, so a prune
    // that keeps exactly those tabs keeps them.
    pruneTabScroll(
      new Set(["CHG-changes.md", "main.rs"].map((l) => tabByLabel(l).dataset.tabId!)),
    );

    await activate("main.rs");
    expect((await sourceSurface()).scrollTop).toBe(200);

    await activate("CHG-changes.md");
    expect((await richBody()).scrollTop).toBe(600);
  });

  it("EDT-FR-UERT: a tab that was closed and opened again starts at the top", async () => {
    await openTwoFiles();
    await activate("CHG-changes.md");
    scroll(await richBody(), 600);
    const id = tabByLabel("CHG-changes.md").dataset.tabId!;

    fireEvent.contextMenu(tabByLabel("CHG-changes.md"));
    await userEvent.click(await screen.findByTestId("tabmenu-close"));
    await waitFor(() =>
      expect(within(strip()).queryByText("CHG-changes.md")).toBeNull(),
    );

    const panel = document.querySelector(".vpanel") as HTMLElement;
    await userEvent.click(await within(panel).findByText("CHG-changes.md"));
    await within(strip()).findByText("CHG-changes.md");
    // The same tab id, so only the drop on close explains the top.
    expect(tabByLabel("CHG-changes.md").dataset.tabId).toBe(id);
    expect((await richBody()).scrollTop).toBe(0);
  });
});
