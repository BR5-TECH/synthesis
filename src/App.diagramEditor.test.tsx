import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen } from "@tauri-apps/api/event";

vi.mock("mermaid", async () => (await import("./test/mermaidMock")).mermaidModule);

import App from "./App";
import { resetDiagramEditorForTest } from "./components/DiagramEditor/store";
import { resetMermaidForTest } from "./components/markdownMermaidRender";
import { resetMermaidMock } from "./test/mermaidMock";
import { resetLogBufferForTest } from "./logging";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { cancelAllScheduledWrites } from "./state/writeSchedule";

const listenMock = vi.mocked(listen);

// App-level wiring for the diagram editor overlay (EDT-editor.md
// EDT-FR-TOVY): in the main window it is one of the mutually-exclusive
// floating overlays of SNV-FR-56, it blocks the window, and it closes when the
// window is replaced. The menu entries reach the frontend as events, which
// these tests capture and fire.

const openUrlMock = vi.fn(async (_url: string) => {});

const invokeMock = vi.fn();
const isFullscreenMock = vi.fn(async () => false);
const setFullscreenMock = vi.fn(async (_v: boolean) => {});
type BusHandler = (event: { payload: unknown }) => void;

// Every registered handler, per event name. An array rather than a single
// handler because `listen` really does support more than one subscriber on a
// channel, and more than one part of the shell uses that: the artifact edit
// store and the Flow store both watch `"artifact changed externally"` and answer
// it differently (EXC-FR-VNLZ / FLO-FR-31). A last-one-wins mock would silently
// disconnect whichever registered first.
let busHandlers: Record<string, BusHandler[]> = {};
// The view the tests read: one callable per event that fans out to every
// handler registered for it, and no key at all once the last one detaches.
let listeners: Record<string, BusHandler> = {};

function refreshListeners() {
  listeners = {};
  for (const [event, handlers] of Object.entries(busHandlers)) {
    if (handlers.length === 0) continue;
    listeners[event] = (e) => {
      for (const h of [...handlers]) h(e);
    };
  }
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: BusHandler) => {
    (busHandlers[event] ??= []).push(cb);
    refreshListeners();
    return () => {
      busHandlers[event] = (busHandlers[event] ?? []).filter((h) => h !== cb);
      refreshListeners();
    };
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  // Captured `onResized` handler so a re-show after leaving full-screen settles
  // (the picker awaits the post-exit resize before sizing/centering).
  let resizeHandler: (() => void) | null = null;
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    isFullscreen: async () => isFullscreenMock(),
    setFullscreen: async (v: boolean) => {
      await setFullscreenMock(v);
      if (v === false && resizeHandler) resizeHandler();
    },
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async (cb: () => void) => {
      resizeHandler = cb;
      return () => {
        resizeHandler = null;
      };
    },
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

function defaultInvoke(cmd: string) {
  switch (cmd) {
    case "list_recent_projects":
      return [
        { name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-05-15T10:00:00Z" },
      ];
    case "open_project_at_path":
      return { name: "acme", path: "~/dev/acme" };
    case "load_layout_preferences":
      return null;
    case "load_app_preferences":
      return { theme: "dark" };
    case "load_project_tree":
      return {
        id: "",
        name: "acme",
        path: "",
        nodeKind: "folder",
        children: [
          {
            id: "a.md",
            name: "a.md",
            path: "a.md",
            nodeKind: "file",
            artifactType: "spec",
          },
          {
            id: "b.md",
            name: "b.md",
            path: "b.md",
            nodeKind: "file",
            artifactType: "spec",
          },
        ],
      };
    case "validate_flow_document":
      // FGV-FR-02 / FLO-FR-46: the backend judges a Flow body before the canvas
      // renders it. The rules themselves are covered by the Rust suite.
      return { valid: true, violations: [] };
    case "load_artifact_contents_by_id":
      return { body: "# heading\n\n```mermaid\ngraph TD\nA-->B\n```\n", checksum: "ck1" };
    case "save_artifact_contents":
      return { checksum: "ck-saved" };
    case "list_installed_plugins":
    case "list_agent_adapters":
    case "list_recently_edited_artifacts":
      return [];
    case "list_drafts":
      return { folders: [], drafts: [] };
    // DRS-FR-06: `create_draft` hands back the record *and* the one Markdown
    // file the draft was created holding, which the tab opens on.
    case "create_draft":
      return {
        draft: {
          id: "d1",
          name: "Untitled",
          promptPath: "Untitled.md",
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        },
        file: "Untitled.md",
      };
    case "open_draft":
    case "rename_draft":
    case "set_draft_status":
      return {
        id: "d1",
        name: "Untitled",
        promptPath: "Untitled.md",
        status: "active",
        createdAt: "2026-07-31T10:00:00Z",
        updatedAt: "2026-07-31T10:00:00Z",
      };
    case "list_draft_files":
      return [{ path: "Untitled.md", name: "Untitled.md", nodeKind: "file" }];
    case "load_draft_file_contents":
      return { body: "", checksum: "c0" };
    case "save_draft_file_contents":
      return { checksum: "c1" };
    case "search_drafts":
      return [];
    default:
      return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  }
}


beforeEach(() => {
  busHandlers = {};
  refreshListeners();
  listenMock.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => defaultInvoke(cmd));
  resetLogBufferForTest();
  resetDiagramEditorForTest();
  resetMermaidForTest();
  resetMermaidMock();
  openUrlMock.mockClear();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  isFullscreenMock.mockClear();
  isFullscreenMock.mockResolvedValue(false);
  setFullscreenMock.mockClear();
  setFullscreenMock.mockResolvedValue(undefined);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cancelAllScheduledWrites();
  cleanup();
  resetLogBufferForTest();
  vi.unstubAllGlobals();
});

async function enterIde() {
  await userEvent.click(await screen.findByText("acme"));
  await screen.findByRole("button", { name: "Settings" });
}

async function fireMenu(event: string) {
  await act(async () => {
    listeners[event]?.({ payload: null });
  });
}

const diagramEditor = () => screen.queryByRole("dialog", { name: "Edit Mermaid diagram" });

async function openDiagramEditor() {
  await userEvent.click(await screen.findByText("a.md"));
  // The fence draws again once its diagram has rendered; the button is taken
  // from that drawing.
  await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
  await userEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
  await screen.findByRole("dialog", { name: "Edit Mermaid diagram" });
}

describe("the diagram editor overlay in the main window", () => {
  it("EDT-FR-TOVY, SNV-FR-56: opening another overlay closes it", async () => {
    render(<App />);
    await enterIde();
    await openDiagramEditor();

    await fireMenu("menu:about");

    await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(diagramEditor()).toBeNull();
  });

  it("EDT-FR-TOVY, SNV-FR-56: opening it closes any other overlay", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    await fireMenu("menu:new-folder");
    await screen.findByRole("dialog", { name: /new folder/i });

    // The New Folder scrim is over the button; the click reaches it directly.
    act(() => screen.getByRole("button", { name: "Edit diagram" }).click());

    await screen.findByRole("dialog", { name: "Edit Mermaid diagram" });
    expect(screen.queryByRole("dialog", { name: /new folder/i })).toBeNull();
  });

  it("EDT-FR-SJGJ: while it is open a Find activation does nothing to the Editor behind it", async () => {
    render(<App />);
    await enterIde();
    await openDiagramEditor();

    await fireMenu("menu:find");

    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
    expect(diagramEditor()).not.toBeNull();
  });

  it("EDT-FR-SJGJ, SNV-FR-56: a window shortcut does nothing while it is open, also with focus outside it", async () => {
    render(<App />);
    await enterIde();
    await openDiagramEditor();

    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(screen.queryByRole("dialog", { name: /search/i })).toBeNull();
    expect(document.querySelector(".search-overlay")).toBeNull();

    // Focus that has left the overlay comes back to it, and the key goes nowhere.
    act(() => (document.activeElement as HTMLElement | null)?.blur());
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(document.querySelector(".search-overlay")).toBeNull();
    expect(screen.getByRole("textbox", { name: "Diagram source" })).toHaveFocus();
    expect(diagramEditor()).not.toBeNull();
  });

  it("EDT-FR-TOVY, SNV-FR-56: the diagram viewer overlay is a floating overlay of the main window too", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    await fireMenu("menu:new-folder");
    await screen.findByRole("dialog", { name: /new folder/i });

    // Opening it closes the other overlay.
    act(() => screen.getByRole("button", { name: "View diagram" }).click());
    await screen.findByRole("dialog", { name: "View Mermaid diagram" });
    expect(screen.queryByRole("dialog", { name: /new folder/i })).toBeNull();

    // Opening another overlay closes it.
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(screen.queryByRole("dialog", { name: "View Mermaid diagram" })).toBeNull();
  });

  it("EDT-FR-TOVY: the diagram viewer overlay closes when its window is replaced", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    await userEvent.click(await screen.findByRole("button", { name: "View diagram" }));
    await screen.findByRole("dialog", { name: "View Mermaid diagram" });

    await fireMenu("menu:close-project");

    await screen.findByText("acme");
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "View Mermaid diagram" })).toBeNull(),
    );
  });

  it("EDT-FR-TOVY: it closes when its window is replaced", async () => {
    render(<App />);
    await enterIde();
    await openDiagramEditor();

    await fireMenu("menu:close-project");

    await screen.findByText("acme");
    await waitFor(() => expect(diagramEditor()).toBeNull());
  });
});
