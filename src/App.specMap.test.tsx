/**
 * The Map tab end to end through the shell: the Project panel's View map
 * button, the one Map tab it opens, and a Spec file clicked in the panel while
 * the map is the active tab (`SMP-specification-map.md`, `LIB-library.md`).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import { defaultInvoke, enterIde, resetAppFixture, tabLabels } from "./test/appFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
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
        workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
        scaleFactor: 1,
      },
    ],
  };
});

beforeEach(() => {
  resetPanelReveals();
  resetAppFixture();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
    defaultInvoke(cmd, args),
  );
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const projectPanel = () => document.querySelector(".vpanel") as HTMLElement;

describe("the Map tab through the shell", () => {
  it("LIB-FR-VMAQ, SMP-FR-HZRA, SMP-FR-KQTD, TAB-FR-KXMW, OVW-FR-06, OVW-FR-07: View map in the Project panel opens one Map tab", async () => {
    render(<App />);
    await enterIde();
    const viewMap = await within(projectPanel()).findByRole("button", { name: "View map" });
    // Pinned below the tree, outside its scroll region, with the layers icon.
    expect(viewMap.closest(".vpanel__body")).toBeNull();
    expect(viewMap.closest(".library__footer")).not.toBeNull();
    // The footer's one child, which its rule centres.
    expect(viewMap.closest(".library__footer")!.children).toHaveLength(1);
    expect(viewMap.querySelector("svg")).not.toBeNull();
    await userEvent.click(viewMap);
    await waitFor(() => expect(tabLabels()).toContain("Map — specifications"));
    expect(await screen.findByRole("toolbar", { name: "Map controls" })).toBeInTheDocument();
    await screen.findByTestId("spec-map-canvas");

    await userEvent.click(within(projectPanel()).getByRole("button", { name: "View map" }));
    expect(tabLabels().filter((label) => label === "Map — specifications")).toHaveLength(1);
  });

  it("LIB-FR-SJDC, LIB-FR-03, SMI-FR-PRSL: a Spec row selects its node while the Map tab is active, and opens its file otherwise", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await within(projectPanel()).findByRole("button", { name: "View map" }));
    await screen.findByTestId("spec-map-canvas");

    await userEvent.click(await within(projectPanel()).findByText("CHG-changes.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "CHG changes panel" })).toHaveAttribute("data-selected", "true"),
    );
    expect(tabLabels()).not.toContain("CHG-changes.md");
    expect(tabLabels()).toContain("Map — specifications");
    // The row becomes the panel's current selection.
    expect(within(projectPanel()).getByText("CHG-changes.md").closest("[data-selected]")).toHaveAttribute(
      "data-selected",
      "true",
    );

    await userEvent.click(within(screen.getByTestId("tabstrip")).getByText("Dashboard"));
    await userEvent.click(within(projectPanel()).getByText("CHG-changes.md"));
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
  });

  it("OVW-FR-07, SMI-FR-GAJD: Open in editor on a spec of the map opens its Editor tab", async () => {
    render(<App />);
    await enterIde();
    await userEvent.click(await within(projectPanel()).findByRole("button", { name: "View map" }));
    await screen.findByTestId("spec-map-canvas");
    await userEvent.click(await within(projectPanel()).findByText("CHG-changes.md"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "CHG changes panel" })).toHaveAttribute("data-selected", "true"),
    );
    const openInspector = screen.queryByRole("button", { name: "inspector" });
    if (openInspector) await userEvent.click(openInspector);
    await userEvent.click(
      within(screen.getByRole("complementary", { name: "Inspector" })).getByRole("button", { name: "Open in editor" }),
    );
    await waitFor(() => expect(tabLabels()).toContain("CHG-changes.md"));
  });
});
