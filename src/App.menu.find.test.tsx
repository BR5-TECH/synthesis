import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen } from "@tauri-apps/api/event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";

const listenMock = vi.mocked(listen);

// App-level wiring for the native File menu (SNV-FR-23..26). The menu is built
// in the Rust shell; each activation reaches the frontend as a Tauri event
// (`menu:new-folder` / `menu:new-artifact` / `menu:close-project`). These tests
// capture those listeners and fire them to assert the UI hand-off:
//   - NFI-FR-06, NFW-FR-06, NTA-FR-07, SNV-FR-24: New Folder / New Artifact each open the modal that owns their
//     creation experience (NFW-new-folder.md / NAW-new-artifact.md).
//   - PST-FR-14, ASC-FR-14, SNV-FR-25: Close Project returns to the Project picker (OVW-FR-02).
// SNV-FR-23, SWN-FR-14 (the menu renders with its four items in order) and SNV-FR-26 (Exit
// quits via the native role) are native-menu behaviors covered in the Rust
// suite / verified manually.

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
  // Both records are cached at module scope; reset them so one test's
  // persisted state is not served to the next.
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  isFullscreenMock.mockClear();
  isFullscreenMock.mockResolvedValue(false);
  setFullscreenMock.mockClear();
  setFullscreenMock.mockResolvedValue(undefined);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function enterIde() {
  // The picker is the startup surface; open a recent to reach the IDE shell.
  const recent = await screen.findByText("acme");
  await userEvent.click(recent);
  await screen.findByRole("button", { name: "Global settings" });
}

// Fire a captured menu event the way the backend's `on_menu_event` -> `emit`
// would, wrapped in act() because the handler updates React state.
async function fireMenu(event: string) {
  await act(async () => {
    listeners[event]?.({ payload: null });
  });
}

/**
 * SNV-FR-43: the Edit menu's two Editor-scoped find items, end to end from the
 * relayed menu event to the panel appearing in the Editor tab.
 *
 * The accelerators themselves belong to the native menu — a disabled item
 * swallows both the click and the chord — so what is verifiable here is the
 * enablement the frontend pushes and what an activation does when it arrives.
 */
describe("Edit menu find wiring (SNV-FR-43)", () => {
  /** The enabled flag of the most recent `set_find_menu_state` push. */
  const lastFindEnabled = () => {
    const calls = invokeMock.mock.calls.filter(
      (c) => c[0] === "set_find_menu_state",
    );
    return (calls[calls.length - 1]?.[1] as { enabled: boolean } | undefined)
      ?.enabled;
  };

  it("EFR-FR-AYNZ, EFR-FR-ABHF, EFR-FR-ABVQ: Find opens the panel in the active Editor tab and toggles it closed", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    await fireMenu("menu:find");
    expect(await screen.findByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "find",
    );

    // EFR-FR-BBBV: a second Find closes it and the formatting toolbar returns.
    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
    expect(screen.getByTitle("Bold")).toBeInTheDocument();
  });

  it("EFR-FR-BSST: Find & Replace expands an open Find, and Find collapses it back", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    await fireMenu("menu:find");
    await fireMenu("menu:find-replace");
    expect(screen.getByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "replace",
    );

    await fireMenu("menu:find");
    expect(screen.getByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "find",
    );

    // Only a second press of the SAME item closes the band.
    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("EFR-FR-BOGW: Find & Replace toggles itself closed", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    await fireMenu("menu:find-replace");
    expect(screen.getByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "replace",
    );
    await fireMenu("menu:find-replace");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("SNV-FR-43, EFR-FR-AYNZ: the items are Editor-scoped — enabled on an Editor tab, greyed out elsewhere", async () => {
    render(<App />);
    await enterIde();

    // The Dashboard is the opening tab and owns no editing surface.
    await waitFor(() => expect(lastFindEnabled()).toBe(false));

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await waitFor(() => expect(lastFindEnabled()).toBe(true));
  });

  it("SNV-FR-43, EFR-FR-AYNZ: an activation on a non-Editor tab opens nothing", async () => {
    // Belt and braces to the native greying: even if an event arrived, there is
    // no Editor to open a panel in, so nothing happens and nothing throws.
    render(<App />);
    await enterIde();

    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("EFR-FR-CLZF, EFR-FR-CWUR: the panel's mode is the artifact's own, not the universal search bar's", async () => {
    // EFR-FR-CWUR: the universal search input's query mode is user-global and
    // persisted (SCH-FR-13); the Editor panel's is the artifact's own value.
    // Here the persisted global mode is `regex`, so a panel that read it would
    // open in regex — and a panel that WROTE it would push a save.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "dark", searchQueryMode: "regex" };
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    // The universal search bar reflects the persisted global mode.
    const chromeToggles = within(
      screen.getByTestId("query-mode-toggles"),
    ).getAllByRole("radio");
    await waitFor(() =>
      expect(
        chromeToggles.find((t) => t.getAttribute("aria-checked") === "true"),
      ).toHaveTextContent(/^\.\*/),
    );

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await fireMenu("menu:find");

    // …and the Editor's panel opens in case-insensitive literal regardless.
    const panelToggles = within(
      screen.getByTestId("find-mode-toggles"),
    ).getAllByRole("radio");
    expect(
      panelToggles.find((t) => t.getAttribute("aria-checked") === "true"),
    ).toHaveTextContent(/^Aa/);

    // Changing the panel's mode writes no app preference and leaves the
    // universal search bar exactly where it was.
    const savesBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_app_preferences",
    ).length;
    await userEvent.click(
      panelToggles.find((t) => t.textContent?.startsWith("aA"))!,
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_app_preferences")
        .length,
    ).toBe(savesBefore);
    expect(
      within(screen.getByTestId("query-mode-toggles"))
        .getAllByRole("radio")
        .find((t) => t.getAttribute("aria-checked") === "true"),
    ).toHaveTextContent(/^\.\*/);
  });

  it("EFR-FR-HLGY: the accelerator opens nothing while the external-change modal blocks the tab", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    // Raise the blocking modal (EXC-FR-LKHZ/EXC-FR-VTUH).
    await act(async () => {
      listeners["artifact-changed-externally"]?.({
        payload: { artifactId: "a.md", checksum: "ck-external" },
      });
    });
    await screen.findByRole("button", { name: "Keep my version" });

    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();

    // Once resolved, it opens normally.
    await userEvent.click(screen.getByRole("button", { name: "Keep my version" }));
    await fireMenu("menu:find");
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
  });

  it("EFR-FR-GBJT: the panel follows the artifact when tabs are switched", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await fireMenu("menu:find");
    await userEvent.type(await screen.findByLabelText("Find"), "heading");

    // b.md has no panel of its own…
    await userEvent.click(await screen.findByText("b.md"));
    await waitFor(() =>
      expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument(),
    );

    // …and returning to a.md resumes its panel with the query intact.
    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("a.md"));
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
    expect((screen.getByLabelText("Find") as HTMLInputElement).value).toBe(
      "heading",
    );
  });

  /**
   * NAW-FR-57 / SNV-FR-43: the same two items over a New Artifact tab.
   *
   * A draft's prompt is edited in the Editor's own surface (NAW-FR-11), so the
   * menu drives the panels that surface already carries rather than a
   * draft-only command path — which is what these exercise from the menu event
   * down to the panel in the tab.
   */
  /** DRP-FR-06 / NAW-FR-03: a draft tab, opened from the Drafts panel. */
  async function openDraftTab() {
    await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
    // DRP-FR-15: this worktree lists no drafts, so the create affordance is the
    // empty state's single button.
    await userEvent.click(
      await screen.findByRole("button", { name: "New draft" }),
    );
    // The tab is open once the Editor's own surface is mounted on the prompt.
    await screen.findByRole("button", { name: "Edit as Markdown source" });
  }

  it("NAW-FR-57, EFR-FR-BBKS, EFR-FR-BSST / SNV-FR-43, EFR-FR-AYNZ, EFR-FR-BJUY: the two items open, switch and close the panels in a draft tab", async () => {
    render(<App />);
    await enterIde();
    await openDraftTab();
    await waitFor(() => expect(lastFindEnabled()).toBe(true));

    // NAW-FR-57: Find opens with focus in the query input.
    await fireMenu("menu:find");
    expect(await screen.findByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "find",
    );
    expect(screen.getByLabelText("Find")).toHaveFocus();

    // EFR-FR-BSST: Find & Replace expands it and focuses the replacement input.
    await fireMenu("menu:find-replace");
    expect(screen.getByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "replace",
    );
    expect(screen.getByLabelText("Replace with")).toHaveFocus();

    // EFR-FR-BBKS: Find collapses it back, carrying the query through, and only a
    // second press of the same item closes the band.
    await fireMenu("menu:find");
    expect(screen.getByTestId("find-panel")).toHaveAttribute(
      "data-form",
      "find",
    );
    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("NAW-FR-57, NAW-FR-59, SNV-FR-43 / NAW-FR-09, NAW-FR-44: a draft reading a past version disables both items and opens nothing", async () => {
    // NAW-FR-40 / DHS-FR-10: the rail's list, and the one version it holds.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_draft_history") {
        return {
          entries: [
            {
              id: "e1",
              draftId: "d1",
              seq: 1,
              path: "Untitled.md",
              createdAt: "2026-07-31T09:00:00Z",
              byteLen: 10,
              sha256: "sha-1",
              source: { kind: "original" },
            },
          ],
          live: {
            path: "Untitled.md",
            byteLen: 10,
            sha256: "sha-1",
            matchesLatest: true,
          },
        };
      }
      if (cmd === "load_draft_history_entry") {
        return { content: "the prompt as it was", sha256: "sha-1" };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    await openDraftTab();
    await waitFor(() => expect(lastFindEnabled()).toBe(true));

    // NAW-FR-09: read the version, which is read-only.
    await userEvent.click(
      await screen.findByRole("button", { name: "Show History" }),
    );
    await userEvent.click(
      document.querySelector(
        ".draft-rail__versions .draft-version",
      ) as HTMLElement,
    );
    await screen.findByTestId("draft-version-reading");

    // NAW-FR-59: both items are greyed out, and an activation that arrived
    // anyway opens no panel and changes no text.
    await waitFor(() => expect(lastFindEnabled()).toBe(false));
    await fireMenu("menu:find");
    await fireMenu("menu:find-replace");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
    expect(screen.getByTestId("draft-version-reading")).toBeInTheDocument();

    // NAW-FR-10: the return to the live prompt makes them reachable again.
    await userEvent.click(
      screen.getByRole("button", { name: /Back to live prompt/ }),
    );
    await waitFor(() => expect(lastFindEnabled()).toBe(true));
    await fireMenu("menu:find");
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
  });

  it("SNV-FR-43, NAW-FR-09, NAW-FR-44, NAW-FR-59: a draft its graduation holds read-only disables both items", async () => {
    // NAW-FR-44: a non-terminal run holds the prompt, so it takes no edit.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_draft_graduation") {
        // `get draft graduation` answers with the run itself (GRD contract).
        return {
          id: "run-9",
          streamId: "w1",
          streamName: "editor work",
          projectKey: "p",
          state: "working",
          input: {
            draftId: "d1",
            draftName: "Untitled",
            prompt: "p",
            promptChecksum: "c",
            capturedAt: "2026-07-31T10:00:00Z",
          },
          commits: [],
          autoStart: true,
          archived: false,
          workTurns: 1,
          reviewTurns: 0,
          checkpoint: { pass: 1, changedPaths: [], pendingEscalationAnswers: [], verdictRefusals: 0 },
          observability: {
            observabilityVersion: 1,
            currentStage: "working",
            stageCondition: "active",
            stageHistory: [],
            passes: [],
          },
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      }
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    await openDraftTab();

    await waitFor(() => expect(lastFindEnabled()).toBe(false));
    await fireMenu("menu:find");
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });
});
