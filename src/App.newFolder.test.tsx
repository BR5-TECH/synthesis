/**
 * New Folder modal wiring (NFW-new-folder.md), end to end through the shell.
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
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { selectorValue } from "./test/selectors";
import type { TreeNode } from "./types";
import { resetPanelReveals } from "./state/panelReveal";
import {
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

describe("New Folder modal wiring (NFW-FR-09 / NFW-FR-11 / NFW-FR-01)", () => {
  // A tree with a typed folder (`specifications`, folder-scope `spec` per
  // ASC-FR-18), an empty untyped folder the default lens hides, and a top-level
  // artifact so the Library has something to render before anything is created.
  function treeWithFolders(): TreeNode & { children: TreeNode[] } {
    return {
      id: "",
      name: "",
      path: "",
      nodeKind: "folder",
      hasArtifacts: true,
      children: [
        {
          id: "specifications",
          name: "specifications",
          path: "specifications",
          nodeKind: "folder",
          hasArtifacts: true,
          artifactType: "spec",
          typeSource: "assigned",
          children: [
            {
              id: "specifications/spec-a.md",
              name: "spec-a.md",
              path: "specifications/spec-a.md",
              nodeKind: "file",
              artifactType: "spec",
              typeSource: "inherited",
            },
          ],
        },
        {
          id: "AGENTS.md",
          name: "AGENTS.md",
          path: "AGENTS.md",
          nodeKind: "file",
          artifactType: "agent",
          typeSource: "inferred",
        },
      ],
    };
  }

  it("NFW-FR-09, NFW-FR-10 / NFW-FR-11, LIB-FR-18: Create invokes create_folder, closes the modal, reveals the folder and opens no tab", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      if (cmd === "create_folder") {
        return {
          id: "notes-inbox",
          name: "notes-inbox",
          path: "notes-inbox",
          nodeKind: "folder",
          hasArtifacts: false,
          children: [],
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    // Only the Dashboard tab is open before the creation.
    expect(screen.getByText("Dashboard")).toBeInTheDocument();

    fireBusEvent("menu:new-folder");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("New Folder");

    await userEvent.type(within(dialog).getByLabelText("Name"), "notes-inbox");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "create_folder" &&
            c[1]?.name === "notes-inbox" &&
            c[1]?.location === null &&
            c[1]?.artifactType === null,
        ),
      ).toBe(true),
    );
    // The modal closed…
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    // …and NFW-FR-11: no tab opened, because a folder has no editing surface.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_artifact_by_id"),
    ).toEqual([]);
  });

  it("NFW-FR-11, LIB-FR-18: the created folder is revealed and selected, collapsed, with the lens relaxed", async () => {
    // The end-to-end reveal: the creation returns the node, the watcher-driven
    // reload brings it into the tree, and the Library then relaxes the lens that
    // would have hidden an untyped empty folder and selects the row.
    let created = false;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") {
        const tree = treeWithFolders();
        if (created) {
          tree.children.push({
            id: "notes-inbox",
            name: "notes-inbox",
            path: "notes-inbox",
            nodeKind: "folder",
            hasArtifacts: false,
            children: [],
          });
        }
        return tree;
      }
      if (cmd === "create_folder") {
        created = true;
        return {
          id: "notes-inbox",
          name: "notes-inbox",
          path: "notes-inbox",
          nodeKind: "folder",
          hasArtifacts: false,
          children: [],
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    // Under the default All Artifacts lens the folder is not there yet.
    expect(screen.queryByText("notes-inbox")).not.toBeInTheDocument();

    fireBusEvent("menu:new-folder");
    let dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "notes-inbox");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // The watcher announces the new folder; the Library reloads and reveals it.
    fireBusEvent("project-tree-changed", { changeCount: 1 });

    const row = await waitFor(() => {
      const el = screen.getByText("notes-inbox").closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
    // LIB-FR-18: the lens was relaxed because All Artifacts would have hidden it.
    expect(selectorValue("Filter by type")).toBe("files");
    // NFW-FR-11: collapsed — the node itself was never expanded — and no tab.
    expect(row.querySelector(".tree-row__caret")?.children.length ?? 0).toBe(1);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_artifact_by_id"),
    ).toEqual([]);
    dialog = screen.queryByRole("dialog") as HTMLElement;
    expect(dialog).toBeNull();
  });

  it("NFW-FR-11, LIB-FR-09: a typed folder is revealed under All Artifacts with no lens change", async () => {
    let created = false;
    const typedNode: TreeNode = {
      id: "scenarios",
      name: "scenarios",
      path: "scenarios",
      nodeKind: "folder",
      hasArtifacts: true,
      artifactType: "scenario",
      typeSource: "assigned",
      children: [],
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") {
        const tree = treeWithFolders();
        if (created) tree.children.push({ ...typedNode });
        return tree;
      }
      if (cmd === "create_folder") {
        created = true;
        return { ...typedNode };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-folder");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "scenarios");
    await userEvent.selectOptions(
      within(dialog).getByLabelText("Artifact Type"),
      "scenario",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    fireBusEvent("project-tree-changed", { changeCount: 1 });

    const row = await waitFor(() => {
      const el = screen.getByText("scenarios").closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
    // A folder-scope assignment makes it visible under All Artifacts already, so
    // the lens the user chose is left alone (LIB-FR-09 / LIB-FR-18).
    expect(selectorValue("Filter by type")).toBe("artifacts");
    // LIB-FR-08: and it wears the type it carries.
    expect(row.querySelector(".chip-type")).toHaveAttribute(
      "data-type",
      "scenario",
    );
  });

  it("NFW-FR-10: the chosen artifact type rides along with the creation", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      if (cmd === "create_folder") {
        return {
          id: "scenarios",
          name: "scenarios",
          path: "scenarios",
          nodeKind: "folder",
          hasArtifacts: true,
          artifactType: "scenario",
          typeSource: "assigned",
          children: [],
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-folder");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "scenarios");
    await userEvent.selectOptions(
      within(dialog).getByLabelText("Artifact Type"),
      "scenario",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "create_folder" && c[1]?.artifactType === "scenario",
        ),
      ).toBe(true),
    );
  });

  it("a folder created inside a typed folder is visible under All Artifacts without a lens change", async () => {
    // The end-to-end shape of creation-time type inheritance: the backend records
    // the parent's type on the new subfolder, so it arrives already typed and the
    // default lens shows it — rather than the user creating a folder inside their
    // Prompt folder and watching it vanish.
    let created = false;
    const inherited: TreeNode = {
      id: "prompts/drafts",
      name: "drafts",
      path: "prompts/drafts",
      nodeKind: "folder",
      hasArtifacts: true,
      artifactType: "prompt",
      typeSource: "assigned",
      children: [],
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") {
        const prompts = {
          id: "prompts",
          name: "prompts",
          path: "prompts",
          nodeKind: "folder",
          hasArtifacts: true,
          artifactType: "prompt",
          typeSource: "assigned",
          children: created ? [{ ...inherited }] : [],
        };
        return {
          id: "",
          name: "",
          path: "",
          nodeKind: "folder",
          hasArtifacts: true,
          children: [prompts],
        };
      }
      if (cmd === "create_folder") {
        created = true;
        return { ...inherited };
      }
      if (cmd === "load_library_panel_state") {
        return {
          expandedPaths: ["prompts"],
          artifactTypeFilter: "all_artifacts",
          textFilter: "",
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    await screen.findByText("prompts");

    // Create the subfolder with the Artifact Type left unset.
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.contextMenu(screen.getByText("prompts"));
    fireEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText(
        "New Folder",
      ),
    );
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Artifact Type")).toHaveValue("");
    await userEvent.type(within(dialog).getByLabelText("Name"), "drafts");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    fireBusEvent("project-tree-changed", { changeCount: 1 });

    const row = await waitFor(() => {
      const el = screen.getByText("drafts").closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    // Visible under the default lens, wearing the inherited type, with the lens
    // untouched — no relaxation was needed (LIB-FR-08 / LIB-FR-09 / LIB-FR-18).
    expect(selectorValue("Filter by type")).toBe("artifacts");
    expect(row.querySelector(".chip-type")).toHaveAttribute("data-type", "prompt");
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
  });

  it("NFW-FR-13: a backend create_folder rejection is shown inline and keeps the modal open", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      if (cmd === "create_folder") throw "already exists: drafts";
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-folder");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "drafts");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "already exists: drafts",
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  // LCM-FR-09, NFW-FR-07 at the App seam: the Library context menu seeds the parent from
  // the right-clicked folder and leaves the type unset even though that folder
  // carries one (NFW-FR-07).
  it("LCM-FR-09, NFW-FR-07: the Library context-menu entry seeds the parent and leaves the type unset", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    const { fireEvent } = await import("@testing-library/react");
    fireEvent.contextMenu(await screen.findByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("New Folder"));

    const dialog = await screen.findByRole("dialog");
    const parent = within(dialog).getByLabelText("Parent Folder");
    expect(parent).toHaveValue("specifications");
    // Still editable — a starting point, not a commitment.
    expect(parent).toBeEnabled();
    // And the folder's own "spec" type did NOT seed the new folder's type.
    expect(within(dialog).getByLabelText("Artifact Type")).toHaveValue("");
  });

  // NFW-FR-04 / NFW NFR: the parent list comes from the tree the Library already
  // published, so opening the window costs no scan of its own.
  it("populates the parent list from the Library's tree without scanning again", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    await screen.findByText("specifications");

    const scansBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_project_tree",
    ).length;

    fireBusEvent("menu:new-folder");
    const dialog = await screen.findByRole("dialog");

    const parent = within(dialog).getByLabelText(
      "Parent Folder",
    ) as HTMLSelectElement;
    expect(Array.from(parent.options).map((o) => o.value)).toEqual([
      "",
      "specifications",
    ]);
    // No further scan was issued to build that list.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree").length,
    ).toBe(scansBefore);
  });

  it("NTA-FR-01, NFW-FR-01: opening New Folder closes the New File modal, and vice versa", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    // New File first, then New Folder: only one modal is ever mounted.
    fireBusEvent("menu:new-file");
    let dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("New File")).toBeInTheDocument();

    fireBusEvent("menu:new-folder");
    await waitFor(() => {
      const only = screen.getAllByRole("dialog");
      expect(only).toHaveLength(1);
      expect(within(only[0]).getByText("New Folder")).toBeInTheDocument();
    });

    // And the other direction.
    fireBusEvent("menu:new-file");
    await waitFor(() => {
      const only = screen.getAllByRole("dialog");
      expect(only).toHaveLength(1);
      expect(within(only[0]).getByText("New File")).toBeInTheDocument();
    });
    dialog = screen.getByRole("dialog");
    expect(within(dialog).queryByText("New Folder")).not.toBeInTheDocument();

    // SNV-FR-56 / NAW-FR-01: New Artifact is not in this set at all. Opening it
    // over a modal closes that modal only because the workspace takes the
    // viewport — the tab itself is no overlay and dismisses nothing.
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
  });

  it("NTA-FR-01, NFW-FR-01: opening the search overlay closes the New Folder modal, and opening it closes the overlay", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    // Overlay → modal.
    await userEvent.click(
      screen.getByPlaceholderText("Search artifacts, flows, runs…"),
    );
    expect(document.querySelector(".search-overlay")).toBeInTheDocument();
    fireBusEvent("menu:new-folder");
    await screen.findByRole("dialog");
    expect(document.querySelector(".search-overlay")).not.toBeInTheDocument();

    // Modal → overlay.
    await userEvent.click(
      screen.getByPlaceholderText("Search artifacts, flows, runs…"),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  it("NFW-FR-01: opening the project switcher dropdown closes the New Folder modal", async () => {
    // Neither creation modal traps focus, so the top-chrome dropdowns are
    // reachable by keyboard from a mounted modal even though the scrim blocks the
    // pointer. Those dropdowns are floating overlays too, so the invariant only
    // holds if they close the modal on the way up.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-folder");
    await screen.findByRole("dialog");

    const { fireEvent } = await import("@testing-library/react");
    fireEvent.click(screen.getByTestId("project-switcher"));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  // NFW-FR-02: the window is re-centered on each open, which falls out of it
  // being unmounted while closed rather than hidden.
  it("NFW-FR-02: the modal is unmounted while closed, so each open is fresh", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-folder");
    let dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "typed-here");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // Re-opening starts from scratch: nothing of the previous attempt survives.
    fireBusEvent("menu:new-folder");
    dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Name")).toHaveValue("");
  });
});
