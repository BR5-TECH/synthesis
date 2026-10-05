/**
 * Appearance through the shell: the top-chrome theme selector and the
 * typographic roles the whole application is drawn with.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { applyFontRoles } from "./state/fontRoles";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import type { ThemePreference } from "./types";
import { resetPanelReveals } from "./state/panelReveal";
import { defaultInvoke, enterIde, resetAppFixture } from "./test/appFixtures";

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

describe("Top-chrome theme selector (SNV-FR-15 / SNV-FR-16)", () => {
  it("SNV-FR-16: persists the chrome selection via save_app_preferences, applies it immediately, and restores it on relaunch", async () => {
    // Persisted preference is "system" and the OS reports light, so the app
    // starts light; selecting "dark" must switch the root and survive relaunch.
    vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: false }));
    let stored: ThemePreference = "system";
    invokeMock.mockImplementation(
      async (cmd: string, args?: { preferences?: { theme: ThemePreference } }) => {
        if (cmd === "load_app_preferences") return { theme: stored };
        if (cmd === "save_app_preferences") {
          stored = args!.preferences!.theme;
          return undefined;
        }
        return defaultInvoke(cmd);
      },
    );

    const { unmount } = render(<App />);
    await enterIde();

    const chrome = screen.getByTestId("chrome-theme-select");
    // Starts resolved to light (system preference + OS light).
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("light"),
    );

    await userEvent.click(within(chrome).getByRole("radio", { name: "Dark" }));

    // Persisted with the documented payload (SNV-FR-16).
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "save_app_preferences"),
      ).toBe(true),
    );
    // GSS-FR-20: the whole record, not a `{ theme }` partial. The record also
    // carries the main window's full-screen state (SNV-FR-38), which a theme
    // change must leave exactly as it found it.
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "save_app_preferences")![1],
    ).toEqual({
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
    // Applied to the root immediately, no relaunch.
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("dark"),
    );

    // Relaunch: a fresh App reads the now-persisted value and starts dark.
    unmount();
    render(<App />);
    await enterIde();
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("dark"),
    );
    expect(
      within(screen.getByTestId("chrome-theme-select")).getByRole("radio", {
        name: "Dark",
      }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("SNV-FR-15: reflects the persisted preference at startup, with no interaction and no save", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "light" };
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    const chrome = screen.getByTestId("chrome-theme-select");
    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Light" }),
      ).toHaveAttribute("aria-checked", "true"),
    );
    // Initialise-only: reading the preference must not persist anything.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "save_app_preferences"),
    ).toBe(false);
  });

  it("SNV-FR-16: selecting System from the chrome persists 'system' and resolves the root to the OS scheme", async () => {
    // The decision log makes "system" reachable from the chrome (not just
    // light/dark). OS reports light; the literal preference, not a resolved
    // value, must be persisted.
    vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: false }));
    let stored: ThemePreference = "dark";
    invokeMock.mockImplementation(
      async (cmd: string, args?: { preferences?: { theme: ThemePreference } }) => {
        if (cmd === "load_app_preferences") return { theme: stored };
        if (cmd === "save_app_preferences") {
          stored = args!.preferences!.theme;
          return undefined;
        }
        return defaultInvoke(cmd);
      },
    );

    render(<App />);
    await enterIde();
    const chrome = screen.getByTestId("chrome-theme-select");
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("dark"),
    );

    await userEvent.click(within(chrome).getByRole("radio", { name: "System" }));

    // Literal "system" persisted (not collapsed to light/dark).
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.find((c) => c[0] === "save_app_preferences")![1],
      ).toEqual({
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
    // Root resolves to the OS scheme (light), and the System radio is selected.
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("light"),
    );
    expect(
      within(chrome).getByRole("radio", { name: "System" }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("SNV-FR-16: rolls back the chrome selection when persistence fails, so the displayed theme matches what survives relaunch", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "dark" };
      if (cmd === "save_app_preferences") throw "disk full";
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    const chrome = screen.getByTestId("chrome-theme-select");
    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Dark" }),
      ).toHaveAttribute("aria-checked", "true"),
    );

    await userEvent.click(within(chrome).getByRole("radio", { name: "Light" }));

    // Save attempted, failed -> rolled back to the last persisted value.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "save_app_preferences"),
      ).toBe(true),
    );
    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Dark" }),
      ).toHaveAttribute("aria-checked", "true"),
    );
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("dark"),
    );
  });

  it("SNV-FR-15: a chrome change is announced, so the Appearance section can reflect it", async () => {
    // GLS-FR-05 / SWN-FR-01: the Appearance section is in the Global settings
    // child window now, which is a webview of its own — it cannot read this
    // window's state, so a chrome selection reaches it as an announcement or not
    // at all. This is the emitting half; `SettingsWindowApp` is the other.
    render(<App />);
    await enterIde();
    const chrome = screen.getByTestId("chrome-theme-select");
    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Dark" }),
      ).toHaveAttribute("aria-checked", "true"),
    );

    await userEvent.click(within(chrome).getByRole("radio", { name: "Light" }));

    await waitFor(() =>
      expect(
        emitMock.mock.calls.filter((c) => c[0] === "app-preferences:changed"),
      ).toHaveLength(1),
    );
  });

  it("SNV-FR-15: a theme written by the settings window is reflected by the chrome selector", async () => {
    // The receiving half of the one shared preference. The settings window
    // persists the theme itself and announces it; this window re-reads the
    // record and applies it, which is what "applies to the application root
    // immediately" means once the editor is another window (GLS-FR-05).
    let stored: ThemePreference = "dark";
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: stored };
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();
    const chrome = screen.getByTestId("chrome-theme-select");
    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Dark" }),
      ).toHaveAttribute("aria-checked", "true"),
    );

    // The other window wrote "light" and said so.
    stored = "light";
    fireBusEvent("app-preferences:changed");

    await waitFor(() =>
      expect(
        within(chrome).getByRole("radio", { name: "Light" }),
      ).toHaveAttribute("aria-checked", "true"),
    );
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("light"),
    );
  });
});

// ---------------------------------------------------------------------------
// Typographic roles applied app-wide (OVW-FR-13 / OVW-FR-14)
// ---------------------------------------------------------------------------

describe("Typographic roles (OVW-FR-14)", () => {
  /** The role properties live on the shared jsdom root; clear them between tests. */
  afterEach(() => {
    for (const role of ["ui", "rich", "source"]) {
      document.documentElement.style.removeProperty(`--font-${role}-family`);
      document.documentElement.style.removeProperty(`--font-${role}-size`);
      document.documentElement.style.removeProperty(`--font-${role}-line-height`);
    }
  });

  const rootProp = (name: string) =>
    document.documentElement.style.getPropertyValue(name);

  it("OVW-FR-14: applies the persisted roles at startup, before any user input", async () => {
    // The Project picker is the startup surface, so the roles have to be on the
    // root before the IDE shell is ever reached — a font applied only once a
    // project is open is a flash of the built-in face on every launch.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences")
        return {
          theme: "dark",
          fonts: {
            ui: { family: "Helvetica Neue" },
            rich: { sizePx: 18 },
          },
        };
      return defaultInvoke(cmd);
    });

    render(<App />);

    await waitFor(() =>
      expect(rootProp("--font-ui-family")).toContain('"Helvetica Neue"'),
    );
    expect(rootProp("--font-rich-size")).toBe("18px");
    // Still on the picker: nothing has been clicked.
    expect(
      screen.queryByRole("button", { name: "Global settings" }),
    ).not.toBeInTheDocument();

    // And the same values are in place once the IDE shell mounts.
    await enterIde();
    expect(rootProp("--font-ui-family")).toContain('"Helvetica Neue"');
    expect(rootProp("--font-rich-size")).toBe("18px");
  });

  it("OVW-FR-14: an unset role leaves the root alone, so the built-in default renders", async () => {
    // The stylesheet declares the built-in value; writing a number over it here
    // would give the app two copies of the same default to keep in step.
    render(<App />);
    await enterIde();
    expect(rootProp("--font-ui-family")).toBe("");
    expect(rootProp("--font-source-size")).toBe("");
    expect(rootProp("--font-rich-line-height")).toBe("");
  });

  it("OVW-FR-14: a role changed in Global settings leaves an open Editor's content and position as it found them", async () => {
    // The risk: a root-level change tearing an editing surface down. Applying
    // the roles to `:root` rather than threading them through the tree is what
    // prevents it — no surface takes a font value as a prop or a key, so
    // nothing above the viewport re-renders on a font change.
    //
    // Since SWN-FR-01 the Appearance section is a child window of its own, so
    // the change reaches this window as an announcement and the Editor is not
    // even unmounted by the author going to change it. That makes the promise
    // stronger and directly checkable: the SAME element survives the change.
    let stored: Record<string, unknown> = { theme: "dark" };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return stored;
      return defaultInvoke(cmd);
    });

    render(<App />);
    await enterIde();

    const panel = within(document.querySelector(".vpanel") as HTMLElement);
    await userEvent.click(await panel.findByText("CHG-changes.md"));
    const editorSurface = (await waitFor(() => {
      const el = document.querySelector(".viewport__body .editor");
      expect(el).not.toBeNull();
      return el;
    })) as HTMLElement;
    const bodyBefore = editorSurface.textContent;
    expect(bodyBefore).toBeTruthy();
    editorSurface.scrollTop = 40;

    // The settings window wrote a new Rich Markdown size and said so.
    stored = { theme: "dark", fonts: { rich: { sizePx: 20 } } };
    fireBusEvent("app-preferences:changed");

    // Applied app-wide immediately, with no relaunch (OVW-FR-14, GLS-FR-20).
    await waitFor(() => expect(rootProp("--font-rich-size")).toBe("20px"));

    // The very same Editor element, still holding the same document at the same
    // position: nothing remounted to apply the change.
    const after = document.querySelector(
      ".viewport__body .editor",
    ) as HTMLElement;
    expect(after).toBe(editorSurface);
    expect(after.textContent).toBe(bodyBefore);
    expect(after.scrollTop).toBe(40);
  });

  it("OVW-FR-14: the roles reach every surface through the root alone, so nothing re-renders to apply them", async () => {
    // The structural half of the requirement, and the part a rendering test
    // cannot reach: if a font value were threaded through the tree as a prop or
    // a key, changing it would remount whatever holds it — an Editor's buffer,
    // its undo history, its scroll position. The applier writing only to
    // `document.documentElement` is what makes that impossible.
    const el = document.createElement("div");
    const sentinel = document.createElement("span");
    el.appendChild(sentinel);

    applyFontRoles(el, { rich: { sizePx: 20 } });
    applyFontRoles(el, { rich: { sizePx: 24 }, ui: { family: "Georgia" } });
    applyFontRoles(el, {});

    // Three applications, and the subtree underneath is untouched throughout —
    // the change lives entirely in the root's own inline properties.
    expect(el.firstChild).toBe(sentinel);
    expect(el.childNodes).toHaveLength(1);
    expect(sentinel.getAttribute("style")).toBeNull();
  });

  it("OVW-FR-14 / GLS-FR-20: the applied roles survive a relaunch", async () => {
    // The Appearance section persists the roles itself, from its own window
    // (GLS-FR-20). What this window owes them is that they are on the root
    // before any surface renders, on this launch and on every one after.
    const stored: Record<string, unknown> = {
      theme: "dark",
      fonts: { source: { sizePx: 16 } },
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return stored;
      return defaultInvoke(cmd);
    });

    const { unmount } = render(<App />);
    await enterIde();
    await waitFor(() => expect(rootProp("--font-source-size")).toBe("16px"));

    unmount();
    for (const role of ["ui", "rich", "source"]) {
      document.documentElement.style.removeProperty(`--font-${role}-size`);
    }
    resetAppPreferencesCache();

    render(<App />);
    await waitFor(() => expect(rootProp("--font-source-size")).toBe("16px"));
  });
});
