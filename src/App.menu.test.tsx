import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listen } from "@tauri-apps/api/event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { letWriteLand } from "./test/autosave";
import { cancelAllScheduledWrites } from "./state/writeSchedule";

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

/** Artifact ids written so far, in write order. */
const savedIds = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_artifact_contents")
    .map((c) => (c[1] as { id: string }).id);

/**
 * Open an artifact from the Library and leave it with an unsaved edit, made the
 * way a user would: through the Editor's raw-Markdown surface, which is a plain
 * textarea (the WYSIWYG surface needs a full Tiptap interaction to type into).
 */
async function openAndEdit(name: string, text: string) {
  await userEvent.click(await screen.findByText(name));
  await userEvent.click(
    await screen.findByRole("button", { name: "Edit as Markdown source" }),
  );
  const source = await screen.findByLabelText("Markdown source");
  await userEvent.type(source, text);
}

/** Two artifacts open in Editor tabs, both dirty, with b.md the active tab. */
async function openTwoDirtyArtifacts() {
  await openAndEdit("a.md", " a-edit");
  await openAndEdit("b.md", " b-edit");
}

/**
 * Put down every rest armed by the edits above, so the only write that can
 * follow is the one the menu item performs.
 *
 * A test about Save's *scope* must not also be a test about whether it outran
 * the artifacts writing themselves (EDT-FR-70): leave the rests running and
 * "a.md was not written" holds only while the test finishes inside 700ms of the
 * keystroke. Cancelling costs the assertion nothing — Save writes because the
 * buffer is dirty, not because a rest was pending.
 */
function stillTheRests() {
  cancelAllScheduledWrites();
}

describe("File menu wiring (SNV-FR-23..26)", () => {
  it("subscribes to every File-menu event exactly once on mount", async () => {
    render(<App />);
    await screen.findByText("acme");
    await waitFor(() => {
      expect(listeners["menu:new-file"]).toBeTypeOf("function");
      expect(listeners["menu:new-folder"]).toBeTypeOf("function");
      expect(listeners["menu:new-artifact"]).toBeTypeOf("function");
      expect(listeners["menu:save"]).toBeTypeOf("function");
      expect(listeners["menu:save-all"]).toBeTypeOf("function");
      expect(listeners["menu:close-project"]).toBeTypeOf("function");
      expect(listeners["menu:exit-requested"]).toBeTypeOf("function");
      expect(listeners["menu:find"]).toBeTypeOf("function");
      expect(listeners["menu:find-replace"]).toBeTypeOf("function");
      // ABT-FR-KMVD: the About entry, offered with or without a project.
      expect(listeners["menu:about"]).toBeTypeOf("function");
    });
    // Exactly one subscription per menu event — proving the latest-ref pattern
    // subscribes once and does not re-register per render. (The shell also
    // watches "artifact changed externally"; that is not a menu channel.)
    const menuEvents = listenMock.mock.calls
      .map((c) => c[0] as string)
      .filter((e) => e.startsWith("menu:"));
    expect(menuEvents).toEqual([
      "menu:new-file",
      "menu:new-folder",
      "menu:new-artifact",
      "menu:save",
      "menu:save-all",
      "menu:close-project",
      "menu:exit-requested",
      // SNV-FR-43: the Edit menu's two Editor-scoped find channels.
      "menu:find",
      "menu:find-replace",
      "menu:about",
    ]);
  });

  it("detaches its menu listeners on unmount", async () => {
    const { unmount } = render(<App />);
    await waitFor(() => {
      expect(
        Object.keys(listeners).filter((e) => e.startsWith("menu:")),
      ).toHaveLength(10);
    });
    unmount();
    // The effect cleanup calls each captured unlisten fn, which the mock uses to
    // delete its event key — so no listener survives the unmount (no leak).
    expect(listeners).toEqual({});
  });

  it("NFW-FR-06, NTA-FR-07, SNV-FR-24 / NFI-FR-06: New File opens the New File modal with the location at the project root", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-file");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("New File")).toBeInTheDocument();
    // NFI-FR-06: the File-menu flow starts at the project root with an empty name.
    expect(within(dialog).getByLabelText("Location")).toHaveValue("");
    expect(within(dialog).getByLabelText("Name")).toHaveValue("");
    // NFI-FR-03: nothing a plain file has no use for.
    expect(within(dialog).queryByLabelText("Artifact Type")).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Content")).not.toBeInTheDocument();
    expect(within(dialog).queryByText("AI prompt")).not.toBeInTheDocument();
    // Opening it invokes no creation call — the window only collects inputs.
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_file")).toBe(false);
  });

  // NTA-FR-01, NFI-FR-01 / SNV-FR-56: the single-overlay invariant, in both directions.
  // New Artifact is deliberately absent from it — it is a tab, not an overlay
  // (NAW-FR-01), and SNV-FR-56 covers that it neither closes one nor is closed
  // by one.
  it("NTA-FR-01, NFI-FR-01: opening New File closes the New Folder window, and vice versa", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-file");
    let dialogs = screen.getAllByRole("dialog");
    expect(dialogs).toHaveLength(1);
    expect(within(dialogs[0]).getByText("New File")).toBeInTheDocument();

    await fireMenu("menu:new-folder");
    dialogs = screen.getAllByRole("dialog");
    expect(dialogs).toHaveLength(1);
    expect(within(dialogs[0]).getByText("New Folder")).toBeInTheDocument();
    expect(screen.queryByText("New File")).not.toBeInTheDocument();
  });

  // NFI-FR-09, NFI-FR-10 / NFI-FR-11, ESH-FR-ATDS, LIB-FR-18 / NFI-FR-12: confirming creates the file, closes the
  // window, opens the new file in an Editor tab, and reveals it in the Library.
  it("NFI-FR-09, NFI-FR-10/10: creating an untyped file opens it in an Editor tab and closes the window", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_file") {
        // An untyped plain file: the backend recorded no assignment and nothing
        // classifies `package.json` (NFI-FR-11).
        return {
          id: "package.json",
          name: "package.json",
          path: "package.json",
          nodeKind: "file",
        };
      }
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-file");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "package.json");
    await userEvent.click(within(dialog).getByText("Create"));

    // NFI-FR-09: the creation call carries the inputs; an unset location is null.
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_file", {
        location: null,
        name: "package.json",
      }),
    );
    // NFI-FR-09: the window closes on success.
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    // NFI-FR-12: the new file opens in a tab — an Editor tab, since it resolves
    // to no artifact type (ESH-FR-ATDS).
    const tabstrip = screen.getByTestId("tabstrip");
    await waitFor(() =>
      expect(within(tabstrip).getByText("package.json")).toBeInTheDocument(),
    );
  });

  // NFI-FR-14: a rejected creation keeps the window open with its
  // inputs intact, opens no tab, and shows the error inline.
  it("NFI-FR-14: a collision keeps the window open with its inputs intact and opens no tab", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_file") throw "already exists: package.json";
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-file");
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "package.json");
    await userEvent.click(within(dialog).getByText("Create"));

    await waitFor(() =>
      expect(within(dialog).getByRole("alert")).toHaveTextContent(
        "already exists: package.json",
      ),
    );
    // Still open, name intact, and no tab was opened for the file that was never
    // created.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Name")).toHaveValue("package.json");
    expect(
      within(screen.getByTestId("tabstrip")).queryByText("package.json"),
    ).not.toBeInTheDocument();
  });

  it("NFI-FR-06, NTA-FR-07, SNV-FR-24 / NFW-FR-06: New Folder opens the New Folder modal with the parent at the project root", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-folder");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("New Folder")).toBeInTheDocument();
    // NFW-FR-06: the File-menu flow starts at the project root, with the name
    // empty and the artifact type unset.
    expect(within(dialog).getByLabelText("Parent Folder")).toHaveValue("");
    expect(within(dialog).getByLabelText("Name")).toHaveValue("");
    expect(within(dialog).getByLabelText("Artifact Type")).toHaveValue("");
  });

  // NFI-FR-06, NFW-FR-06, SNV-FR-24 / NTA-FR-07: New Artifact opens the typed-artifact modal with its
  // location at the project root and its artifact type unchosen. No New
  // Artifact tab is opened and no draft is created — a draft is created in the
  // Drafts panel alone (DRP-FR-06, DRP-FR-26), so the three File-menu creation
  // items are three modals.
  it("NFI-FR-06, NFW-FR-06, NTA-FR-07, SNV-FR-24: New Artifact opens the typed-artifact modal and creates no draft", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:new-artifact");

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("New Artifact")).toBeInTheDocument();
    // NTA-FR-07: the project root, an empty name, and no type chosen.
    expect(within(dialog).getByLabelText(/^Location/)).toHaveValue("");
    expect(within(dialog).getByLabelText(/^Name/)).toHaveValue("");
    expect(within(dialog).getByLabelText(/^Artifact Type/)).toHaveValue("");
    // NTA-FR-14: no draft, and no tab of the New Artifact kind.
    expect(invokeMock.mock.calls.some((c) => c[0] === "create_draft")).toBe(
      false,
    );
    expect(
      screen.queryByRole("button", { name: "Draft actions" }),
    ).not.toBeInTheDocument();
  });

  it("OVW-FR-02, PST-FR-14, ASC-FR-14, SNV-FR-25: Close project returns to the Project picker", async () => {
    render(<App />);
    await enterIde();
    // Precondition: the IDE shell is mounted (the activity bar exists).
    expect(
      screen.getByRole("button", { name: "Global settings" }),
    ).toBeInTheDocument();

    await fireMenu("menu:close-project");

    // Back on the picker; the main shell is gone (OVW-FR-02).
    expect(await screen.findByTestId("picker-left")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Global settings" }),
    ).not.toBeInTheDocument();
  });

  // SNV-FR-25 / EDT-FR-33: the backend teardown is now driven from here, AFTER
  // the frontend has had its chance to write pending Editor changes — a closed
  // project has no root, so a write attempted after it would fail.
  it("OVW-FR-02, PST-FR-14, ASC-FR-14, SNV-FR-25: Close project drives the backend teardown from the frontend", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:close-project");

    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "close_project")).toBe(true),
    );
  });

  // SNV-FR-26 / EDT-FR-33: the held quit is answered once the frontend has
  // written its pending changes.
  it("SNV-FR-26: Exit releases the held quit after flushing", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:exit-requested");

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("finish_exit", { proceed: true }),
    );
  });

  // SNV-FR-28, DFV-FR-05 / SNV-FR-30: with a project freshly open — Dashboard focused,
  // nothing edited — neither Save nor Save All has anything to act on, and the
  // shell says so to the native menu. A greyed item is what makes its
  // accelerator inert, so this is also what makes ⌘S a no-op here.
  it("SNV-FR-28, DFV-FR-05/29: reports both save items greyed out while nothing is unsaved", async () => {
    render(<App />);
    await enterIde();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_save_menu_state", {
        save: false,
        saveAll: false,
      }),
    );
    // Every push so far agrees — nothing has been edited, so no call may have
    // claimed either item was available.
    const states = invokeMock.mock.calls
      .filter((c) => c[0] === "set_save_menu_state")
      .map((c) => c[1]);
    expect(states.every((s) => s && !(s as { save: boolean }).save)).toBe(true);
    expect(
      states.every((s) => s && !(s as { saveAll: boolean }).saveAll),
    ).toBe(true);
  });

  // SNV-FR-28: with nothing unsaved, neither channel writes.
  it("Save and Save All write nothing while nothing is unsaved", async () => {
    render(<App />);
    await enterIde();

    await fireMenu("menu:save");
    await fireMenu("menu:save-all");

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents"),
    ).toBe(false);
    // The shell is still mounted — neither handler threw.
    expect(
      screen.getByRole("button", { name: "Global settings" }),
    ).toBeInTheDocument();
  });

  // SNV-FR-28, SNV-FR-29 / SNV-FR-30, SNV-FR-31: the two channels differ in scope, driven end to end
  // from the Tauri event through the shell to the write. This is what catches a
  // Save ↔ Save All swap in the wiring: with only the "nothing unsaved" case
  // above, both handlers are no-ops and a swap passes unnoticed.
  it("SNV-FR-28, SNV-FR-29/28: Save writes the active tab alone; Save All writes everything", async () => {
    render(<App />);
    await enterIde();
    await openTwoDirtyArtifacts();
    stillTheRests();

    // b.md is the active tab (opened last).
    await fireMenu("menu:save");
    await waitFor(() => expect(savedIds()).toEqual(["b.md"]));

    await fireMenu("menu:save-all");
    await waitFor(() => expect(savedIds()).toEqual(["b.md", "a.md"]));
  });

  // SNV-FR-28, DFV-FR-05, second clause: ⌘S on a tab that owns nothing savable writes
  // nothing — even while another tab is sitting on unsaved changes.
  it("SNV-FR-28, DFV-FR-05: Save writes nothing from a Dashboard tab, dirty artifact or not", async () => {
    render(<App />);
    await enterIde();
    await openAndEdit("a.md", " edit");
    stillTheRests();

    await userEvent.click(screen.getByTitle("Dashboard"));
    await fireMenu("menu:save");

    expect(savedIds()).toEqual([]);
    // Save All is still on offer, though — its scope is the session, not the
    // active tab (SNV-FR-30).
    await fireMenu("menu:save-all");
    await waitFor(() => expect(savedIds()).toEqual(["a.md"]));
  });

  // SNV-FR-28 / EDT-FR-34 / EDT-FR-70: the menu Save brings the artifact's own
  // pending write forward, exactly once — and it writes whatever the buffer
  // holds, an emptied one included, with nothing standing in the way.
  it("SNV-FR-28, EDT-FR-34, EDT-FR-70: ⌘S brings the pending write forward, emptied buffer and all", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await userEvent.click(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    );
    await userEvent.clear(await screen.findByLabelText("Markdown source"));

    await fireMenu("menu:save");

    await waitFor(() => expect(savedIds()).toEqual(["a.md"]));
    expect(screen.queryByRole("dialog")).toBeNull();
    // EDT-FR-70: the schedule is spent, so the rest firing behind the Save does
    // not write the same buffer a second time.
    await letWriteLand();
    expect(savedIds()).toEqual(["a.md"]);
  });

  it("SNV-FR-27: Close project from OS full-screen leaves full-screen before showing the picker", async () => {
    // The shared main window is in native full-screen when the project closes.
    isFullscreenMock.mockResolvedValue(true);

    render(<App />);
    await enterIde();
    // Ignore any chrome calls from the startup picker mount; we assert only the
    // exit triggered by the close → picker re-show.
    setFullscreenMock.mockClear();

    await fireMenu("menu:close-project");

    // The picker reclaims the window and exits full-screen (SNV-FR-27) before it
    // is shown as a normal, fixed-size, centered window.
    await waitFor(() =>
      expect(setFullscreenMock).toHaveBeenCalledWith(false),
    );
    expect(await screen.findByTestId("picker-left")).toBeInTheDocument();

    // SNV-FR-27: and the persisted preference is left SET. Exiting
    // here is a rule about how the picker is presented, not a decision by the
    // user to stop working full-screen — if the shell's resize listener wrote
    // `false` on the way past, Close project → reopen would silently drop the
    // user out of full-screen and the feature would be dead for anyone who
    // uses Close project.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_app_preferences"),
    ).toHaveLength(0);
  });

  it("SNV-FR-27, SNV-FR-39: reopening after a full-screen Close project mounts full-screen again", async () => {
    // The user worked full-screen, so the preference is persisted set — that is
    // what must survive the trip through the picker (SNV-FR-27).
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return { theme: "dark", mainWindowFullscreen: true };
      return defaultInvoke(cmd);
    });
    isFullscreenMock.mockResolvedValue(true);

    render(<App />);
    await enterIde();
    await fireMenu("menu:close-project");
    expect(await screen.findByTestId("picker-left")).toBeInTheDocument();

    // The window is windowed now that the picker has it.
    isFullscreenMock.mockResolvedValue(false);
    setFullscreenMock.mockClear();

    // Reopen the same project from the picker.
    await userEvent.click(await screen.findByText("acme"));
    await screen.findByRole("button", { name: "Global settings" });

    await waitFor(() =>
      expect(setFullscreenMock).toHaveBeenCalledWith(true),
    );
  });
});
