/**
 * The surfaces a project opens on, through the shell: the Library's Notes
 * action, what the Notes panel binds to, the Changes panel, and the Dashboard
 * tab with its Home affordance.
 *
 * The activity bar that reaches these panels is in `App.panels.test.tsx`.
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
import { selectorButton, selectorValue } from "./test/selectors";
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

// LCM-FR-05: the Library "Notes" context-menu action switches the vertical
// panel to Notes scoped to the chosen artifact, WITHOUT opening a tab.
describe("Library Notes action scopes the Notes panel without opening a tab (LCM-FR-05)", () => {
  function treeWithSpec() {
    return {
      id: "",
      name: "",
      path: "",
      nodeKind: "folder",
      hasArtifacts: true,
      children: [
        {
          id: "specs/spec-a.md",
          name: "spec-a.md",
          path: "specs/spec-a.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "inferred",
        },
      ],
    };
  }

  it("reveals Notes scoped to the artifact and adds no new tab", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return treeWithSpec();
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    // Right-click the artifact in the Library and choose "Notes".
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.contextMenu(await screen.findByText("spec-a.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Notes"));

    // The Notes panel is now scoped to that artifact — its selector sits in the
    // entity position naming it (NTS-FR-02 / NTS-FR-09)…
    await screen.findByRole("radiogroup", { name: "Notes scope" });
    expect(selectorValue("Notes scope")).toBe("entity");
    // Scoped to the Notes row rather than to the window: an unscoped query for
    // a radio named `spec-a.md` would be satisfied by any list in the shell.
    expect(selectorButton("Notes scope", "entity")).toHaveAttribute(
      "aria-label",
      "spec-a.md",
    );
    // …and the panel loaded that artifact's notes rather than the project's.
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_notes_for_entity", {
        entityId: "specs/spec-a.md",
      }),
    );
    // …the Notes activity-bar surface is active…
    expect(screen.getByRole("button", { name: "Notes" })).toHaveAttribute(
      "data-active",
      "true",
    );
    // …and NO tab was opened for it.
    expect(tabLabels()).not.toContain("spec-a.md");
  });
});

// NTS-FR-02 / NTS-FR-03 / NTS-FR-08: what the Notes panel's entity position
// actually binds to. The panel's own suite takes the entity as a prop, so this
// is the only place the tab -> entity derivation in `useShellSession` is
// exercised — and it is the derivation that decides whether the backend is
// asked for a real entity id or for a display label.
describe("the Notes panel binds to the active tab's entity id (NTS-FR-02)", () => {
  it("asks for the artifact's id, not its label", async () => {
    render(<App />);
    await enterIde();

    // Open a real artifact from the Library, then switch to Notes.
    await userEvent.click(await screen.findByText("CHG-changes.md"));
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_notes_for_entity", {
        // The path-derived id (ASC-FR-13). The basename alone would name no
        // note's scope, and two files can share one.
        entityId: "specifications/ui/CHG-changes.md",
      }),
    );
  });

  it("falls back to project-wide on a tab that owns no artifact", async () => {
    render(<App />);
    await enterIde();

    // The Dashboard tab is entity-bound to nothing.
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_project_notes"),
    );
    expect(selectorValue("Notes scope")).toBe("project");
    expect(selectorButton("Notes scope", "entity")).toBeDisabled();
    expect(selectorButton("Notes scope", "entity")).toHaveAttribute(
      "aria-label",
      "Current artifact",
    );
  });

  it("drops a Library-chosen scope when the active tab changes (NTS-FR-08)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree" || cmd === "rescan_project_tree")
        return treeWithSpecForNotes();
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    // Open spec-a in a tab, so there is a tab to navigate away from later.
    await userEvent.click(await screen.findByText("spec-a.md"));

    // Library -> Notes on spec-a, which scopes the panel without opening a tab
    // of its own (LCM-FR-05).
    const { fireEvent } = await import("@testing-library/react");
    // The name is now in the tree AND on the tab; the tree row is the one that
    // carries the context menu.
    const treeRow = screen
      .getAllByText("spec-a.md")
      .find((el) => el.closest(".tree-row"))!;
    fireEvent.contextMenu(treeRow);
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Notes"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_notes_for_entity", {
        entityId: "specs/spec-a.md",
      }),
    );

    // Activating another tab drops the override, so the panel rescopes rather
    // than sitting on an artifact the user has navigated away from.
    invokeMock.mockClear();
    await userEvent.click(screen.getByText("Dashboard"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_project_notes"),
    );
  });
});

function treeWithSpecForNotes() {
  return {
    id: "",
    name: "",
    path: "",
    nodeKind: "folder",
    hasArtifacts: true,
    children: [
      {
        id: "specs/spec-a.md",
        name: "spec-a.md",
        path: "specs/spec-a.md",
        nodeKind: "file",
        artifactType: "spec",
        typeSource: "inferred",
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Changes panel (CHG-FR-01 / CHG-FR-13, CHG-FR-18 / CHG-FR-20)
// ---------------------------------------------------------------------------

describe("Changes vertical panel (CHG-FR-01)", () => {
  it("is reachable from the activity bar, and is not the surface a project opens on", async () => {
    render(<App />);
    await enterIde();

    // OVW-FR-05: the Project panel is the initially active vertical-panel surface.
    expect(screen.getByText("Project")).toBeInTheDocument();
    expect(screen.queryByLabelText("Refresh changes")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Changes" }));

    expect(await screen.findByLabelText("Refresh changes")).toBeInTheDocument();
    // The panel replaced the previously active surface rather than stacking.
    expect(screen.queryByLabelText("Rescan project tree")).not.toBeInTheDocument();
  });

  /**
   * LIB-FR-06 against the real `VPanel`, not a stand-in. `VPanel` renders the
   * three surfaces from one branch, so choosing Changes unmounts the Library —
   * the expand state has to live above that branch to survive. `Library.test.tsx`
   * models this with its own host component, which can only prove the design as
   * the test author wrote it; this pins the actual wiring, so moving the hook
   * inside the `library` branch fails here even though it still type-checks.
   */
  it("keeps the Library's expanded folders across a surface switch (LIB-FR-06)", async () => {
    render(<App />);
    await enterIde();

    // The fixture's persisted state lists `specifications/ui` as expanded, so
    // its child is visible under the default lens.
    const panel = () => document.querySelector(".vpanel") as HTMLElement;
    expect(await within(panel()).findByText("CHG-changes.md")).toBeInTheDocument();

    // Collapse it, so what we check for is a state the user chose rather than
    // whatever a fresh restore would produce.
    await userEvent.click(within(panel()).getByText("ui"));
    expect(within(panel()).queryByText("CHG-changes.md")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Changes" }));
    expect(await screen.findByLabelText("Refresh changes")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Project" }));

    // Back on the Library: `ui` is still collapsed. If the state lived inside
    // `Library` it would have been destroyed and re-restored as expanded.
    expect(await screen.findByLabelText("Rescan project tree")).toBeInTheDocument();
    expect(within(panel()).getByText("ui")).toBeInTheDocument();
    expect(within(panel()).queryByText("CHG-changes.md")).not.toBeInTheDocument();
  });

  it("opens a Diff tab in the main viewport when a changed file is clicked (CHG-FR-13, CHG-FR-18)", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(screen.getByRole("button", { name: "Changes" }));

    await userEvent.click(await screen.findByText("CHG-changes.md"));

    // The diff renders in the viewport, derived from the original the tab
    // fetched and the target it holds an editing session on.
    expect(await screen.findByText("a new line")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "get_file_revisions"),
    ).toHaveLength(1);
  });

  it("coexists with an Editor tab on the same artifact and greys out Save (CHG-FR-20)", async () => {
    render(<App />);
    await enterIde();

    // Open the artifact in an Editor tab from the Library first. The Library
    // opens on the "All artifacts" lens, and the fixture file is a classified
    // Spec, so it is visible without touching the filter. Queries are scoped to
    // the vertical panel: once a tab is open, its label matches the row's text
    // too.
    const panel = () => within(document.querySelector(".vpanel") as HTMLElement);
    await userEvent.click(await panel().findByText("CHG-changes.md"));
    const tabLabels = () =>
      Array.from(document.querySelectorAll(".tabstrip .tab__label")).map(
        (n) => n.textContent,
      );
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
    const tabsAfterEditor = tabLabels().length;

    // Now open the same artifact's diff from the Changes panel.
    await userEvent.click(screen.getByRole("button", { name: "Changes" }));
    await userEvent.click(await panel().findByText("CHG-changes.md"));
    await screen.findByText("a new line");

    // TAB-FR-06 / DFV-FR-05: the Diff tab was ADDED, not swapped in for the
    // Editor tab — the single-tab-per-artifact rule does not apply to it.
    await waitFor(() => expect(tabLabels()).toHaveLength(tabsAfterEditor + 1));
    // DFV-FR-02: both tabs are on the same artifact, and the `Diff:` prefix is
    // what tells them apart at a glance in the strip.
    expect(tabLabels()).toContain("CHG-changes.md");
    expect(tabLabels()).toContain("Diff: CHG-changes.md");

    // SNV-FR-28 / DFV-FR-05 is asserted where it can actually fail — with a
    // *dirty* target, so the enablement is observable at all. That lives in
    // `useShellSession.test.tsx` ("enables Save over a Diff tab whose target is
    // dirty"), which drives the store directly; asserting it here, over a clean
    // target, would hold whether or not the tab were treated as savable.
  });
});

// SNV-FR-09 / SNV-FR-41 / TAB-FR-08: the Dashboard is represented in the tab
// strip exactly once — as its tab, or, while that tab is closed, as the Home
// affordance standing in its place at the head of the strip.
describe("Dashboard tab and Home affordance (SNV-FR-09 / SNV-FR-41 / TAB-FR-08)", () => {
  // A structural read of every entry in the strip, not just its title. The
  // affordance is a control rather than a tab (SNV-FR-09), so a regression that
  // gave it a label, a close control or an active state — i.e. turned it back
  // into a second Dashboard tab — has to be visible here; matching on the title
  // alone would let that through.
  // TAB-FR-29: the Home affordance sits *outside* the scrolling region, at the
  // strip's head, so it stays reachable at every scroll position. Reading both
  // levels here keeps this a structural read of the whole strip in order —
  // affordance first, then the tabs — rather than of one nesting level.
  const stripEntries = () => {
    const root = screen.getByTestId("tabstrip");
    const scroll = screen.getByTestId("tabstrip-scroll");
    return Array.from(root.children).flatMap((el) =>
      el === scroll ? Array.from(el.children) : [el],
    );
  };
  const strip = () =>
    stripEntries().map((el) => ({
      title: el.getAttribute("title"),
      label: el.querySelector(".tab__label")?.textContent ?? null,
      closable: !!el.querySelector(".tab__close"),
      // TAB-FR-16: an inert close control keeps the `.tab__close` element, so
      // `closable` alone cannot see enablement. Reading the modifier here makes
      // every strip assertion below carry the enablement contract too, rather
      // than leaving it to the two tests that check `aria-disabled` directly.
      closeDisabled: !!el.querySelector(".tab__close--disabled"),
      active: el.getAttribute("data-active"),
    }));
  const HOME = {
    title: "Home — open Dashboard",
    label: null,
    closable: false,
    closeDisabled: false,
    active: null,
  };
  /** The Dashboard tab. Its close control is inert only when it stands alone. */
  const dashboardTab = (active: boolean, alone = false) => ({
    title: "Dashboard",
    label: "Dashboard",
    closable: true,
    closeDisabled: alone,
    active: String(active),
  });
  const editorTab = (active: boolean) => ({
    title: "CHG-changes.md",
    label: "CHG-changes.md",
    closable: true,
    closeDisabled: false,
    active: String(active),
  });

  // The Editor tab SNV-FR-09, SNV-FR-41 and TAB-FR-08 are written against — the one tab
  // type whose close path can write (TAB-FR-10), so closing the Dashboard
  // beside it exercises more than a Settings tab would. The fixture artifact is
  // a classified Spec, visible in the Library's default lens.
  const openEditorTab = async () => {
    const panel = within(document.querySelector(".vpanel") as HTMLElement);
    await userEvent.click(await panel.findByText("CHG-changes.md"));
    await waitFor(() =>
      expect(strip().some((t) => t.label === "CHG-changes.md")).toBe(true),
    );
  };

  // SNV-FR-09.
  it("shows the Dashboard once, with no Home affordance beside it, while its tab is open", async () => {
    render(<App />);
    await enterIde();

    // Exactly one entry, and it is the tab — carrying its label and its own
    // close control (TAB-FR-08), with no icon-only twin at the head of the strip.
    expect(strip()).toEqual([dashboardTab(true, true)]);
  });

  // TAB-FR-08, SNV-FR-09.
  it("closes the Dashboard tab, leaves other tabs open, and puts the Home affordance in its place", async () => {
    render(<App />);
    await enterIde();
    await openEditorTab();

    await userEvent.click(screen.getByTestId("close-dashboard"));

    // The Dashboard tab is gone and the affordance stands in its place at the
    // head of the strip; the Editor tab is untouched, still focused, and still
    // rendering its artifact.
    expect(strip()).toEqual([HOME, editorTab(true)]);
    expect(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
    // Closing a tab that owns no savable content writes nothing.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents"),
    ).toEqual([]);
  });

  // DSH-FR-12 / NAW-FR-03 / TAB-FR-17: the Active workstreams route, both halves.
  it("opens a draft from Active workstreams once, and refocuses it the second time", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_active_drafts")
        return [
          {
            draftId: "d1",
            name: "checkout-v2",
            status: "active",
            activityAt: "2026-05-15T12:00:00.000Z",
          },
        ];
      if (cmd === "open_draft")
        return {
          id: "d1",
          name: "checkout-v2",
          promptPath: "checkout-v2.md",
          status: "active",
          createdAt: "2026-05-15T09:00:00Z",
          updatedAt: "2026-05-15T12:00:00Z",
        };
      return defaultInvoke(cmd, args);
    });
    render(<App />);
    await enterIde();

    await screen.findByText("Active workstreams");
    await userEvent.click(
      screen
        .getByText("Active workstreams")
        .closest(".dash-widget")!
        .querySelector(".dash-row__name") as HTMLElement,
    );

    // A New Artifact tab on that draft, focused, beside the Dashboard.
    await waitFor(() =>
      expect(strip().map((t) => t.label)).toEqual(["Dashboard", "checkout-v2"]),
    );
    expect(strip()[1].active).toBe("true");
    // DSH-FR-12: it opens no Project panel filter and no other surface — and
    // NAW-FR-03: it opens the draft named rather than making one.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "create_draft"),
    ).toEqual([]);

    // Focus something else, then activate the same row again. Queried inside
    // the Dashboard: the tab strip now carries the draft's name too, so a
    // whole-document lookup would be ambiguous between the row and its tab.
    await userEvent.click(screen.getByTitle("Dashboard"));
    await waitFor(() => expect(strip()[0].active).toBe("true"));
    const widgetRow = () =>
      screen
        .getByText("Active workstreams")
        .closest(".dash-widget")!
        .querySelector(".dash-row__name") as HTMLElement;
    await waitFor(() => expect(widgetRow()).toHaveTextContent("checkout-v2"));
    await userEvent.click(widgetRow());

    // Focus jumps to the tab already showing it; no second tab is created.
    await waitFor(() => expect(strip()[1].active).toBe("true"));
    expect(strip().map((t) => t.label)).toEqual(["Dashboard", "checkout-v2"]);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "create_draft"),
    ).toEqual([]);
  });

  // SNV-FR-09, SNV-FR-41 / TAB-FR-08.
  it("reopens the Dashboard in the leading position, focused, and drops the affordance", async () => {
    render(<App />);
    await enterIde();
    await openEditorTab();
    await userEvent.click(screen.getByTestId("close-dashboard"));

    await userEvent.click(screen.getByTestId("home-affordance"));

    // Leading position — ahead of the tab that was already open — focused, and
    // the affordance is gone rather than sitting beside its own tab.
    expect(strip()).toEqual([dashboardTab(true), editorTab(false)]);
    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
  });

  // TAB-FR-16, TAB-FR-08 / DSH-FR-02: the Dashboard alone in the strip carries a greyed-out
  // close control, so the one route to an empty strip is shut. Clicking it
  // closes nothing rather than closing-and-reopening.
  it("greys out the Dashboard's close control while it is the only open tab", async () => {
    render(<App />);
    await enterIde();

    const close = screen.getByTestId("close-dashboard");
    expect(close).toHaveAttribute("aria-disabled", "true");
    expect(close).toHaveAttribute(
      "title",
      "Dashboard stays open while it is the only tab",
    );

    await userEvent.click(close);

    expect(strip()).toEqual([dashboardTab(true, true)]);
    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
  });

  // The other half of TAB-FR-16, TAB-FR-08: opening a second tab makes the control live
  // again, and closing the Dashboard then leaves that tab alone in the strip.
  it("re-enables the Dashboard's close control once another tab is open", async () => {
    render(<App />);
    await enterIde();
    await openEditorTab();

    const close = screen.getByTestId("close-dashboard");
    expect(close).not.toHaveAttribute("aria-disabled");
    expect(close).toHaveAttribute("title", "Close Dashboard");

    await userEvent.click(close);
    expect(strip()).toEqual([HOME, editorTab(true)]);
  });

  // The transition the other two tests miss between them: the Dashboard's
  // control is live beside another tab and goes inert again when that tab
  // closes, so enablement tracks the strip rather than being decided once.
  it("returns the Dashboard's close control to inert when the strip shrinks back to it", async () => {
    render(<App />);
    await enterIde();
    await openEditorTab();
    expect(strip()).toEqual([dashboardTab(false), editorTab(true)]);

    await userEvent.click(
      screen.getByTestId("close-art:specifications/ui/CHG-changes.md"),
    );

    await waitFor(() => expect(strip()).toEqual([dashboardTab(true, true)]));
    expect(screen.getByTestId("close-dashboard")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  // TAB-FR-15, SNV-FR-09 / DSH-FR-10: with the Dashboard closed and one Editor tab left,
  // closing that tab brings the Dashboard back in the leading position, focused
  // and rendering — rather than leaving an empty strip behind the affordance.
  it("reopens the Dashboard focused when the last remaining tab closes", async () => {
    render(<App />);
    await enterIde();
    await openEditorTab();
    await userEvent.click(screen.getByTestId("close-dashboard"));
    expect(strip()).toEqual([HOME, editorTab(true)]);

    await userEvent.click(
      screen.getByTestId("close-art:specifications/ui/CHG-changes.md"),
    );

    // The affordance is gone because the tab it stands in for is back, and the
    // viewport is rendering it rather than nothing.
    await waitFor(() => expect(strip()).toEqual([dashboardTab(true, true)]));
    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
  });
});
