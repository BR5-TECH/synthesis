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

describe("the AI API and Agentic AI sections (GLS-FR-03 / GLS-FR-16)", () => {
  /** The three vendor records the section renders from (AIC-FR-02). */
  const cliBase = {
    kind: "cli",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: false,
  };
  const vendors = [
    {
      ...cliBase,
      vendor: "claude_code",
      displayName: "Claude Code",
      binaryPath: "/opt/homebrew/bin/claude",
      pathOrigin: "detected",
      state: "verified",
      version: "2.1.4",
      verifiedAt: "2026-07-01T09:00:00Z",
      models: [{ id: "opus", label: "Opus" }],
      modelsOrigin: "catalog",
      selectedModel: null,
      reasoningEfforts: [{ id: "high", label: "High" }],
      selectedEffort: null,
      active: false,
    },
    {
      ...cliBase,
      vendor: "codex",
      displayName: "Codex",
      binaryPath: null,
      pathOrigin: "unset",
      state: "unconfigured",
      version: null,
      verifiedAt: null,
      models: [],
      modelsOrigin: "catalog",
      selectedModel: null,
      reasoningEfforts: [],
      selectedEffort: null,
      active: false,
    },
    {
      ...cliBase,
      vendor: "opencode",
      displayName: "OpenCode",
      binaryPath: null,
      pathOrigin: "unset",
      state: "unconfigured",
      version: null,
      verifiedAt: null,
      models: [],
      modelsOrigin: "catalog",
      selectedModel: null,
      reasoningEfforts: [],
      selectedEffort: null,
      active: false,
    },
  ];
  /** The AI API level's providers — the other AI section (GLS-FR-16). */
  const providers = [
    {
      provider: "anthropic",
      displayName: "Anthropic",
      baseUrl: null,
      keyState: "unset",
      maskedHint: null,
      keyRequired: true,
      state: "unconfigured",
      verifiedAt: null,
      models: [],
      modelsOrigin: "catalog",
      selectedModel: null,
      active: false,
    },
  ];

  it("GLS-FR-03, GLS-FR-16 gives each level its own section, distinct from each other and from Installed adapters", async () => {
    // GLS-FR-03 / GLS-FR-16: the two levels of AII-ai-integrations.md are two
    // navigable sections, not two halves of one. Both are also distinct from
    // Installed adapters (GLS-FR-09): an adapter transforms Synthesis artifacts
    // into an external agent's workspace layout, while an AI integration is
    // something this application calls or drives itself.
    wireBackend({
      adapters: [{ id: "claude-code", name: "Claude Code adapter", source: "built-in" }],
      overrides: {
        list_agentic_integrations: () => vendors,
        list_ai_api_integrations: () => providers,
        detect_agentic_cli_binary: () => ({ path: null }),
      },
    });
    render(<GlobalSettings />);

    expect(
      screen.getByRole("tab", { name: "Installed adapters" }),
    ).toBeInTheDocument();

    // AI API holds the API level and nothing of the agentic one.
    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    const apiLevel = within(await screen.findByTestId("ai-api-level"));
    expect(
      await apiLevel.findByRole("tab", { name: /Anthropic/ }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("agentic-level")).not.toBeInTheDocument();
    // The section heading names the section — the level renders no heading of
    // its own (AII-FR-02).
    expect(
      screen.getByRole("heading", { name: "AI API" }),
    ).toBeInTheDocument();

    // Agentic AI holds the other level, and equally nothing of the first.
    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    const agentic = within(await screen.findByTestId("agentic-level"));
    expect(
      await agentic.findByRole("tab", { name: /Claude Code/ }),
    ).toBeInTheDocument();
    expect(agentic.getByRole("tab", { name: /Codex/ })).toBeInTheDocument();
    expect(agentic.getByRole("tab", { name: /OpenCode/ })).toBeInTheDocument();
    expect(screen.queryByTestId("ai-api-level")).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Agentic AI" }),
    ).toBeInTheDocument();
  });

  it("AII-FR-01, AII-FR-02, AII-FR-10 issues neither level's operations on account of the other, and discards a candidate on the way out", async () => {
    // AII-FR-01: "neither level's operations are issued on account of the
    // other". This is the only place that claim can be tested — a component
    // test mounts the level directly, so it can say nothing about what mounting
    // one does to the other. It matters most for `detect_agentic_cli_binary`,
    // the one operation in this surface that touches the filesystem: running it
    // because the author opened the *AI API* section would break the
    // non-functional requirement that no binary is touched before Verify.
    //
    // AII-FR-10: the fields hold candidates until a verification commits them,
    // and reopening shows the last configuration that verified rather than an
    // abandoned edit. Splitting the levels into two sections is what makes that
    // reachable — before it, both levels stayed mounted for the tab's whole
    // life and no navigation could discard anything.
    const listed = vi.fn(() => providers);
    wireBackend({
      overrides: {
        list_ai_api_integrations: listed,
        list_agentic_integrations: () => vendors,
        detect_agentic_cli_binary: () => ({ path: null }),
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    const url = await within(
      await screen.findByTestId("ai-api-level"),
    ).findByLabelText("Base URL");
    expect(listed).toHaveBeenCalledTimes(1);
    // Nothing of the agentic level was reached for — no list, and above all no
    // filesystem probe.
    expect(calls("list_agentic_integrations").length).toBe(0);
    expect(calls("detect_agentic_cli_binary").length).toBe(0);

    await userEvent.clear(url);
    await userEvent.type(url, "https://abandoned.example/v1");

    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    await screen.findByTestId("agentic-level");
    expect(screen.queryByTestId("ai-api-level")).not.toBeInTheDocument();
    // The agentic tab shows its own stored value, never the other section's.
    expect(
      within(screen.getByTestId("agentic-level")).getByLabelText("Binary"),
    ).not.toHaveValue("https://abandoned.example/v1");

    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    const reopened = within(await screen.findByTestId("ai-api-level"));
    // The level asked the backend again rather than restoring what it held, and
    // the candidate that never verified is gone — replaced by the provider
    // default, not merely blanked.
    await waitFor(() => expect(listed).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(reopened.getByLabelText("Base URL")).toHaveValue(
        "https://api.anthropic.com/v1",
      ),
    );
    // And nothing was written on the way out.
    expect(calls("verify_ai_api_integration").length).toBe(0);
  });

  it("AII-FR-26, AII-FR-17, AII-FR-29 offers detection afresh on a later visit, having suppressed it after Clear", async () => {
    // AII-FR-26 suppresses detection for the rest of the visit, so that Clear
    // is not immediately undone by the field refilling. AII-FR-17 then applies
    // again on a later visit, because the tab genuinely has no stored path —
    // the same thing it would show after a relaunch. What it offers is a
    // candidate; Clear stored nothing and this stores nothing either.
    //
    // The component-level guard for this (AiIntegrations.test.tsx, AII-FR-26, AII-FR-17, AII-FR-29)
    // never unmounts, so only a test that navigates can see the boundary.
    // Widened to the record shape the backend actually returns: the fixture's
    // inferred literal types would not admit a cleared vendor.
    let list: Record<string, unknown>[] = vendors;
    const detect = vi.fn(() => ({ path: "/opt/homebrew/bin/claude" }));
    wireBackend({
      overrides: {
        list_ai_api_integrations: () => providers,
        list_agentic_integrations: () => list,
        detect_agentic_cli_binary: detect,
        clear_agentic_integration: () => {
          list = list.map((v) =>
            v.vendor === "claude_code"
              ? {
                  ...v,
                  binaryPath: null,
                  state: "unconfigured",
                  version: null,
                  active: false,
                }
              : v,
          );
          return list;
        },
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    const level = within(await screen.findByTestId("agentic-level"));
    // Claude Code has a stored path, so nothing was detected for it yet.
    await waitFor(() =>
      expect(level.getByLabelText("Binary")).toHaveValue(
        "/opt/homebrew/bin/claude",
      ),
    );
    const before = detect.mock.calls.length;

    await userEvent.click(level.getByRole("button", { name: "Clear" }));
    await waitFor(() =>
      expect(level.getByLabelText("Binary")).toHaveValue(""),
    );
    // Suppressed: the field the author just emptied stays empty.
    expect(detect.mock.calls.length).toBe(before);

    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    await screen.findByTestId("ai-api-level");
    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));

    const revisited = within(await screen.findByTestId("agentic-level"));
    await waitFor(() =>
      expect(revisited.getByLabelText("Binary")).toHaveValue(
        "/opt/homebrew/bin/claude",
      ),
    );
    expect(detect.mock.calls.length).toBeGreaterThan(before);
    // An offer, not a stored value: nothing was written to make it so.
    expect(calls("verify_agentic_integration").length).toBe(0);
  });

  it("GLS-FR-03, GLS-FR-16 puts nothing in the window's save sweep on account of an AI action", async () => {
    // GLS-FR-16: every action in either AI section applies immediately, so
    // neither can be what triggers the discard-on-close confirmation
    // (GLS-FR-13). The spec scenario requires acting in *both* sections, so
    // both are acted in here.
    //
    // The registry the window's sweep spends is the observable: a section that
    // held a change for a save would be in it. The other load-bearing half is
    // that each activation reached the backend *during* the action rather than
    // being held for a save, which is what "applies immediately" actually
    // means — so both are asserted.
    let list = vendors;
    let apiList = [
      {
        ...providers[0],
        baseUrl: "https://api.anthropic.com/v1",
        keyState: "stored",
        maskedHint: "••••3f9a",
        state: "verified",
        verifiedAt: "2026-07-01T09:00:00Z",
      },
    ];
    wireBackend({
      overrides: {
        list_agentic_integrations: () => list,
        list_ai_api_integrations: () => apiList,
        detect_agentic_cli_binary: () => ({ path: null }),
        set_active_agentic_integration: (args) => {
          list = list.map((v) => ({ ...v, active: v.vendor === args?.vendor }));
          return list;
        },
        set_active_ai_api_integration: (args) => {
          apiList = apiList.map((i) => ({
            ...i,
            active: i.provider === args?.provider,
          }));
          return apiList;
        },
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    const agenticLevel = within(await screen.findByTestId("agentic-level"));
    await userEvent.click(
      await agenticLevel.findByRole("button", { name: /use this integration/i }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("agentic-active-marker")).toBeInTheDocument(),
    );
    expect(calls("set_active_agentic_integration").length).toBe(1);
    // Nothing is left pending: no save control exists to press.
    expect(screen.queryByRole("button", { name: /^save/i })).toBeNull();

    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    const api = within(await screen.findByTestId("ai-api-level"));
    await userEvent.click(
      await api.findByRole("button", { name: /use this integration/i }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("ai-api-active-marker")).toBeInTheDocument(),
    );
    expect(calls("set_active_ai_api_integration").length).toBe(1);
    expect(screen.queryByRole("button", { name: /^save/i })).toBeNull();

    // Leaving the section did not strand the agentic activation either — it was
    // written when it was made, so returning shows it still active.
    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    await waitFor(() =>
      expect(screen.getByTestId("agentic-active-marker")).toBeInTheDocument(),
    );

    // SWN-FR-08 / GLS-FR-13: nothing is saved on account of this section when
    // the window closes, because it holds nothing for the sweep to write.
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("GLS-FR-12 renders an empty state in every section on a fresh machine", async () => {
    // GLS-FR-12: no plugin or adapter installed, no GitHub token stored, no AI
    // CLI integration configured, and no project ever opened — each section
    // renders its own empty state and none reports an error.
    wireBackend({
      recents: [],
      plugins: [],
      adapters: [],
      overrides: {
        list_github_tokens: () => [],
        list_agentic_integrations: () =>
          vendors.map((v) => ({
            ...v,
            binaryPath: null,
            state: "unconfigured",
            version: null,
            active: false,
          })),
        list_ai_api_integrations: () => providers,
        detect_agentic_cli_binary: () => ({ path: null }),
      },
    });
    render(<GlobalSettings />);

    await userEvent.click(screen.getByRole("tab", { name: "Recent projects" }));
    expect(await screen.findByText(/No recent projects/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Installed plugins" }));
    expect(await screen.findByText(/No plugins installed/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Installed adapters" }));
    expect(await screen.findByText(/No adapters installed/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "GitHub" }));
    expect(await screen.findByTestId("github-tokens-empty")).toBeInTheDocument();

    // GLS-FR-12 names both levels, and each is now its own section — so each
    // has to be navigated to and each has to carry its own empty state.
    await userEvent.click(screen.getByRole("tab", { name: "AI API" }));
    expect(await screen.findByTestId("ai-api-empty")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Agentic AI" }));
    expect(await screen.findByTestId("agentic-empty")).toBeInTheDocument();

    // None of it reported an error.
    expect(screen.queryByText(/✗/)).not.toBeInTheDocument();
  });
});
