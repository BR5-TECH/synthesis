import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { GlobalSettings, resolveTheme } from "./GlobalSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  resetSettingsSections,
  runSettingsSaveSweep,
  settingsSectionsPending,
} from "../state/settingsSweep";
import type {
  AppPreferences,
  FontFamily,
  InstalledAdapter,
  InstalledPlugin,
  RecentProject,
} from "../types";

const invokeMock = vi.fn();
const promptMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// `formatRelative` (imported transitively via ProjectPicker) pulls in
// @tauri-apps/api/window through that module; mock it so the import resolves
// under jsdom.
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: async () => {},
      setMaximizable: async () => {},
      setSize: async () => {},
      isMaximized: async () => false,
      unmaximize: async () => {},
    }),
  };
});
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));

interface Backend {
  prefs?: AppPreferences;
  recents?: RecentProject[];
  plugins?: InstalledPlugin[];
  adapters?: InstalledAdapter[];
  /** FNT-FR-02: the installed families the Appearance controls offer. */
  fonts?: FontFamily[];
  overrides?: Record<string, (args?: Record<string, unknown>) => unknown>;
}

function wireBackend(b: Backend) {
  const state: Backend = {
    prefs: b.prefs ?? { theme: "system" },
    recents: b.recents ?? [],
    plugins: b.plugins ?? [],
    adapters: b.adapters ?? [],
    fonts: b.fonts ?? [],
  };
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (b.overrides?.[cmd]) return b.overrides[cmd](args);
      switch (cmd) {
        case "load_app_preferences":
          return state.prefs;
        case "save_app_preferences":
          // GSS-FR-20 is a whole-record write, so the stub keeps what it was
          // sent — a test that then re-reads sees what really landed rather
          // than what it hoped would.
          state.prefs = (args?.preferences as AppPreferences) ?? state.prefs;
          return undefined;
        case "list_system_fonts":
          return state.fonts;
        case "list_recent_projects":
          return state.recents;
        case "remove_recent_project":
        case "clear_recent_projects":
        case "pin_recent_project":
        case "unpin_recent_project":
          return undefined;
        case "list_installed_plugins":
          return state.plugins;
        case "install_plugin":
        case "uninstall_plugin":
          return { id: "x", name: "x", source: "s" };
        case "list_agent_adapters":
          return state.adapters;
        case "install_adapter":
          return { id: "x", name: "x", source: "s" };
        default:
          return undefined;
      }
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  // The app-preferences record is cached at module scope (GSS-FR-20); reset it
  // so one test's persisted theme is not served to the next.
  resetAppPreferencesCache();
  promptMock.mockReset();
  vi.stubGlobal("prompt", promptMock);
  // Deterministic matchMedia for resolveTheme("system").
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockReturnValue({ matches: true }) as unknown,
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function calls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

describe("resolveTheme", () => {
  it("returns the literal value for light/dark", () => {
    expect(resolveTheme("light")).toBe("light");
    expect(resolveTheme("dark")).toBe("dark");
  });
  it("resolves system via matchMedia prefers-color-scheme", () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockReturnValue({ matches: true }) as unknown,
    );
    expect(resolveTheme("system")).toBe("dark");
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockReturnValue({ matches: false }) as unknown,
    );
    expect(resolveTheme("system")).toBe("light");
  });
  it("falls back to light for 'system' when matchMedia is unavailable", () => {
    vi.stubGlobal("matchMedia", undefined as unknown);
    expect(resolveTheme("system")).toBe("light");
  });
});

describe("Appearance section (GLS-FR-04, GLS-FR-05)", () => {
  it("reflects load_app_preferences on mount and reports it for application", async () => {
    const onThemeChange = vi.fn();
    wireBackend({ prefs: { theme: "light" } });

    render(<GlobalSettings onThemeChange={onThemeChange} />);

    await waitFor(() =>
      expect(calls("load_app_preferences").length).toBe(1),
    );
    // Selector reflects persisted value.
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Light" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    // Mount reports the persisted preference so the app applies it to the root.
    await waitFor(() => expect(onThemeChange).toHaveBeenCalledWith("light"));
  });

  it("on selecting dark, invokes save_app_preferences and applies dark immediately", async () => {
    const onThemeChange = vi.fn();
    wireBackend({ prefs: { theme: "light" } });

    render(<GlobalSettings onThemeChange={onThemeChange} />);
    await waitFor(() => expect(onThemeChange).toHaveBeenCalledWith("light"));
    onThemeChange.mockClear();

    await userEvent.click(screen.getByRole("radio", { name: "Dark" }));

    // Applied to root immediately (GLS-FR-05) via the reported preference.
    expect(onThemeChange).toHaveBeenCalledWith("dark");
    // Persisted with the documented payload shape. GLS-FR-14 / GSS-FR-20: the
    // write is the WHOLE record, not a `{ theme }` partial — the record also
    // carries the main window's full-screen state, which this section must not
    // clear on its way past.
    await waitFor(() => expect(calls("save_app_preferences").length).toBe(1));
    expect(calls("save_app_preferences")[0][1]).toEqual({
      preferences: {
        theme: "dark",
        mainWindowFullscreen: false,
        searchQueryMode: "literal_insensitive",
        diffVisualizationMode: "unified",
        diffRenderingMode: "source",
        changesCommitAction: "commit",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
      },
    });
  });

  it("carries the full-screen flag AND the query mode through a theme save (GLS-FR-14, GSS-FR-20)", async () => {
    // The clobber this guards against is silent: the theme applies correctly,
    // and the user only discovers full-screen and their query mode were
    // forgotten on relaunch.
    wireBackend({
      prefs: {
        theme: "light",
        mainWindowFullscreen: true,
        searchQueryMode: "regex",
        diffVisualizationMode: "side_by_side",
        diffRenderingMode: "rich",
        changesCommitAction: "commit",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
        // GLS-FR-14, GSS-FR-20: all three roles' font settings ride the same record, and
        // a theme save must carry them through unchanged too.
        fonts: {
          ui: { family: "Helvetica Neue", sizePx: 12, lineHeight: 1.4 },
          rich: { family: "Georgia", sizePx: 17, lineHeight: 1.8 },
          source: { family: "Fira Code", sizePx: 14, lineHeight: 1.6 },
        },
      },
    });

    render(<GlobalSettings />);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Light" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    await userEvent.click(screen.getByRole("radio", { name: "Dark" }));

    await waitFor(() => expect(calls("save_app_preferences").length).toBe(1));
    expect(calls("save_app_preferences")[0][1]).toEqual({
      preferences: {
        theme: "dark",
        mainWindowFullscreen: true,
        searchQueryMode: "regex",
        // GLS-FR-14 / DFV-FR-23: the Diff tab's two modes ride the same record
        // and this section presents no control for either, so a theme save
        // carries them through untouched.
        diffVisualizationMode: "side_by_side",
        diffRenderingMode: "rich",
        changesCommitAction: "commit",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
        // GSS-FR-20 / GLS-FR-14: and all three roles, for the same reason —
        // the theme and the fonts share this record, and a theme save that
        // dropped the fonts would be discovered only on relaunch.
        fonts: {
          ui: { family: "Helvetica Neue", sizePx: 12, lineHeight: 1.4 },
          rich: { family: "Georgia", sizePx: 17, lineHeight: 1.8 },
          source: { family: "Fira Code", sizePx: 14, lineHeight: 1.6 },
        },
      },
    });
  });

  it("offers no control for full-screen presentation or the search query mode (GLS-FR-14)", async () => {
    // GLS-FR-14: full-screen is ambient window state the shell owns
    // (SNV-FR-38) and the query mode belongs to the search input that presents
    // it (SCH-FR-13); neither is a setting this tab exposes.
    wireBackend({
      prefs: {
        theme: "light",
        mainWindowFullscreen: true,
        searchQueryMode: "regex",
        diffVisualizationMode: "side_by_side",
        diffRenderingMode: "rich",
        changesCommitAction: "commit",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
      },
    });
    render(<GlobalSettings />);
    await waitFor(() =>
      expect(calls("load_app_preferences").length).toBe(1),
    );

    // GLS-FR-14 says "every section", so the loop has to be every section —
    // including the two AI ones this tab gained, which is exactly where a stray
    // control would be easiest to add without noticing.
    for (const section of [
      "Appearance",
      "Recent projects",
      "Installed plugins",
      "Installed adapters",
      "GitHub",
      "AI API",
      "Agentic AI",
    ]) {
      await userEvent.click(screen.getByRole("tab", { name: section }));
      expect(screen.queryByText(/full.?screen/i)).toBeNull();
      expect(screen.queryByText(/query mode/i)).toBeNull();
      expect(screen.queryByText(/regular expression/i)).toBeNull();
      expect(screen.queryByText(/smart.?case/i)).toBeNull();
    }
  });

  it("reports the raw 'system' preference (not a resolved value) on selection", async () => {
    // GLS-FR-05 + OVW-FR-10: the app must receive the preference verbatim so
    // it can track the OS scheme; this surface must not collapse "system" to a
    // resolved light/dark before reporting.
    const onThemeChange = vi.fn();
    wireBackend({ prefs: { theme: "light" } });

    render(<GlobalSettings onThemeChange={onThemeChange} />);
    await waitFor(() => expect(onThemeChange).toHaveBeenCalledWith("light"));
    onThemeChange.mockClear();

    await userEvent.click(screen.getByRole("radio", { name: "System" }));

    expect(onThemeChange).toHaveBeenCalledWith("system");
    await waitFor(() =>
      expect(calls("save_app_preferences")[0][1]).toEqual({
        preferences: {
          theme: "system",
          mainWindowFullscreen: false,
          searchQueryMode: "literal_insensitive",
          diffVisualizationMode: "unified",
          diffRenderingMode: "source",
          changesCommitAction: "commit",
          notificationsEnabled: true,
          selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
        },
      }),
    );
  });

  it("GLS-FR-13, GLS-FR-10: a stranded selection is what the window's close sweep writes", async () => {
    // GLS-FR-10 / SWN-FR-08: a section holding a dirty state is a pending change
    // the window writes before it closes. Asserted through the registry the
    // sweep actually spends rather than through a reported flag — the flag was
    // the tab model's confirm-on-close driver and no longer governs anything.
    let failing = true;
    wireBackend({
      prefs: { theme: "light" },
      overrides: {
        save_app_preferences: () => {
          if (failing) throw "disk full";
          return undefined;
        },
      },
    });

    render(<GlobalSettings />);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Light" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(settingsSectionsPending()).toEqual([]);

    await userEvent.click(screen.getByRole("radio", { name: "Dark" }));

    // The save failed, so the selection is applied but unpersisted — and the
    // window's sweep now has something to write.
    expect(await screen.findByText(/disk full/)).toBeInTheDocument();
    await waitFor(() =>
      expect(settingsSectionsPending()).toEqual(["appearance"]),
    );

    // SWN-FR-11: while the write still fails, the sweep reports this section and
    // the author's selection is kept exactly as it was.
    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "appearance",
    });
    expect(screen.getByRole("radio", { name: "Dark" })).toHaveAttribute(
      "aria-checked",
      "true",
    );

    // …and once the write can land, the sweep writes it and the section is
    // clean, so a second close does not write it again.
    failing = false;
    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });
    await waitFor(() => expect(settingsSectionsPending()).toEqual([]));
    const writes = calls("save_app_preferences");
    expect(writes[writes.length - 1][1]).toMatchObject({
      preferences: { theme: "dark" },
    });
  });

  it("contributes nothing to the sweep once the save has landed", async () => {
    wireBackend({ prefs: { theme: "light" } });

    render(<GlobalSettings />);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Light" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    await userEvent.click(screen.getByRole("radio", { name: "Dark" }));
    await waitFor(() => expect(calls("save_app_preferences").length).toBe(1));

    // GLS-FR-05: the theme persists on selection, so the window closes with
    // nothing left to write and no second write is made.
    await waitFor(() => expect(settingsSectionsPending()).toEqual([]));
    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });
    expect(calls("save_app_preferences").length).toBe(1);
  });
});

describe("Recent projects section (GLS-FR-06..06)", () => {
  const RECENTS: RecentProject[] = [
    {
      name: "pinned-one",
      path: "~/dev/pinned-one",
      lastOpenedAt: "2026-01-01T00:00:00Z",
      pinned: true,
      missing: false,
    },
    {
      name: "fresh-two",
      path: "~/dev/fresh-two",
      lastOpenedAt: "2026-06-01T00:00:00Z",
      pinned: false,
      missing: false,
    },
  ];

  async function gotoRecents() {
    await userEvent.click(screen.getByText("Recent projects"));
  }

  it("renders entries in the order returned, pinned first (GLS-FR-06)", async () => {
    wireBackend({ recents: RECENTS });
    render(<GlobalSettings />);
    await gotoRecents();

    const rows = await screen.findAllByTestId("recent-row");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("pinned-one")).toBeInTheDocument();
    expect(within(rows[0]).getByText("pinned")).toBeInTheDocument();
    expect(within(rows[1]).getByText("fresh-two")).toBeInTheDocument();
  });

  it("remove invokes remove_recent_project and refreshes (GLS-FR-07)", async () => {
    // First list has both; after removal the reload returns only the survivor.
    let listing = [...RECENTS];
    wireBackend({
      recents: RECENTS,
      overrides: {
        list_recent_projects: () => listing,
        remove_recent_project: (args) => {
          listing = listing.filter((r) => r.path !== args?.path);
          return undefined;
        },
      },
    });
    render(<GlobalSettings />);
    await gotoRecents();
    await screen.findAllByTestId("recent-row");

    await userEvent.click(
      screen.getByRole("button", { name: "Remove fresh-two" }),
    );

    expect(calls("remove_recent_project")[0][1]).toEqual({
      path: "~/dev/fresh-two",
    });
    await waitFor(() =>
      expect(screen.queryByText("fresh-two")).not.toBeInTheDocument(),
    );
  });

  it("clear invokes clear_recent_projects and empties the list (GLS-FR-07)", async () => {
    let listing = [...RECENTS];
    wireBackend({
      recents: RECENTS,
      overrides: {
        list_recent_projects: () => listing,
        clear_recent_projects: () => {
          listing = [];
          return undefined;
        },
      },
    });
    render(<GlobalSettings />);
    await gotoRecents();
    await screen.findAllByTestId("recent-row");

    await userEvent.click(screen.getByRole("button", { name: "Clear all" }));

    expect(calls("clear_recent_projects").length).toBe(1);
    await waitFor(() =>
      expect(screen.getByText(/No recent projects/)).toBeInTheDocument(),
    );
  });

  it("pin invokes pin_recent_project; unpin invokes unpin_recent_project (GLS-FR-06, GLS-FR-07)", async () => {
    wireBackend({ recents: RECENTS });
    render(<GlobalSettings />);
    await gotoRecents();
    const rows = await screen.findAllByTestId("recent-row");

    // Row 0 is pinned -> button says Unpin; row 1 is unpinned -> Pin.
    await userEvent.click(within(rows[1]).getByRole("button", { name: "Pin" }));
    expect(calls("pin_recent_project")[0][1]).toEqual({
      path: "~/dev/fresh-two",
    });

    await userEvent.click(
      within(rows[0]).getByRole("button", { name: "Unpin" }),
    );
    expect(calls("unpin_recent_project")[0][1]).toEqual({
      path: "~/dev/pinned-one",
    });
  });

  it("shows a first-class empty state (GLS-FR-12)", async () => {
    wireBackend({ recents: [] });
    render(<GlobalSettings />);
    await gotoRecents();
    expect(await screen.findByText(/No recent projects/)).toBeInTheDocument();
  });

  it("distinguishes loading (null) from empty ([]) — shows Loading… first", async () => {
    // Keep list_recent_projects pending so recents stays `null`.
    let resolveList: (v: RecentProject[]) => void = () => {};
    const pending = new Promise<RecentProject[]>((r) => {
      resolveList = r;
    });
    wireBackend({ overrides: { list_recent_projects: () => pending } });

    render(<GlobalSettings />);
    await gotoRecents();

    // Before the list resolves: Loading… (NOT the empty card).
    expect(screen.getByText(/Loading…/)).toBeInTheDocument();
    expect(screen.queryByText(/No recent projects/)).not.toBeInTheDocument();

    resolveList([]);
    expect(await screen.findByText(/No recent projects/)).toBeInTheDocument();
    expect(screen.queryByText(/Loading…/)).not.toBeInTheDocument();
  });
});

describe("Installed plugins section (GLS-FR-08)", () => {
  async function gotoPlugins() {
    await userEvent.click(screen.getByText("Installed plugins"));
  }

  it("lists plugins, installs via install_plugin, uninstalls via uninstall_plugin", async () => {
    const PLUGINS: InstalledPlugin[] = [
      { id: "git-pr", name: "git-pr", source: "https://x/git-pr" },
    ];
    wireBackend({ plugins: PLUGINS });
    render(<GlobalSettings />);
    await gotoPlugins();

    expect(await screen.findByTestId("plugin-row")).toBeInTheDocument();
    expect(screen.getByText("git-pr")).toBeInTheDocument();

    // Install: prompt returns a source -> install_plugin invoked with it.
    promptMock.mockReturnValue("https://x/markdown-toolbar");
    await userEvent.click(
      screen.getByRole("button", { name: /Install plugin/ }),
    );
    expect(calls("install_plugin")[0][1]).toEqual({
      source: "https://x/markdown-toolbar",
    });

    // Uninstall the existing plugin.
    await userEvent.click(
      screen.getByRole("button", { name: "Uninstall git-pr" }),
    );
    expect(calls("uninstall_plugin")[0][1]).toEqual({ id: "git-pr" });
  });

  it("does not invoke install_plugin when the prompt is cancelled", async () => {
    wireBackend({ plugins: [] });
    render(<GlobalSettings />);
    await gotoPlugins();
    await screen.findByText(/No plugins installed/);

    promptMock.mockReturnValue(null);
    await userEvent.click(
      screen.getByRole("button", { name: /Install plugin/ }),
    );
    expect(calls("install_plugin").length).toBe(0);
  });

  it("shows a first-class empty state (GLS-FR-12)", async () => {
    wireBackend({ plugins: [] });
    render(<GlobalSettings />);
    await gotoPlugins();
    expect(await screen.findByText(/No plugins installed/)).toBeInTheDocument();
  });
});

describe("Installed adapters section (GLS-FR-09)", () => {
  async function gotoAdapters() {
    await userEvent.click(screen.getByText("Installed adapters"));
  }

  it("lists adapters and installs via install_adapter", async () => {
    const ADAPTERS: InstalledAdapter[] = [
      { id: "claude-code", name: "claude-code", source: "https://x/cc" },
    ];
    wireBackend({ adapters: ADAPTERS });
    render(<GlobalSettings />);
    await gotoAdapters();

    expect(await screen.findByTestId("adapter-row")).toBeInTheDocument();

    promptMock.mockReturnValue("https://x/codex");
    await userEvent.click(
      screen.getByRole("button", { name: /Install adapter/ }),
    );
    expect(calls("install_adapter")[0][1]).toEqual({
      source: "https://x/codex",
    });
  });

  it("shows a first-class empty state (GLS-FR-12)", async () => {
    wireBackend({ adapters: [] });
    render(<GlobalSettings />);
    await gotoAdapters();
    expect(
      await screen.findByText(/No adapters installed/),
    ).toBeInTheDocument();
  });
});

describe("Navigation section (GLS-FR-03 / GLS-FR-28)", () => {
  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: is one of the tab's sections, directly after Appearance", async () => {
    wireBackend({});
    render(<GlobalSettings />);

    const sections = screen
      .getAllByRole("tab")
      .map((el) => el.textContent?.trim());
    expect(sections).toContain("Navigation");
    expect(sections.indexOf("Navigation")).toBe(
      sections.indexOf("Appearance") + 1,
    );

    await userEvent.click(screen.getByRole("tab", { name: "Navigation" }));

    expect(
      await screen.findByRole("checkbox", { name: "Selection follows tab" }),
    ).toBeChecked();
  });

  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: closing the tab after toggling asks for no confirmation", async () => {
    // GLS-FR-10 / GLS-FR-13: the section applies immediately and holds no dirty
    // state, so it never contributes to the discard confirmation.
    wireBackend({});
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "Navigation" }));
    await userEvent.click(
      await screen.findByRole("checkbox", { name: "Selection follows tab" }),
    );

    await waitFor(() =>
      expect(calls("save_app_preferences").length).toBeGreaterThan(0),
    );
    // SWN-FR-08 / GLS-FR-13: nothing is saved on account of this section when
    // the window closes, because it holds nothing for the sweep to write.
    expect(settingsSectionsPending()).toEqual([]);
  });
});

describe("GitHub section (GLS-FR-03 / GLS-FR-15)", () => {
  it("GLS-FR-03, GLS-FR-15 is one of the tab's sections and renders the token list", async () => {
    wireBackend({
      overrides: {
        list_github_tokens: () => [
          {
            id: "t1",
            label: "work laptop",
            accountLogin: "raver119",
            scopes: ["repo"],
            maskedHint: "a3f9",
            addedAt: "2026-03-12T10:00:00Z",
            lastVerifiedAt: null,
            state: "valid",
          },
        ],
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "GitHub" }));

    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toHaveTextContent(
        "work laptop",
      ),
    );
  });

  it("GHA-FR-14 never reports the tab dirty on account of a GitHub action", async () => {
    // GLS-FR-15 / GHA-FR-14: every action in this section applies immediately,
    // so it can never be what triggers the discard-on-close confirmation
    // (GLS-FR-13).
    wireBackend({
      overrides: {
        list_github_tokens: () => [],
        add_github_token: () => ({
          id: "t1",
          label: "work",
          accountLogin: "raver119",
          scopes: ["repo"],
          maskedHint: "a3f9",
          addedAt: "2026-03-12T10:00:00Z",
          lastVerifiedAt: null,
          state: "valid",
        }),
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "GitHub" }));
    await waitFor(() =>
      expect(screen.getByTestId("github-tokens-empty")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: /Add token/ }));
    await userEvent.type(screen.getByLabelText("Token"), "ghp_good");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toBeInTheDocument(),
    );
    // SWN-FR-08 / GLS-FR-13: nothing is saved on account of this section when
    // the window closes, because it holds nothing for the sweep to write.
    expect(settingsSectionsPending()).toEqual([]);
  });
});

describe("Agents section (GLS-FR-03 / GLS-FR-24)", () => {
  // The agent surfaces read the keychain-free catalogue, not the credential
  // listing, so this is the shape they are given (AGR-FR-16).
  const verifiedOpenRouter = {
    provider: "openrouter",
    displayName: "OpenRouter",
    state: "verified",
    models: [{ id: "m", label: "Model M" }],
  };

  const anAgent = {
    id: "a1",
    nickname: "arch",
    title: "",
    provider: "openrouter",
    modelId: "m",
    instructions: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    reasoning: null,
  };

  it("GLS-FR-03, GLS-FR-12, GLS-FR-24 is one of the tab's sections and holds no dirty state", async () => {
    // GLS-FR-03 / GLS-FR-24 / AGT-FR-21: every action here applies immediately
    // through its own operation, so creating, editing, or deleting an agent can
    // never be what triggers the discard-on-close confirmation (GLS-FR-13).
    // The stub keeps what it was sent, so the re-read every write triggers sees
    // what really landed rather than the fixture it started from — a stub that
    // served the original list back would make a deletion look like it failed.
    let registry = [anAgent];
    wireBackend({
      overrides: {
        list_agents: () => registry,
        get_active_ai_api_catalog: () => ({ resolution: "inherited", catalog: verifiedOpenRouter }),
        delete_agent: () => {
          registry = [];
          return registry;
        },
        update_agent: (args) => {
          const saved = { ...anAgent, ...(args?.draft as Record<string, unknown>) };
          registry = [saved as typeof anAgent];
          return saved;
        },
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "Agents" }));
    await waitFor(() =>
      expect(screen.getByTestId("agent-row")).toHaveTextContent("@arch"),
    );

    // An edit that lands.
    await userEvent.click(screen.getByTestId("agent-edit"));
    await userEvent.type(
      screen.getByTestId("agent-instructions"),
      "Argue about structure.",
    );
    await userEvent.click(screen.getByTestId("agent-editor-confirm"));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument(),
    );

    // And a deletion.
    await userEvent.click(screen.getByTestId("agent-delete"));
    await userEvent.click(screen.getByTestId("agent-delete-confirm-ok"));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-row")).not.toBeInTheDocument(),
    );

    // SWN-FR-08 / GLS-FR-13: nothing is saved on account of this section when
    // the window closes, because it holds nothing for the sweep to write.
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("opens no editor when the section is reached any other way", async () => {
    // The Agents section is an ordinary section: reaching it from the nav must
    // not open a modal the author did not ask for.
    wireBackend({
      overrides: {
        list_agents: () => [anAgent],
        get_active_ai_api_catalog: () => ({ resolution: "inherited", catalog: verifiedOpenRouter }),
      },
    });
    render(<GlobalSettings />);
    await userEvent.click(screen.getByRole("tab", { name: "Agents" }));
    await waitFor(() => expect(screen.getByTestId("agent-row")).toBeInTheDocument());
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();
  });

  it("honours a roster request against a tab that is already open", async () => {
    // AGT-FR-06: the requirement is not conditional on the tab being closed.
    // Read only at mount, the request would be dropped whenever the author
    // already had Global settings open — which is most of the time.
    wireBackend({
      overrides: {
        list_agents: () => [anAgent],
        get_active_ai_api_catalog: () => ({ resolution: "inherited", catalog: verifiedOpenRouter }),
      },
    });
    function Host() {
      const [request, setRequest] = useState<{
        agentId: string | null;
        nonce: number;
      } | null>(null);
      return (
        <>
          <button onClick={() => setRequest({ agentId: "a1", nonce: 1 })}>
            roster row
          </button>
          <GlobalSettings
            agentEditorRequest={request}
            onAgentEditorRequestHandled={() => setRequest(null)}
          />
        </>
      );
    }
    render(<Host />);
    // The tab is already open, on another section.
    await screen.findByRole("tab", { name: "Appearance" });
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "roster row" }));
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-nickname")).toHaveValue("arch");
    expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("opens on the Agents section when the roster asked for an editor", async () => {
    // AGT-FR-06 / AGT-FR-07: the roster is how an agent is reached to be looked
    // at, so the tab it opens lands on the right section rather than on
    // Appearance with the section still to be found.
    wireBackend({
      overrides: {
        list_agents: () => [anAgent],
        get_active_ai_api_catalog: () => ({ resolution: "inherited", catalog: verifiedOpenRouter }),
      },
    });
    render(<GlobalSettings agentEditorRequest={{ agentId: "a1", nonce: 1 }} />);
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-nickname")).toHaveValue("arch");
    expect(screen.getByRole("tab", { name: "Agents" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
