/**
 * New Artifact as a TAB over a draft (NAW-new-artifact.md), end to end through
 * the shell.
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
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { pickSelector } from "./test/selectors";
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

// NAW-new-artifact.md: New Artifact is a TAB over a draft, not a modal. Its two
// entry points both create a draft and open it, and nothing reaches the project
// until the draft is graduated (NAW-FR-24).
describe("New Artifact tab wiring (NAW-FR-01 / NAW-FR-03 / NAW-FR-22)", () => {
  function treeWithSpecsFolder() {
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

  // NAW-FR-01 / SNV-FR-56: the workspace is a tab, so it neither closes an
  // overlay nor is closed by one — the exact inversion of the old modal.
  it("NAW-FR-01: the workspace is a tab, so the search overlay does not close it", async () => {
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // Open the search overlay over it.
    await userEvent.keyboard("{Meta>}k{/Meta}");

    // The workspace is still mounted: a tab is not an overlay.
    expect(
      screen.getByRole("button", { name: "Draft actions" }),
    ).toBeInTheDocument();
  });

  // NAW-FR-03 / DRP-FR-06: the Drafts panel creates a draft and opens it, and
  // it is the only surface that does.
  it("NAW-FR-03: the Drafts panel creates a draft and opens its tab", async () => {
    render(<App />);
    await enterIde();

    await createDraftFromPanel();

    const created = invokeMock.mock.calls.find((c) => c[0] === "create_draft");
    expect(created).toBeTruthy();
    // DRS-FR-07: a creation says one thing about where anything goes — which
    // drafts folder the draft is filed in — and the pinned affordance names
    // none, creating the draft at the drafts root.
    expect(created![1]).toEqual({ name: null, folder: null });
    expect(created![1]).not.toHaveProperty("destinationRoot");
    // Nothing was written into the project (NAW-FR-24 is the only writer).
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_artifact")).toBe(
      false,
    );
  });

  /**
   * DRP-FR-06, NAW-FR-03, NAW-FR-04 / DRP-FR-36 end to end through the shell: the two halves of the
   * one act land in two different places. The New Artifact tab opens and is the
   * ACTIVE tab in the main viewport, and the keystrokes go to the Drafts
   * panel's inline name field — the author names the draft first and writes in
   * it second.
   */
  it("DRP-FR-36: the created draft's tab is active while focus stays in the panel's name field", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
    await userEvent.click(
      screen.getByRole("button", { name: "New draft" }),
    );

    // The tab opened and is the active one.
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");
    await waitFor(() =>
      expect(
        strip.querySelector('.tab[data-active="true"] .tab__label')
          ?.textContent,
      ).toBe("Untitled"),
    );

    // And the field — in the panel, not the tab — has the keystrokes, seeded
    // with the name the draft was created under and with its text selected.
    const field = (await screen.findByLabelText(
      "Draft name",
    )) as HTMLInputElement;
    expect(field).toHaveValue("Untitled");
    expect(field).toHaveFocus();
    expect(field.closest(".vpanel")).not.toBeNull();
    expect(field.selectionStart).toBe(0);
    expect(field.selectionEnd).toBe("Untitled".length);

    // DRP-FR-11: typing renames the draft, and the tab's label follows.
    await userEvent.keyboard("overview{Enter}");
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "rename_draft" && c[1]?.name === "overview",
        ),
      ).toBe(true),
    );
  });

  /**
   * DRP-FR-06, DRP-FR-36, NAW-FR-03, NAW-FR-04 tail: a second creation is `Untitled 2`, and abandoning its name
   * field with Escape leaves that draft existing under the name it was created
   * with — a create and a rename being two operations rather than one.
   */
  it("DRP-FR-06, DRP-FR-36, NAW-FR-03, NAW-FR-04: a second draft is Untitled 2 and survives an abandoned rename", async () => {
    render(<App />);
    await enterIde();
    await createDraftFromPanel();

    await userEvent.click(screen.getByRole("button", { name: /Draft$/ }));
    const field = await screen.findByLabelText("Draft name");
    expect(field).toHaveValue("Untitled 2");

    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByLabelText("Draft name")).toBeNull(),
    );
    // Nothing was renamed, and both drafts are in the tree.
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_draft")).toBe(
      false,
    );
    const panel = document.querySelector(".vpanel") as HTMLElement;
    expect(within(panel).getByText("Untitled")).toBeInTheDocument();
    expect(within(panel).getByText("Untitled 2")).toBeInTheDocument();
  });

  // NAW-FR-01, NAW-FR-03, NAW-FR-04, NAW-FR-05, NAW-FR-06, NAW-FR-07, NAW-FR-08, NAW-FR-37, NTA-FR-14 tail / DRP-FR-06 / NFI-FR-06, NFW-FR-06, NTA-FR-07, SNV-FR-24 / OVW-FR-06, OVW-FR-07: the File menu's New
  // Artifact opens the typed-artifact window instead. It creates no draft and
  // opens no tab of this kind — the two share a name in the menus and nothing
  // else (NAW-FR-03, NTA-FR-14).
  it("NAW-FR-03: File → New Artifact opens the typed-artifact window and creates no draft", async () => {
    render(<App />);
    await enterIde();

    fireBusEvent("menu:new-artifact");

    const dialog = await screen.findByRole("dialog", {
      name: "New Artifact",
    });
    expect(within(dialog).getByLabelText(/^Artifact Type/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Draft actions" }),
    ).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_draft")).toBe(
      false,
    );
  });

  // NTA-FR-08, LCM-FR-08, LCM-FR-09, LCM-FR-10 / NTA-FR-06 / DRP-FR-06, NTA-FR-14: the Project context menu's New Artifact
  // opens the typed-artifact window with the right-clicked folder as the
  // STARTING location and the type unchosen. Until it is confirmed nothing is
  // written into that folder, no draft is created, and no tab opens anywhere.
  it("NAW-FR-07, NAW-FR-38: the Project entry opens the typed-artifact window and creates no draft", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_project_tree") return treeWithSpecsFolder();
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    const folder = await screen.findByText("specifications");
    fireEvent.contextMenu(folder);
    fireEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText(
        "New Artifact",
      ),
    );

    const dialog = await screen.findByRole("dialog", { name: "New Artifact" });
    expect(within(dialog).getByLabelText(/^Location/)).toHaveValue(
      "specifications",
    );
    // NTA-FR-08: the type is never seeded from the location.
    expect(within(dialog).getByLabelText(/^Artifact Type/)).toHaveValue("");
    expect(
      screen.queryByRole("button", { name: "Draft actions" }),
    ).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_draft")).toBe(
      false,
    );
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "create_typed_file"),
    ).toBe(false);
  });

  // DRP-FR-11 / NAW-FR-25 / NAW-FR-04: the strip's label follows the draft's
  // name whichever surface the rename was made from. Asserted end to end here
  // because the panel and the tab strip are wired together in `App` — the panel
  // reporting a rename and the shell relabelling a tab are each covered in
  // isolation, and neither notices if the two are not connected.
  it("DRP-FR-11, NAW-FR-25: renaming a draft in the panel renames its open tab and its file", async () => {
    // NAW-FR-25: the rename carries the draft's prompt, so the open tab has to
    // follow it as surely as the strip's label does.
    let primary = "Untitled.md";
    let draftName = "Untitled";
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts")
        return { folders: [], drafts: [
          {
            id: "d1",
            name: draftName,
            promptPath: primary,
            status: "active",
            updatedAt: "2026-07-31T10:00:00Z",
          },
        ] };
      if (cmd === "open_draft")
        return {
          id: "d1",
          name: draftName,
          promptPath: primary,
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      if (cmd === "rename_draft") {
        draftName = "Agent personas";
        primary = "Agent personas.md";
        return {
          id: "d1",
          name: draftName,
          promptPath: primary,
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      }
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();

    // SNV-FR-64: no Drafts toggle is clicked here. The New Artifact tab that
    // just opened is an activation, so the vertical panel has already followed
    // it to the Drafts panel and revealed this very draft — and clicking the
    // toggle now would *hide* the panel again (SNV-FR-45). The row query below
    // is what waits for the panel to arrive.
    // DRP-FR-22: a row's menu opens on right-click, the way the Project
    // panel's does — there is no per-row button to press.
    fireEvent.contextMenu(
      await screen.findByRole("treeitem", { name: /^Draft Untitled/ }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    await userEvent.type(field, "Agent personas{Enter}");

    await waitFor(() =>
      expect(within(strip).getByText("Agent personas")).toBeInTheDocument(),
    );
    expect(within(strip).queryByText("Untitled")).not.toBeInTheDocument();

    // …and the open tab's own chrome reads it too. A rename is a rename
    // whichever surface it was made from: the workspace used to render the name
    // from the record it loaded on mount, which no rename elsewhere ever
    // reached, leaving the strip and the tab under it disagreeing.
    const workspace = document.querySelector(
      ".draft-workspace__chrome",
    ) as HTMLElement;
    expect(within(workspace).getByText(/Agent personas/)).toBeInTheDocument();

    // NAW-FR-25: and the tab re-read the record, which is how its session
    // follows the prompt to its new path. A tab that heard only about the name
    // would go on writing the author's typing to `Untitled.md`, which the
    // atomic write recreates — leaving the draft holding two files, exactly
    // what DRS-FR-11 forbids. (That the session's keys travel is asserted
    // against the store itself in `NewArtifactWorkspace.test.tsx`.)
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "open_draft").length,
      ).toBeGreaterThan(1),
    );
  });

  // DRP-FR-12: "confirming … closes that draft's New Artifact tab".
  // The panel reporting the deletion and the shell dropping the tab are each
  // covered in isolation, and neither notices if the two are not connected.
  it("DRP-FR-12: deleting a draft in the panel closes its open tab", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts")
        return { folders: [], drafts: [
          {
            id: "d1",
            name: "Untitled",
            promptPath: "Untitled.md",
            status: "active",
            updatedAt: "2026-07-31T10:00:00Z",
          },
        ] };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();

    // SNV-FR-64: no Drafts toggle is clicked here. The New Artifact tab that
    // just opened is an activation, so the vertical panel has already followed
    // it to the Drafts panel and revealed this very draft — and clicking the
    // toggle now would *hide* the panel again (SNV-FR-45). The row query below
    // is what waits for the panel to arrive.
    // DRP-FR-22: a row's menu opens on right-click, the way the Project
    // panel's does — there is no per-row button to press.
    fireEvent.contextMenu(
      await screen.findByRole("treeitem", { name: /^Draft Untitled/ }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    // Nothing yet: the confirmation is a step, not a formality.
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(false);
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete draft" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );

    await waitFor(() =>
      expect(within(strip).queryByText("Untitled")).not.toBeInTheDocument(),
    );
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(true);
    // TAB-FR-15: the strip never empties.
    expect(within(strip).getByText("Dashboard")).toBeInTheDocument();
  });

  // NAW-FR-21 / NAW-FR-20: "the tab closes, and the draft is gone from the
  // Drafts panel". Same argument as above — the workspace reporting the
  // graduation and the shell dropping the tab are wired together only in `App`.
  /** One graduation run, in the shape `list_graduation_queue` returns. */
  function graduationRunFixture(id: string, state: string) {
    return {
      id,
      projectKey: "p",
      draftId: `draft-${id}`,
      mode: "git",
      state,
      queue: "graduation",
      input: {
        draftId: `draft-${id}`,
        draftName: id,
        prompt: "p",
        promptChecksum: "c",
        capturedAt: "2026-07-31T10:00:00Z",
      },
      source: {
        kind: "git",
        sourceWorktreePath: "/dev/acme",
        sourceBranch: "main",
        sourceRevision: "abc",
        graduationBranch: `synthesis/graduation/${id}`,
        graduationWorktreePath: `/store/${id}/worktree`,
      },
      iteration: 0,
      enqueuedAt: "2026-07-31T10:00:00Z",
      updatedAt: "2026-07-31T10:00:00Z",
    };
  }

  it("NAW-FR-19: graduating keeps the tab and opens the run in the Runs panel", async () => {
    // NAW-FR-20 / GRU-FR-MYFA: a graduation enqueues a run; the draft is not
    // ended by it, so the tab stays open — read-only — and the shell routes to
    // the run rather than closing anything.
    let graduation: unknown = null;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts")
        return {
          folders: [],
          drafts: [
            {
              id: "d1",
              name: "Untitled",
              promptPath: "Untitled.md",
              status: "active",
              updatedAt: "2026-07-31T10:00:00Z",
              graduation,
            },
          ],
        };
      if (cmd === "open_draft")
        return {
          id: "d1",
          name: "Untitled",
          promptPath: "Untitled.md",
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      if (cmd === "get_draft_graduation") return graduation;
      if (cmd === "start_graduation") {
        graduation = { runId: "run-1", state: "queued", locked: true };
        return {
          id: "run-1",
          projectKey: "p",
          draftId: "d1",
          mode: "git",
          state: "queued",
          input: {
            draftId: "d1",
            draftName: "Untitled",
            prompt: "p",
            promptChecksum: "c",
            capturedAt: "2026-07-31T10:00:00Z",
          },
          source: {
            kind: "git",
            sourceWorktreePath: "/dev/acme",
            sourceBranch: "main",
            sourceRevision: "abc",
            graduationBranch: "synthesis/graduation/run-1",
            graduationWorktreePath: "/store/run-1/worktree",
          },
          iteration: 0,
          enqueuedAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      }
      if (cmd === "list_graduation_queue")
        return { projectKey: "p", runs: [] };
      // The start dialog asks which work stream the run belongs to, so a test
      // that graduates must offer one (WKS-FR-SGCM).
      if (cmd === "list_work_streams")
        return [
          {
            stream: {
              id: "w1",
              projectKey: "p",
              name: "editor work",
              branch: "synthesis/stream/editor-work",
              worktreePath: "/store/w/w1",
              baseBranch: "main",
              baseRevision: "abc",
              createdAt: "2026-07-31T10:00:00Z",
              busyRunId: null,
              isMissing: false,
            },
            queuedRunCount: 0,
            aheadOfBase: 0,
          },
        ];
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");

    // NAW-FR-27 / NAW-FR-28: Graduate is behind the tab's one action control.
    await userEvent.click(
      await screen.findByRole("button", { name: "Draft actions" }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate" }));
    const dialog = await screen.findByRole("dialog", {
      name: /Graduate .Untitled./,
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Graduate" }),
    );

    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: /Graduate .Untitled./ }),
      ).toBeNull(),
    );
    // NAW-FR-20: the tab is still there, and nothing was published or deleted.
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "graduate_draft")).toBe(
      false,
    );
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(
      false,
    );
    // GRU-FR-MYFA / GRU-FR-MYFA: the shell routed to the run, which is in the Runs
    // panel's graduation section.
    await waitFor(() =>
      expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Runs"),
    );
  });

  it("GRU-FR-MYFA: graduating from agent output routes to Runs and leaves the reading position", async () => {
    // An end-to-end reading of GRU-FR-MYFA's second clause: the shell still
    // routes to the Runs panel (GRU-FR-MYFA), the run that started is listed, and
    // the run the author was reading is the one they come back to.
    //
    // It is a **smoke test rather than the pin**: switching to agent output
    // unmounts the section, so the route mounts it afresh and the remembered
    // selection would be restored here whether or not the override was
    // withheld. What actually pins the withholding is the pair of tests in
    const runs: unknown[] = [
      graduationRunFixture("older-run", "running"),
      graduationRunFixture("being-read", "running"),
    ];
    let graduation: unknown = null;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts")
        return {
          folders: [],
          drafts: [
            {
              id: "d1",
              name: "Untitled",
              promptPath: "Untitled.md",
              status: "active",
              updatedAt: "2026-07-31T10:00:00Z",
              graduation,
            },
          ],
        };
      if (cmd === "open_draft")
        return {
          id: "d1",
          name: "Untitled",
          promptPath: "Untitled.md",
          status: "active",
          createdAt: "2026-07-31T10:00:00Z",
          updatedAt: "2026-07-31T10:00:00Z",
        };
      if (cmd === "get_draft_graduation") return graduation;
      // The start dialog asks which work stream the run belongs to
      // (WKS-FR-SGCM).
      if (cmd === "list_work_streams")
        return [
          {
            stream: {
              id: "w1",
              projectKey: "p",
              name: "editor work",
              branch: "synthesis/stream/editor-work",
              worktreePath: "/store/w/w1",
              baseBranch: "main",
              baseRevision: "abc",
              createdAt: "2026-07-31T10:00:00Z",
              busyRunId: null,
              isMissing: false,
            },
            queuedRunCount: 0,
            aheadOfBase: 0,
          },
        ];
      if (cmd === "start_graduation") {
        graduation = { runId: "just-started", state: "queued", locked: true };
        const started = graduationRunFixture("just-started", "queued");
        runs.push(started);
        return started;
      }
      if (cmd === "list_graduation_queue") return { projectKey: "p", runs };
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    render(<App />);
    await enterIde();
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });

    // Open the Runs panel on its graduation section and choose a run to read.
    await userEvent.click(screen.getByRole("button", { name: "Runs" }));
    const rail = await screen.findByRole("list", {
      name: "Graduation run history",
    });
    const rowFor = (name: string) =>
      within(rail)
        .getAllByRole("button")
        .filter((b) => b.classList.contains("graduation__row-open"))
        .find((b) => b.getAttribute("aria-label") === name)!;
    await userEvent.click(rowFor("being-read"));
    expect(rowFor("being-read")).toHaveAttribute("aria-current", "true");

    // Move to agent output, and graduate from there.
    await userEvent.click(screen.getByRole("tab", { name: "Agent output" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "Draft actions" }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate" }));
    const dialog = await screen.findByRole("dialog", {
      name: /Graduate .Untitled./,
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Graduate" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: /Graduate .Untitled./ }),
      ).toBeNull(),
    );

    // GRU-FR-MYFA: the shell still routes to the Runs panel's graduation section.
    const back = await screen.findByRole("list", {
      name: "Graduation run history",
    });
    const rowIn = (name: string) =>
      within(back)
        .getAllByRole("button")
        .filter((b) => b.classList.contains("graduation__row-open"))
        .find((b) => b.getAttribute("aria-label") === name);
    // GRU-FR-MYFA: but the run it started did not take the selection, and it is
    // listed like any other run.
    await waitFor(() => expect(rowIn("just-started")).toBeTruthy());
    expect(rowIn("being-read")).toHaveAttribute("aria-current", "true");
    expect(rowIn("just-started")).not.toHaveAttribute("aria-current");
  });

  /**
   * A draft whose status the mock actually holds, so an archive made on one
   * surface is visible from the other.
   */
  function wireArchivableDraft() {
    const record: Record<string, unknown> = {
      id: "d1",
      name: "Untitled",
      promptPath: "Untitled.md",
      status: "active",
      createdAt: "2026-07-31T10:00:00Z",
      updatedAt: "2026-07-31T10:00:00Z",
    };
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts")
        return { folders: [], drafts: [{ ...record, fileCount: 1 }] };
      if (cmd === "open_draft") return { ...record };
      if (cmd === "set_draft_status") {
        record.status = (args as { status: string }).status;
        return { ...record };
      }
      return defaultInvoke(cmd, args as Record<string, unknown>);
    });
    return record;
  }

  // NAW-FR-16, NAW-FR-36 / NAW-FR-28: "the tab closes". The workspace reporting the
  // archive and the shell closing the tab are wired together only in `App`, and
  // the workspace's own test can see no further than its `onArchived` callback.
  it("NAW-FR-16, NAW-FR-28, NAW-FR-36: archiving from the tab's action control closes the tab", async () => {
    wireArchivableDraft();
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Draft actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Archive" }));

    await waitFor(() =>
      expect(within(strip).queryByText("Untitled")).not.toBeInTheDocument(),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "set_draft_status")?.[1],
    ).toEqual({ id: "d1", status: "archived" });
    // NAW-FR-16 / NAW-FR-23: retired, not removed.
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(false);
    // TAB-FR-15: the strip never empties.
    expect(within(strip).getByText("Dashboard")).toBeInTheDocument();
  });

  // DRP-FR-05, DRP-FR-07 / DRP-FR-18: "a draft with an open New Artifact tab keeps it
  // open, the tab following the change into or out of its archived marker".
  // The whole of that wiring lives in `App` — the panel bumps the shell's draft
  // revision, and the open workspace re-reads its record from it. The panel's
  // own test can see only that a callback fired.
  it("DRP-FR-18, DRP-FR-05, DRP-FR-07: archiving from the panel leaves the tab open and marks it", async () => {
    wireArchivableDraft();
    render(<App />);
    await enterIde();

    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    const strip = screen.getByTestId("tabstrip");
    const workspace = document.querySelector(
      ".draft-workspace__chrome",
    ) as HTMLElement;
    expect(within(workspace).queryByText("Archived")).not.toBeInTheDocument();

    // SNV-FR-64: no Drafts toggle is clicked here. The New Artifact tab that
    // just opened is an activation, so the vertical panel has already followed
    // it to the Drafts panel and revealed this very draft — and clicking the
    // toggle now would *hide* the panel again (SNV-FR-45). The row query below
    // is what waits for the panel to arrive.
    fireEvent.contextMenu(
      await screen.findByRole("treeitem", { name: /^Draft Untitled/ }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Archive" }));

    // The marker appears in the tab under it, and the tab stays open — a tab
    // closed out from under the author by something done in a panel is a
    // surprise, where a tab closed by the Archive action taken *in* it is not.
    await waitFor(() =>
      expect(within(workspace).getByText("Archived")).toBeInTheDocument(),
    );
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "set_draft_status")?.[1],
    ).toEqual({ id: "d1", status: "archived" });

    // DRP-FR-18: the row left the panel's default position, because that
    // position no longer admits its status. It is one position away, not gone.
    expect(
      screen.queryByRole("treeitem", { name: /^Draft Untitled/ }),
    ).not.toBeInTheDocument();
    await pickSelector("Draft status", "archived");

    // …and Restore from the same menu clears it, still without touching the tab.
    fireEvent.contextMenu(
      await screen.findByRole("treeitem", { name: /^Draft Untitled/ }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Restore" }));

    await waitFor(() =>
      expect(within(workspace).queryByText("Archived")).not.toBeInTheDocument(),
    );
    expect(within(strip).getByText("Untitled")).toBeInTheDocument();
  });
});
