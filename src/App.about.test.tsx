import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen } from "@tauri-apps/api/event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { cancelAllScheduledWrites } from "./state/writeSchedule";

const listenMock = vi.mocked(listen);

// App-level wiring for the About entry (ABT-about-panel.md). The entry is a
// native menu item whose activation reaches the frontend as the `menu:about`
// event. These tests capture that listener and fire it to assert the panel:
// it opens over the Project picker and over the main window, it is one of the
// main window's mutually-exclusive overlays, it closes with its surface, and
// its link goes to the operating system's browser. Where each menu carries the
// entry is native and is covered in the Rust suite.

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
      return { body: "# heading\n", checksum: "ck1" };
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
      return undefined;
  }
}


beforeEach(() => {
  busHandlers = {};
  refreshListeners();
  listenMock.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => defaultInvoke(cmd));
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
  vi.unstubAllGlobals();
});

async function enterIde() {
  await userEvent.click(await screen.findByText("acme"));
  await screen.findByRole("button", { name: "Global settings" });
}

async function fireMenu(event: string) {
  await act(async () => {
    listeners[event]?.({ payload: null });
  });
}

const aboutDialog = () => screen.queryByRole("dialog", { name: "About Synthesis" });

describe("About panel over the Project picker", () => {
  it("ABT-FR-KMVD, ABT-FR-QZHW, ABT-FR-TNRB: the About entry opens a modal dialog with the description and the link", async () => {
    render(<App />);
    await screen.findByText("acme");
    expect(aboutDialog()).toBeNull();

    await fireMenu("menu:about");

    const dialog = await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(
      screen.getByText("AI-powered IDE for spec-driven development."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Synthesis on GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/BR5-TECH/synthesis",
    );
    // The panel is drawn inside this window, not in a window of its own.
    expect(dialog.closest(".app")).not.toBeNull();
  });

  it("PPK-FR-EWQH, ABT-FR-ZEJM: the picker stays mounted and unchanged once the panel closes", async () => {
    render(<App />);
    await screen.findByText("acme");
    const gitUrl = screen.getByPlaceholderText("git@github.com:org/repo.git");
    await userEvent.type(gitUrl, "https://example.com/repo.git");

    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });
    // The picker renders behind the panel, still holding what it held.
    expect(screen.getByText("acme")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("git@github.com:org/repo.git")).toHaveValue("https://example.com/repo.git");

    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(aboutDialog()).toBeNull();
    expect(screen.getByText("acme")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("git@github.com:org/repo.git")).toHaveValue("https://example.com/repo.git");
  });

  it("ABT-FR-NVQT: a second activation while the panel is open opens no second panel", async () => {
    render(<App />);
    await screen.findByText("acme");

    await fireMenu("menu:about");
    await fireMenu("menu:about");

    expect(screen.getAllByRole("dialog", { name: "About Synthesis" })).toHaveLength(1);
  });

  it("ABT-FR-SDFA: opening and closing the panel calls no backend operation and writes nothing", async () => {
    render(<App />);
    await screen.findByText("acme");
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    // Let the startup reads finish, so the baseline does not move underneath.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100));
    });
    const before = invokeMock.mock.calls.length;

    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });
    await userEvent.keyboard("{Escape}");

    expect(aboutDialog()).toBeNull();
    expect(invokeMock.mock.calls.length).toBe(before);
    expect(openUrlMock).not.toHaveBeenCalled();
  });
});

describe("About panel over the main window", () => {
  it("SNV-FR-KXPE, ABT-FR-KMVD: the About entry opens the panel while a project is open", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:about");

    expect(
      await screen.findByRole("dialog", { name: "About Synthesis" }),
    ).toBeInTheDocument();
  });

  it("SNV-FR-KXPE, ABT-FR-GCPK: opening About closes another open overlay", async () => {
    render(<App />);
    await enterIde();
    await fireMenu("menu:new-file");
    await screen.findByRole("dialog", { name: /new file/i });

    await fireMenu("menu:about");

    await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(screen.queryByRole("dialog", { name: /new file/i })).toBeNull();
  });

  it("SNV-FR-56, ABT-FR-GCPK: opening another overlay closes the About panel", async () => {
    render(<App />);
    await enterIde();
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });

    await fireMenu("menu:new-folder");

    await screen.findByRole("dialog", { name: /new folder/i });
    expect(aboutDialog()).toBeNull();
  });

  it("SNV-FR-56, ABT-FR-GCPK: opening About closes an open chrome dropdown", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(screen.getByTestId("project-switcher"));
    expect(screen.getByTestId("project-switcher")).toHaveAttribute("aria-expanded", "true");

    await fireMenu("menu:about");

    await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(screen.getByTestId("project-switcher")).toHaveAttribute("aria-expanded", "false");
  });

  it("ABT-FR-QZHW: while the panel is open a Find activation does nothing to the Editor behind it", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });

    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    await fireMenu("menu:find");
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
  });

  it("ABT-FR-HYFE, ABT-FR-GCPK: an overlay that replaces the panel keeps the focus it took", async () => {
    render(<App />);
    await enterIde();
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });

    await fireMenu("menu:new-folder");

    const dialog = await screen.findByRole("dialog", { name: /new folder/i });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
  });

  it("ABT-FR-LBNA: the panel closes when the project closes and the picker returns", async () => {
    render(<App />);
    await enterIde();
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });

    await fireMenu("menu:close-project");

    await screen.findByText("acme");
    await waitFor(() => expect(aboutDialog()).toBeNull());
  });
});

describe("About panel keyboard and link", () => {
  it("ABT-FR-DXGC: Escape closes the panel and leaves the window as it was", async () => {
    render(<App />);
    await enterIde();
    await fireMenu("menu:about");
    await screen.findByRole("dialog", { name: "About Synthesis" });

    await userEvent.keyboard("{Escape}");

    expect(aboutDialog()).toBeNull();
    expect(screen.getByRole("button", { name: "Global settings" })).toBeInTheDocument();
  });

  it("ABT-FR-HYFE: focus enters the panel on open and returns to the previous element on close", async () => {
    render(<App />);
    await screen.findByText("acme");
    const gitUrl = screen.getByPlaceholderText("git@github.com:org/repo.git");
    gitUrl.focus();
    expect(gitUrl).toHaveFocus();

    await fireMenu("menu:about");
    const dialog = await screen.findByRole("dialog", { name: "About Synthesis" });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    await userEvent.keyboard("{Escape}");

    expect(gitUrl).toHaveFocus();
  });

  it("ABT-FR-WPLJ: activating the link opens the address through the platform opener and keeps the panel open", async () => {
    render(<App />);
    await screen.findByText("acme");
    await fireMenu("menu:about");

    await userEvent.click(await screen.findByRole("link", { name: "Synthesis on GitHub" }));

    expect(openUrlMock).toHaveBeenCalledTimes(1);
    expect(openUrlMock).toHaveBeenCalledWith("https://github.com/BR5-TECH/synthesis");
    expect(aboutDialog()).not.toBeNull();
  });
});
