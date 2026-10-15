import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TopChrome } from "./Shell";
import { resetAppPreferencesCache } from "../state/appPreferences";

// SNV-FR-HMCZ: the top chrome wires its two clusters to the measurement that
// centres the search bar. jsdom does no layout, so each cluster reports a width
// from its class name, and jsdom has no ResizeObserver, so only the measurement
// on mount runs. That is enough to show the wiring.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
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

const CLUSTER_WIDTHS: Record<string, number> = {
  "top-chrome__left": 503,
  "top-chrome__right": 84,
};

beforeEach(() => {
  resetAppPreferencesCache();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "load_app_preferences") {
      return { theme: "system", mainWindowFullscreen: false };
    }
    if (cmd === "list_recent_projects") return [];
    if (cmd === "list_project_agents") return [];
    if (cmd === "list_agent_turns") return [];
    throw new Error(`unexpected command ${cmd}`);
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const width = CLUSTER_WIDTHS[this.className] ?? 0;
      return { width, height: 0, top: 0, left: 0, right: width, bottom: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
    },
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function renderChrome() {
  return render(
    <TopChrome
      projectName="acme"
      projectPath="~/dev/acme"
      onSwitchProject={() => {}}
      onBeforeSwitchProject={async () => true}
      onOpenAnother={() => {}}
      activeWorktree={null}
      onSwitchWorktree={async () => ({ ok: true })}
      onRequestMergeCommit={async () => null}
      onOpenChanges={() => {}}
      onOpenRun={() => {}}
      themePref="system"
      onSelectTheme={() => {}}
      searchOpen={false}
      setSearchOpen={() => {}}
      onOverlayOpening={() => {}}
      onOpenSearchResults={() => {}}
      onActivateSearchHit={() => {}}
      onRequestGithubToken={async () => false}
      onOpenGlobalSettings={() => {}}
      onOpenAgentInSettings={() => {}}
      onOpenProjectSettings={() => {}}
      agentsRosterOpen={false}
      setAgentsRosterOpen={() => {}}
      themeMenuOpen={false}
      setThemeMenuOpen={() => {}}
    />,
  );
}

describe("the top chrome centres its search bar (SNV-FR-HMCZ)", () => {
  it("SNV-FR-HMCZ: gives both side tracks the width of the wider cluster", async () => {
    const { container } = renderChrome();
    await screen.findByTestId("chrome-agents-control");
    const chrome = container.querySelector<HTMLElement>(".top-chrome")!;
    expect(chrome.style.getPropertyValue("--top-chrome-side")).toBe("503px");
  });

  it("SNV-FR-HMCZ: holds the leading cluster, the search bar, and the trailing cluster, in that order and nothing else", async () => {
    // The stylesheet centres the second of exactly three grid items.
    const { container } = renderChrome();
    await screen.findByTestId("chrome-agents-control");
    const items = Array.from(container.querySelector(".top-chrome")!.children);
    expect(items).toHaveLength(3);
    expect(items[0]).toHaveClass("top-chrome__left");
    expect(items[1]).toContainElement(screen.getByPlaceholderText(/Search artifacts/i));
    expect(items[2]).toHaveClass("top-chrome__right");
  });
});
