/**
 * New Artifact modal wiring (NTA-new-typed-artifact.md), end to end through the
 * shell.
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
import type { TreeNode } from "./types";
import { resetPanelReveals } from "./state/panelReveal";
import { defaultInvoke, enterIde, resetAppFixture } from "./test/appFixtures";

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
 * New Artifact modal wiring (`NTA-new-typed-artifact.md`) end to end through the
 * shell: the two invocation points, the one creation call, the routing of the
 * new file to its natural surface, the reveal-and-select, and the whole set of
 * things this window deliberately does not do.
 */
describe("New Artifact modal wiring (NTA-FR-10 … NTA-FR-14)", () => {
  function emptyTree(): TreeNode & { children: TreeNode[] } {
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
          children: [],
        },
      ],
    };
  }

  const tabLabels = () =>
    Array.from(
      screen.getByTestId("tabstrip").querySelectorAll(".tab__label"),
    ).map((el) => el.textContent);

  /**
   * The backend a successful typed creation meets: the tree gains the new node
   * once `create_typed_file` has returned it, exactly as the watcher-driven
   * reload would deliver it.
   */
  function backendCreating(node: TreeNode) {
    let created = false;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_project_tree") {
        const tree = emptyTree();
        if (created) tree.children[0].children!.push({ ...node });
        return tree;
      }
      if (cmd === "create_typed_file") {
        created = true;
        return { ...node };
      }
      if (cmd === "validate_flow_document") return { valid: true, violations: [] };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "", checksum: "ck-empty" };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
  }

  async function fillAndCreate(name: string, artifactType: string) {
    const dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    await userEvent.type(
      within(dialog).getByLabelText("Name (required)"),
      name,
    );
    await userEvent.selectOptions(
      within(dialog).getByLabelText("Artifact Type (required)"),
      artifactType,
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));
  }

  /**
   * NTA-FR-10, NTA-FR-11 / NTA-FR-12, NTA-FR-13, ASC-FR-05, ASC-FR-06, LIB-FR-18 / NTA-FR-14, DRS-FR-39 / LCM-FR-08, NTA-FR-08 / OVW-FR-06, OVW-FR-07: the whole seam
   * for a non-Flow type — one creation call, the window closes, an Editor tab
   * opens on the new file, the Project panel reveals and selects it, and no
   * draft operation was invoked anywhere.
   */
  it("NTA-FR-10, NTA-FR-11 / NTA-FR-12, NTA-FR-13, ASC-FR-05, ASC-FR-06, LIB-FR-18: creates once, opens an Editor tab, and reveals the file", async () => {
    backendCreating({
      id: "specifications/overview.md",
      name: "overview.md",
      path: "specifications/overview.md",
      nodeKind: "file",
      artifactType: "spec",
      typeSource: "assigned",
    });

    render(<App />);
    await enterIde();

    // Everything the shell reads at project open has already happened. Anything
    // after this line is the window's own traffic, which is what NTA-FR-14, DRS-FR-39 is
    // about.
    const configReadsAtOpen = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_project_config",
    ).length;

    // NTA-FR-08, LCM-FR-08, LCM-FR-09, LCM-FR-10: the folder menu's creation cluster leads the menu — New File,
    // New Artifact, New Folder — and selecting New Artifact opens the window.
    const folder = await screen.findByText("specifications");
    fireEvent.contextMenu(folder);
    const menu = document.querySelector(".menu") as HTMLElement;
    const entries = Array.from(menu.querySelectorAll(".menu-item")).map(
      (el) => el.textContent?.trim(),
    );
    expect(entries.slice(0, 3)).toEqual([
      "New File",
      "New Artifact",
      "New Folder",
    ]);
    fireEvent.click(within(menu).getByText("New Artifact"));

    // NTA-FR-08: seeded to the right-clicked folder, with the type unchosen
    // even though that folder carries a folder-scope "Spec" assignment.
    const dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    expect(within(dialog).getByLabelText("Location (required)")).toHaveValue(
      "specifications",
    );
    expect(
      within(dialog).getByLabelText("Artifact Type (required)"),
    ).toHaveValue("");

    await fillAndCreate("overview.md", "spec");

    // NTA-FR-10: exactly one call, carrying the three inputs.
    const calls = invokeMock.mock.calls.filter(
      (c) => c[0] === "create_typed_file",
    );
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({
      location: "specifications",
      name: "overview.md",
      artifactType: "spec",
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // NTA-FR-13: an Editor tab on the new file, loading it live.
    expect(tabLabels()).toContain("overview.md");
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "load_artifact_contents_by_id" &&
            c[1]?.id === "specifications/overview.md",
        ),
      ).toBe(true),
    );

    // NTA-FR-13 / LIB-FR-18: revealed and selected in the Project panel once
    // the watcher's reload has delivered it.
    fireBusEvent("project-tree-changed", { changeCount: 1 });
    const panel = document.querySelector(".vpanel") as HTMLElement;
    const row = await waitFor(() => {
      const el = within(panel).getByText("overview.md").closest(".tree-row");
      expect(el).not.toBeNull();
      return el!;
    });
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));

    // NTA-FR-14 / DRS-FR-39 / DRP-FR-06: no draft, no draft template, no New
    // Artifact tab, and exactly one creation call in total.
    // Nothing was written under `.synthesis/drafts/`: no draft was created,
    // renamed, filed, or written to on this window's account.
    for (const op of [
      "create_draft",
      "create_drafts_folder",
      "rename_draft",
      "save_draft_file_contents",
      "delete_draft",
    ])
      expect(invokeMock.mock.calls.some((c) => c[0] === op)).toBe(false);
    expect(
      screen.queryByRole("button", { name: "Draft actions" }),
    ).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_file")).toBe(
      false,
    );
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "assign_artifact_type"),
    ).toBe(false);
    // The project's draft template was neither read nor copied: the window
    // read no project config of its own, before or after confirming.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_project_config")
        .length,
    ).toBe(configReadsAtOpen);
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_project_config"),
    ).toBe(false);
  });

  /**
   * NTA-FR-11, NTA-FR-12, NTA-FR-13, FLO-FR-04 / NTA-FR-06: a file created with the type Flow opens in a Flow
   * tab, on the empty graph its empty body deserializes to (FLO-FR-04) — and it
   * does so inside a folder whose own assignment would otherwise have typed it
   * `spec`, because a file-scope assignment outranks an ancestor's.
   */
  it("NTA-FR-11, NTA-FR-12, NTA-FR-13, FLO-FR-04: a Flow opens in a Flow tab on the empty graph", async () => {
    backendCreating({
      id: "specifications/checks.md",
      name: "checks.md",
      path: "specifications/checks.md",
      nodeKind: "file",
      artifactType: "flow",
      typeSource: "assigned",
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-artifact");
    await fillAndCreate("checks.md", "flow");

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(tabLabels()).toContain("checks.md");
    // The Flow surface, not the Editor: a Flow tab validates its document
    // rather than loading Markdown into an editing session.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "validate_flow_document"),
      ).toBe(true),
    );
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "load_artifact_contents_by_id",
      ),
    ).toBe(true);
    // No error state: an empty body is the empty graph (FGV-FR-05).
    expect(screen.queryByRole("alert")).toBeNull();
  });

  /**
   * NTA-FR-16, NTA-FR-10 / PST-FR-29: a refused creation is shown inline with the three
   * inputs intact, nothing is opened or revealed, and a corrected retry
   * succeeds — the project having been left exactly as the first attempt found
   * it (PST-FR-29).
   */
  it("NTA-FR-16, NTA-FR-10: a collision is shown inline and the corrected retry succeeds", async () => {
    let attempts = 0;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_project_tree") return emptyTree();
      if (cmd === "create_typed_file") {
        attempts += 1;
        if (attempts === 1) throw "already exists: specifications/overview.md";
        return {
          id: "specifications/overview-2.md",
          name: "overview-2.md",
          path: "specifications/overview-2.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "assigned",
        };
      }
      if (cmd === "load_artifact_contents_by_id")
        return { body: "", checksum: "ck-empty" };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-artifact");
    const dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    await userEvent.selectOptions(
      within(dialog).getByLabelText("Location (required)"),
      "specifications",
    );
    await fillAndCreate("overview.md", "spec");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "already exists",
    );
    // The window stays open with everything the user filled in.
    expect(within(dialog).getByLabelText("Location (required)")).toHaveValue(
      "specifications",
    );
    expect(within(dialog).getByLabelText("Name (required)")).toHaveValue(
      "overview.md",
    );
    expect(
      within(dialog).getByLabelText("Artifact Type (required)"),
    ).toHaveValue("spec");
    expect(tabLabels()).not.toContain("overview.md");

    await userEvent.clear(within(dialog).getByLabelText("Name (required)"));
    await userEvent.type(
      within(dialog).getByLabelText("Name (required)"),
      "overview-2.md",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(tabLabels()).toContain("overview-2.md");
  });

  // NTA-FR-15: dismissal invokes nothing and opens nothing.
  it("NTA-FR-15: dismissing the window creates nothing", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_project_tree") return emptyTree();
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });

    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-artifact");
    const dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    await userEvent.type(
      within(dialog).getByLabelText("Name (required)"),
      "overview.md",
    );
    await userEvent.selectOptions(
      within(dialog).getByLabelText("Artifact Type (required)"),
      "spec",
    );
    await userEvent.keyboard("{Escape}");

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "create_typed_file"),
    ).toBe(false);
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_draft")).toBe(
      false,
    );
  });

  // NTA-FR-04, NTA-FR-07, NTA-FR-09 / NTA-FR-02: unmounted while closed, so each open is fresh — the
  // previous invocation's inputs never survive into the next one.
  it("NTA-FR-02: each open starts from the flow's own defaults", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_project_tree") return emptyTree();
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });

    render(<App />);
    await enterIde();

    // Seeded from the Project menu first.
    const folder = await screen.findByText("specifications");
    fireEvent.contextMenu(folder);
    fireEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText(
        "New Artifact",
      ),
    );
    let dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    await userEvent.type(
      within(dialog).getByLabelText("Name (required)"),
      "overview.md",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // The File menu's own flow: the project root, an empty name, no type.
    fireBusEvent("menu:new-artifact");
    dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    expect(within(dialog).getByLabelText("Location (required)")).toHaveValue("");
    expect(within(dialog).getByLabelText("Name (required)")).toHaveValue("");
    expect(
      within(dialog).getByLabelText("Artifact Type (required)"),
    ).toHaveValue("");
  });
});
