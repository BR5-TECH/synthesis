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
import type { LayoutPreferences } from "./types";

// SNV-shell-navigation.md SNV-FR-44 – SNV-FR-49 at the App seam: the activity
// bar's two clusters driving the two panels, the toggle-off that hides them,
// and the per-project persistence of the bottom panel's active surface.

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
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
        workArea: {
          position: { x: 0, y: 0 },
          size: { width: 2560, height: 1400 },
        },
        scaleFactor: 1,
      },
    ],
  };
});

/** What `load_layout_preferences` reports, and what `save_` was handed. */
let storedLayout: LayoutPreferences | null;
let savedLayouts: LayoutPreferences[];

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
      return storedLayout;
    case "save_layout_preferences":
      savedLayouts.push(args?.preferences as LayoutPreferences);
      return undefined;
    case "load_app_preferences":
      return { theme: "dark" };
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
        ],
      };
    case "validate_flow_document":
      // FGV-FR-02 / FLO-FR-46: the backend judges a Flow body before the canvas
      // renders it. The rules themselves are covered by the Rust suite.
      return { valid: true, violations: [] };
    case "load_artifact_contents_by_id":
      return { body: "# a\n", checksum: "ck1" };
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "get_default_branch":
      return "main";
    case "list_comparison_branches":
      return [{ name: "main", isCurrent: true, isDefault: true }];
    case "list_uncommitted_changes":
      return { comparison: { kind: "uncommitted" }, entries: [] };
    case "list_in_flight_operations":
      return [];
    // DSH-FR-09 / DSH-FR-13 / DSH-FR-14: the Dashboard's widget loaders. The
    // Dashboard is the tab that opens on project open, so these decide which
    // widgets the shell renders at all (DSH-FR-04). The agent-activity run is
    // deliberately a **terminated** one: the live-indicator suite below asserts
    // that an idle window animates nothing, and a running run would make that
    // assertion fail for the right reason at the wrong time.
    case "list_recently_edited_artifacts":
      return [
        // Deliberately NOT the `a.md` the project tree above holds: a name
        // shared with a tree node would make every `findByText("a.md")` in this
        // file ambiguous between the panel and the Dashboard.
        {
          id: "specs/checkout.spec.md",
          name: "checkout.spec.md",
          kind: "markdown",
          modifiedAt: "2026-05-15T09:00:00.000Z",
        },
      ];
    case "list_active_drafts":
      return [];
    case "list_recent_agent_runs":
      return [
        {
          runId: "run-7e3",
          draftId: "d1",
          draftName: "design-review",
          state: "published",
          stage: "acceptance",
          stageCondition: "complete",
          updatedAt: "2026-05-15T08:00:00.000Z",
        },
      ];
    case "list_pending_git_activity":
      return {
        modifiedArtifacts: 3,
        modifiedSourceFiles: 1,
        unpushedCommits: 2,
        fetchableCommits: 0,
      };
    case "load_project_config":
      return { lineEndings: "lf" };
    default:
      return undefined;
  }
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) =>
      defaultInvoke(cmd, args),
  );
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  storedLayout = null;
  savedLayouts = [];
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

/**
 * A toggle in the activity bar, looked up by its accessible name. Scoped to the
 * strip because the top chrome carries its own "History" control, and an
 * unscoped query would match both.
 */
const toggle = (name: string) =>
  within(document.querySelector(".activity-bar") as HTMLElement).getByRole(
    "button",
    { name },
  );
const vpanel = () => document.querySelector(".vpanel");
const bottomPanel = () => document.querySelector(".bottom-panel");
/** The last layout record handed to the backend. */
const lastSaved = () => savedLayouts[savedLayouts.length - 1];

describe("the vertical panel's toggles (SNV-FR-45 / SNV-FR-37 / SNV-FR-45)", () => {
  it("hides the panel when the shown surface's own toggle is clicked, and restores it", async () => {
    render(<App />);
    await enterIde();
    expect(vpanel()).toBeTruthy();
    expect(toggle("Project")).toHaveAttribute("data-active", "true");

    // Toggle off: the panel goes, the splitter with it, the strip stays.
    await userEvent.click(toggle("Project"));
    await waitFor(() => expect(vpanel()).toBeNull());
    expect(screen.queryByTestId("vpanel-resizer")).toBeNull();
    expect(document.querySelector(".activity-bar")).toBeTruthy();
    // SNV-FR-45: a hidden panel marks none of its toggles.
    expect(toggle("Project")).toHaveAttribute("data-active", "false");

    // Toggle on: the same surface returns.
    await userEvent.click(toggle("Project"));
    await waitFor(() => expect(vpanel()).toBeTruthy());
    expect(toggle("Project")).toHaveAttribute("data-active", "true");
  });

  it("leaves the persisted width fraction untouched when it hides", async () => {
    // SNV-FR-37: hiding is not a resize, so nothing overwrites the fraction the
    // author dragged to — only the hidden flag is written.
    storedLayout = { verticalPanelFraction: 0.42 };
    render(<App />);
    await enterIde();

    await userEvent.click(toggle("Project"));
    await waitFor(() => expect(savedLayouts.length).toBeGreaterThan(0));
    expect(lastSaved().verticalPanelHidden).toBe(true);
    expect(lastSaved().verticalPanelFraction).toBe(0.42);
  });

  it("reopens on the clicked surface rather than the remembered one", async () => {
    // SNV-FR-45: hide the Library, then click Notes — Notes opens.
    render(<App />);
    await enterIde();
    await userEvent.click(toggle("Project"));
    await waitFor(() => expect(vpanel()).toBeNull());

    await userEvent.click(toggle("Notes"));
    await waitFor(() => expect(vpanel()).toBeTruthy());
    expect(toggle("Notes")).toHaveAttribute("data-active", "true");
    expect(toggle("Project")).toHaveAttribute("data-active", "false");
  });

  it("switches surface without hiding when a different toggle is clicked (SNV-FR-04, SNV-FR-45)", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(toggle("Changes"));
    await waitFor(() =>
      expect(toggle("Changes")).toHaveAttribute("data-active", "true"),
    );
    expect(vpanel()).toBeTruthy();
    expect(toggle("Project")).toHaveAttribute("data-active", "false");
  });
});

describe("the bottom panel's toggles (SNV-FR-46)", () => {
  it("opens, switches, and hides — and leaves the vertical panel alone", async () => {
    render(<App />);
    await enterIde();
    expect(bottomPanel()).toBeNull();

    // Hidden → open on Git.
    await userEvent.click(toggle("Git"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Git");
    expect(toggle("Git")).toHaveAttribute("data-active", "true");
    // The vertical panel is untouched by a bottom-cluster click.
    expect(vpanel()).toBeTruthy();
    expect(toggle("Project")).toHaveAttribute("data-active", "true");

    // Open on Git → switch to Runs, still open.
    await userEvent.click(toggle("Runs"));
    await waitFor(() =>
      expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent(
        "Runs",
      ),
    );
    expect(bottomPanel()).toBeTruthy();
    expect(toggle("Git")).toHaveAttribute("data-active", "false");

    // Open on Runs → the same toggle again hides it.
    await userEvent.click(toggle("Runs"));
    await waitFor(() => expect(bottomPanel()).toBeNull());
    for (const name of ["Runs", "Git"]) {
      expect(toggle(name)).toHaveAttribute("data-active", "false");
    }
  });

  it("closes from the panel's own hide control, as the toggle would (SNV-FR-47)", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(toggle("Runs"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());

    const panel = bottomPanel() as HTMLElement;
    await userEvent.click(within(panel).getByRole("button", { name: "Hide" }));
    await waitFor(() => expect(bottomPanel()).toBeNull());
    expect(toggle("Runs")).toHaveAttribute("data-active", "false");
  });

  it("marks the toggle for a surface reached from a Dashboard widget (DSH-FR-06, SNV-FR-47)", async () => {
    // SNV-FR-47: the strip always names where the panel is, however it got
    // there — DSH-FR-06 routes here without going through the strip.
    render(<App />);
    await enterIde();
    expect(bottomPanel()).toBeNull();

    // DSH-FR-14: a Pending Git count, which routes to the Git panel.
    await userEvent.click(await screen.findByText("modified artifacts"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Git");
    expect(toggle("Git")).toHaveAttribute("data-active", "true");
  });
});

describe("the bottom panel's splitter (SNV-FR-50 / SNV-FR-53)", () => {
  it("exists only while the panel is open, and the height survives a hide", async () => {
    storedLayout = { bottomPanelHeight: 420, bottomPanelHidden: false };
    render(<App />);
    await enterIde();
    await waitFor(() => expect(bottomPanel()).toBeTruthy());

    const shell = document.querySelector(".shell") as HTMLElement;
    expect(shell.style.gridTemplateRows).toContain("420px");
    expect(screen.getByTestId("bottom-panel-resizer")).toBeInTheDocument();

    // Hidden: no panel, no splitter, and the height is not rewritten.
    savedLayouts = [];
    await userEvent.click(toggle("Runs"));
    await waitFor(() => expect(bottomPanel()).toBeNull());
    expect(screen.queryByTestId("bottom-panel-resizer")).toBeNull();
    expect(lastSaved()).toMatchObject({
      bottomPanelHidden: true,
      bottomPanelHeight: 420,
    });

    // Reopened at exactly the height it had.
    await userEvent.click(toggle("Runs"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());
    expect(shell.style.gridTemplateRows).toContain("420px");
  });

  it("resizes the panel from a real pointer drag on the handle (SNV-FR-50, SNV-FR-53)", async () => {
    // Every other bottom-panel drag test calls the hook's `onResizeStart`
    // directly. This one goes through the DOM, so a wiring mistake — an
    // onMouseDown instead of onPointerDown on BottomPanelResizer, or the
    // `resizer` prop dropped from the <BottomPanel> call — is caught rather
    // than shipped.
    storedLayout = { bottomPanelHidden: false, bottomPanelHeight: 280 };
    render(<App />);
    await enterIde();
    await waitFor(() => expect(bottomPanel()).toBeTruthy());

    const shell = document.querySelector(".shell") as HTMLElement;
    // jsdom reports a zero-sized shell, so the hook falls back to
    // `window.innerHeight` and a bottom edge at 0 — stub a real box.
    const shellHeight = 1000;
    vi.spyOn(shell, "getBoundingClientRect").mockReturnValue({
      width: 1244,
      height: shellHeight,
      left: 0,
      top: 0,
      right: 1244,
      bottom: shellHeight,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect);

    const before = shell.style.gridTemplateRows;
    const resizer = screen.getByTestId("bottom-panel-resizer");

    await act(async () => {
      resizer.dispatchEvent(
        new MouseEvent("pointerdown", {
          bubbles: true,
          clientY: shellHeight - 280,
        }),
      );
    });
    // Drag upward to 400px above the shell's bottom edge.
    await act(async () => {
      window.dispatchEvent(
        new MouseEvent("pointermove", { clientY: shellHeight - 400 }),
      );
    });

    const during = shell.style.gridTemplateRows;
    expect(during).not.toBe(before);
    expect(during).toBe("44px 1fr 400px");

    savedLayouts = [];
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });
    await waitFor(() => expect(savedLayouts).toHaveLength(1));
    expect(lastSaved()).toMatchObject({ bottomPanelHeight: 400 });
  });

  it("announces itself as a horizontal separator", async () => {
    // The vertical-panel splitter is the vertical one; these must not be
    // confusable to a screen reader or to a test.
    storedLayout = { bottomPanelHidden: false };
    render(<App />);
    await enterIde();
    await waitFor(() => expect(bottomPanel()).toBeTruthy());

    const resizer = screen.getByTestId("bottom-panel-resizer");
    expect(resizer).toHaveAttribute("role", "separator");
    expect(resizer).toHaveAttribute("aria-orientation", "horizontal");
    expect(screen.getByTestId("vpanel-resizer")).toHaveAttribute(
      "aria-orientation",
      "vertical",
    );
  });
});

describe("the History toggle against the active tab (SNV-FR-48)", () => {
  it("is disabled on the Dashboard and enabled on an Editor tab (SNV-FR-48, HVW-FR-01)", async () => {
    render(<App />);
    await enterIde();

    expect(toggle("History — requires an open artifact")).toBeDisabled();
    expect(bottomPanel()).toBeNull();

    // Open an artifact: the toggle comes alive.
    await userEvent.click(await screen.findByText("a.md"));
    await waitFor(() => expect(toggle("History")).toBeEnabled());

    await userEvent.click(toggle("History"));
    await waitFor(() =>
      expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent(
        "History",
      ),
    );
  });

  it("keeps the panel on History and stays marked active when the tab changes (SNV-FR-48)", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await screen.findByText("a.md"));
    await waitFor(() => expect(toggle("History")).toBeEnabled());
    await userEvent.click(toggle("History"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());

    // Back to the Dashboard: the panel does not move out from under the user.
    const strip = screen.getByTestId("tabstrip");
    await userEvent.click(within(strip).getByText("Dashboard"));
    await waitFor(() =>
      expect(toggle("History — requires an open artifact")).toBeDisabled(),
    );
    const greyed = toggle("History — requires an open artifact");
    expect(greyed).toBeDisabled();
    expect(greyed).toHaveAttribute("data-active", "true");
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent(
      "History",
    );
  });
});

describe("the bottom panel's persisted surface (SNV-FR-08 / SNV-FR-46)", () => {
  it("writes the active surface and the open state when a toggle is used", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(toggle("Git"));
    await waitFor(() => expect(savedLayouts.length).toBeGreaterThan(0));
    expect(lastSaved()).toMatchObject({
      bottomPanelSurface: "git",
      bottomPanelHidden: false,
    });
  });

  it("restores the panel onto the surface it was left on", async () => {
    storedLayout = { bottomPanelSurface: "git", bottomPanelHidden: false };
    render(<App />);
    await enterIde();

    await waitFor(() => expect(bottomPanel()).toBeTruthy());
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Git");
    expect(toggle("Git")).toHaveAttribute("data-active", "true");
  });

  it("keeps the panel shut for a record that carries no decision about it", async () => {
    // A slot written for some unrelated field must not open the panel: only an
    // explicit `false` counts as "the author left it open".
    storedLayout = { verticalPanelFraction: 0.3, mainWindowMaximized: true };
    render(<App />);
    await enterIde();

    // Give the restore effect a chance to run before asserting on absence.
    await waitFor(() => expect(vpanel()).toBeTruthy());
    expect(bottomPanel()).toBeNull();
  });

  it("restores the panel hidden when that is what was persisted", async () => {
    storedLayout = { bottomPanelSurface: "history", bottomPanelHidden: true };
    render(<App />);
    await enterIde();

    await waitFor(() => expect(vpanel()).toBeTruthy());
    expect(bottomPanel()).toBeNull();
  });
});

describe("every activity-bar toggle carries a tooltip (SNV-FR-49)", () => {
  it("names each surface on keyboard focus", async () => {
    render(<App />);
    await enterIde();

    for (const name of ["Project", "Notes", "Changes", "Runs", "Git"]) {
      toggle(name).focus();
      const tip = await screen.findByRole("tooltip");
      expect(tip).toHaveTextContent(name);
      // SNV-FR-49: placed on the strip's inner side.
      expect(tip).toHaveAttribute("data-side", "left");
      toggle(name).blur();
      await waitFor(() => expect(screen.queryByRole("tooltip")).toBeNull());
    }
  });

  it("shows one on the disabled History toggle, stating why (SNV-FR-48, SNV-FR-49)", async () => {
    render(<App />);
    await enterIde();

    const greyed = toggle("History — requires an open artifact");
    greyed.focus();
    // A disabled button does not take focus, so hover is the route the user has.
    await userEvent.hover(greyed);
    const tip = await screen.findByRole("tooltip", {}, { timeout: 2000 });
    expect(tip).toHaveTextContent("requires an open artifact");
  });
});

describe("an idle window animates nothing (DSH-FR-07 / RUN-FR-04)", () => {
  /**
   * The regression test for the bug this suite's seam actually had: the
   * Dashboard — the tab that opens automatically on project open (DSH-FR-01)
   * and cannot be closed while it is the only one — rendered a live indicator
   * unconditionally, and the bottom panel added a second one gated on nothing
   * but which surface was showing.
   *
   * `.dot--live` carries `animation: pulse … infinite`. While it is mounted the
   * browser produces a frame every display refresh and the OS compositor
   * re-uploads the window surface to match — a core's worth of work, forever,
   * on a window nobody is touching. The component tests pin each surface; this
   * one pins the assembled shell, which is where a third surface would
   * reintroduce it unnoticed.
   */
  /**
   * Every infinite animation the app has, not just the one this change touched
   * — the test's name promises the general property and this is the one place
   * that sees the assembled shell, where a regression would otherwise hide.
   * `src/test/motion-policy.test.ts` derives the same three from the
   * stylesheets, so a fourth would be caught there and belongs here too.
   */
  const liveIndicators = () =>
    document.querySelectorAll(
      '.dot--live, .status-bar__track[data-determinate="false"], .wt-select__refresh[data-busy="true"]',
    );

  it("mounts no live indicator on the shell the user lands on", async () => {
    render(<App />);
    await enterIde();

    // The Dashboard is the tab that opened, and the bottom panel is shut.
    expect(
      await screen.findByText("Last / current agent activity"),
    ).toBeInTheDocument();
    expect(bottomPanel()).toBeNull();

    expect(liveIndicators()).toHaveLength(0);
  });

  it("still mounts none once the Runs panel is opened", async () => {
    render(<App />);
    await enterIde();

    await userEvent.click(toggle("Runs"));
    await waitFor(() => expect(bottomPanel()).toBeTruthy());
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Runs");

    // Neither the panel chrome nor the Runs surface is running anything.
    expect(liveIndicators()).toHaveLength(0);
  });
});
