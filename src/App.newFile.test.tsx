/**
 * New File modal wiring (NFI-new-file.md), end to end through the shell.
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

// ---------------------------------------------------------------------------
// New File modal wiring (NFI-new-file.md)
// ---------------------------------------------------------------------------

describe("New File modal wiring (NFI-FR-09 / NFI-FR-12 / NFI-FR-01)", () => {
  // A tree with a typed folder (`specifications`, folder-scope `spec` per
  // ASC-FR-18), a nested untyped folder, and a top-level artifact so the Library
  // has something to render before anything is created.
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

  /**
   * NFI-FR-11, NFI-FR-12, ESH-FR-ATDS, LIB-FR-18 end to end: an untyped plain file is created, opened in an Editor
   * tab as plain text, and revealed/selected with the lens relaxed.
   *
   * Deliberately drives the whole seam rather than the `create_file` argument
   * shape alone: the reveal, the selection, the lens relaxation and the routing
   * are four separate claims of NFI-FR-12 that an argument assertion cannot see.
   */
  it("NFI-FR-11, NFI-FR-12, ESH-FR-ATDS, LIB-FR-18: an untyped file opens as plain text in an Editor tab and is revealed with the lens relaxed", async () => {
    let created = false;
    const node: TreeNode = {
      id: "package.json",
      name: "package.json",
      path: "package.json",
      nodeKind: "file",
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") {
        const tree = treeWithFolders();
        if (created) tree.children.push({ ...node });
        return tree;
      }
      if (cmd === "create_file") {
        created = true;
        return { ...node };
      }
      if (cmd === "validate_flow_document") return { valid: true, violations: [] };
      if (cmd === "load_artifact_contents_by_id")
        return { body: '{\n  "name": "acme"\n}\n', checksum: "ck-json" };
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    // Under the default All Artifacts lens an untyped file is not shown.
    expect(screen.queryByText("package.json")).not.toBeInTheDocument();

    fireBusEvent("menu:new-file");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("New File");
    await userEvent.type(
      within(dialog).getByLabelText("Name"),
      "package.json",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "create_file" &&
            c[1]?.name === "package.json" &&
            c[1]?.location === null,
        ),
      ).toBe(true),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // NFI-FR-12: unlike a folder, a file HAS an editing surface, so a tab opens
    // on it — and the Editor loads its content live through the backend, which is
    // what makes it a real editing session rather than a label in the strip
    // (ESH-FR-TLJZ / PST-FR-24).
    expect(tabLabels()).toContain("package.json");
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "load_artifact_contents_by_id" &&
            c[1]?.id === "package.json",
        ),
      ).toBe(true),
    );
    // EDT-FR-16 / ESH-FR-ROWK: opened with the ordinary cluster and no
    // artifact-scoped action.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Inject" })).toBeNull(),
    );

    // The watcher announces the new file; the Library reloads and reveals it.
    fireBusEvent("project-tree-changed", { changeCount: 1 });

    const panel = document.querySelector(".vpanel") as HTMLElement;
    const row = await waitFor(() => {
      const el = within(panel).getByText("package.json").closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
    // LIB-FR-18: All Artifacts would have hidden an untyped file, so the lens
    // was relaxed to reveal it.
    expect(selectorValue("Filter by type")).toBe("files");
    // LIB-FR-08: an unclassified file wears no type tag.
    expect(row.querySelector(".chip-type")).toBeNull();
  });

  /**
   * NFI-FR-11, NFI-FR-12, ASC-FR-06, LIB-FR-03, FLO-FR-04 first half: created inside a typed folder, the file arrives already
   * carrying that folder's type by inheritance — so it opens in an Editor tab and
   * is revealed under the unchanged All Artifacts lens.
   */
  it("NFI-FR-11, NFI-FR-12, ASC-FR-06, LIB-FR-03, FLO-FR-04: a file inheriting its folder's type opens in an Editor tab with no lens change", async () => {
    let created = false;
    const inherited: TreeNode = {
      id: "specifications/NFI-new-file.md",
      name: "NFI-new-file.md",
      path: "specifications/NFI-new-file.md",
      nodeKind: "file",
      artifactType: "spec",
      typeSource: "inherited",
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") {
        const tree = treeWithFolders();
        if (created) tree.children[0].children!.push({ ...inherited });
        return tree;
      }
      if (cmd === "create_file") {
        created = true;
        return { ...inherited };
      }
      if (cmd === "load_library_panel_state") {
        return {
          expandedPaths: ["specifications"],
          artifactTypeFilter: "all_artifacts",
          textFilter: "",
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    await screen.findByText("specifications");

    fireEvent.contextMenu(screen.getByText("specifications"));
    fireEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText(
        "New File",
      ),
    );
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText("Name"),
      "NFI-new-file.md",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // NFI-FR-11: the window recorded no type — no assignment call was made.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "assign_artifact_type"),
    ).toEqual([]);
    expect(tabLabels()).toContain("NFI-new-file.md");

    fireBusEvent("project-tree-changed", { changeCount: 1 });

    const panel = document.querySelector(".vpanel") as HTMLElement;
    const row = await waitFor(() => {
      const el = within(panel)
        .getByText("NFI-new-file.md")
        .closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
    // Already visible under All Artifacts, so the lens is left alone.
    expect(selectorValue("Filter by type")).toBe("artifacts");
    expect(row.querySelector(".chip-type")).toHaveAttribute("data-type", "spec");
  });

  // NFI-FR-11, ASC-FR-06, LIB-FR-03, FLO-FR-04 second half / NFI-FR-12: a created file resolving to Flow opens
  // in a Flow tab rather than an Editor tab, rendering the empty graph its empty
  // body deserializes to (FLO-FR-04).
  it("NFI-FR-11, NFI-FR-12, ASC-FR-06, LIB-FR-03, FLO-FR-04: a file resolving to Flow opens in a Flow tab", async () => {
    const flow: TreeNode = {
      id: "flows/pipeline.flow",
      name: "pipeline.flow",
      path: "flows/pipeline.flow",
      nodeKind: "file",
      artifactType: "flow",
      typeSource: "inherited",
    };
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      if (cmd === "create_file") return { ...flow };
      return defaultInvoke(cmd, args);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-file");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText("Name"),
      "pipeline.flow",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    await waitFor(() => expect(tabLabels()).toContain("pipeline.flow"));
    // A Flow tab, not an Editor tab: the canvas is what renders (LIB-FR-03),
    // showing the empty graph with no error state.
    await waitFor(() =>
      expect(document.querySelector(".flow")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "0 nodes · 0 edges",
    );
    // No surface of the application stages an artifact to an agent any more.
    // Asserted here as well as on the canvas, because the shell is what would
    // thread such an action in — a tab that regained one would regain it from
    // this seam.
    expect(
      screen.queryByRole("button", { name: /inject/i }),
    ).not.toBeInTheDocument();
  });

  // NFI-FR-07, LCM-FR-10 at the App seam: the Library context menu seeds the location from
  // the right-clicked folder and leaves it editable (NFI-FR-07).
  it("NFI-FR-07, LCM-FR-10: the Library context-menu entry seeds the location and leaves it editable", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("New File"));

    const dialog = await screen.findByRole("dialog");
    const location = within(dialog).getByLabelText("Location");
    expect(location).toHaveValue("specifications");
    // Still editable — a starting point, not a commitment (NFI-FR-07).
    expect(location).toBeEnabled();
    // NFI-FR-03: the folder's own "spec" type seeds nothing, because there is no
    // type control at all.
    expect(
      within(dialog).queryByLabelText("Artifact Type"),
    ).not.toBeInTheDocument();
  });

  // NFI-FR-04 / NFI NFR: the location list comes from the tree the Library
  // already published, so opening the window costs no scan of its own.
  it("populates the location list from the Library's tree without scanning again", async () => {
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

    fireBusEvent("menu:new-file");
    const dialog = await screen.findByRole("dialog");

    const location = within(dialog).getByLabelText(
      "Location",
    ) as HTMLSelectElement;
    expect(Array.from(location.options).map((o) => o.value)).toEqual([
      "",
      "specifications",
    ]);
    // No further scan was issued to build that list.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree").length,
    ).toBe(scansBefore);
  });

  // NTA-FR-01, NFI-FR-01 / NFW-FR-01 / NTA-FR-01: the three creation windows are one
  // floating overlay each, so opening any of them closes whichever of the other
  // two is up rather than coexisting with it.
  it("NTA-FR-01, NFI-FR-01 / NFW-FR-01 / NTA-FR-01: the three creation modals are mutually exclusive", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    const onlyModal = async (title: string) =>
      waitFor(() => {
        const open = screen.getAllByRole("dialog");
        expect(open).toHaveLength(1);
        expect(within(open[0]).getByText(title)).toBeInTheDocument();
      });

    // Each window in turn closes the one before it, in both directions.
    fireBusEvent("menu:new-file");
    await onlyModal("New File");
    fireBusEvent("menu:new-artifact");
    await onlyModal("New Artifact");
    fireBusEvent("menu:new-folder");
    await onlyModal("New Folder");
    fireBusEvent("menu:new-artifact");
    await onlyModal("New Artifact");
    fireBusEvent("menu:new-file");
    await onlyModal("New File");
  });

  it("NTA-FR-01, NFI-FR-01: opening the search overlay closes the New File modal, and opening it closes the overlay", async () => {
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
    fireBusEvent("menu:new-file");
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

  it("NFI-FR-01: opening the project switcher dropdown closes the New File modal", async () => {
    // The modal does not trap focus, so the top-chrome dropdowns stay reachable
    // by keyboard while it is up. They are floating overlays too, so the
    // invariant holds only if they close the modal on the way in.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-file");
    await screen.findByRole("dialog");

    fireEvent.click(screen.getByTestId("project-switcher"));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  // NFI-FR-02: the window is re-centered on each open, which falls out of it
  // being unmounted while closed rather than hidden.
  it("NFI-FR-02: the modal is unmounted while closed, so each open is fresh", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithFolders();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-file");
    let dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "typed-here.ts");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // Re-opening starts from scratch: nothing of the previous attempt survives.
    fireBusEvent("menu:new-file");
    dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Name")).toHaveValue("");
  });
});
