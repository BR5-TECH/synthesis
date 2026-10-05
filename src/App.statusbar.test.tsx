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
import type { Operation } from "./types";
import { CHANGES_UPDATED, OPERATION_PROGRESS } from "./events";
import { letWriteLand } from "./test/autosave";

// STB-status-bar.md wiring at the App level: which backend operations the strip
// runs and when, and how it participates in the shell (the sixth zone, the
// single-overlay invariant, and the shared line-ending preference it edits
// jointly with the Project settings tab).

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
const { emitMock } = vi.hoisted(() => ({ emitMock: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  // SWN-FR-01: the Project settings window is a webview of its own, so the one
  // shared line-ending value crosses between it and the status bar as an event.
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
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

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
        workArea: {
          position: { x: 0, y: 0 },
          size: { width: 2560, height: 1400 },
        },
        scaleFactor: 1,
      },
    ],
  };
});

/** What the backend reports for the status bar's three reads. */
let inFlight: Operation[];
let lineEndings: "lf" | "crlf";
let diffTotals: unknown;
let savedConfigs: unknown[];

function defaultInvoke(cmd: string, args?: Record<string, unknown>) {
  switch (cmd) {
    case "list_recent_projects":
      return [
        { name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-05-15T10:00:00Z" },
      ];
    case "open_project_at_path":
      return {
        name: "acme",
        path: "~/dev/acme",
        activeWorktreePath: "~/dev/acme",
      };
    case "get_active_worktree":
      return {
        path: "~/dev/acme",
        name: "acme",
        branch: "main",
        headShortHash: "4f2a10c",
        isDetached: false,
        isActive: true,
        isPrimary: true,
        isMissing: false,
      };
    case "list_worktrees_and_branches":
      return {
        repositoryRoot: "~/dev/acme",
        activeWorktreePath: "~/dev/acme",
        worktrees: [],
        branches: [],
      };
    case "load_layout_preferences":
      return null;
    case "load_app_preferences":
      return { theme: "dark" };
    case "list_installed_plugins":
    case "list_agent_adapters":
      return [];
    case "load_project_tree":
    case "rescan_project_tree":
      return {
        id: "",
        name: "acme",
        path: "",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "a.md",
            name: "a.md",
            path: "a.md",
            nodeKind: "file",
            artifactType: "spec",
            typeSource: "inferred",
          },
          {
            id: "b.md",
            name: "b.md",
            path: "b.md",
            nodeKind: "file",
            artifactType: "spec",
            typeSource: "inferred",
          },
        ],
      };
    case "validate_flow_document":
      // FGV-FR-02 / FLO-FR-46: the backend judges a Flow body before the canvas
      // renders it. The rules themselves are covered by the Rust suite.
      return { valid: true, violations: [] };
    case "load_artifact_contents_by_id":
      // Indented with two spaces, so detection has something to find.
      return { body: "- one\n  - nested\n", checksum: "ck1" };
    case "save_artifact_contents":
      return { checksum: "ck2" };
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "get_default_branch":
      return "main";
    case "list_comparison_branches":
      return [{ name: "main", isCurrent: true, isDefault: true }];
    case "list_uncommitted_changes":
      return { comparison: { kind: "uncommitted" }, entries: [] };

    // --- the status bar's own reads ---
    case "list_in_flight_operations":
      return inFlight;
    case "get_uncommitted_diff_totals":
      if (diffTotals instanceof Error) throw diffTotals;
      return diffTotals;
    case "load_project_config":
      return { lineEndings };
    case "save_project_config":
      savedConfigs.push(args?.config);
      lineEndings = (args?.config as { lineEndings: "lf" | "crlf" }).lineEndings;
      return undefined;
    case "list_drafts":
      return { folders: [], drafts: [] };
    case "create_draft":
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
      return [];
    case "load_draft_file_contents":
      return { body: "", checksum: "dck" };
    case "save_draft_file_contents":
      return { checksum: "dck2" };
    default:
      return undefined;
  }
}

beforeEach(() => {
  invokeMock.mockReset();
  emitMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) =>
      defaultInvoke(cmd, args),
  );
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  inFlight = [];
  lineEndings = "lf";
  diffTotals = { addedLines: 412, removedLines: 87, fileCount: 5 };
  savedConfigs = [];
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function enterIde() {
  await userEvent.click(await screen.findByText("acme"));
  await screen.findByTestId("status-bar");
}

/** Commands the frontend invoked, in order. */
function invoked(): string[] {
  return invokeMock.mock.calls.map((c) => c[0] as string);
}

/** Artifact ids `"save artifact contents"` was invoked with, in order. */
function savedArtifactIds(): string[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "save_artifact_contents")
    .map((c) => (c[1] as { id: string }).id);
}

describe("the status bar as the sixth zone (STB-FR-01 / SNV-FR-42)", () => {
  it("is absent on the Project picker and present in the main window", async () => {
    // STB-FR-02 / STB-FR-01: the picker is a window with no shell chrome
    // (PPK-FR-09), so no strip; opening a project brings one.
    render(<App />);
    expect(screen.queryByTestId("status-bar")).not.toBeInTheDocument();
    await enterIde();
    expect(screen.getByTestId("status-bar")).toBeInTheDocument();
  });

  it("exposes no hide, collapse, or resize affordance", async () => {
    // STB-FR-01, second clause / STB-FR-02.
    render(<App />);
    await enterIde();
    const bar = screen.getByTestId("status-bar");
    expect(within(bar).queryByRole("separator")).not.toBeInTheDocument();
    for (const forbidden of [/hide/i, /collapse/i, /resize/i]) {
      expect(within(bar).queryByLabelText(forbidden)).not.toBeInTheDocument();
    }
    // The vertical-panel splitter is the shell's, and lives above the strip.
    expect(bar.contains(screen.getByTestId("vpanel-resizer"))).toBe(false);
  });

  it("persists no layout value describing itself", async () => {
    // SNV-FR-08, SNV-FR-42 / STB-FR-02: the saved payload describes the panel and the
    // window, and carries nothing about the strip, which renders at its fixed
    // height regardless.
    render(<App />);
    await enterIde();

    // Drag the panel splitter, which is what makes a layout write happen at
    // all — asserting over an empty list of saves would prove nothing.
    const resizer = screen.getByTestId("vpanel-resizer");
    await act(async () => {
      resizer.dispatchEvent(
        new MouseEvent("pointerdown", { bubbles: true, clientX: 225 }),
      );
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 344 }));
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });

    await waitFor(() =>
      expect(invoked()).toContain("save_layout_preferences"),
    );
    const saves = invokeMock.mock.calls.filter(
      (c) => c[0] === "save_layout_preferences",
    );
    expect(saves.length).toBeGreaterThan(0);
    for (const [, args] of saves) {
      const preferences = (args as { preferences: Record<string, unknown> })
        .preferences;
      // The payload describes the panel and the window…
      expect(preferences).toHaveProperty("verticalPanelFraction");
      // …and carries no value describing the strip, which renders at its fixed
      // height regardless (SNV-FR-42).
      expect(Object.keys(preferences).join(" ")).not.toMatch(/status/i);
    }
    expect(screen.getByTestId("status-bar")).toBeInTheDocument();
  });
});

describe("the settings buttons moved out of the activity bar (SNV-FR-03 / STB-FR-04)", () => {
  it("leaves the activity bar holding only vertical-panel toggles", async () => {
    // STB-FR-04, SWN-FR-01, SWN-FR-02, SNV-FR-03 / SNV-FR-42. The buttons still exist — in the status bar — so
    // an assertion on the document alone would pass either way; this scopes to
    // the activity bar.
    render(<App />);
    await enterIde();

    const activityBar = document.querySelector(".activity-bar") as HTMLElement;
    expect(activityBar).toBeTruthy();
    expect(
      within(activityBar).queryByLabelText("Global settings"),
    ).not.toBeInTheDocument();
    expect(
      within(activityBar).queryByLabelText("Project settings"),
    ).not.toBeInTheDocument();
    // SNV-FR-44: the panel toggles are all that remain — the vertical-panel
    // cluster and the bottom-panel one.
    for (const name of ["Project", "Notes", "Changes", "Runs", "Git"]) {
      expect(
        within(activityBar).getByRole("button", { name }),
      ).toBeInTheDocument();
    }
  });

  it("STB-FR-04, SWN-FR-01, SWN-FR-02, SNV-FR-03: opens each settings child window and creates no tab", async () => {
    // STB-FR-04 / GLS-FR-01, GLS-FR-02, SWN-FR-06 / SET-FR-01, SET-FR-02: the status bar's two buttons and the
    // application menu's two entries are how either window is opened from the
    // shell. Neither button creates a tab in the main viewport (SWN-FR-01,
    // SWN-FR-18) and neither draws a settings surface inside this window.
    render(<App />);
    await enterIde();
    const bar = screen.getByTestId("status-bar");
    const tabsBefore = Array.from(
      document.querySelectorAll(".tabstrip .tab .tab__label"),
    ).map((el) => el.textContent);

    await userEvent.click(
      within(bar).getByRole("button", { name: "Global settings" }),
    );
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "open_settings_window"),
      ).toHaveLength(1),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")![1],
    ).toEqual({ kind: "global", section: null });

    await userEvent.click(
      within(bar).getByRole("button", { name: "Project settings" }),
    );
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "open_settings_window"),
      ).toHaveLength(2),
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_settings_window")[1][1],
    ).toEqual({ kind: "project", section: null });

    // Nothing was added to the strip, and no settings surface is in this window.
    expect(
      Array.from(document.querySelectorAll(".tabstrip .tab .tab__label")).map(
        (el) => el.textContent,
      ),
    ).toEqual(tabsBefore);
    expect(screen.queryByTestId("global-settings")).toBeNull();
    expect(
      screen.queryByRole("heading", { name: /appearance/i }),
    ).toBeNull();
  });
});

describe("progress (STB-FR-15 / STB-FR-12)", () => {
  it("establishes the in-flight set from the command on mount", async () => {
    // STB-FR-15: two operations already running when the window mounts; the
    // more recently started shows, without waiting for any event.
    inFlight = [
      {
        id: "op-2",
        kind: "install",
        label: "Installing plugin…",
        state: "running",
        sequence: 2,
      },
      {
        id: "op-1",
        kind: "scan",
        label: "Indexing project…",
        state: "running",
        sequence: 1,
      },
    ];
    render(<App />);
    await enterIde();

    expect(invoked()).toContain("list_in_flight_operations");
    expect(await screen.findByText("Installing plugin…")).toBeInTheDocument();
  });

  it("tracks operations arriving and terminating on the event bus", async () => {
    render(<App />);
    await enterIde();
    expect(screen.queryByTestId("status-bar-progress")).not.toBeInTheDocument();

    act(() => {
      fireBusEvent(OPERATION_PROGRESS, {
        id: "op-7",
        kind: "scan",
        label: "Indexing project…",
        state: "running",
        sequence: 7,
      });
    });
    expect(await screen.findByText("Indexing project…")).toBeInTheDocument();

    // PRG-FR-08 / STB-FR-07: a terminal event leaves the in-flight set, and the
    // centre region goes back to rendering nothing at all.
    act(() => {
      fireBusEvent(OPERATION_PROGRESS, {
        id: "op-7",
        kind: "scan",
        label: "Indexing project…",
        state: "finished",
        sequence: 7,
      });
    });
    await waitFor(() =>
      expect(screen.queryByTestId("status-bar-progress")).not.toBeInTheDocument(),
    );
  });

  it("closes the overlay when another of the window's overlays opens", async () => {
    // SNV-FR-56 / STB-FR-12 / NAW-FR-01 (single-overlay invariant).
    inFlight = [
      {
        id: "op-1",
        kind: "scan",
        label: "Indexing project…",
        state: "running",
        sequence: 1,
      },
    ];
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByTestId("status-bar-progress"));
    expect(screen.getByTestId("in-flight-overlay")).toBeInTheDocument();

    // The top-chrome project switcher is another floating overlay.
    await userEvent.click(screen.getByRole("button", { name: /acme/i }));
    await waitFor(() =>
      expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument(),
    );
  });

  it("closes on a keyboard-opened overlay, not merely on the outside click", async () => {
    // The pointer test above passes for the wrong reason: any outside
    // pointer-down closes the overlay via its own document listener, so
    // deleting the shell's `onOverlayOpening` wiring would not fail it. ⌘K
    // opens the search overlay with no pointer event at all, so only the
    // shell's single-overlay enforcement can close this one (NAW-FR-01).
    inFlight = [
      {
        id: "op-1",
        kind: "scan",
        label: "Indexing project…",
        state: "running",
        sequence: 1,
      },
    ];
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByTestId("status-bar-progress"));
    expect(screen.getByTestId("in-flight-overlay")).toBeInTheDocument();

    await userEvent.keyboard("{Meta>}k{/Meta}");
    await waitFor(() =>
      expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument(),
    );
  });

  it("closes the other overlays when it opens", async () => {
    // The reverse direction of NAW-FR-01, which nothing else covers: opening
    // this overlay must close whatever was already up.
    inFlight = [
      {
        id: "op-1",
        kind: "scan",
        label: "Indexing project…",
        state: "running",
        sequence: 1,
      },
    ];
    render(<App />);
    await enterIde();

    // Open the search overlay first.
    await userEvent.keyboard("{Meta>}k{/Meta}");
    await waitFor(() =>
      expect(document.querySelector(".search-overlay")).not.toBeNull(),
    );

    await userEvent.click(await screen.findByTestId("status-bar-progress"));

    expect(screen.getByTestId("in-flight-overlay")).toBeInTheDocument();
    await waitFor(() =>
      expect(document.querySelector(".search-overlay")).toBeNull(),
    );
  });
});

describe("the diff summary (STB-FR-25 / STB-FR-27 / STB-FR-29)", () => {
  it("loads the totals without the Changes panel ever being opened", async () => {
    // STB-FR-25 / STB-FR-27: the summary holds no change set of its
    // own and does not depend on the panel.
    render(<App />);
    await enterIde();

    await waitFor(() =>
      expect(invoked()).toContain("get_uncommitted_diff_totals"),
    );
    const diffstat = await screen.findByTestId("status-bar-diffstat");
    expect(diffstat).toHaveTextContent("+412");
    expect(diffstat).toHaveTextContent("−87");
  });

  it("reloads on `changes updated`", async () => {
    // STB-FR-27: an external process modifies a tracked file.
    render(<App />);
    await enterIde();
    await screen.findByTestId("status-bar-diffstat");

    diffTotals = { addedLines: 500, removedLines: 90, fileCount: 6 };
    act(() => fireBusEvent(CHANGES_UPDATED, { changeCount: 1 }));

    await waitFor(() =>
      expect(screen.getByTestId("status-bar-diffstat")).toHaveTextContent("+500"),
    );
  });

  it("renders nothing in its place outside a Git repository", async () => {
    // STB-FR-29: the typed error yields no diffstat, no zeros, and
    // no error message — and the rest of the strip renders normally.
    diffTotals = new Error("not a git repository");
    render(<App />);
    await enterIde();

    await waitFor(() =>
      expect(invoked()).toContain("get_uncommitted_diff_totals"),
    );
    expect(screen.queryByTestId("status-bar-diffstat")).not.toBeInTheDocument();
    expect(screen.queryByText(/not a git repository/i)).not.toBeInTheDocument();
    const bar = screen.getByTestId("status-bar");
    expect(
      within(bar).getByRole("button", { name: "Global settings" }),
    ).toBeInTheDocument();
    expect(await screen.findByTestId("line-ending-select")).toBeInTheDocument();
  });
});

describe("the shared line-ending preference (STB-FR-16–STB-FR-19 / SET-FR-10 / SET-FR-11)", () => {
  it("shows the persisted convention and persists a change immediately", async () => {
    // STB-FR-16, STB-FR-17.
    render(<App />);
    await enterIde();

    const select = await screen.findByTestId("line-ending-select");
    expect(select).toHaveValue("lf");

    await userEvent.selectOptions(select, "crlf");
    await waitFor(() => expect(savedConfigs).toEqual([{ lineEndings: "crlf" }]));
    expect(select).toHaveValue("crlf");
  });

  it("marks every open Editor tab dirty, and each writes itself shortly after", async () => {
    // EDT-FR-70 / EDT-FR-41 / STB-FR-18: two clean open tabs go
    // dirty so the pending conversion is visible, and each then writes itself on
    // the ordinary terms of EDT-FR-70 — so the conversion lands shortly after
    // rather than riding invisibly on some later unrelated edit.
    //
    // The other two clauses of EDT-FR-41 — that no buffer changed, and that an
    // artifact with no open tab gains no marker — are asserted against the store
    // in `state/editSessions.conventions.test.ts`, which can see both; from here
    // only the dirty markers and the writes are observable.
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByLabelText("artifact body");
    await userEvent.click(screen.getByText("b.md"));
    await screen.findByLabelText("artifact body");

    const dirtyMarkers = () =>
      document.querySelectorAll(".tab__dirty").length;
    expect(dirtyMarkers()).toBe(0);

    await userEvent.selectOptions(
      screen.getByTestId("line-ending-select"),
      "crlf",
    );

    // Both open Editor tabs, and only those two.
    await waitFor(() => expect(dirtyMarkers()).toBe(2));
    // The selection itself writes nothing — the rest has not elapsed yet.
    expect(invoked()).not.toContain("save_artifact_contents");

    // EDT-FR-70, STB-FR-18's second clause: both are written and both indicators clear.
    await letWriteLand();
    await waitFor(() => expect(dirtyMarkers()).toBe(0));
    expect(savedArtifactIds().sort()).toEqual(["a.md", "b.md"]);
  });

  it("SET-FR-11: a change made in the Project settings window marks every open Editor tab dirty", async () => {
    // STB-FR-18 / SET-FR-11 / EDT-FR-41: a change made there marks the artifact
    // of every open Editor tab dirty exactly as one made here does — and those
    // tabs are in THIS window, so the announcement has to carry that far or the
    // pending conversion rides invisibly on some later unrelated edit.
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByLabelText("artifact body");
    await userEvent.click(screen.getByText("b.md"));
    await screen.findByLabelText("artifact body");

    const dirtyMarkers = () => document.querySelectorAll(".tab__dirty").length;
    expect(dirtyMarkers()).toBe(0);

    act(() => {
      fireBusEvent("project-config:line-endings-changed", { value: "crlf" });
    });

    await waitFor(() => expect(dirtyMarkers()).toBe(2));
    // Neither buffer's content changed, and this window wrote nothing — the
    // other one already did.
    expect(invoked().filter((c) => c === "save_project_config")).toHaveLength(0);
    expect(screen.getByTestId("line-ending-select")).toHaveValue("crlf");
  });

  it("STB-FR-19: is one value shared with the Project settings window", async () => {
    // STB-FR-19 / SET-FR-10, PST-FR-22 / SET-FR-11: the two controls that edit
    // it are in two different windows now (SWN-FR-01), so "each reflects a
    // change made from the other without a reload" means each announces its own
    // write and adopts the other's. This is the status bar's half; the Project
    // section's is in `SettingsWindowApp.test.tsx`.
    render(<App />);
    await enterIde();
    await waitFor(() =>
      expect(screen.getByTestId("line-ending-select")).toHaveValue("lf"),
    );

    // The Project settings window persisted CRLF and said so.
    act(() => {
      fireBusEvent("project-config:line-endings-changed", { value: "crlf" });
    });
    await waitFor(() =>
      expect(screen.getByTestId("line-ending-select")).toHaveValue("crlf"),
    );
    // It reflected the write rather than repeating it.
    expect(invoked().filter((c) => c === "save_project_config")).toHaveLength(0);

    // …and back the other way: a change made here is persisted at once and
    // announced, so the other window reflects it on the same terms.
    await userEvent.selectOptions(
      screen.getByTestId("line-ending-select"),
      "lf",
    );
    await waitFor(() => expect(savedConfigs).toEqual([{ lineEndings: "lf" }]));
    await waitFor(() =>
      expect(
        emitMock.mock.calls.filter(
          (c) => c[0] === "project-config:line-endings-changed",
        ),
      ).toEqual([["project-config:line-endings-changed", { value: "lf" }]]),
    );
  });
});

describe("the diff summary is always the uncommitted one (STB-FR-26)", () => {
  it("ignores the Changes panel's mode and target branch", async () => {
    // The panel is configured in Branch mode against `main`; the status bar
    // must still report the uncommitted totals. Structurally they read different
    // backend operations, and this pins that: the branch command's answer is
    // deliberately different, so a summary that ever read it would show 999.
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "load_changes_panel_state")
          return { mode: "branch", targetBranch: "main" };
        if (cmd === "list_branch_changes")
          return {
            comparison: {
              kind: "branch",
              targetBranch: "main",
              mergeBase: "abc",
            },
            entries: [
              {
                id: "a.md",
                path: "a.md",
                name: "a.md",
                changeStatus: "modified",
                addedLines: 999,
                removedLines: 999,
                isBinary: false,
              },
            ],
          };
        return defaultInvoke(cmd, args);
      },
    );
    render(<App />);
    await enterIde();

    // Open the Changes panel so its branch-mode load actually runs.
    await userEvent.click(screen.getByRole("button", { name: "Changes" }));
    await waitFor(() => expect(invoked()).toContain("list_branch_changes"));

    const diffstat = await screen.findByTestId("status-bar-diffstat");
    expect(diffstat).toHaveTextContent("+412");
    expect(diffstat).toHaveTextContent("\u2212 87".replace(" ", ""));
    expect(diffstat).not.toHaveTextContent("999");
  });
});

describe("the indentation control (STB-FR-21 / STB-FR-22)", () => {
  it("is inert on a Dashboard tab and describes the artifact on an Editor tab", async () => {
    // STB-FR-20, STB-FR-22 / STB-FR-21, STB-FR-23, DFV-FR-42, DFV-FR-54: the Dashboard owns no artifact, so there is
    // nothing to describe; opening an Editor tab on a two-space-indented file
    // reports spaces with a width of 2 (EDT-FR-37).
    render(<App />);
    await enterIde();

    expect(screen.getByTestId("indentation-inert")).toBeInTheDocument();
    expect(screen.queryByTestId("indentation-select")).not.toBeInTheDocument();

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByLabelText("artifact body");

    const select = await screen.findByTestId("indentation-select");
    expect(select).toHaveValue("spaces-2");

    // STB-FR-23: overriding it marks nothing dirty and writes nothing.
    await userEvent.selectOptions(select, "tabs");
    expect(select).toHaveValue("tabs");
    expect(invoked()).not.toContain("save_artifact_contents");
  });

  it("STB-FR-20, STB-FR-22: goes inert on every tab type that edits no artifact", async () => {
    // STB-FR-22 names the tab types that own no artifact: Dashboard, Flow,
    // Search results, History detail, and a Diff tab whose comparison deletes
    // the file. Neither settings surface is among them any more — each is a
    // native child window rather than a tab of this viewport (SWN-FR-01), so
    // there is no settings tab for the control to describe or fail to; the
    // window-level assertion that the strip is untouched lives in
    // `App.test.tsx`.
    //
    // The interesting direction is each tab reached *after* an Editor tab has
    // put a convention on screen, where a stale value would linger. So the
    // Editor is returned to between each, and the assertion is made on the way
    // out of it rather than from a cold start.
    render(<App />);
    await enterIde();

    await userEvent.click(await screen.findByText("a.md"));
    await screen.findByLabelText("artifact body");
    expect(await screen.findByTestId("indentation-select")).toBeInTheDocument();

    const strip = () => within(screen.getByTestId("tabstrip"));

    // A Search results tab, opened from the search overlay.
    const input = screen.getByPlaceholderText(/Search artifacts/i);
    await userEvent.type(input, "convention{Enter}");
    await waitFor(() =>
      expect(screen.getByTestId("indentation-inert")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("indentation-select")).not.toBeInTheDocument();
    expect(screen.getByTestId("line-ending-select")).toBeEnabled();

    // Back to the Editor: the convention comes back with it.
    await userEvent.click(strip().getByText("a.md"));
    await waitFor(() =>
      expect(screen.getByTestId("indentation-select")).toBeInTheDocument(),
    );

    // …and the Dashboard.
    await userEvent.click(strip().getByText("Dashboard"));
    await waitFor(() =>
      expect(screen.getByTestId("indentation-inert")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("indentation-select")).not.toBeInTheDocument();
    // STB-FR-20: the line-ending control is unaffected by the tab type.
    expect(screen.getByTestId("line-ending-select")).toBeEnabled();
  });
});
