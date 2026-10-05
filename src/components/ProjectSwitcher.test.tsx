import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  ProjectSwitcher,
  SWITCHER_MAX_VISIBLE_ROWS,
  SWITCHER_ROW_PX,
} from "./ProjectSwitcher";
import type { ProjectHandle, RecentProject } from "../types";

// The top-chrome project switcher (SNV-FR-17..22). It reuses the existing
// "list recent projects" (GSS-FR-05) and "open project at path" (PST-FR-01)
// operations, so the only backend surface here is `invoke`.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
// ProjectSwitcher reuses `formatRelative` from ProjectPicker, which imports the
// Tauri window + dialog plugins at module load. Stub them so jsdom never needs
// the real runtime (the switcher itself uses neither).
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
  }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));

const CURRENT_PATH = "~/dev/acme";

function recent(
  over: Partial<RecentProject> & { name: string; path: string },
): RecentProject {
  return { lastOpenedAt: "2026-06-20T10:00:00Z", ...over };
}

let recentsResult: RecentProject[];
// Either a handle to return from open_project_at_path, or an Error to throw.
let openResult: ProjectHandle | Error;

beforeEach(() => {
  recentsResult = [
    recent({ name: "acme", path: CURRENT_PATH }),
    recent({ name: "beta", path: "~/dev/beta", lastOpenedAt: "2026-06-19T10:00:00Z" }),
  ];
  openResult = {
    name: "beta",
    path: "~/dev/beta",
    activeWorktreePath: "~/dev/beta",
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_recent_projects") return recentsResult;
    if (cmd === "open_project_at_path") {
      if (openResult instanceof Error) throw openResult;
      return openResult;
    }
    return undefined;
  });
});

afterEach(cleanup);

function renderSwitcher(opts?: {
  onSwitch?: (h: ProjectHandle) => void;
  onOpenAnother?: () => void;
  onBeforeSwitch?: () => Promise<boolean>;
}) {
  const onSwitch = opts?.onSwitch ?? vi.fn();
  const onOpenAnother = opts?.onOpenAnother ?? vi.fn();
  // Nothing pending to write, unless the test says otherwise.
  const onBeforeSwitch = opts?.onBeforeSwitch ?? (async () => true);
  render(
    <ProjectSwitcher
      projectName="acme"
      projectPath={CURRENT_PATH}
      onSwitch={onSwitch}
      onBeforeSwitch={onBeforeSwitch}
      onOpenAnother={onOpenAnother}
    />,
  );
  return { onSwitch, onOpenAnother };
}

/** Open the dropdown and pick the non-current recent entry. */
async function pickBeta() {
  await userEvent.click(screen.getByTestId("project-switcher"));
  const menu = await screen.findByTestId("project-switcher-menu");
  await userEvent.click(await within(menu).findByText("beta"));
}

describe("Project switcher (SNV-FR-17..22)", () => {
  it("SNV-FR-17, SNV-FR-18, SNV-FR-20: opening the switcher lists recent projects from 'list recent projects', current flagged", async () => {
    renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));

    const menu = await screen.findByTestId("project-switcher-menu");
    await within(menu).findByText("acme");
    expect(within(menu).getByText("beta")).toBeInTheDocument();
    // The currently-open project is shown flagged as current and is
    // non-actionable to assistive tech (SNV-FR-20).
    expect(within(menu).getByText("current")).toBeInTheDocument();
    const currentRow = within(menu).getByText("acme").closest("[role='menuitem']");
    expect(currentRow).toHaveAttribute("aria-disabled", "true");
    // The list came from the shared "list recent projects" operation (SNV-FR-18).
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "list_recent_projects"),
    ).toBe(true);
  });

  it("OVW-FR-11, SNV-FR-19: selecting a non-current entry invokes 'open project at path' and hands the handle up to switch", async () => {
    const { onSwitch } = renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await userEvent.click(await within(menu).findByText("beta"));

    await waitFor(() =>
      expect(onSwitch).toHaveBeenCalledWith(openResult),
    );
    const call = invokeMock.mock.calls.find((c) => c[0] === "open_project_at_path");
    expect(call?.[1]).toEqual({ path: "~/dev/beta" });
  });

  it("SNV-FR-20: clicking the current entry does not re-open the project or trigger a switch", async () => {
    const { onSwitch } = renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await userEvent.click(await within(menu).findByText("acme"));

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "open_project_at_path"),
    ).toBe(false);
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("SNV-FR-21: with more than five entries the viewport caps at five rows, scrolls vertically only, and never horizontally", async () => {
    // None of these match CURRENT_PATH, so all are switchable rows.
    recentsResult = Array.from({ length: 8 }, (_, i) =>
      recent({ name: `project-${i}`, path: `~/dev/project-${i}` }),
    );
    renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const list = await screen.findByTestId("project-switcher-list");

    // Every entry is rendered (reachable by scrolling, not dropped from the DOM).
    await waitFor(() =>
      expect(within(list).getAllByRole("menuitem")).toHaveLength(8),
    );
    // Viewport height is capped to five rows, with vertical-only scroll and no
    // horizontal scrollbar (labels truncate via CSS instead).
    expect(list.style.maxHeight).toBe(
      `${SWITCHER_MAX_VISIBLE_ROWS * SWITCHER_ROW_PX}px`,
    );
    expect(list.style.overflowY).toBe("auto");
    expect(list.style.overflowX).toBe("hidden");
  });

  it("OVW-FR-11, SNV-FR-22: 'Open Another Project…' delegates to the picker route", async () => {
    const { onOpenAnother } = renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    await userEvent.click(
      await screen.findByTestId("project-switcher-open-another"),
    );

    expect(onOpenAnother).toHaveBeenCalledTimes(1);
  });

  it("keeps the dropdown open with an inline error and does not switch when the open fails", async () => {
    openResult = new Error("not a synthesis project");
    const { onSwitch } = renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await userEvent.click(await within(menu).findByText("beta"));

    await waitFor(() =>
      expect(within(menu).getByText(/not a synthesis project/)).toBeInTheDocument(),
    );
    expect(onSwitch).not.toHaveBeenCalled();
    // The dropdown remains mounted so the user can retry or pick another.
    expect(screen.getByTestId("project-switcher-menu")).toBeInTheDocument();
  });

  it("does not query the backend until the switcher is opened", () => {
    renderSwitcher();
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "list_recent_projects"),
    ).toBe(false);
  });

  it("SNV-FR-18: renders entries in backend order and surfaces (does not drop) pinned/missing entries", async () => {
    recentsResult = [
      recent({ name: "zeta", path: "~/dev/zeta", pinned: true }),
      recent({ name: "acme", path: CURRENT_PATH }),
      recent({ name: "gone", path: "~/dev/gone", missing: true }),
      recent({ name: "beta", path: "~/dev/beta" }),
    ];
    renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const list = await screen.findByTestId("project-switcher-list");
    await within(list).findByText("zeta");

    // Order is exactly as returned (no client-side re-sort).
    const labels = within(list)
      .getAllByRole("menuitem")
      .map((b) => b.textContent ?? "");
    expect(labels[0]).toContain("zeta");
    expect(labels[1]).toContain("acme");
    expect(labels[2]).toContain("gone");
    expect(labels[3]).toContain("beta");
    // pinned / missing flags are surfaced, not filtered away.
    expect(within(list).getByText("pinned")).toBeInTheDocument();
    expect(within(list).getByText("missing")).toBeInTheDocument();
  });

  it("SNV-FR-20: flags nothing as current when the open project is absent from the recent list", async () => {
    recentsResult = [
      recent({ name: "beta", path: "~/dev/beta" }),
      recent({ name: "gamma", path: "~/dev/gamma" }),
    ];
    renderSwitcher(); // projectPath (~/dev/acme) is not in this list

    await userEvent.click(screen.getByTestId("project-switcher"));
    const list = await screen.findByTestId("project-switcher-list");
    await within(list).findByText("beta");

    expect(within(list).queryByText("current")).not.toBeInTheDocument();
    within(list)
      .getAllByRole("menuitem")
      .forEach((row) => expect(row).not.toHaveAttribute("aria-disabled"));
  });

  it("SNV-FR-18/22: shows an empty state but still offers 'Open Another Project…' when there are no recents", async () => {
    recentsResult = [];
    renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    await screen.findByText("No recent projects.");
    expect(
      within(screen.getByTestId("project-switcher-list")).queryAllByRole(
        "menuitem",
      ),
    ).toHaveLength(0);
    // The switcher is not a dead end: the escape hatch remains usable.
    expect(
      screen.getByTestId("project-switcher-open-another"),
    ).toBeInTheDocument();
  });

  it("shows an inline error (and keeps 'Open another…' usable) when 'list recent projects' fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") throw new Error("store unreadable");
      return undefined;
    });
    renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await within(menu).findByText(/store unreadable/);
    expect(within(menu).getByText("No recent projects.")).toBeInTheDocument();
    expect(
      screen.getByTestId("project-switcher-open-another"),
    ).toBeInTheDocument();
  });

  it("SNV-FR-18: reloads the recent list each time the dropdown reopens", async () => {
    recentsResult = [recent({ name: "acme", path: CURRENT_PATH })];
    renderSwitcher();

    // First open.
    await userEvent.click(screen.getByTestId("project-switcher"));
    await within(await screen.findByTestId("project-switcher-menu")).findByText(
      "acme",
    );
    // Close.
    await userEvent.click(screen.getByTestId("project-switcher"));
    await waitFor(() =>
      expect(
        screen.queryByTestId("project-switcher-menu"),
      ).not.toBeInTheDocument(),
    );

    // Backend list changes between opens.
    recentsResult = [
      recent({ name: "acme", path: CURRENT_PATH }),
      recent({ name: "delta", path: "~/dev/delta" }),
    ];

    // Reopen reflects the new list (and refetched).
    await userEvent.click(screen.getByTestId("project-switcher"));
    await within(await screen.findByTestId("project-switcher-menu")).findByText(
      "delta",
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "list_recent_projects"),
    ).toHaveLength(2);
  });

  it("ignores a second click while a switch is already in flight (busy guard)", async () => {
    let resolveOpen!: (h: ProjectHandle) => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") return recentsResult;
      if (cmd === "open_project_at_path") {
        return new Promise<ProjectHandle>((res) => {
          resolveOpen = res;
        });
      }
      return undefined;
    });
    const { onSwitch } = renderSwitcher();

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    const beta = await within(menu).findByText("beta");
    await userEvent.click(beta); // starts the switch (busy = true)
    await userEvent.click(beta); // should be ignored while pending

    resolveOpen({
      name: "beta",
      path: "~/dev/beta",
      activeWorktreePath: "~/dev/beta",
    });

    await waitFor(() => expect(onSwitch).toHaveBeenCalledTimes(1));
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_project_at_path"),
    ).toHaveLength(1);
  });
});

// OVW-FR-11 / EDT-FR-33: the outgoing project's pending Editor changes are
// written before the switch reaches the backend — once "open project at path"
// runs, the previous project is torn down and can no longer be saved to.
describe("Project switcher flush gate (OVW-FR-11, EDT-FR-28, EDT-FR-33)", () => {
  it("writes the outgoing project's pending edits before opening the next project", async () => {
    const order: string[] = [];
    const onBeforeSwitch = vi.fn(async () => {
      order.push("flush");
      return true;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") return recentsResult;
      if (cmd === "open_project_at_path") {
        order.push("open_project_at_path");
        return {
          name: "beta",
          path: "~/dev/beta",
          activeWorktreePath: "~/dev/beta",
        };
      }
      return undefined;
    });
    const { onSwitch } = renderSwitcher({ onBeforeSwitch });

    await pickBeta();

    await waitFor(() => expect(onSwitch).toHaveBeenCalled());
    expect(order).toEqual(["flush", "open_project_at_path"]);
  });

  it("cancels the switch when a pending write is blocked", async () => {
    const onBeforeSwitch = vi.fn(async () => false);
    const { onSwitch } = renderSwitcher({ onBeforeSwitch });

    await pickBeta();

    await waitFor(() => expect(onBeforeSwitch).toHaveBeenCalled());
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "open_project_at_path"),
    ).toBe(false);
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("releases its in-flight guard after a cancelled switch, so a retry still works", async () => {
    let allow = false;
    const onBeforeSwitch = vi.fn(async () => allow);
    const { onSwitch } = renderSwitcher({ onBeforeSwitch });

    await pickBeta();
    await waitFor(() => expect(onBeforeSwitch).toHaveBeenCalledTimes(1));

    // The user resolves the blocker and picks the project again.
    allow = true;
    await pickBeta();

    await waitFor(() =>
      expect(onSwitch).toHaveBeenCalledWith(openResult),
    );
  });
});
